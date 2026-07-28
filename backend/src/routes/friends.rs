use std::collections::HashSet;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, patch, put},
};
use chrono::{DateTime, Utc};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, Postgres, Transaction};
use tracing::error;

use crate::{
    auth, client,
    error::{ErrorBody, ErrorEnvelope},
    routes::contract::HttpMethod,
    state::AppState,
};

const MAX_NAME_CHARS: usize = 100;
const MAX_URL_CHARS: usize = 2_048;
const MAX_DESCRIPTION_CHARS: usize = 500;
const MAX_EMAIL_CHARS: usize = 254;
const PUBLIC_SUBMISSIONS_PER_HOUR: i64 = 2;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/friends", get(public_list).post(public_apply))
        .route("/api/v1/admin/friends", get(admin_list))
        .route("/api/v1/admin/friends/order", put(admin_reorder))
        .route(
            "/api/v1/admin/friends/{id}",
            patch(admin_update).delete(admin_delete),
        )
}

pub fn implements(method: HttpMethod, path: &str) -> bool {
    matches!(
        (method, path),
        (HttpMethod::Get, "/api/v1/friends")
            | (HttpMethod::Post, "/api/v1/friends")
            | (HttpMethod::Get, "/api/v1/admin/friends")
            | (HttpMethod::Patch, "/api/v1/admin/friends/{id}")
            | (HttpMethod::Delete, "/api/v1/admin/friends/{id}")
            | (HttpMethod::Put, "/api/v1/admin/friends/order")
    )
}

#[derive(Debug, thiserror::Error)]
enum FriendError {
    #[error("需要有效的管理员会话")]
    Unauthorized,
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Conflict(String),
    #[error("提交过于频繁")]
    RateLimited,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl FriendError {
    fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }
}

impl IntoResponse for FriendError {
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
            Self::Conflict(message) => (StatusCode::CONFLICT, "conflict", message),
            Self::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "每个访客每小时最多提交 2 次友链申请，请稍后再试".to_owned(),
            ),
            Self::Database(database_error) => {
                error!(error = %database_error, "friend-link database operation failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "友链操作失败".to_owned(),
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
struct ListQuery {
    #[serde(default = "default_page")]
    page: i64,
    #[serde(default = "default_per_page")]
    per_page: i64,
}

#[derive(Debug, Deserialize)]
struct AdminListQuery {
    #[serde(default = "default_page")]
    page: i64,
    #[serde(default = "default_admin_per_page")]
    per_page: i64,
    status: Option<String>,
    search: Option<String>,
}

fn default_page() -> i64 {
    1
}

fn default_per_page() -> i64 {
    12
}

fn default_admin_per_page() -> i64 {
    20
}

fn page_values(page: i64, per_page: i64) -> Result<(i64, i64), FriendError> {
    if page < 1 || !(1..=50).contains(&per_page) {
        return Err(FriendError::validation(
            "page 必须 >= 1，per_page 必须为 1..50",
        ));
    }
    let offset = (page - 1)
        .checked_mul(per_page)
        .ok_or_else(|| FriendError::validation("page 数值过大"))?;
    Ok((page, offset))
}

#[derive(Debug, FromRow)]
struct FriendRow {
    id: i64,
    name: String,
    url: String,
    avatar_url: String,
    avatar_asset_id: Option<i64>,
    avatar_object_key: Option<String>,
    contact_email: String,
    description: String,
    status: String,
    sort_order: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    reviewed_at: Option<DateTime<Utc>>,
}

const FRIEND_SELECT: &str = "
    SELECT
        friend.id,
        friend.name,
        friend.url,
        friend.avatar_url,
        friend.avatar_asset_id,
        upload.object_key AS avatar_object_key,
        friend.contact_email,
        friend.description,
        friend.status,
        friend.sort_order,
        friend.created_at,
        friend.updated_at,
        friend.reviewed_at
    FROM friends friend
    LEFT JOIN assets asset
      ON asset.id = friend.avatar_asset_id
     AND asset.status = 'active'
     AND asset.media_type = 'image'
    LEFT JOIN uploads upload
      ON upload.id = asset.upload_id";

#[derive(Debug, Deserialize)]
struct FriendApplication {
    name: String,
    url: String,
    #[serde(default)]
    avatar_url: String,
    contact_email: String,
    #[serde(default)]
    description: String,
}

#[derive(Debug, Deserialize)]
struct FriendUpdate {
    name: Option<String>,
    url: Option<String>,
    avatar_url: Option<String>,
    avatar_asset_id: Option<i64>,
    contact_email: Option<String>,
    description: Option<String>,
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FriendOrder {
    order: Vec<i64>,
}

#[derive(Debug, Serialize)]
struct FriendCounts {
    pending: i64,
    approved: i64,
    rejected: i64,
}

#[derive(Debug, FromRow)]
struct FriendCountRow {
    pending: i64,
    approved: i64,
    rejected: i64,
}

async fn public_list(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, FriendError> {
    let (page, offset) = page_values(query.page, query.per_page)?;
    let sql = format!(
        "{FRIEND_SELECT}
         WHERE friend.status = 'approved'
         ORDER BY friend.sort_order, friend.created_at, friend.id
         LIMIT $1 OFFSET $2"
    );
    let rows = sqlx::query_as::<_, FriendRow>(&sql)
        .bind(query.per_page)
        .bind(offset)
        .fetch_all(state.pool())
        .await?;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM friends WHERE status = 'approved'")
        .fetch_one(state.pool())
        .await?;
    let items = rows
        .into_iter()
        .map(|row| {
            json!({
                "name": row.name,
                "url": row.url,
                "avatar_url": row.avatar_object_key
                    .map(|key| state.object_storage().public_url(&key))
                    .unwrap_or_default(),
                "description": row.description
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "page": page,
        "per_page": query.per_page,
        "total": total,
        "items": items
    })))
}

async fn public_apply(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<FriendApplication>,
) -> Result<(StatusCode, Json<Value>), FriendError> {
    let name = required_text(&request.name, "站点名称", MAX_NAME_CHARS)?;
    let url = normalized_url(&request.url, "站点地址", true)?;
    let avatar_url = optional_url(&request.avatar_url, "头像地址")?;
    let contact_email = normalized_email(&request.contact_email)?;
    let description = bounded_text(&request.description, "站点介绍", MAX_DESCRIPTION_CHARS)?;
    let submission_ip_hash = request_fingerprint(&state, &headers);

    let mut transaction = state.pool().begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('friend-rate:' || $1))")
        .bind(&submission_ip_hash)
        .execute(&mut *transaction)
        .await?;
    let recent_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM friends
         WHERE submission_ip_hash = $1
           AND created_at >= now() - interval '1 hour'",
    )
    .bind(&submission_ip_hash)
    .fetch_one(&mut *transaction)
    .await?;
    if recent_count >= PUBLIC_SUBMISSIONS_PER_HOUR {
        return Err(FriendError::RateLimited);
    }

    sqlx::query("SELECT pg_advisory_xact_lock(hashtext(lower($1)))")
        .bind(&url)
        .execute(&mut *transaction)
        .await?;
    let duplicate: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM friends WHERE lower(url) = lower($1))")
            .bind(&url)
            .fetch_one(&mut *transaction)
            .await?;
    if duplicate {
        return Err(FriendError::Conflict(
            "这个站点地址已经提交过申请".to_owned(),
        ));
    }

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO friends (
            name, url, avatar_url, contact_email, description, status, submission_ip_hash
         )
         VALUES ($1, $2, $3, $4, $5, 'pending', $6)
         RETURNING id",
    )
    .bind(name)
    .bind(url)
    .bind(avatar_url)
    .bind(contact_email)
    .bind(description)
    .bind(submission_ip_hash)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({ "id": id, "status": "pending" })),
    ))
}

async fn admin_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AdminListQuery>,
) -> Result<Json<Value>, FriendError> {
    require_admin(&state, &headers).await?;
    let (page, offset) = page_values(query.page, query.per_page)?;
    let status = normalized_status_filter(query.status.as_deref())?;
    let search = query.search.unwrap_or_default().trim().to_owned();
    if search.chars().count() > 100 {
        return Err(FriendError::validation("搜索词不能超过 100 个字符"));
    }

    let sql = format!(
        "{FRIEND_SELECT}
         WHERE ($1::text IS NULL OR friend.status = $1)
           AND (
             $2 = ''
             OR friend.name ILIKE '%' || $2 || '%'
             OR friend.url ILIKE '%' || $2 || '%'
             OR friend.contact_email ILIKE '%' || $2 || '%'
           )
         ORDER BY
           CASE friend.status WHEN 'pending' THEN 0 WHEN 'approved' THEN 1 ELSE 2 END,
           friend.sort_order,
           friend.created_at DESC,
           friend.id DESC
         LIMIT $3 OFFSET $4"
    );
    let rows = sqlx::query_as::<_, FriendRow>(&sql)
        .bind(&status)
        .bind(&search)
        .bind(query.per_page)
        .bind(offset)
        .fetch_all(state.pool())
        .await?;
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM friends
         WHERE ($1::text IS NULL OR status = $1)
           AND (
             $2 = ''
             OR name ILIKE '%' || $2 || '%'
             OR url ILIKE '%' || $2 || '%'
             OR contact_email ILIKE '%' || $2 || '%'
           )",
    )
    .bind(&status)
    .bind(&search)
    .fetch_one(state.pool())
    .await?;
    let counts = sqlx::query_as::<_, FriendCountRow>(
        "SELECT
           COUNT(*) FILTER (WHERE status = 'pending') AS pending,
           COUNT(*) FILTER (WHERE status = 'approved') AS approved,
           COUNT(*) FILTER (WHERE status = 'rejected') AS rejected
         FROM friends",
    )
    .fetch_one(state.pool())
    .await?;
    let counts = FriendCounts {
        pending: counts.pending,
        approved: counts.approved,
        rejected: counts.rejected,
    };
    let items = rows
        .into_iter()
        .map(|row| admin_friend_json(&state, row))
        .collect::<Vec<_>>();

    Ok(Json(json!({
        "page": page,
        "per_page": query.per_page,
        "total": total,
        "counts": counts,
        "items": items
    })))
}

async fn admin_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(request): Json<FriendUpdate>,
) -> Result<Json<Value>, FriendError> {
    require_admin(&state, &headers).await?;
    if id <= 0 {
        return Err(FriendError::validation("友链编号无效"));
    }
    if request.name.is_none()
        && request.url.is_none()
        && request.avatar_url.is_none()
        && request.avatar_asset_id.is_none()
        && request.contact_email.is_none()
        && request.description.is_none()
        && request.status.is_none()
    {
        return Err(FriendError::validation("至少提交一个需要修改的字段"));
    }

    let mut transaction = state.pool().begin().await?;
    let existing = fetch_friend_for_update(&mut transaction, id).await?;
    let name = match request.name {
        Some(value) => required_text(&value, "站点名称", MAX_NAME_CHARS)?,
        None => existing.name,
    };
    let url = match request.url {
        Some(value) => normalized_url(&value, "站点地址", true)?,
        None => existing.url.clone(),
    };
    let avatar_url = match request.avatar_url {
        Some(value) => optional_url(&value, "头像地址")?,
        None => existing.avatar_url,
    };
    let contact_email = match request.contact_email {
        Some(value) => normalized_email(&value)?,
        None => existing.contact_email,
    };
    let description = match request.description {
        Some(value) => bounded_text(&value, "站点介绍", MAX_DESCRIPTION_CHARS)?,
        None => existing.description,
    };
    let status = match request.status {
        Some(value) => normalized_status(&value)?,
        None => existing.status.clone(),
    };
    let avatar_asset_id = request.avatar_asset_id.or(existing.avatar_asset_id);
    if status == "approved" {
        let asset_id = avatar_asset_id
            .ok_or_else(|| FriendError::validation("通过申请前，请从素材库选择一张友链头像"))?;
        validate_image_asset(&mut transaction, asset_id).await?;
    }

    if !url.eq_ignore_ascii_case(&existing.url) {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext(lower($1)))")
            .bind(&url)
            .execute(&mut *transaction)
            .await?;
        let duplicate: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM friends WHERE id <> $1 AND lower(url) = lower($2)
             )",
        )
        .bind(id)
        .bind(&url)
        .fetch_one(&mut *transaction)
        .await?;
        if duplicate {
            return Err(FriendError::Conflict(
                "另一个友链记录已经使用这个站点地址".to_owned(),
            ));
        }
    }

    let sort_order = if status == "approved" && existing.status != "approved" {
        sqlx::query_scalar::<_, i32>(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM friends WHERE status = 'approved'",
        )
        .fetch_one(&mut *transaction)
        .await?
    } else {
        existing.sort_order
    };
    sqlx::query(
        "UPDATE friends
         SET name = $1,
             url = $2,
             avatar_url = $3,
             avatar_asset_id = $4,
             contact_email = $5,
             description = $6,
             status = $7,
             sort_order = $8,
             reviewed_at = CASE
               WHEN $7 = 'pending' THEN NULL
               WHEN status IS DISTINCT FROM $7 THEN now()
               ELSE reviewed_at
             END
         WHERE id = $9",
    )
    .bind(name)
    .bind(url)
    .bind(avatar_url)
    .bind(avatar_asset_id)
    .bind(contact_email)
    .bind(description)
    .bind(&status)
    .bind(sort_order)
    .bind(id)
    .execute(&mut *transaction)
    .await?;
    if existing.status == "approved" && status != "approved" {
        compact_approved_order(&mut transaction).await?;
    }
    transaction.commit().await?;
    let row = fetch_friend(state.pool(), id).await?;
    Ok(Json(admin_friend_json(&state, row)))
}

async fn admin_delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<StatusCode, FriendError> {
    require_admin(&state, &headers).await?;
    if id <= 0 {
        return Err(FriendError::validation("友链编号无效"));
    }
    let mut transaction = state.pool().begin().await?;
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM friends WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *transaction)
            .await?;
    let status = status.ok_or_else(|| FriendError::NotFound("友链记录不存在".to_owned()))?;
    sqlx::query("DELETE FROM friends WHERE id = $1")
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    if status == "approved" {
        compact_approved_order(&mut transaction).await?;
    }
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn admin_reorder(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<FriendOrder>,
) -> Result<Json<Value>, FriendError> {
    require_admin(&state, &headers).await?;
    if request.order.iter().any(|id| *id <= 0)
        || request.order.iter().copied().collect::<HashSet<_>>().len() != request.order.len()
    {
        return Err(FriendError::validation(
            "order 只能包含无重复的有效友链编号",
        ));
    }
    let mut transaction = state.pool().begin().await?;
    let current = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM friends WHERE status = 'approved' ORDER BY id FOR UPDATE",
    )
    .fetch_all(&mut *transaction)
    .await?;
    let current_set = current.iter().copied().collect::<HashSet<_>>();
    let requested_set = request.order.iter().copied().collect::<HashSet<_>>();
    if current_set != requested_set || current.len() != request.order.len() {
        return Err(FriendError::validation(
            "order 必须完整包含当前全部已通过友链",
        ));
    }
    for (sort_order, id) in request.order.iter().enumerate() {
        sqlx::query("UPDATE friends SET sort_order = $1 WHERE id = $2")
            .bind(
                i32::try_from(sort_order)
                    .map_err(|_| FriendError::validation("友链数量超出可排序范围"))?,
            )
            .bind(id)
            .execute(&mut *transaction)
            .await?;
    }
    transaction.commit().await?;

    let mut items = Vec::with_capacity(request.order.len());
    for id in request.order {
        items.push(admin_friend_json(
            &state,
            fetch_friend(state.pool(), id).await?,
        ));
    }
    Ok(Json(json!({ "items": items })))
}

async fn fetch_friend(pool: &sqlx::PgPool, id: i64) -> Result<FriendRow, FriendError> {
    let sql = format!("{FRIEND_SELECT} WHERE friend.id = $1");
    sqlx::query_as::<_, FriendRow>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| FriendError::NotFound("友链记录不存在".to_owned()))
}

async fn fetch_friend_for_update(
    transaction: &mut Transaction<'_, Postgres>,
    id: i64,
) -> Result<FriendRow, FriendError> {
    let sql = format!("{FRIEND_SELECT} WHERE friend.id = $1 FOR UPDATE OF friend");
    sqlx::query_as::<_, FriendRow>(&sql)
        .bind(id)
        .fetch_optional(&mut **transaction)
        .await?
        .ok_or_else(|| FriendError::NotFound("友链记录不存在".to_owned()))
}

async fn validate_image_asset(
    transaction: &mut Transaction<'_, Postgres>,
    id: i64,
) -> Result<(), FriendError> {
    if id <= 0 {
        return Err(FriendError::validation("友链头像素材编号无效"));
    }
    let valid: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1
            FROM assets
            WHERE id = $1
              AND status = 'active'
              AND media_type = 'image'
              AND upload_id IS NOT NULL
         )",
    )
    .bind(id)
    .fetch_one(&mut **transaction)
    .await?;
    if valid {
        Ok(())
    } else {
        Err(FriendError::validation("请选择可用的图片素材作为友链头像"))
    }
}

async fn compact_approved_order(
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<(), FriendError> {
    sqlx::query(
        "WITH ranked AS (
           SELECT id, (row_number() OVER (ORDER BY sort_order, created_at, id) - 1)::INTEGER AS next_order
           FROM friends
           WHERE status = 'approved'
         )
         UPDATE friends
         SET sort_order = ranked.next_order
         FROM ranked
         WHERE friends.id = ranked.id
           AND friends.sort_order IS DISTINCT FROM ranked.next_order",
    )
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn admin_friend_json(state: &AppState, row: FriendRow) -> Value {
    json!({
        "id": row.id,
        "name": row.name,
        "url": row.url,
        "avatar_url": row.avatar_url,
        "avatar_asset_id": row.avatar_asset_id,
        "avatar_asset_url": row.avatar_object_key
            .map(|key| state.object_storage().public_url(&key)),
        "contact_email": row.contact_email,
        "description": row.description,
        "status": row.status,
        "sort_order": row.sort_order,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "reviewed_at": row.reviewed_at
    })
}

async fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<(), FriendError> {
    if auth::has_valid_admin_session(state, headers).await {
        Ok(())
    } else {
        Err(FriendError::Unauthorized)
    }
}

fn normalized_status_filter(value: Option<&str>) -> Result<Option<String>, FriendError> {
    value.map(normalized_status).transpose()
}

fn normalized_status(value: &str) -> Result<String, FriendError> {
    match value.trim() {
        "pending" | "approved" | "rejected" => Ok(value.trim().to_owned()),
        _ => Err(FriendError::validation(
            "status 必须是 pending、approved 或 rejected",
        )),
    }
}

fn required_text(value: &str, label: &str, max: usize) -> Result<String, FriendError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(FriendError::validation(format!("{label}不能为空")));
    }
    if value.chars().count() > max {
        return Err(FriendError::validation(format!(
            "{label}不能超过 {max} 个字符"
        )));
    }
    Ok(value.to_owned())
}

fn bounded_text(value: &str, label: &str, max: usize) -> Result<String, FriendError> {
    let value = value.trim();
    if value.chars().count() > max {
        return Err(FriendError::validation(format!(
            "{label}不能超过 {max} 个字符"
        )));
    }
    Ok(value.to_owned())
}

fn normalized_url(value: &str, label: &str, required: bool) -> Result<String, FriendError> {
    let value = value.trim();
    if value.is_empty() && !required {
        return Ok(String::new());
    }
    if value.is_empty() || value.chars().count() > MAX_URL_CHARS {
        return Err(FriendError::validation(format!(
            "{label}不能为空且不能超过 {MAX_URL_CHARS} 个字符"
        )));
    }
    let mut url = Url::parse(value)
        .map_err(|_| FriendError::validation(format!("{label}必须是有效的 HTTP(S) 地址")))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(FriendError::validation(format!(
            "{label}必须是有效的 HTTP(S) 地址"
        )));
    }
    url.set_fragment(None);
    Ok(url.to_string())
}

fn optional_url(value: &str, label: &str) -> Result<String, FriendError> {
    normalized_url(value, label, false)
}

fn normalized_email(value: &str) -> Result<String, FriendError> {
    let value = value.trim().to_ascii_lowercase();
    let valid = !value.is_empty()
        && value.chars().count() <= MAX_EMAIL_CHARS
        && !value.chars().any(char::is_whitespace)
        && value.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.ends_with('.')
        });
    if valid {
        Ok(value)
    } else {
        Err(FriendError::validation("请输入有效的联系邮箱"))
    }
}

fn request_fingerprint(state: &AppState, headers: &HeaderMap) -> String {
    client::fingerprint(headers, state.auth_jwt_secret(), "friend-submission")
}

#[cfg(test)]
mod tests {
    use super::{normalized_email, normalized_status, normalized_url, page_values};

    #[test]
    fn pagination_rejects_offsets_that_cannot_fit_in_i64() {
        assert_eq!(page_values(2, 10).expect("valid page"), (2, 10));
        assert!(page_values(i64::MAX, 50).is_err());
    }

    #[test]
    fn friend_inputs_reject_unsafe_or_invalid_values() {
        assert!(normalized_url("javascript:alert(1)", "站点地址", true).is_err());
        assert!(normalized_url("https://user:pass@example.com", "站点地址", true).is_err());
        assert_eq!(
            normalized_url("https://example.com/path#fragment", "站点地址", true)
                .expect("valid URL"),
            "https://example.com/path"
        );
        assert!(normalized_email("not-an-email").is_err());
        assert!(normalized_status("deleted").is_err());
    }
}
