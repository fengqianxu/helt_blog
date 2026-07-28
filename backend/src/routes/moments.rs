use std::collections::{HashMap, HashSet};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, Postgres, Transaction};
use tracing::error;

use crate::{
    auth,
    error::{ErrorBody, ErrorEnvelope},
    routes::contract::HttpMethod,
    state::AppState,
};

const MAX_CONTENT_CHARS: usize = 5_000;
const MAX_IMAGES: usize = 9;
const MAX_VISITOR_ID_CHARS: usize = 200;
const MAX_LIKE_TOGGLES_PER_MINUTE: i64 = 30;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/moments", get(public_list))
        .route(
            "/api/v1/moments/{id}/like",
            axum::routing::post(toggle_like),
        )
        .route("/api/v1/admin/moments", get(admin_list).post(admin_create))
        .route(
            "/api/v1/admin/moments/{id}",
            axum::routing::put(admin_update).delete(admin_delete),
        )
}

pub fn implements(method: HttpMethod, path: &str) -> bool {
    matches!(
        (method, path),
        (HttpMethod::Get, "/api/v1/moments")
            | (HttpMethod::Post, "/api/v1/moments/{id}/like")
            | (HttpMethod::Get, "/api/v1/admin/moments")
            | (HttpMethod::Post, "/api/v1/admin/moments")
            | (HttpMethod::Put, "/api/v1/admin/moments/{id}")
            | (HttpMethod::Delete, "/api/v1/admin/moments/{id}")
    )
}

#[derive(Debug, thiserror::Error)]
enum MomentError {
    #[error("需要有效的管理员会话")]
    Unauthorized,
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Validation(String),
    #[error("操作过于频繁")]
    RateLimited,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl MomentError {
    fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }
}

impl IntoResponse for MomentError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
                "需要有效的管理员会话".to_owned(),
            ),
            Self::NotFound(message) => (StatusCode::NOT_FOUND, "not_found", message),
            Self::Validation(message) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_error",
                message,
            ),
            Self::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "点赞操作太频繁，请稍后再试".to_owned(),
            ),
            Self::Database(database_error) => {
                error!(error = %database_error, "moment database operation failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "说说操作失败".to_owned(),
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
struct PublicListQuery {
    #[serde(default = "default_page")]
    page: i64,
    #[serde(default = "default_per_page")]
    per_page: i64,
    visitor_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AdminListQuery {
    #[serde(default = "default_page")]
    page: i64,
    #[serde(default = "default_admin_per_page")]
    per_page: i64,
    search: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LikeRequest {
    visitor_id: String,
}

#[derive(Debug, Deserialize)]
struct SaveMomentRequest {
    content: String,
    #[serde(default)]
    asset_ids: Vec<i64>,
    created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow)]
struct MomentRow {
    id: i64,
    content: String,
    like_count: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    liked_by_me: bool,
}

#[derive(Debug, FromRow)]
struct MomentImageRow {
    moment_id: i64,
    asset_id: i64,
    url: String,
    alt_text: String,
}

#[derive(Debug, Serialize)]
struct MomentImage {
    asset_id: i64,
    url: String,
    alt_text: String,
}

#[derive(Debug, Serialize)]
struct MomentItem {
    id: i64,
    content: String,
    images: Vec<MomentImage>,
    like_count: i32,
    created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<DateTime<Utc>>,
    liked_by_me: bool,
}

fn default_page() -> i64 {
    1
}

fn default_per_page() -> i64 {
    10
}

fn default_admin_per_page() -> i64 {
    20
}

fn page_values(page: i64, per_page: i64) -> Result<(i64, i64), MomentError> {
    if page < 1 || !(1..=50).contains(&per_page) {
        return Err(MomentError::validation(
            "page 必须 >= 1，per_page 必须为 1..50",
        ));
    }
    let offset = (page - 1)
        .checked_mul(per_page)
        .ok_or_else(|| MomentError::validation("page 数值过大"))?;
    Ok((page, offset))
}

fn normalized_visitor_id(value: &str) -> Result<String, MomentError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > MAX_VISITOR_ID_CHARS {
        return Err(MomentError::validation("visitor_id 格式无效"));
    }
    Ok(value.to_owned())
}

fn normalized_content(content: &str, asset_ids: &[i64]) -> Result<String, MomentError> {
    let content = content.trim();
    if content.is_empty() && asset_ids.is_empty() {
        return Err(MomentError::validation("说说内容和图片不能同时为空"));
    }
    if content.chars().count() > MAX_CONTENT_CHARS {
        return Err(MomentError::validation(format!(
            "说说内容不能超过 {MAX_CONTENT_CHARS} 个字符"
        )));
    }
    Ok(content.to_owned())
}

fn normalized_search(search: Option<&str>) -> Option<String> {
    search
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<(), MomentError> {
    if auth::has_valid_admin_session(state, headers) {
        Ok(())
    } else {
        Err(MomentError::Unauthorized)
    }
}

async fn validate_assets(pool: &sqlx::PgPool, asset_ids: &[i64]) -> Result<(), MomentError> {
    if asset_ids.len() > MAX_IMAGES || asset_ids.iter().any(|id| *id <= 0) {
        return Err(MomentError::validation(format!(
            "每条说说最多选择 {MAX_IMAGES} 张图片"
        )));
    }
    let unique = asset_ids.iter().copied().collect::<HashSet<_>>();
    if unique.len() != asset_ids.len() {
        return Err(MomentError::validation("图片素材不能重复"));
    }
    if asset_ids.is_empty() {
        return Ok(());
    }
    let found: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM assets
         WHERE id = ANY($1) AND status = 'active' AND media_type = 'image'",
    )
    .bind(asset_ids)
    .fetch_one(pool)
    .await?;
    if found != asset_ids.len() as i64 {
        return Err(MomentError::validation("存在不可用或非图片素材"));
    }
    Ok(())
}

async fn sync_assets(
    transaction: &mut Transaction<'_, Postgres>,
    moment_id: i64,
    asset_ids: &[i64],
) -> Result<(), MomentError> {
    sqlx::query("DELETE FROM moment_assets WHERE moment_id = $1")
        .bind(moment_id)
        .execute(&mut **transaction)
        .await?;
    for (sort_order, asset_id) in asset_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO moment_assets (moment_id, asset_id, sort_order)
             VALUES ($1, $2, $3)",
        )
        .bind(moment_id)
        .bind(asset_id)
        .bind(sort_order as i32)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

async fn images_for_moments(
    pool: &sqlx::PgPool,
    moment_ids: &[i64],
) -> Result<HashMap<i64, Vec<MomentImage>>, MomentError> {
    if moment_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query_as::<_, MomentImageRow>(
        "SELECT ma.moment_id, ma.asset_id,
                '/storage/' || u.object_key AS url, ma.alt_text
         FROM moment_assets ma
         JOIN assets a ON a.id = ma.asset_id AND a.status = 'active'
         JOIN uploads u ON u.id = a.upload_id
         WHERE ma.moment_id = ANY($1)
         ORDER BY ma.moment_id, ma.sort_order, ma.asset_id",
    )
    .bind(moment_ids)
    .fetch_all(pool)
    .await?;
    let mut images = HashMap::<i64, Vec<MomentImage>>::new();
    for row in rows {
        images.entry(row.moment_id).or_default().push(MomentImage {
            asset_id: row.asset_id,
            url: row.url,
            alt_text: row.alt_text,
        });
    }
    Ok(images)
}

async fn moment_items(
    pool: &sqlx::PgPool,
    rows: Vec<MomentRow>,
    include_updated_at: bool,
) -> Result<Vec<MomentItem>, MomentError> {
    let ids = rows.iter().map(|row| row.id).collect::<Vec<_>>();
    let mut images = images_for_moments(pool, &ids).await?;
    Ok(rows
        .into_iter()
        .map(|row| MomentItem {
            id: row.id,
            content: row.content,
            images: images.remove(&row.id).unwrap_or_default(),
            like_count: row.like_count,
            created_at: row.created_at,
            updated_at: include_updated_at.then_some(row.updated_at),
            liked_by_me: row.liked_by_me,
        })
        .collect())
}

async fn fetch_moment(pool: &sqlx::PgPool, id: i64) -> Result<Option<MomentItem>, MomentError> {
    let row = sqlx::query_as::<_, MomentRow>(
        "SELECT id, content, like_count, created_at, updated_at, false AS liked_by_me
         FROM moments WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    Ok(moment_items(pool, vec![row], true).await?.pop())
}

async fn public_list(
    State(state): State<AppState>,
    Query(query): Query<PublicListQuery>,
) -> Result<Json<Value>, MomentError> {
    let (page, offset) = page_values(query.page, query.per_page)?;
    let visitor_id = query
        .visitor_id
        .as_deref()
        .map(normalized_visitor_id)
        .transpose()?;
    let rows = sqlx::query_as::<_, MomentRow>(
        "SELECT m.id, m.content, m.like_count, m.created_at, m.updated_at,
                EXISTS(
                    SELECT 1 FROM moment_likes ml
                    WHERE ml.moment_id = m.id AND ml.visitor_id = $1
                ) AS liked_by_me
         FROM moments m
         ORDER BY m.created_at DESC, m.id DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(visitor_id.as_deref())
    .bind(query.per_page)
    .bind(offset)
    .fetch_all(state.pool())
    .await?;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM moments")
        .fetch_one(state.pool())
        .await?;
    let items = moment_items(state.pool(), rows, false).await?;
    Ok(Json(json!({
        "page": page,
        "per_page": query.per_page,
        "total": total,
        "items": items
    })))
}

async fn toggle_like(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(request): Json<LikeRequest>,
) -> Result<Json<Value>, MomentError> {
    if id <= 0 {
        return Err(MomentError::validation("说说 id 必须为正整数"));
    }
    let visitor_id = normalized_visitor_id(&request.visitor_id)?;
    let mut transaction = state.pool().begin().await?;
    let exists = sqlx::query_scalar::<_, i64>("SELECT id FROM moments WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *transaction)
        .await?;
    if exists.is_none() {
        return Err(MomentError::NotFound("说说不存在".to_owned()));
    }

    sqlx::query(
        "DELETE FROM moment_like_attempts
         WHERE visitor_id = $1 AND created_at < now() - interval '1 minute'",
    )
    .bind(&visitor_id)
    .execute(&mut *transaction)
    .await?;
    let recent_attempts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM moment_like_attempts
         WHERE visitor_id = $1 AND created_at >= now() - interval '1 minute'",
    )
    .bind(&visitor_id)
    .fetch_one(&mut *transaction)
    .await?;
    if recent_attempts >= MAX_LIKE_TOGGLES_PER_MINUTE {
        return Err(MomentError::RateLimited);
    }
    sqlx::query("INSERT INTO moment_like_attempts (moment_id, visitor_id) VALUES ($1, $2)")
        .bind(id)
        .bind(&visitor_id)
        .execute(&mut *transaction)
        .await?;

    let removed = sqlx::query_scalar::<_, i64>(
        "DELETE FROM moment_likes
         WHERE moment_id = $1 AND visitor_id = $2
         RETURNING moment_id",
    )
    .bind(id)
    .bind(&visitor_id)
    .fetch_optional(&mut *transaction)
    .await?;
    let liked = removed.is_none();
    if liked {
        sqlx::query("INSERT INTO moment_likes (moment_id, visitor_id) VALUES ($1, $2)")
            .bind(id)
            .bind(&visitor_id)
            .execute(&mut *transaction)
            .await?;
    }
    let like_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM moment_likes WHERE moment_id = $1")
            .bind(id)
            .fetch_one(&mut *transaction)
            .await?;
    sqlx::query("UPDATE moments SET like_count = $1 WHERE id = $2")
        .bind(like_count as i32)
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(Json(json!({ "like_count": like_count, "liked": liked })))
}

async fn admin_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AdminListQuery>,
) -> Result<Json<Value>, MomentError> {
    require_admin(&state, &headers)?;
    let (page, offset) = page_values(query.page, query.per_page)?;
    let search = normalized_search(query.search.as_deref());
    let rows = sqlx::query_as::<_, MomentRow>(
        "SELECT id, content, like_count, created_at, updated_at, false AS liked_by_me
         FROM moments
         WHERE $1::text IS NULL OR content ILIKE '%' || $1 || '%'
         ORDER BY created_at DESC, id DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(search.as_deref())
    .bind(query.per_page)
    .bind(offset)
    .fetch_all(state.pool())
    .await?;
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM moments
         WHERE $1::text IS NULL OR content ILIKE '%' || $1 || '%'",
    )
    .bind(search.as_deref())
    .fetch_one(state.pool())
    .await?;
    let items = moment_items(state.pool(), rows, true).await?;
    Ok(Json(json!({
        "page": page,
        "per_page": query.per_page,
        "total": total,
        "items": items
    })))
}

async fn admin_create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SaveMomentRequest>,
) -> Result<(StatusCode, Json<MomentItem>), MomentError> {
    require_admin(&state, &headers)?;
    validate_assets(state.pool(), &request.asset_ids).await?;
    let content = normalized_content(&request.content, &request.asset_ids)?;
    let mut transaction = state.pool().begin().await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO moments (content, created_at)
         VALUES ($1, COALESCE($2, now()))
         RETURNING id",
    )
    .bind(content)
    .bind(request.created_at)
    .fetch_one(&mut *transaction)
    .await?;
    sync_assets(&mut transaction, id, &request.asset_ids).await?;
    transaction.commit().await?;
    let item = fetch_moment(state.pool(), id)
        .await?
        .ok_or_else(|| MomentError::NotFound("说说不存在".to_owned()))?;
    Ok((StatusCode::CREATED, Json(item)))
}

async fn admin_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(request): Json<SaveMomentRequest>,
) -> Result<Json<MomentItem>, MomentError> {
    require_admin(&state, &headers)?;
    if id <= 0 {
        return Err(MomentError::validation("说说 id 必须为正整数"));
    }
    validate_assets(state.pool(), &request.asset_ids).await?;
    let content = normalized_content(&request.content, &request.asset_ids)?;
    let mut transaction = state.pool().begin().await?;
    let result = sqlx::query(
        "UPDATE moments
         SET content = $1, created_at = COALESCE($2, created_at)
         WHERE id = $3",
    )
    .bind(content)
    .bind(request.created_at)
    .bind(id)
    .execute(&mut *transaction)
    .await?;
    if result.rows_affected() == 0 {
        return Err(MomentError::NotFound("说说不存在".to_owned()));
    }
    sync_assets(&mut transaction, id, &request.asset_ids).await?;
    transaction.commit().await?;
    let item = fetch_moment(state.pool(), id)
        .await?
        .ok_or_else(|| MomentError::NotFound("说说不存在".to_owned()))?;
    Ok(Json(item))
}

async fn admin_delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<StatusCode, MomentError> {
    require_admin(&state, &headers)?;
    if id <= 0 {
        return Err(MomentError::validation("说说 id 必须为正整数"));
    }
    let result = sqlx::query("DELETE FROM moments WHERE id = $1")
        .bind(id)
        .execute(state.pool())
        .await?;
    if result.rows_affected() == 0 {
        return Err(MomentError::NotFound("说说不存在".to_owned()));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::{normalized_content, normalized_visitor_id};

    #[test]
    fn text_or_images_can_make_a_valid_moment() {
        assert_eq!(normalized_content("  今天很好  ", &[]).unwrap(), "今天很好");
        assert_eq!(normalized_content("", &[1]).unwrap(), "");
        assert!(normalized_content("   ", &[]).is_err());
    }

    #[test]
    fn visitor_id_must_be_bounded_and_non_empty() {
        assert!(normalized_visitor_id("visitor-1").is_ok());
        assert!(normalized_visitor_id("  ").is_err());
        assert!(normalized_visitor_id(&"x".repeat(201)).is_err());
    }
}
