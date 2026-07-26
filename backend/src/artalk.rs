use anyhow::{Context, Result};
use reqwest::{Client, Response, StatusCode, Url};
use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Clone)]
pub struct ArtalkClient {
    inner: Option<ArtalkClientInner>,
}

#[derive(Clone)]
struct ArtalkClientInner {
    http: Client,
    base_url: Url,
    site_name: String,
    admin_name: String,
    admin_email: String,
    admin_password: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ArtalkError {
    #[error("Artalk request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Artalk {operation} returned {status}: {message}")]
    Http {
        operation: &'static str,
        status: StatusCode,
        message: String,
    },
    #[error("Artalk {0} returned an invalid response")]
    InvalidResponse(&'static str),
}

#[derive(Serialize)]
struct LoginRequest<'a> {
    name: &'a str,
    email: &'a str,
    password: &'a str,
}

#[derive(Deserialize)]
struct LoginResponse {
    token: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ArtalkPage {
    id: u64,
    key: String,
    site_name: String,
}

#[derive(Deserialize)]
struct PageListResponse {
    #[serde(default)]
    pages: Option<Vec<ArtalkPage>>,
}

#[derive(Serialize)]
struct PageUpdateRequest<'a> {
    site_name: &'a str,
    key: &'a str,
    title: &'a str,
    admin_only: bool,
}

#[derive(Serialize)]
struct PagePvRequest<'a> {
    page_key: &'a str,
    page_title: &'a str,
    site_name: &'a str,
}

impl ArtalkClient {
    pub fn new(http: Client, config: &Config) -> Result<Self> {
        let Some(raw_base_url) = config.artalk_internal_url.as_deref() else {
            return Ok(Self { inner: None });
        };
        let base_url = Url::parse(&format!("{}/", raw_base_url.trim_end_matches('/')))
            .context("ARTALK_INTERNAL_URL must be an absolute URL")?;
        if !matches!(base_url.scheme(), "http" | "https") {
            anyhow::bail!("ARTALK_INTERNAL_URL must use http or https");
        }
        Ok(Self {
            inner: Some(ArtalkClientInner {
                http,
                base_url,
                site_name: config.artalk_site_name.clone(),
                admin_name: config.artalk_admin_name.clone(),
                admin_email: config.artalk_admin_email.clone(),
                admin_password: config.artalk_admin_password.clone(),
            }),
        })
    }

    pub async fn set_page_commenting(
        &self,
        page_key: &str,
        page_title: &str,
        allowed: bool,
    ) -> Result<(), ArtalkError> {
        let Some(inner) = &self.inner else {
            return Ok(());
        };
        let token = inner.login().await?;
        let page = match inner.find_page(&token, page_key).await? {
            Some(page) => page,
            None if allowed => return Ok(()),
            None => {
                // Artalk has no dedicated page-create endpoint. Its official PV
                // endpoint creates the page record, which can then be locked.
                inner.ensure_page(page_key, page_title).await?;
                inner
                    .find_page(&token, page_key)
                    .await?
                    .ok_or(ArtalkError::InvalidResponse("page creation"))?
            }
        };
        let response = inner
            .http
            .put(inner.endpoint(&format!("api/v2/pages/{}", page.id)))
            .bearer_auth(token)
            .json(&PageUpdateRequest {
                site_name: &inner.site_name,
                key: &page.key,
                title: page_title,
                admin_only: !allowed,
            })
            .send()
            .await?;
        expect_success(response, "page update").await?;
        Ok(())
    }

    pub async fn delete_pages<'a>(
        &self,
        page_keys: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), ArtalkError> {
        let Some(inner) = &self.inner else {
            return Ok(());
        };
        let page_keys = page_keys.into_iter().collect::<Vec<_>>();
        if page_keys.is_empty() {
            return Ok(());
        }
        let token = inner.login().await?;
        for page_key in page_keys {
            let Some(page) = inner.find_page(&token, page_key).await? else {
                continue;
            };
            let response = inner
                .http
                .delete(inner.endpoint(&format!("api/v2/pages/{}", page.id)))
                .bearer_auth(&token)
                .send()
                .await?;
            expect_success(response, "page delete").await?;
        }
        Ok(())
    }
}

impl ArtalkClientInner {
    fn endpoint(&self, path: &str) -> Url {
        self.base_url
            .join(path)
            .expect("fixed Artalk API path must be valid")
    }

    async fn login(&self) -> Result<String, ArtalkError> {
        let response = self
            .http
            .post(self.endpoint("api/v2/user/access_token"))
            .json(&LoginRequest {
                name: &self.admin_name,
                email: &self.admin_email,
                password: &self.admin_password,
            })
            .send()
            .await?;
        let response = expect_success(response, "login").await?;
        let payload = response
            .json::<LoginResponse>()
            .await
            .map_err(|_| ArtalkError::InvalidResponse("login"))?;
        if payload.token.trim().is_empty() {
            return Err(ArtalkError::InvalidResponse("login"));
        }
        Ok(payload.token)
    }

    async fn find_page(
        &self,
        token: &str,
        page_key: &str,
    ) -> Result<Option<ArtalkPage>, ArtalkError> {
        let response = self
            .http
            .get(self.endpoint("api/v2/pages"))
            .bearer_auth(token)
            .query(&[
                ("site_name", self.site_name.as_str()),
                ("search", page_key),
                ("limit", "100"),
                ("offset", "0"),
            ])
            .send()
            .await?;
        let response = expect_success(response, "page lookup").await?;
        let payload = response
            .json::<PageListResponse>()
            .await
            .map_err(|_| ArtalkError::InvalidResponse("page lookup"))?;
        Ok(select_exact_page(
            payload.pages.unwrap_or_default(),
            page_key,
            &self.site_name,
        ))
    }

    async fn ensure_page(&self, page_key: &str, page_title: &str) -> Result<(), ArtalkError> {
        let response = self
            .http
            .post(self.endpoint("api/v2/pages/pv"))
            .json(&PagePvRequest {
                page_key,
                page_title,
                site_name: &self.site_name,
            })
            .send()
            .await?;
        expect_success(response, "page creation").await?;
        Ok(())
    }
}

async fn expect_success(
    response: Response,
    operation: &'static str,
) -> Result<Response, ArtalkError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let message = response
        .text()
        .await
        .unwrap_or_else(|_| "unreadable response body".to_owned())
        .chars()
        .take(512)
        .collect();
    Err(ArtalkError::Http {
        operation,
        status,
        message,
    })
}

fn select_exact_page(
    pages: Vec<ArtalkPage>,
    page_key: &str,
    site_name: &str,
) -> Option<ArtalkPage> {
    pages
        .into_iter()
        .find(|page| page.key == page_key && page.site_name == site_name)
}

pub fn article_page_key(slug: &str) -> String {
    format!("/posts/{slug}")
}

#[cfg(test)]
mod tests {
    use super::{ArtalkPage, article_page_key, select_exact_page};

    #[test]
    fn article_keys_match_the_frontend_permalink() {
        assert_eq!(article_page_key("p42"), "/posts/p42");
    }

    #[test]
    fn page_lookup_rejects_fuzzy_and_cross_site_matches() {
        let pages = vec![
            ArtalkPage {
                id: 1,
                key: "/posts/p42-extra".to_owned(),
                site_name: "helt.".to_owned(),
            },
            ArtalkPage {
                id: 2,
                key: "/posts/p42".to_owned(),
                site_name: "other".to_owned(),
            },
            ArtalkPage {
                id: 3,
                key: "/posts/p42".to_owned(),
                site_name: "helt.".to_owned(),
            },
        ];
        let page = select_exact_page(pages, "/posts/p42", "helt.").expect("exact page");
        assert_eq!(page.id, 3);
    }
}
