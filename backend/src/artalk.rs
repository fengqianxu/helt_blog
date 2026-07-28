use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use reqwest::{Client, Response, StatusCode, Url};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

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
    access_token: Arc<Mutex<Option<CachedAccessToken>>>,
}

struct CachedAccessToken {
    value: String,
    refresh_at: Instant,
}

#[derive(Debug, thiserror::Error)]
pub enum ArtalkError {
    #[error("Artalk is not configured")]
    Unavailable,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtalkCommentStatus {
    All,
    Pending,
    Approved,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ArtalkComment {
    pub id: u64,
    #[serde(default)]
    pub rid: u64,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub nick: String,
    #[serde(default)]
    pub link: String,
    #[serde(default)]
    pub page_key: String,
    #[serde(default)]
    pub page_url: String,
    #[serde(default)]
    pub site_name: String,
    #[serde(default)]
    pub ua: String,
    #[serde(default)]
    pub ip_region: String,
    #[serde(default)]
    pub is_pending: bool,
    #[serde(default)]
    pub is_collapsed: bool,
    #[serde(default)]
    pub is_pinned: bool,
    #[serde(default)]
    pub visible: bool,
    #[serde(default)]
    pub vote_up: i64,
    #[serde(default)]
    pub vote_down: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtalkCommentCounts {
    pub all: u64,
    pub pending: u64,
    pub approved: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtalkCommentPage {
    pub page: u64,
    pub per_page: u64,
    pub items: Vec<ArtalkComment>,
    pub total: u64,
    pub counts: ArtalkCommentCounts,
}

#[derive(Debug, Deserialize)]
struct CommentListResponse {
    #[serde(default)]
    comments: Vec<ArtalkComment>,
    #[serde(default)]
    count: u64,
}

#[derive(Debug, Deserialize)]
struct CommentGetResponse {
    comment: ArtalkComment,
}

#[derive(Serialize)]
struct CommentUpdateRequest<'a> {
    content: &'a str,
    is_collapsed: bool,
    is_pending: bool,
    is_pinned: bool,
    link: &'a str,
    nick: &'a str,
    page_key: &'a str,
    rid: u64,
    site_name: &'a str,
    ua: &'a str,
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
                access_token: Arc::new(Mutex::new(None)),
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

    pub async fn list_comments(
        &self,
        page: u64,
        per_page: u64,
        status: ArtalkCommentStatus,
        search: &str,
    ) -> Result<ArtalkCommentPage, ArtalkError> {
        let inner = self.inner.as_ref().ok_or(ArtalkError::Unavailable)?;
        let token = inner.login().await?;
        let offset = page.saturating_sub(1).saturating_mul(per_page);

        let all_page = inner
            .fetch_comments(
                &token,
                "all",
                if status == ArtalkCommentStatus::All {
                    per_page
                } else {
                    1
                },
                if status == ArtalkCommentStatus::All {
                    offset
                } else {
                    0
                },
                search,
            )
            .await?;
        let pending_page = inner
            .fetch_comments(
                &token,
                "pending",
                if status == ArtalkCommentStatus::Pending {
                    per_page
                } else {
                    1
                },
                if status == ArtalkCommentStatus::Pending {
                    offset
                } else {
                    0
                },
                search,
            )
            .await?;
        let counts = ArtalkCommentCounts {
            all: all_page.count,
            pending: pending_page.count,
            approved: all_page.count.saturating_sub(pending_page.count),
        };

        let items = match status {
            ArtalkCommentStatus::All => all_page.comments,
            ArtalkCommentStatus::Pending => pending_page.comments,
            ArtalkCommentStatus::Approved => {
                inner
                    .fetch_approved_comments(&token, offset, per_page, counts.all, search)
                    .await?
            }
        };
        let total = match status {
            ArtalkCommentStatus::All => counts.all,
            ArtalkCommentStatus::Pending => counts.pending,
            ArtalkCommentStatus::Approved => counts.approved,
        };
        Ok(ArtalkCommentPage {
            page,
            per_page,
            items,
            total,
            counts,
        })
    }

    pub async fn set_comment_pending(
        &self,
        id: u64,
        is_pending: bool,
    ) -> Result<ArtalkComment, ArtalkError> {
        let inner = self.inner.as_ref().ok_or(ArtalkError::Unavailable)?;
        let token = inner.login().await?;
        let comment = inner.get_comment(&token, id).await?;
        inner.ensure_own_site(&comment)?;
        let response = inner
            .http
            .put(inner.endpoint(&format!("api/v2/comments/{id}")))
            .bearer_auth(&token)
            .json(&CommentUpdateRequest {
                content: &comment.content,
                is_collapsed: comment.is_collapsed,
                is_pending,
                is_pinned: comment.is_pinned,
                link: &comment.link,
                nick: &comment.nick,
                page_key: &comment.page_key,
                rid: comment.rid,
                site_name: &comment.site_name,
                ua: &comment.ua,
            })
            .send()
            .await?;
        let response = expect_success(response, "comment update").await?;
        response
            .json::<ArtalkComment>()
            .await
            .map_err(|_| ArtalkError::InvalidResponse("comment update"))
    }

    pub async fn delete_comment(&self, id: u64) -> Result<(), ArtalkError> {
        let inner = self.inner.as_ref().ok_or(ArtalkError::Unavailable)?;
        let token = inner.login().await?;
        let comment = inner.get_comment(&token, id).await?;
        inner.ensure_own_site(&comment)?;
        let response = inner
            .http
            .delete(inner.endpoint(&format!("api/v2/comments/{id}")))
            .bearer_auth(&token)
            .send()
            .await?;
        expect_success(response, "comment delete").await?;
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
        let mut access_token = self.access_token.lock().await;
        if let Some(cached) = access_token.as_ref()
            && cached.refresh_at > Instant::now()
        {
            return Ok(cached.value.clone());
        }
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
        let token = payload.token;
        *access_token = Some(CachedAccessToken {
            value: token.clone(),
            // Artalk 2.10 access tokens currently last three days. Refreshing
            // well before expiry avoids repeated logins and captcha throttling.
            refresh_at: Instant::now() + Duration::from_secs(12 * 60 * 60),
        });
        Ok(token)
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

    async fn fetch_comments(
        &self,
        token: &str,
        list_type: &str,
        limit: u64,
        offset: u64,
        search: &str,
    ) -> Result<CommentListResponse, ArtalkError> {
        let mut query = vec![
            ("page_key", "/"),
            ("site_name", self.site_name.as_str()),
            ("scope", "site"),
            ("type", list_type),
            ("flat_mode", "true"),
            ("sort_by", "date_desc"),
        ];
        let limit = limit.to_string();
        let offset = offset.to_string();
        query.push(("limit", &limit));
        query.push(("offset", &offset));
        if !search.is_empty() {
            query.push(("search", search));
        }
        let response = self
            .http
            .get(self.endpoint("api/v2/comments"))
            .bearer_auth(token)
            .query(&query)
            .send()
            .await?;
        let response = expect_success(response, "comment list").await?;
        response
            .json::<CommentListResponse>()
            .await
            .map_err(|_| ArtalkError::InvalidResponse("comment list"))
    }

    async fn fetch_approved_comments(
        &self,
        token: &str,
        offset: u64,
        limit: u64,
        all_count: u64,
        search: &str,
    ) -> Result<Vec<ArtalkComment>, ArtalkError> {
        const CHUNK_SIZE: u64 = 100;
        let mut source_offset = 0;
        let mut approved_seen = 0;
        let mut items = Vec::new();
        while source_offset < all_count && items.len() < limit as usize {
            let page = self
                .fetch_comments(token, "all", CHUNK_SIZE, source_offset, search)
                .await?;
            let fetched = page.comments.len() as u64;
            if fetched == 0 {
                break;
            }
            for comment in page.comments {
                if comment.is_pending {
                    continue;
                }
                if approved_seen >= offset && items.len() < limit as usize {
                    items.push(comment);
                }
                approved_seen += 1;
            }
            source_offset = source_offset.saturating_add(fetched);
        }
        Ok(items)
    }

    async fn get_comment(&self, token: &str, id: u64) -> Result<ArtalkComment, ArtalkError> {
        let response = self
            .http
            .get(self.endpoint(&format!("api/v2/comments/{id}")))
            .bearer_auth(token)
            .send()
            .await?;
        let response = expect_success(response, "comment lookup").await?;
        let payload = response
            .json::<CommentGetResponse>()
            .await
            .map_err(|_| ArtalkError::InvalidResponse("comment lookup"))?;
        Ok(payload.comment)
    }

    fn ensure_own_site(&self, comment: &ArtalkComment) -> Result<(), ArtalkError> {
        if comment.site_name == self.site_name {
            Ok(())
        } else {
            Err(ArtalkError::Http {
                operation: "comment lookup",
                status: StatusCode::NOT_FOUND,
                message: "comment not found".to_owned(),
            })
        }
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
