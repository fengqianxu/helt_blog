use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use reqwest::{Client, Method};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

pub struct ObjectStorage {
    endpoint: String,
    access_key: String,
    secret_key: String,
    public_bucket: String,
}

impl ObjectStorage {
    pub fn new(
        endpoint: String,
        access_key: String,
        secret_key: String,
        public_bucket: String,
    ) -> Self {
        Self {
            endpoint,
            access_key,
            secret_key,
            public_bucket,
        }
    }

    pub fn public_bucket(&self) -> &str {
        &self.public_bucket
    }

    pub fn public_url(&self, object_key: &str) -> String {
        format!("/storage/{object_key}")
    }

    pub async fn put_public_object(
        &self,
        client: &Client,
        object_key: &str,
        content_type: &str,
        body: Vec<u8>,
    ) -> Result<()> {
        let request = self.signed_request(
            client,
            Method::PUT,
            object_key,
            Some(content_type),
            body,
            Utc::now(),
        )?;
        let response = client
            .execute(request)
            .await
            .context("MinIO upload request failed")?;
        if !response.status().is_success() {
            let status = response.status();
            let message = response.text().await.unwrap_or_default();
            bail!("MinIO rejected object upload with {status}: {message}");
        }
        Ok(())
    }

    pub async fn delete_public_object(&self, client: &Client, object_key: &str) -> Result<()> {
        let request = self.signed_request(
            client,
            Method::DELETE,
            object_key,
            None,
            Vec::new(),
            Utc::now(),
        )?;
        let response = client
            .execute(request)
            .await
            .context("MinIO cleanup request failed")?;
        if !response.status().is_success() && response.status().as_u16() != 404 {
            bail!("MinIO rejected object cleanup with {}", response.status());
        }
        Ok(())
    }

    pub async fn get_public_object(&self, client: &Client, object_key: &str) -> Result<Vec<u8>> {
        let response = self.open_public_object(client, object_key).await?;
        Ok(response
            .bytes()
            .await
            .context("MinIO object body could not be read")?
            .to_vec())
    }

    pub async fn open_public_object(
        &self,
        client: &Client,
        object_key: &str,
    ) -> Result<reqwest::Response> {
        let request = self.signed_request(
            client,
            Method::GET,
            object_key,
            None,
            Vec::new(),
            Utc::now(),
        )?;
        let response = client
            .execute(request)
            .await
            .context("MinIO download request failed")?;
        if !response.status().is_success() {
            bail!("MinIO rejected object download with {}", response.status());
        }
        Ok(response)
    }

    fn signed_request(
        &self,
        client: &Client,
        method: Method,
        object_key: &str,
        content_type: Option<&str>,
        body: Vec<u8>,
        now: DateTime<Utc>,
    ) -> Result<reqwest::Request> {
        let host = self
            .endpoint
            .split_once("://")
            .map(|(_, authority)| authority)
            .unwrap_or(&self.endpoint)
            .trim_end_matches('/');
        let canonical_uri = format!("/{}/{}", self.public_bucket, object_key);
        let url = format!("{}{}", self.endpoint, canonical_uri);
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date = now.format("%Y%m%d").to_string();
        let payload_hash = sha256_hex(&body);
        let (canonical_headers, signed_headers) = if let Some(content_type) = content_type {
            (
                format!(
                    "content-type:{content_type}\nhost:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n"
                ),
                "content-type;host;x-amz-content-sha256;x-amz-date",
            )
        } else {
            (
                format!(
                    "host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n"
                ),
                "host;x-amz-content-sha256;x-amz-date",
            )
        };
        let canonical_request = format!(
            "{}\n{canonical_uri}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}",
            method.as_str()
        );
        let credential_scope = format!("{date}/us-east-1/s3/aws4_request");
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{}",
            sha256_hex(canonical_request.as_bytes())
        );
        let date_key = hmac(
            format!("AWS4{}", self.secret_key).as_bytes(),
            date.as_bytes(),
        )?;
        let region_key = hmac(&date_key, b"us-east-1")?;
        let service_key = hmac(&region_key, b"s3")?;
        let signing_key = hmac(&service_key, b"aws4_request")?;
        let signature = hex(&hmac(&signing_key, string_to_sign.as_bytes())?);
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}",
            self.access_key
        );

        let mut request = client
            .request(method, url)
            .header("host", host)
            .header("x-amz-date", amz_date)
            .header("x-amz-content-sha256", payload_hash)
            .header("authorization", authorization);
        if let Some(content_type) = content_type {
            request = request.header("content-type", content_type);
        }
        request
            .body(body)
            .build()
            .context("signed MinIO request could not be built")
    }
}

fn sha256_hex(value: &[u8]) -> String {
    hex(&Sha256::digest(value))
}

fn hmac(key: &[u8], value: &[u8]) -> Result<Vec<u8>> {
    let mut signer = HmacSha256::new_from_slice(key).context("invalid HMAC key")?;
    signer.update(value);
    Ok(signer.finalize().into_bytes().to_vec())
}

fn hex(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::{ObjectStorage, sha256_hex};
    use chrono::{TimeZone, Utc};
    use reqwest::{Client, Method};

    #[test]
    fn signed_request_targets_the_public_bucket_without_exposing_the_secret() {
        let storage = ObjectStorage::new(
            "http://minio:9000".to_owned(),
            "access".to_owned(),
            "do-not-expose-this-secret".to_owned(),
            "blog-public".to_owned(),
        );
        let now = Utc.with_ymd_and_hms(2026, 7, 23, 12, 0, 0).unwrap();
        let request = storage
            .signed_request(
                &Client::new(),
                Method::PUT,
                "avatars/admin/1/example.png",
                Some("image/png"),
                b"image".to_vec(),
                now,
            )
            .unwrap();
        let authorization = request.headers()["authorization"].to_str().unwrap();

        assert_eq!(
            request.url().as_str(),
            "http://minio:9000/blog-public/avatars/admin/1/example.png"
        );
        assert!(authorization.contains("Credential=access/20260723/us-east-1/s3/aws4_request"));
        assert!(!authorization.contains("do-not-expose-this-secret"));
        assert_eq!(sha256_hex(b"image").len(), 64);
    }
}
