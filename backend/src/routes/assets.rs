use std::{
    collections::HashSet,
    io::{Cursor, Write},
};

use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::FromRow;
use tracing::{error, warn};
use uuid::Uuid;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use crate::{
    auth,
    error::{ErrorBody, ErrorEnvelope},
    routes::contract::{ASSET_MULTIPART_BODY_LIMIT_BYTES, ASSET_UPLOAD_LIMIT_BYTES, HttpMethod},
    state::AppState,
    storage_gc,
};

const MAX_BATCH: usize = 100;
const MAX_BATCH_DOWNLOAD_BYTES: i64 = 512 * 1024 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/admin/assets",
            get(list_assets)
                .post(upload_asset)
                .layer(DefaultBodyLimit::max(ASSET_MULTIPART_BODY_LIMIT_BYTES)),
        )
        .route("/api/v1/admin/assets/batch-delete", post(batch_delete))
        .route("/api/v1/admin/assets/batch-download", post(batch_download))
        .route(
            "/api/v1/admin/assets/{id}",
            get(asset_detail).patch(rename_asset).delete(delete_asset),
        )
        .route(
            "/api/v1/admin/assets/{id}/replace",
            post(replace_asset).layer(DefaultBodyLimit::max(ASSET_MULTIPART_BODY_LIMIT_BYTES)),
        )
}

pub fn implements(method: HttpMethod, path: &str) -> bool {
    matches!(
        (method, path),
        (HttpMethod::Get, "/api/v1/admin/assets")
            | (HttpMethod::Post, "/api/v1/admin/assets")
            | (HttpMethod::Get, "/api/v1/admin/assets/{id}")
            | (HttpMethod::Patch, "/api/v1/admin/assets/{id}")
            | (HttpMethod::Post, "/api/v1/admin/assets/{id}/replace")
            | (HttpMethod::Delete, "/api/v1/admin/assets/{id}")
            | (HttpMethod::Post, "/api/v1/admin/assets/batch-delete")
            | (HttpMethod::Post, "/api/v1/admin/assets/batch-download")
    )
}

#[derive(Debug, FromRow, Serialize)]
struct AssetRow {
    id: i64,
    name: String,
    media_type: String,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    object_key: String,
    mime: String,
    size_bytes: i64,
    original_filename: Option<String>,
    metadata: Value,
    reference_count: i64,
}

#[derive(Debug, FromRow, Serialize)]
struct ReferenceRow {
    source_type: String,
    source_id: String,
    source_label: String,
    admin_path: String,
}

fn reference_type_label(source_type: &str) -> &'static str {
    match source_type {
        "admin_avatar" | "system_default" => "头像",
        "login_voice" => "登录语音",
        "theme_voice" => "开屏语音",
        "raiment_voice" => "灵衣封面语音",
        "raiment_success_voice" => "灵衣登录成功语音",
        "music_track" => "背景音乐",
        "article_cover" => "文章封面",
        "article_content" => "文章内容",
        "theme_cover" | "raiment_cover" => "灵衣封面",
        "live2d_model" | "raiment_kanban" => "Live2D 模型",
        "moment_image" => "动态图片",
        "game_cover" => "游戏封面",
        "bangumi_cover" => "追番封面",
        "friend_avatar" => "友链头像",
        _ => "其他引用",
    }
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(default = "default_page")]
    page: i64,
    #[serde(default = "default_per_page")]
    per_page: i64,
    media_type: Option<String>,
    search: Option<String>,
    sort: Option<String>,
    order: Option<String>,
    usable_for: Option<String>,
}

fn default_page() -> i64 {
    1
}
fn default_per_page() -> i64 {
    20
}

#[derive(Debug, Deserialize)]
struct RenameRequest {
    name: String,
}

#[derive(Debug, Deserialize)]
struct BatchRequest {
    asset_ids: Vec<i64>,
}

#[derive(Debug, Serialize)]
struct BatchDeleteResponse {
    deleted: Vec<i64>,
    blocked: Vec<i64>,
    missing: Vec<i64>,
}

async fn list_assets(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, AssetError> {
    require_admin(&state, &headers)?;
    let page = query.page.max(1);
    let per_page = query.per_page.clamp(1, 100);
    let media_type = normalized_filter(query.media_type.as_deref(), query.usable_for.as_deref())?;
    let search = query.search.unwrap_or_default().trim().to_owned();
    let order = if query.order.as_deref() == Some("asc") {
        "ASC"
    } else {
        "DESC"
    };
    let sort = match query.sort.as_deref() {
        Some("name") => "a.name",
        Some("size") => "u.size_bytes",
        _ => "a.created_at",
    };
    let sql = format!(
        "{ASSET_SELECT}
         WHERE a.status = 'active'
           AND ($1::text IS NULL OR a.media_type = $1)
           AND ($2 = '' OR a.name ILIKE '%' || $2 || '%')
         ORDER BY {sort} {order}, a.id {order}
         LIMIT $3 OFFSET $4"
    );
    let items = sqlx::query_as::<_, AssetRow>(&sql)
        .bind(&media_type)
        .bind(&search)
        .bind(per_page)
        .bind((page - 1) * per_page)
        .fetch_all(state.pool())
        .await?;
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM assets
         WHERE status = 'active'
           AND ($1::text IS NULL OR media_type = $1)
           AND ($2 = '' OR name ILIKE '%' || $2 || '%')",
    )
    .bind(&media_type)
    .bind(&search)
    .fetch_one(state.pool())
    .await?;
    let items = items
        .into_iter()
        .map(|row| asset_json(&state, row))
        .collect::<Vec<_>>();
    Ok(Json(
        json!({ "items": items, "page": page, "per_page": per_page, "total": total }),
    ))
}

async fn asset_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<Value>, AssetError> {
    require_admin(&state, &headers)?;
    let asset = fetch_asset(&state, id).await?;
    let mut references = sqlx::query_as::<_, ReferenceRow>(
        "SELECT source_type, source_id, source_label, admin_path
         FROM asset_usage WHERE asset_id = $1 ORDER BY source_type, source_id",
    )
    .bind(id)
    .fetch_all(state.pool())
    .await?;
    for reference in &mut references {
        reference.source_label = reference_type_label(&reference.source_type).to_owned();
        reference.admin_path.clear();
    }
    let mut seen_reference_types = HashSet::new();
    references.retain(|reference| seen_reference_types.insert(reference.source_label.clone()));
    Ok(Json(json!({
        "asset": asset_json(&state, asset),
        "references": references
    })))
}

async fn upload_asset(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<(StatusCode, Json<Value>), AssetError> {
    let admin_id = require_admin(&state, &headers)?;
    let upload = read_upload(multipart).await?;
    let name = normalized_asset_name(upload.name.as_deref(), &upload.filename)?;
    let stored = store_upload(&state, admin_id, upload, None).await?;
    let asset_result = sqlx::query_scalar(
        "INSERT INTO assets (name, media_type, upload_id) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(name)
    .bind(&stored.media_type)
    .bind(stored.upload_id)
    .fetch_one(state.pool())
    .await;
    let asset_id: i64 = match asset_result {
        Ok(asset_id) => asset_id,
        Err(error) => {
            discard_stored_upload(&state, &stored, "asset_registration_failed").await?;
            return Err(AssetError::Database(error));
        }
    };
    Ok((
        StatusCode::CREATED,
        Json(json!({ "asset": asset_json(&state, fetch_asset(&state, asset_id).await?) })),
    ))
}

async fn rename_asset(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<RenameRequest>,
) -> Result<Json<Value>, AssetError> {
    require_admin(&state, &headers)?;
    let name = body.name.trim();
    if name.is_empty() || name.chars().count() > 255 {
        return Err(AssetError::validation(
            "素材名称不能为空且不能超过 255 个字符",
        ));
    }
    let changed = sqlx::query("UPDATE assets SET name = $1 WHERE id = $2 AND status = 'active'")
        .bind(name)
        .bind(id)
        .execute(state.pool())
        .await?
        .rows_affected();
    if changed == 0 {
        return Err(AssetError::NotFound);
    }
    Ok(Json(
        json!({ "asset": asset_json(&state, fetch_asset(&state, id).await?) }),
    ))
}

async fn replace_asset(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    multipart: Multipart,
) -> Result<(StatusCode, Json<Value>), AssetError> {
    let admin_id = require_admin(&state, &headers)?;
    let current = fetch_asset(&state, id).await?;
    let upload = read_upload(multipart).await?;
    let stored = store_upload(&state, admin_id, upload, Some(&current.media_type)).await?;
    let replace_result: Result<(), AssetError> = async {
        let mut tx = state.pool().begin().await?;
        let (old_upload_id, old_object_key): (i64, String) = sqlx::query_as(
            "SELECT a.upload_id, u.object_key
             FROM assets a
             JOIN uploads u ON u.id = a.upload_id
             WHERE a.id = $1 AND a.status = 'active'
             FOR UPDATE OF a",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AssetError::NotFound)?;
        sqlx::query("UPDATE assets SET upload_id = $1 WHERE id = $2")
            .bind(stored.upload_id)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        storage_gc::enqueue(&mut tx, &old_object_key, "asset_replaced").await?;
        sqlx::query("DELETE FROM uploads WHERE id = $1")
            .bind(old_upload_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    .await;
    if let Err(error) = replace_result {
        discard_stored_upload(&state, &stored, "asset_replace_failed").await?;
        return Err(error);
    }
    Ok((
        StatusCode::CREATED,
        Json(json!({ "asset": asset_json(&state, fetch_asset(&state, id).await?) })),
    ))
}

async fn delete_asset(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<StatusCode, AssetError> {
    require_admin(&state, &headers)?;
    match delete_one(&state, id).await? {
        DeleteResult::Deleted => Ok(StatusCode::NO_CONTENT),
        DeleteResult::Blocked => Err(AssetError::Conflict("素材仍被业务引用，不能删除".into())),
        DeleteResult::Missing => Err(AssetError::NotFound),
    }
}

async fn batch_delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<BatchRequest>,
) -> Result<Json<BatchDeleteResponse>, AssetError> {
    require_admin(&state, &headers)?;
    validate_batch(&body.asset_ids)?;
    let mut result = BatchDeleteResponse {
        deleted: vec![],
        blocked: vec![],
        missing: vec![],
    };
    for id in body.asset_ids {
        match delete_one(&state, id).await? {
            DeleteResult::Deleted => result.deleted.push(id),
            DeleteResult::Blocked => result.blocked.push(id),
            DeleteResult::Missing => result.missing.push(id),
        }
    }
    Ok(Json(result))
}

async fn batch_download(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<BatchRequest>,
) -> Result<Response, AssetError> {
    require_admin(&state, &headers)?;
    validate_batch(&body.asset_ids)?;
    let rows = sqlx::query_as::<_, DownloadRow>(
        "SELECT a.id, a.name, u.object_key, u.size_bytes
         FROM assets a
         JOIN uploads u ON u.id = a.upload_id
         WHERE a.status = 'active' AND a.id = ANY($1)",
    )
    .bind(&body.asset_ids)
    .fetch_all(state.pool())
    .await?;
    let total: i64 = rows.iter().map(|row| row.size_bytes).sum();
    if total > MAX_BATCH_DOWNLOAD_BYTES {
        return Err(AssetError::validation("批量下载展开后不能超过 512 MB"));
    }
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (index, row) in rows.into_iter().enumerate() {
        let data = state
            .object_storage()
            .get_public_object(state.storage_http_client(), &row.object_key)
            .await
            .map_err(AssetError::Storage)?;
        let filename = safe_archive_name(&row.name, row.id, index);
        archive
            .start_file(filename, options)
            .map_err(AssetError::Zip)?;
        archive.write_all(&data).map_err(AssetError::Io)?;
    }
    let bytes = archive.finish().map_err(AssetError::Zip)?.into_inner();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/zip")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"assets.zip\"",
        )
        .body(Body::from(bytes))
        .map_err(|error| AssetError::Internal(error.to_string()))
}

#[derive(Debug, FromRow)]
struct DownloadRow {
    id: i64,
    name: String,
    object_key: String,
    size_bytes: i64,
}

const ASSET_SELECT: &str = "
    SELECT a.id, a.name, a.media_type, a.status, a.created_at, a.updated_at,
           u.object_key, u.mime, u.size_bytes, u.original_filename,
           u.metadata, COALESCE(c.reference_count, 0) AS reference_count
    FROM assets a
    JOIN uploads u ON u.id = a.upload_id
    LEFT JOIN asset_usage_counts c ON c.asset_id = a.id";

async fn fetch_asset(state: &AppState, id: i64) -> Result<AssetRow, AssetError> {
    let sql = format!("{ASSET_SELECT} WHERE a.id = $1 AND a.status = 'active'");
    sqlx::query_as(&sql)
        .bind(id)
        .fetch_optional(state.pool())
        .await?
        .ok_or(AssetError::NotFound)
}

fn asset_json(state: &AppState, row: AssetRow) -> Value {
    json!({
        "id": row.id, "name": row.name, "media_type": row.media_type, "status": row.status,
        "created_at": row.created_at, "updated_at": row.updated_at,
        "file": {
            "url": state.object_storage().public_url(&row.object_key),
            "object_key": row.object_key, "mime": row.mime, "size_bytes": row.size_bytes,
            "original_filename": row.original_filename, "metadata": row.metadata
        },
        "reference_count": row.reference_count
    })
}

struct IncomingUpload {
    bytes: Vec<u8>,
    filename: String,
    content_type: Option<String>,
    name: Option<String>,
    requested_type: Option<String>,
}

struct StoredUpload {
    upload_id: i64,
    media_type: String,
    object_key: String,
}

async fn discard_stored_upload(
    state: &AppState,
    stored: &StoredUpload,
    reason: &str,
) -> Result<(), AssetError> {
    let mut transaction = state.pool().begin().await?;
    storage_gc::enqueue(&mut transaction, &stored.object_key, reason).await?;
    sqlx::query("DELETE FROM uploads WHERE id = $1")
        .bind(stored.upload_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}

async fn read_upload(mut multipart: Multipart) -> Result<IncomingUpload, AssetError> {
    let mut file = None;
    let mut name = None;
    let mut requested_type = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AssetError::validation(e.to_string()))?
    {
        match field.name() {
            Some("file") if file.is_none() => {
                let filename = field.file_name().unwrap_or("asset").to_owned();
                let content_type = field.content_type().map(str::to_owned);
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| AssetError::validation(e.to_string()))?
                    .to_vec();
                file = Some((bytes, filename, content_type));
            }
            Some("name") => {
                name = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| AssetError::validation(e.to_string()))?,
                )
            }
            Some("media_type") => {
                requested_type = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| AssetError::validation(e.to_string()))?,
                )
            }
            _ => {}
        }
    }
    let (bytes, filename, content_type) =
        file.ok_or_else(|| AssetError::validation("multipart 必须包含 file 字段"))?;
    if bytes.is_empty() {
        return Err(AssetError::validation("不能上传空文件"));
    }
    if bytes.len() > ASSET_UPLOAD_LIMIT_BYTES {
        return Err(AssetError::validation("单文件不能超过 200 MB"));
    }
    if filename.trim().is_empty() || filename.chars().count() > 512 {
        return Err(AssetError::validation(
            "原始文件名不能为空且不能超过 512 个字符",
        ));
    }
    Ok(IncomingUpload {
        bytes,
        filename,
        content_type,
        name,
        requested_type,
    })
}

async fn store_upload(
    state: &AppState,
    admin_id: i64,
    upload: IncomingUpload,
    expected_type: Option<&str>,
) -> Result<StoredUpload, AssetError> {
    let media_type = detect_media_type(
        &upload.filename,
        upload.content_type.as_deref(),
        upload.requested_type.as_deref(),
    )?;
    validate_file_signature(&upload.filename, &media_type, &upload.bytes)?;
    if expected_type.is_some_and(|expected| expected != media_type) {
        return Err(AssetError::validation("替换文件的媒体类型必须与原素材一致"));
    }
    let mime = upload
        .content_type
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or("application/octet-stream");
    let checksum = format!("{:x}", Sha256::digest(&upload.bytes));
    let size_bytes = upload.bytes.len() as i64;
    let extension = upload
        .filename
        .rsplit_once('.')
        .map(|(_, ext)| format!(".{}", sanitize(ext)))
        .unwrap_or_default();
    let object_key = format!(
        "assets/{admin_id}/{}/{}{}",
        Utc::now().format("%Y/%m"),
        Uuid::now_v7(),
        extension
    );
    state
        .object_storage()
        .put_public_object(state.storage_http_client(), &object_key, mime, upload.bytes)
        .await
        .map_err(AssetError::Storage)?;
    let result = sqlx::query_scalar::<_, i64>(
        "INSERT INTO uploads (object_key, bucket, mime, size_bytes, kind, original_filename, checksum_sha256, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, $7, '{}'::jsonb) RETURNING id"
    ).bind(&object_key).bind(state.object_storage().public_bucket()).bind(mime)
        .bind(size_bytes).bind(&media_type).bind(&upload.filename).bind(checksum)
        .fetch_one(state.pool()).await;
    match result {
        Ok(upload_id) => Ok(StoredUpload {
            upload_id,
            media_type,
            object_key,
        }),
        Err(error) => {
            if let Err(cleanup_error) = state
                .object_storage()
                .delete_public_object(state.storage_http_client(), &object_key)
                .await
            {
                warn!(
                    %cleanup_error,
                    object_key,
                    "unregistered upload object could not be cleaned up immediately"
                );
            }
            Err(AssetError::Database(error))
        }
    }
}

enum DeleteResult {
    Deleted,
    Blocked,
    Missing,
}

async fn delete_one(state: &AppState, id: i64) -> Result<DeleteResult, AssetError> {
    let mut tx = state.pool().begin().await?;
    let upload: Option<(i64, String)> = sqlx::query_as(
        "SELECT u.id, u.object_key
         FROM assets a
         JOIN uploads u ON u.id = a.upload_id
         WHERE a.id = $1 AND a.status = 'active'
         FOR UPDATE OF a",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((upload_id, object_key)) = upload else {
        tx.rollback().await?;
        return Ok(DeleteResult::Missing);
    };
    let count: i64 = sqlx::query_scalar(
        "SELECT COALESCE(
             (SELECT reference_count FROM asset_usage_counts WHERE asset_id = $1),
             0
         )",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if count > 0 {
        tx.rollback().await?;
        return Ok(DeleteResult::Blocked);
    }
    sqlx::query("UPDATE assets SET status = 'deleting', deleted_at = now() WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM assets WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    storage_gc::enqueue(&mut tx, &object_key, "asset_deleted").await?;
    sqlx::query("DELETE FROM uploads WHERE id = $1")
        .bind(upload_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(DeleteResult::Deleted)
}

fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<i64, AssetError> {
    auth::authenticated_admin_id(state, headers).ok_or(AssetError::Unauthorized)
}

fn normalized_filter(
    media_type: Option<&str>,
    usable_for: Option<&str>,
) -> Result<Option<String>, AssetError> {
    let value = media_type.or(match usable_for {
        Some(
            "article_cover" | "article_content" | "moment" | "game_cover" | "friend_avatar"
            | "raiment_cover",
        ) => Some("image"),
        Some("bgm" | "opening_voice" | "raiment_voice") => Some("audio"),
        Some("live2d_model" | "raiment_kanban") => Some("live2d"),
        _ => None,
    });
    match value {
        None => Ok(None),
        Some(value)
            if matches!(
                value,
                "image" | "audio" | "video" | "live2d" | "font" | "other"
            ) =>
        {
            Ok(Some(value.to_owned()))
        }
        Some(_) => Err(AssetError::validation("无效的素材类型筛选")),
    }
}

fn detect_media_type(
    filename: &str,
    mime: Option<&str>,
    requested: Option<&str>,
) -> Result<String, AssetError> {
    let lower = filename.to_ascii_lowercase();
    let detected = if lower.ends_with(".zip") || lower.ends_with(".model3.json") {
        "live2d"
    } else if mime.is_some_and(|v| v.starts_with("image/")) {
        "image"
    } else if mime.is_some_and(|v| v.starts_with("audio/")) {
        "audio"
    } else if mime.is_some_and(|v| v.starts_with("video/")) {
        "video"
    } else if lower.ends_with(".woff")
        || lower.ends_with(".woff2")
        || lower.ends_with(".ttf")
        || lower.ends_with(".otf")
    {
        "font"
    } else {
        "other"
    };
    if let Some(requested) = requested {
        if !matches!(
            requested,
            "image" | "audio" | "video" | "live2d" | "font" | "other"
        ) {
            return Err(AssetError::validation("media_type 无效"));
        }
        if requested != detected && detected != "other" {
            return Err(AssetError::validation("media_type 与文件类型不一致"));
        }
        return Ok(requested.to_owned());
    }
    Ok(detected.to_owned())
}

fn validate_file_signature(
    filename: &str,
    media_type: &str,
    bytes: &[u8],
) -> Result<(), AssetError> {
    let lower = filename.to_ascii_lowercase();
    let valid = match media_type {
        "image" => {
            bytes.starts_with(b"\x89PNG\r\n\x1a\n")
                || bytes.starts_with(b"\xff\xd8\xff")
                || bytes.starts_with(b"GIF87a")
                || bytes.starts_with(b"GIF89a")
                || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"))
        }
        "audio" => {
            bytes.starts_with(b"ID3")
                || bytes.starts_with(b"fLaC")
                || bytes.starts_with(b"OggS")
                || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE"))
                || bytes
                    .windows(2)
                    .next()
                    .is_some_and(|header| header[0] == 0xff && header[1] & 0xe0 == 0xe0)
        }
        "video" => bytes.get(4..8) == Some(b"ftyp") || bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]),
        "font" => {
            bytes.starts_with(b"wOFF")
                || bytes.starts_with(b"wOF2")
                || bytes.starts_with(b"OTTO")
                || bytes.starts_with(&[0x00, 0x01, 0x00, 0x00])
        }
        "live2d" if lower.ends_with(".model3.json") => serde_json::from_slice::<Value>(bytes)
            .ok()
            .is_some_and(|value| value.is_object()),
        "live2d" if lower.ends_with(".zip") => validate_live2d_zip(bytes)?,
        "live2d" => false,
        "other" => true,
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(AssetError::validation("文件内容与扩展名或媒体类型不一致"))
    }
}

fn validate_live2d_zip(bytes: &[u8]) -> Result<bool, AssetError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| AssetError::validation("Live2D 压缩包不是有效 ZIP"))?;
    let mut expanded = 0_u64;
    let mut model_entries = 0_usize;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|_| AssetError::validation("Live2D 压缩包目录损坏"))?;
        if entry.enclosed_name().is_none() {
            return Err(AssetError::validation("Live2D 压缩包包含不安全路径"));
        }
        expanded = expanded.saturating_add(entry.size());
        if expanded > 1024 * 1024 * 1024 {
            return Err(AssetError::validation("Live2D 压缩包展开后不能超过 1 GB"));
        }
        if entry.name().to_ascii_lowercase().ends_with(".model3.json") {
            model_entries += 1;
        }
    }
    if model_entries != 1 {
        return Err(AssetError::validation(
            "Live2D 压缩包必须且只能包含一个 .model3.json 入口",
        ));
    }
    Ok(true)
}

fn validate_batch(ids: &[i64]) -> Result<(), AssetError> {
    if ids.is_empty() || ids.len() > MAX_BATCH || ids.iter().any(|id| *id <= 0) {
        return Err(AssetError::validation(
            "asset_ids 必须包含 1 到 100 个有效 ID",
        ));
    }
    if ids.iter().copied().collect::<HashSet<_>>().len() != ids.len() {
        return Err(AssetError::validation("asset_ids 不能包含重复 ID"));
    }
    Ok(())
}

fn normalized_asset_name(requested: Option<&str>, filename: &str) -> Result<String, AssetError> {
    let name = requested
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| filename.trim());
    if name.is_empty() || name.chars().count() > 255 {
        return Err(AssetError::validation(
            "素材名称不能为空且不能超过 255 个字符",
        ));
    }
    Ok(name.to_owned())
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(12)
        .collect::<String>()
        .to_ascii_lowercase()
}

fn safe_archive_name(name: &str, id: i64, index: usize) -> String {
    let clean = name.replace(['/', '\\', '\0'], "_");
    let clean = clean.trim_matches(['.', ' ']);
    if clean.is_empty() {
        format!("asset-{id}-{index}")
    } else {
        let clean = clean.chars().take(180).collect::<String>();
        format!("{id}-{clean}")
    }
}

#[derive(Debug)]
enum AssetError {
    Unauthorized,
    NotFound,
    Validation(String),
    Conflict(String),
    Database(sqlx::Error),
    Storage(anyhow::Error),
    Zip(zip::result::ZipError),
    Io(std::io::Error),
    Internal(String),
}

impl AssetError {
    fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }
}

impl From<sqlx::Error> for AssetError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value)
    }
}

impl IntoResponse for AssetError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
                "需要有效的管理员会话".to_owned(),
            ),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found", "素材不存在".to_owned()),
            Self::Validation(message) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_error",
                message,
            ),
            Self::Conflict(message) => (StatusCode::CONFLICT, "asset_in_use", message),
            Self::Database(error) => {
                error!(%error, "asset database operation failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "素材操作失败".to_owned(),
                )
            }
            Self::Storage(error) => {
                error!(%error, "asset storage operation failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "素材操作失败".to_owned(),
                )
            }
            Self::Zip(error) => {
                error!(%error, "asset archive operation failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "素材操作失败".to_owned(),
                )
            }
            Self::Io(error) => {
                error!(%error, "asset archive I/O failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "素材操作失败".to_owned(),
                )
            }
            Self::Internal(error) => {
                error!(%error, "asset response construction failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "素材操作失败".to_owned(),
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

#[cfg(test)]
mod tests {
    use super::{
        MAX_BATCH, detect_media_type, normalized_asset_name, normalized_filter,
        reference_type_label, safe_archive_name, sanitize, validate_batch, validate_file_signature,
    };
    use std::io::Write;

    #[test]
    fn usable_for_filters_map_to_the_supported_media_types() {
        for target in [
            "article_cover",
            "article_content",
            "moment",
            "game_cover",
            "friend_avatar",
            "raiment_cover",
        ] {
            assert_eq!(
                normalized_filter(None, Some(target)).unwrap().as_deref(),
                Some("image")
            );
        }
        for target in ["bgm", "opening_voice", "raiment_voice"] {
            assert_eq!(
                normalized_filter(None, Some(target)).unwrap().as_deref(),
                Some("audio")
            );
        }
        for target in ["live2d_model", "raiment_kanban"] {
            assert_eq!(
                normalized_filter(None, Some(target)).unwrap().as_deref(),
                Some("live2d")
            );
        }
        assert_eq!(normalized_filter(None, Some("unknown")).unwrap(), None);
        assert!(normalized_filter(Some("executable"), None).is_err());
    }

    #[test]
    fn requested_and_detected_media_types_cannot_disagree() {
        assert_eq!(
            detect_media_type("cover.png", Some("image/png"), None).unwrap(),
            "image"
        );
        assert_eq!(
            detect_media_type("voice.mp3", Some("audio/mpeg"), None).unwrap(),
            "audio"
        );
        assert_eq!(
            detect_media_type("clip.mp4", Some("video/mp4"), None).unwrap(),
            "video"
        );
        assert_eq!(detect_media_type("font.woff2", None, None).unwrap(), "font");
        assert_eq!(
            detect_media_type("model.zip", None, None).unwrap(),
            "live2d"
        );
        assert_eq!(
            detect_media_type("data.bin", None, Some("other")).unwrap(),
            "other"
        );
        assert!(detect_media_type("cover.png", Some("image/png"), Some("audio")).is_err());
        assert!(detect_media_type("data.bin", None, Some("invalid")).is_err());
    }

    #[test]
    fn file_signatures_are_checked_for_every_supported_family() {
        assert!(validate_file_signature("a.png", "image", b"\x89PNG\r\n\x1a\n").is_ok());
        assert!(validate_file_signature("a.jpg", "image", b"\xff\xd8\xff").is_ok());
        assert!(validate_file_signature("a.gif", "image", b"GIF89a").is_ok());
        assert!(validate_file_signature("a.mp3", "audio", b"ID3payload").is_ok());
        assert!(validate_file_signature("a.wav", "audio", b"RIFFxxxxWAVE").is_ok());
        assert!(validate_file_signature("a.mp4", "video", b"xxxxftyp").is_ok());
        assert!(validate_file_signature("a.woff2", "font", b"wOF2").is_ok());
        assert!(validate_file_signature("a.model3.json", "live2d", br#"{"Version":3}"#).is_ok());
        assert!(validate_file_signature("a.bin", "other", b"anything").is_ok());
        assert!(validate_file_signature("a.png", "image", b"not an image").is_err());
        assert!(validate_file_signature("a.model3.json", "live2d", b"[]").is_err());
    }

    #[test]
    fn live2d_archives_reject_unsafe_or_ambiguous_entries() {
        fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
            let mut output = std::io::Cursor::new(Vec::new());
            {
                let mut zip = zip::ZipWriter::new(&mut output);
                for (name, body) in entries {
                    zip.start_file(*name, zip::write::SimpleFileOptions::default())
                        .unwrap();
                    zip.write_all(body).unwrap();
                }
                zip.finish().unwrap();
            }
            output.into_inner()
        }

        assert!(
            validate_file_signature(
                "model.zip",
                "live2d",
                &archive(&[("model/avatar.model3.json", b"{}")]),
            )
            .is_ok()
        );
        assert!(
            validate_file_signature(
                "model.zip",
                "live2d",
                &archive(&[("a.model3.json", b"{}"), ("b.model3.json", b"{}")]),
            )
            .is_err()
        );
        assert!(
            validate_file_signature(
                "model.zip",
                "live2d",
                &archive(&[("../escape.model3.json", b"{}")]),
            )
            .is_err()
        );
    }

    #[test]
    fn batch_and_archive_names_are_bounded_and_safe() {
        assert!(validate_batch(&[]).is_err());
        assert!(validate_batch(&[0]).is_err());
        assert!(validate_batch(&(1..=(MAX_BATCH as i64 + 1)).collect::<Vec<_>>()).is_err());
        assert!(validate_batch(&[1, 1]).is_err());
        assert!(validate_batch(&[1, 2, 3]).is_ok());
        assert_eq!(
            normalized_asset_name(Some("  "), "cover.png").unwrap(),
            "cover.png"
        );
        assert_eq!(
            normalized_asset_name(Some(" 海边 "), "cover.png").unwrap(),
            "海边"
        );
        assert!(normalized_asset_name(Some(&"a".repeat(256)), "cover.png").is_err());
        assert_eq!(sanitize("PNG<script>"), "pngscript");
        assert_eq!(safe_archive_name("../a\\b.png", 9, 0), "9-_a_b.png");
        assert_eq!(safe_archive_name("...", 9, 2), "asset-9-2");
        assert_eq!(
            safe_archive_name(&"图".repeat(300), 9, 2).chars().count(),
            182
        );
        assert_eq!(reference_type_label("article_cover"), "文章封面");
        assert_eq!(reference_type_label("raiment_voice"), "灵衣封面语音");
        assert_eq!(reference_type_label("unknown"), "其他引用");
    }
}
