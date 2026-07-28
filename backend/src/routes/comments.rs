use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, patch},
};
use serde::Deserialize;
use tracing::error;

use crate::{
    artalk::{ArtalkComment, ArtalkCommentPage, ArtalkCommentStatus, ArtalkError},
    auth,
    error::{ErrorBody, ErrorEnvelope},
    routes::contract::HttpMethod,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/admin/comments", get(admin_list))
        .route(
            "/api/v1/admin/comments/{id}",
            patch(admin_update).delete(admin_delete),
        )
}

pub fn implements(method: HttpMethod, path: &str) -> bool {
    matches!(
        (method, path),
        (HttpMethod::Get, "/api/v1/admin/comments")
            | (HttpMethod::Patch, "/api/v1/admin/comments/{id}")
            | (HttpMethod::Delete, "/api/v1/admin/comments/{id}")
    )
}

#[derive(Debug, thiserror::Error)]
enum CommentError {
    #[error("需要有效的管理员会话")]
    Unauthorized,
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Artalk(#[from] ArtalkError),
}

impl CommentError {
    fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }
}

impl IntoResponse for CommentError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
                "需要有效的管理员会话".to_owned(),
            ),
            Self::Validation(message) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_error",
                message,
            ),
            Self::Artalk(ArtalkError::Http {
                status: StatusCode::NOT_FOUND,
                ..
            }) => (StatusCode::NOT_FOUND, "not_found", "评论不存在".to_owned()),
            Self::Artalk(artalk_error) => {
                error!(error = %artalk_error, "Artalk comment moderation request failed");
                (
                    StatusCode::BAD_GATEWAY,
                    "comment_service_unavailable",
                    "评论服务暂时不可用，请稍后重试".to_owned(),
                )
            }
        };
        (
            status,
            Json(ErrorEnvelope {
                error: ErrorBody { code, message },
            }),
        )
            .into_response()
    }
}

#[derive(Debug, Deserialize)]
struct CommentListQuery {
    #[serde(default = "default_page")]
    page: u64,
    #[serde(default = "default_per_page")]
    per_page: u64,
    #[serde(default)]
    status: String,
    #[serde(default)]
    search: String,
}

#[derive(Debug, Deserialize)]
struct CommentUpdate {
    status: String,
}

async fn admin_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CommentListQuery>,
) -> Result<Json<ArtalkCommentPage>, CommentError> {
    require_admin(&state, &headers).await?;
    if query.page == 0 {
        return Err(CommentError::validation("page 必须大于 0"));
    }
    if !(1..=100).contains(&query.per_page) {
        return Err(CommentError::validation("per_page 必须在 1 到 100 之间"));
    }
    let search = query.search.trim();
    if search.chars().count() > 100 {
        return Err(CommentError::validation("搜索词不能超过 100 个字符"));
    }
    let status = match query.status.trim() {
        "" | "all" => ArtalkCommentStatus::All,
        "pending" => ArtalkCommentStatus::Pending,
        "approved" => ArtalkCommentStatus::Approved,
        _ => {
            return Err(CommentError::validation(
                "status 必须是 all、pending 或 approved",
            ));
        }
    };
    let payload = state
        .artalk()
        .list_comments(query.page, query.per_page, status, search)
        .await?;
    Ok(Json(payload))
}

async fn admin_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<u64>,
    Json(request): Json<CommentUpdate>,
) -> Result<Json<ArtalkComment>, CommentError> {
    require_admin(&state, &headers).await?;
    if id == 0 {
        return Err(CommentError::validation("评论编号无效"));
    }
    let is_pending = match request.status.trim() {
        "pending" => true,
        "approved" => false,
        _ => {
            return Err(CommentError::validation(
                "status 必须是 pending 或 approved",
            ));
        }
    };
    let comment = state.artalk().set_comment_pending(id, is_pending).await?;
    Ok(Json(comment))
}

async fn admin_delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<u64>,
) -> Result<StatusCode, CommentError> {
    require_admin(&state, &headers).await?;
    if id == 0 {
        return Err(CommentError::validation("评论编号无效"));
    }
    state.artalk().delete_comment(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<(), CommentError> {
    if auth::has_valid_admin_session(state, headers).await {
        Ok(())
    } else {
        Err(CommentError::Unauthorized)
    }
}

const fn default_page() -> u64 {
    1
}

const fn default_per_page() -> u64 {
    20
}

#[cfg(test)]
mod tests {
    use super::{ArtalkCommentStatus, CommentError};

    #[test]
    fn moderation_statuses_keep_pending_and_approved_distinct() {
        assert_ne!(ArtalkCommentStatus::Pending, ArtalkCommentStatus::Approved);
    }

    #[test]
    fn validation_errors_are_stable() {
        assert_eq!(CommentError::validation("bad").to_string(), "bad");
    }
}
