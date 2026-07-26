use std::{
    collections::HashSet,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::Duration,
};

use reqwest::{
    Client, Method, Response, StatusCode, Url,
    header::{AUTHORIZATION, LOCATION},
    redirect::Policy,
};
use serde_json::Value;
use tokio::net::lookup_host;

const MAX_REDIRECTS: usize = 5;

#[derive(Clone)]
pub struct LlmHttpClient {
    require_public_https: bool,
    private_host_allowlist: HashSet<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum LlmHttpError {
    #[error("LLM API URL must use HTTP or HTTPS")]
    InvalidScheme,
    #[error("production LLM API URLs must use HTTPS unless the host is explicitly allowlisted")]
    HttpsRequired,
    #[error("LLM API URL must contain a host")]
    MissingHost,
    #[error("LLM API URL must not contain credentials or fragments")]
    UnsafeUrl,
    #[error("LLM API host could not be resolved")]
    DnsResolutionFailed,
    #[error("LLM API host resolved to a private, loopback, link-local, or reserved address")]
    PrivateAddressBlocked,
    #[error("LLM API redirect is missing a valid Location header")]
    InvalidRedirect,
    #[error("LLM API cross-origin redirects are not allowed")]
    CrossOriginRedirect,
    #[error("LLM API POST redirect must preserve its method")]
    UnsafePostRedirect,
    #[error("LLM API exceeded the redirect limit")]
    TooManyRedirects,
    #[error("LLM API HTTP client could not be built: {0}")]
    ClientBuild(reqwest::Error),
    #[error("LLM API request failed: {0}")]
    Request(reqwest::Error),
}

struct ValidatedTarget {
    host: String,
    addresses: Vec<SocketAddr>,
    pin_dns: bool,
}

impl LlmHttpClient {
    pub fn new(environment: &str, private_hosts: &[String]) -> Self {
        Self {
            require_public_https: environment.eq_ignore_ascii_case("production"),
            private_host_allowlist: private_hosts
                .iter()
                .map(|host| normalized_host(host))
                .collect(),
        }
    }

    pub fn validate_configured_url(&self, url: &Url) -> Result<(), LlmHttpError> {
        let host = url.host_str().ok_or(LlmHttpError::MissingHost)?;
        let allowlisted = self.is_allowlisted(host);
        if !matches!(url.scheme(), "http" | "https") {
            return Err(LlmHttpError::InvalidScheme);
        }
        if self.require_public_https && url.scheme() != "https" && !allowlisted {
            return Err(LlmHttpError::HttpsRequired);
        }
        if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
            return Err(LlmHttpError::UnsafeUrl);
        }
        Ok(())
    }

    pub async fn get(
        &self,
        url: Url,
        bearer_token: Option<&str>,
        timeout: Duration,
    ) -> Result<Response, LlmHttpError> {
        self.send(Method::GET, url, bearer_token, None, timeout)
            .await
    }

    pub async fn post_json(
        &self,
        url: Url,
        bearer_token: &str,
        body: &Value,
        timeout: Duration,
    ) -> Result<Response, LlmHttpError> {
        self.send(Method::POST, url, Some(bearer_token), Some(body), timeout)
            .await
    }

    async fn send(
        &self,
        method: Method,
        initial_url: Url,
        bearer_token: Option<&str>,
        body: Option<&Value>,
        timeout: Duration,
    ) -> Result<Response, LlmHttpError> {
        let initial_origin = origin(&initial_url)?;
        let mut url = initial_url;

        for redirect_count in 0..=MAX_REDIRECTS {
            // Resolve on every hop, then pin this client to exactly those
            // approved addresses so DNS cannot change between validation and
            // connection establishment.
            let target = self.validate_and_resolve(&url).await?;
            let mut builder = Client::builder()
                .no_proxy()
                .redirect(Policy::none())
                .connect_timeout(Duration::from_secs(5))
                .timeout(timeout);
            if target.pin_dns {
                builder = builder.resolve_to_addrs(&target.host, &target.addresses);
            }
            let client = builder.build().map_err(LlmHttpError::ClientBuild)?;
            let mut request = client.request(method.clone(), url.clone());
            if let Some(token) = bearer_token {
                request = request.header(AUTHORIZATION, format!("Bearer {token}"));
            }
            if let Some(body) = body {
                request = request.json(body);
            }
            let response = request.send().await.map_err(LlmHttpError::Request)?;
            if !is_redirect(response.status()) {
                return Ok(response);
            }
            if redirect_count == MAX_REDIRECTS {
                return Err(LlmHttpError::TooManyRedirects);
            }
            if method == Method::POST
                && !matches!(
                    response.status(),
                    StatusCode::TEMPORARY_REDIRECT | StatusCode::PERMANENT_REDIRECT
                )
            {
                return Err(LlmHttpError::UnsafePostRedirect);
            }
            let location = response
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or(LlmHttpError::InvalidRedirect)?;
            let next = url
                .join(location)
                .map_err(|_| LlmHttpError::InvalidRedirect)?;
            self.validate_configured_url(&next)?;
            if origin(&next)? != initial_origin {
                return Err(LlmHttpError::CrossOriginRedirect);
            }
            url = next;
        }

        Err(LlmHttpError::TooManyRedirects)
    }

    async fn validate_and_resolve(&self, url: &Url) -> Result<ValidatedTarget, LlmHttpError> {
        self.validate_configured_url(url)?;
        let host = url.host_str().ok_or(LlmHttpError::MissingHost)?;
        let port = url
            .port_or_known_default()
            .ok_or(LlmHttpError::InvalidScheme)?;
        let allowlisted = self.is_allowlisted(host);
        let normalized = normalized_host(host);

        let (addresses, pin_dns) = if let Ok(ip) = normalized.parse::<IpAddr>() {
            (vec![SocketAddr::new(ip, port)], false)
        } else {
            let addresses = lookup_host((normalized.as_str(), port))
                .await
                .map_err(|_| LlmHttpError::DnsResolutionFailed)?
                .collect::<Vec<_>>();
            if addresses.is_empty() {
                return Err(LlmHttpError::DnsResolutionFailed);
            }
            (addresses, true)
        };
        if !allowlisted && addresses.iter().any(|address| !is_public_ip(address.ip())) {
            return Err(LlmHttpError::PrivateAddressBlocked);
        }

        Ok(ValidatedTarget {
            host: normalized,
            addresses,
            pin_dns,
        })
    }

    fn is_allowlisted(&self, host: &str) -> bool {
        self.private_host_allowlist.contains(&normalized_host(host))
    }
}

fn origin(url: &Url) -> Result<(String, String, u16), LlmHttpError> {
    Ok((
        url.scheme().to_owned(),
        normalized_host(url.host_str().ok_or(LlmHttpError::MissingHost)?),
        url.port_or_known_default()
            .ok_or(LlmHttpError::InvalidScheme)?,
    ))
}

fn normalized_host(host: &str) -> String {
    host.trim()
        .trim_matches(['[', ']'])
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

fn is_redirect(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::MOVED_PERMANENTLY
            | StatusCode::FOUND
            | StatusCode::SEE_OTHER
            | StatusCode::TEMPORARY_REDIRECT
            | StatusCode::PERMANENT_REDIRECT
    )
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => is_public_ipv6(ip),
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    !matches!(
        octets,
        [0, ..]
            | [10, ..]
            | [100, 64..=127, ..]
            | [127, ..]
            | [169, 254, ..]
            | [172, 16..=31, ..]
            | [192, 0, 0, ..]
            | [192, 0, 2, ..]
            | [192, 88, 99, ..]
            | [192, 168, ..]
            | [198, 18..=19, ..]
            | [198, 51, 100, ..]
            | [203, 0, 113, ..]
            | [224..=255, ..]
    )
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    let segments = ip.segments();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || segments[0] == 0
        || (segments[0] == 0x0064 && segments[1] == 0xff9b)
        || (segments[0] == 0x0100 && segments[1] == 0 && segments[2] == 0 && segments[3] == 0)
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] & 0xffc0) == 0xfec0
        || (segments[0] == 0x2001 && segments[1] == 0x0002)
        || (segments[0] == 0x2001 && (segments[1] & 0xfff0) == 0x0010)
        || (segments[0] == 0x2001 && (segments[1] & 0xfff0) == 0x0020)
        || (segments[0] == 0x2001 && segments[1] == 0x0db8)
        || segments[0] == 0x2002
        || segments[0] == 0x5f00)
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use reqwest::Url;

    use super::{LlmHttpClient, LlmHttpError, is_public_ip};

    #[test]
    fn private_loopback_link_local_and_documentation_addresses_are_not_public() {
        for value in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.169.254",
            "192.168.1.2",
            "100.64.0.1",
            "192.0.2.1",
            "::1",
            "fe80::1",
            "fc00::1",
            "2001:db8::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(
                !is_public_ip(value.parse::<IpAddr>().expect("valid test IP")),
                "{value} must be blocked"
            );
        }
        assert!(is_public_ip("8.8.8.8".parse().expect("public IPv4")));
        assert!(is_public_ip(
            "2606:4700:4700::1111".parse().expect("public IPv6")
        ));
    }

    #[test]
    fn production_requires_https_except_for_explicit_private_hosts() {
        let policy = LlmHttpClient::new("production", &[]);
        let http = Url::parse("http://api.example.com/v1").expect("URL");
        assert!(matches!(
            policy.validate_configured_url(&http),
            Err(LlmHttpError::HttpsRequired)
        ));

        let private_policy = LlmHttpClient::new("production", &["ollama".to_owned()]);
        let allowlisted = Url::parse("http://ollama:11434/v1").expect("URL");
        private_policy
            .validate_configured_url(&allowlisted)
            .expect("explicit private host");
    }

    #[tokio::test]
    async fn direct_private_targets_are_blocked_before_connection() {
        let policy = LlmHttpClient::new("test", &[]);
        let error = policy
            .get(
                Url::parse("http://127.0.0.1:9/v1/models").expect("URL"),
                None,
                std::time::Duration::from_secs(1),
            )
            .await
            .expect_err("loopback must be blocked");
        assert!(matches!(error, LlmHttpError::PrivateAddressBlocked));
    }
}
