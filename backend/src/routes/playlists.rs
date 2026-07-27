use std::collections::HashSet;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, put},
};
use chrono::{DateTime, Utc};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, Postgres, Transaction};
use tracing::{error, warn};

use crate::{
    auth,
    error::{ErrorBody, ErrorEnvelope},
    routes::contract::HttpMethod,
    state::AppState,
};

const MAX_EXTERNAL_TRACKS: usize = 2_000;
const MAX_METING_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_QQ_SHARE_REDIRECTS: usize = 3;
const DEFAULT_ADMIN_TRACK_PAGE_SIZE: i64 = 10;
const MAX_ADMIN_TRACK_PAGE_SIZE: i64 = 100;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/playlists", get(public_list))
        .route("/api/v1/music", get(public_list))
        .route(
            "/api/v1/playlists/{source}/{track_id}/stream",
            get(external_stream),
        )
        .route(
            "/api/v1/admin/playlists",
            get(admin_list).post(admin_create),
        )
        .route("/api/v1/admin/playlists/order", put(admin_update_order))
        .route(
            "/api/v1/admin/playlists/{id}",
            put(admin_update).delete(admin_delete),
        )
        .route(
            "/api/v1/admin/playlists/{id}/tracks",
            get(admin_list_tracks).post(admin_add_track),
        )
        .route(
            "/api/v1/admin/playlists/{id}/tracks/{track_id}",
            axum::routing::delete(admin_delete_track),
        )
        // Compatibility aliases keep old clients working while the UI and
        // response model move from a flat music list to playlists.
        .route("/api/v1/admin/music", get(admin_list).post(admin_create))
        .route("/api/v1/admin/music/order", put(admin_update_order))
        .route(
            "/api/v1/admin/music/{id}",
            axum::routing::delete(admin_delete),
        )
}

pub fn implements(method: HttpMethod, path: &str) -> bool {
    matches!(
        (method, path),
        (HttpMethod::Get, "/api/v1/music")
            | (HttpMethod::Get, "/api/v1/admin/music")
            | (HttpMethod::Post, "/api/v1/admin/music")
            | (HttpMethod::Put, "/api/v1/admin/music/order")
            | (HttpMethod::Delete, "/api/v1/admin/music/{id}")
            | (HttpMethod::Get, "/api/v1/admin/playlists/{id}/tracks")
            | (HttpMethod::Post, "/api/v1/admin/playlists/{id}/tracks")
            | (
                HttpMethod::Delete,
                "/api/v1/admin/playlists/{id}/tracks/{track_id}"
            )
            | (HttpMethod::Put, "/api/v1/admin/playlists/{id}")
    )
}

#[derive(Debug, thiserror::Error)]
enum PlaylistError {
    #[error("需要有效的管理员会话")]
    Unauthorized,
    #[error("歌单或歌曲不存在")]
    NotFound,
    #[error("歌单目录已关闭")]
    Disabled,
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Upstream(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("{0}")]
    CorruptData(String),
}

impl PlaylistError {
    fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }
}

impl IntoResponse for PlaylistError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
                "需要有效的管理员会话".to_owned(),
            ),
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                "not_found",
                "歌单或歌曲不存在".to_owned(),
            ),
            Self::Disabled => (
                StatusCode::FORBIDDEN,
                "feature_disabled",
                "歌单目录已关闭".to_owned(),
            ),
            Self::Validation(message) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_error",
                message,
            ),
            Self::Conflict(message) => (StatusCode::CONFLICT, "conflict", message),
            Self::Upstream(message) => (StatusCode::BAD_GATEWAY, "upstream_error", message),
            Self::Database(database_error) => {
                error!(error = %database_error, "playlist database operation failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "歌单操作失败".to_owned(),
                )
            }
            Self::CorruptData(message) => {
                error!(%message, "persisted playlist data is invalid");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "歌单配置损坏".to_owned(),
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

#[derive(Debug, Clone, FromRow)]
struct PlaylistRow {
    id: i64,
    name: String,
    description: String,
    source_kind: String,
    external_id: Option<String>,
    external_url: Option<String>,
    enabled: bool,
    sort_order: i32,
    track_count: Option<i64>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct LocalTrackRow {
    id: i64,
    title: String,
    artist: String,
    duration_s: i32,
    sort_order: i32,
    file_asset_id: Option<i64>,
    object_key: String,
}

#[derive(Debug, Serialize)]
struct PlaylistItem {
    id: i64,
    name: String,
    description: String,
    source_kind: String,
    external_id: Option<String>,
    external_url: Option<String>,
    enabled: bool,
    sort_order: i32,
    status: &'static str,
    status_message: Option<String>,
    track_count: i64,
    tracks: Vec<PlaylistTrack>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct AdminPlaylistItem {
    id: i64,
    name: String,
    description: String,
    source_kind: String,
    external_id: Option<String>,
    external_url: Option<String>,
    enabled: bool,
    sort_order: i32,
    status: &'static str,
    status_message: Option<String>,
    track_count: Option<i64>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct PlaylistTrack {
    id: String,
    title: String,
    artist: String,
    url: String,
    cover_url: Option<String>,
    source_kind: String,
    duration_s: i32,
    sort_order: i32,
    asset_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct MetingTrack {
    title: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    pic: String,
    url: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreatePlaylistRequest {
    name: Option<String>,
    #[serde(default)]
    description: String,
    source_kind: String,
    external_reference: Option<String>,
    #[serde(default = "default_true")]
    enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdatePlaylistRequest {
    name: String,
    #[serde(default)]
    description: String,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AddTrackRequest {
    asset_id: i64,
    title: Option<String>,
    #[serde(default)]
    artist: String,
    #[serde(default)]
    duration_s: i32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OrderRequest {
    order: Vec<i64>,
}

#[derive(Debug, Deserialize)]
struct TrackListQuery {
    page: Option<i64>,
    per_page: Option<i64>,
}

async fn public_list(State(state): State<AppState>) -> Result<Json<Value>, PlaylistError> {
    if !music_feature_enabled(&state).await? {
        return Err(PlaylistError::Disabled);
    }
    public_payload(&state).await.map(Json)
}

async fn admin_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, PlaylistError> {
    require_admin(&state, &headers)?;
    admin_payload(&state).await.map(Json)
}

async fn public_payload(state: &AppState) -> Result<Value, PlaylistError> {
    let rows = fetch_playlists(state, true).await?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(build_playlist_item(state, row).await?);
    }
    Ok(json!({ "items": items }))
}

async fn admin_payload(state: &AppState) -> Result<Value, PlaylistError> {
    let items = fetch_playlists(state, false)
        .await?
        .into_iter()
        .map(build_admin_playlist_item)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "items": items }))
}

async fn fetch_playlists(
    state: &AppState,
    enabled_only: bool,
) -> Result<Vec<PlaylistRow>, PlaylistError> {
    let rows = sqlx::query_as::<_, PlaylistRow>(
        "SELECT playlist.id, playlist.name, playlist.description, playlist.source_kind,
                playlist.external_id, playlist.external_url, playlist.enabled,
                playlist.sort_order,
                CASE WHEN playlist.source_kind = 'local' THEN (
                    SELECT COUNT(*)
                    FROM playlist_tracks track
                    JOIN assets asset ON asset.id = track.file_asset_id
                                      AND asset.status = 'active'
                                      AND asset.media_type = 'audio'
                    WHERE track.playlist_id = playlist.id
                ) END AS track_count,
                playlist.created_at, playlist.updated_at
         FROM playlists playlist
         WHERE ($1 = false OR playlist.enabled = true)
         ORDER BY playlist.sort_order, playlist.id",
    )
    .bind(enabled_only)
    .fetch_all(state.pool())
    .await?;
    Ok(rows)
}

fn build_admin_playlist_item(row: PlaylistRow) -> Result<AdminPlaylistItem, PlaylistError> {
    if row.source_kind != "local" && row.external_id.is_none() {
        return Err(PlaylistError::CorruptData(format!(
            "external playlist {} has no source id",
            row.id
        )));
    }
    Ok(AdminPlaylistItem {
        id: row.id,
        name: row.name,
        description: row.description,
        source_kind: row.source_kind,
        external_id: row.external_id,
        external_url: row.external_url,
        enabled: row.enabled,
        sort_order: row.sort_order,
        status: "ready",
        status_message: None,
        track_count: row.track_count,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

async fn build_playlist_item(
    state: &AppState,
    row: PlaylistRow,
) -> Result<PlaylistItem, PlaylistError> {
    let (tracks, status, status_message) = if row.source_kind == "local" {
        (fetch_local_tracks(state, row.id).await?, "ready", None)
    } else if let Some(external_id) = row.external_id.as_deref() {
        match fetch_external_tracks(state, &row.source_kind, external_id).await {
            Ok(tracks) => (tracks, "ready", None),
            Err(error) => {
                warn!(
                    playlist_id = row.id,
                    source = %row.source_kind,
                    %error,
                    "external playlist is temporarily unavailable"
                );
                (
                    Vec::new(),
                    "unavailable",
                    Some("外部歌单暂时无法读取，本地歌单不受影响".to_owned()),
                )
            }
        }
    } else {
        return Err(PlaylistError::CorruptData(format!(
            "external playlist {} has no source id",
            row.id
        )));
    };

    let track_count = tracks.len() as i64;
    Ok(PlaylistItem {
        id: row.id,
        name: row.name,
        description: row.description,
        source_kind: row.source_kind,
        external_id: row.external_id,
        external_url: row.external_url,
        enabled: row.enabled,
        sort_order: row.sort_order,
        status,
        status_message,
        track_count,
        tracks,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

async fn admin_list_tracks(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Query(query): Query<TrackListQuery>,
) -> Result<Json<Value>, PlaylistError> {
    require_admin(&state, &headers)?;
    let (page, per_page, offset) = track_page_values(query)?;
    let playlist = fetch_playlist(&state, id).await?;
    let (items, total, status, status_message) = if playlist.source_kind == "local" {
        (
            fetch_local_tracks_page(&state, id, per_page, offset).await?,
            playlist.track_count.unwrap_or_default(),
            "ready",
            None,
        )
    } else {
        let external_id = playlist.external_id.as_deref().ok_or_else(|| {
            PlaylistError::CorruptData(format!(
                "external playlist {} has no source id",
                playlist.id
            ))
        })?;
        match fetch_external_tracks(&state, &playlist.source_kind, external_id).await {
            Ok(tracks) => {
                let total = tracks.len() as i64;
                let offset = usize::try_from(offset)
                    .map_err(|_| PlaylistError::validation("page 数值过大"))?;
                (
                    tracks
                        .into_iter()
                        .skip(offset)
                        .take(per_page as usize)
                        .collect(),
                    total,
                    "ready",
                    None,
                )
            }
            Err(error) => {
                warn!(
                    playlist_id = playlist.id,
                    source = %playlist.source_kind,
                    %error,
                    "external playlist is temporarily unavailable"
                );
                (
                    Vec::new(),
                    0,
                    "unavailable",
                    Some("外部歌单暂时无法读取，请检查歌单是否公开或稍后重试".to_owned()),
                )
            }
        }
    };
    Ok(Json(json!({
        "page": page,
        "per_page": per_page,
        "total": total,
        "items": items,
        "status": status,
        "status_message": status_message,
    })))
}

fn track_page_values(query: TrackListQuery) -> Result<(i64, i64, i64), PlaylistError> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(DEFAULT_ADMIN_TRACK_PAGE_SIZE);
    if page < 1 {
        return Err(PlaylistError::validation("page 必须大于等于 1"));
    }
    if !(1..=MAX_ADMIN_TRACK_PAGE_SIZE).contains(&per_page) {
        return Err(PlaylistError::validation("per_page 必须在 1 到 100 之间"));
    }
    let offset = (page - 1)
        .checked_mul(per_page)
        .ok_or_else(|| PlaylistError::validation("page 数值过大"))?;
    Ok((page, per_page, offset))
}

async fn fetch_local_tracks(
    state: &AppState,
    playlist_id: i64,
) -> Result<Vec<PlaylistTrack>, PlaylistError> {
    let rows = sqlx::query_as::<_, LocalTrackRow>(
        "SELECT track.id, track.title, track.artist, track.duration_s,
                track.sort_order, track.file_asset_id, upload.object_key
         FROM playlist_tracks track
         JOIN assets asset ON asset.id = track.file_asset_id
                           AND asset.status = 'active'
                           AND asset.media_type = 'audio'
         JOIN uploads upload ON upload.id = asset.upload_id
         WHERE track.playlist_id = $1
         ORDER BY track.sort_order, track.id",
    )
    .bind(playlist_id)
    .fetch_all(state.pool())
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| PlaylistTrack {
            id: row.id.to_string(),
            title: row.title,
            artist: row.artist,
            url: state.object_storage().public_url(&row.object_key),
            cover_url: None,
            source_kind: "local".to_owned(),
            duration_s: row.duration_s,
            sort_order: row.sort_order,
            asset_id: row.file_asset_id,
        })
        .collect())
}

async fn fetch_local_tracks_page(
    state: &AppState,
    playlist_id: i64,
    per_page: i64,
    offset: i64,
) -> Result<Vec<PlaylistTrack>, PlaylistError> {
    let rows = sqlx::query_as::<_, LocalTrackRow>(
        "SELECT track.id, track.title, track.artist, track.duration_s,
                track.sort_order, track.file_asset_id, upload.object_key
         FROM playlist_tracks track
         JOIN assets asset ON asset.id = track.file_asset_id
                           AND asset.status = 'active'
                           AND asset.media_type = 'audio'
         JOIN uploads upload ON upload.id = asset.upload_id
         WHERE track.playlist_id = $1
         ORDER BY track.sort_order, track.id
         LIMIT $2 OFFSET $3",
    )
    .bind(playlist_id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(state.pool())
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| PlaylistTrack {
            id: row.id.to_string(),
            title: row.title,
            artist: row.artist,
            url: state.object_storage().public_url(&row.object_key),
            cover_url: None,
            source_kind: "local".to_owned(),
            duration_s: row.duration_s,
            sort_order: row.sort_order,
            asset_id: row.file_asset_id,
        })
        .collect())
}

async fn fetch_external_tracks(
    state: &AppState,
    source_kind: &str,
    external_id: &str,
) -> Result<Vec<PlaylistTrack>, PlaylistError> {
    let mut url = meting_url(state, source_kind, "playlist", external_id)?;
    // A changing value prevents an intermediary from keeping expired playback
    // URLs while the provider itself remains the source of truth.
    url.query_pairs_mut().append_pair("r", "1");
    let response = state
        .meting_http_client()
        .get(url)
        .send()
        .await
        .map_err(|error| PlaylistError::Upstream(format!("外部歌单请求失败：{error}")))?;
    if !response.status().is_success() {
        return Err(PlaylistError::Upstream(format!(
            "外部歌单返回 HTTP {}",
            response.status()
        )));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_METING_RESPONSE_BYTES as u64)
    {
        return Err(PlaylistError::Upstream("外部歌单响应过大".to_owned()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| PlaylistError::Upstream(format!("外部歌单读取失败：{error}")))?;
    if bytes.len() > MAX_METING_RESPONSE_BYTES {
        return Err(PlaylistError::Upstream("外部歌单响应过大".to_owned()));
    }
    let parsed: Vec<MetingTrack> = serde_json::from_slice(&bytes)
        .map_err(|error| PlaylistError::Upstream(format!("外部歌单格式无效：{error}")))?;
    let mut tracks = Vec::new();
    for (index, item) in parsed.into_iter().take(MAX_EXTERNAL_TRACKS).enumerate() {
        let Some(track_id) = external_track_id(&item.url) else {
            continue;
        };
        tracks.push(PlaylistTrack {
            id: track_id.clone(),
            title: item.title,
            artist: item.author,
            url: format!(
                "/api/v1/playlists/{source_kind}/{}/stream",
                percent_encode_segment(&track_id)
            ),
            cover_url: (!item.pic.trim().is_empty()).then_some(item.pic),
            source_kind: source_kind.to_owned(),
            duration_s: 0,
            sort_order: index as i32,
            asset_id: None,
        });
    }
    if tracks.is_empty() {
        return Err(PlaylistError::Upstream(
            "外部歌单没有可播放的公开歌曲".to_owned(),
        ));
    }
    Ok(tracks)
}

async fn external_stream(
    State(state): State<AppState>,
    Path((source, track_id)): Path<(String, String)>,
) -> Result<Response, PlaylistError> {
    validate_source(&source)?;
    if !valid_external_track_id(&track_id) {
        return Err(PlaylistError::validation("外部歌曲 ID 无效"));
    }
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM playlists
            WHERE enabled = true AND source_kind = $1
         )",
    )
    .bind(&source)
    .fetch_one(state.pool())
    .await?;
    if !exists {
        return Err(PlaylistError::NotFound);
    }

    let url = meting_url(&state, &source, "url", &track_id)?;
    let response = state
        .meting_http_client()
        .get(url)
        .send()
        .await
        .map_err(|error| PlaylistError::Upstream(format!("播放地址请求失败：{error}")))?;
    if !response.status().is_redirection() {
        return Err(PlaylistError::Upstream(format!(
            "音乐平台未返回可播放地址（HTTP {}）",
            response.status()
        )));
    }
    let location = response
        .headers()
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| PlaylistError::Upstream("音乐平台未返回播放地址".to_owned()))?;
    validate_media_redirect(&source, location)?;
    let location = HeaderValue::from_str(location)
        .map_err(|_| PlaylistError::Upstream("音乐平台播放地址无效".to_owned()))?;
    Ok((
        StatusCode::TEMPORARY_REDIRECT,
        [
            (header::LOCATION, location),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
    )
        .into_response())
}

async fn admin_create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreatePlaylistRequest>,
) -> Result<(StatusCode, Json<Value>), PlaylistError> {
    require_admin(&state, &headers)?;
    let source_kind = request.source_kind.trim().to_ascii_lowercase();
    validate_source(&source_kind)?;
    let description = normalize_text("歌单说明", &request.description, 500, true)?;

    let (external_id, external_url) = if source_kind == "local" {
        if request
            .external_reference
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        {
            return Err(PlaylistError::validation("本地歌单不需要外部链接"));
        }
        (None, None)
    } else {
        let reference = request
            .external_reference
            .as_deref()
            .ok_or_else(|| PlaylistError::validation("请填写外部歌单链接或 ID"))?;
        let (id, url) = resolve_external_reference(&state, &source_kind, reference).await?;
        // Creation validates the source immediately; subsequent public reads
        // degrade per playlist if a provider is temporarily unavailable.
        fetch_external_tracks(&state, &source_kind, &id).await?;
        (Some(id), Some(url))
    };

    let fallback_name = external_id.as_ref().map(|id| {
        format!(
            "{}歌单 · {id}",
            if source_kind == "netease" {
                "网易云"
            } else {
                "QQ 音乐"
            }
        )
    });
    let name = normalize_text(
        "歌单名称",
        request
            .name
            .as_deref()
            .or(fallback_name.as_deref())
            .unwrap_or(""),
        120,
        false,
    )?;
    let next_order: i32 =
        sqlx::query_scalar("SELECT COALESCE(MAX(sort_order), -1) + 1 FROM playlists")
            .fetch_one(state.pool())
            .await?;
    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO playlists (
            name, description, source_kind, external_id, external_url, enabled, sort_order
         ) VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id",
    )
    .bind(name)
    .bind(description)
    .bind(&source_kind)
    .bind(external_id)
    .bind(external_url)
    .bind(request.enabled)
    .bind(next_order)
    .fetch_one(state.pool())
    .await
    .map_err(map_unique_conflict)?;

    let item = fetch_playlist(&state, id).await?;
    Ok((
        StatusCode::CREATED,
        Json(json!(build_admin_playlist_item(item)?)),
    ))
}

async fn admin_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(request): Json<UpdatePlaylistRequest>,
) -> Result<Json<Value>, PlaylistError> {
    require_admin(&state, &headers)?;
    let name = normalize_text("歌单名称", &request.name, 120, false)?;
    let description = normalize_text("歌单说明", &request.description, 500, true)?;
    let mut transaction = state.pool().begin().await?;
    lock_site_settings(&mut transaction).await?;
    let current_enabled =
        sqlx::query_scalar::<_, bool>("SELECT enabled FROM playlists WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or(PlaylistError::NotFound)?;
    if current_enabled && !request.enabled && playlist_is_scheduled(&mut transaction, id).await? {
        return Err(PlaylistError::Conflict(
            "该歌单仍被灵衣时间段引用，请先取消对应背景音乐".to_owned(),
        ));
    }
    let changed =
        sqlx::query("UPDATE playlists SET name = $1, description = $2, enabled = $3 WHERE id = $4")
            .bind(name)
            .bind(description)
            .bind(request.enabled)
            .bind(id)
            .execute(&mut *transaction)
            .await?
            .rows_affected();
    if changed == 0 {
        return Err(PlaylistError::NotFound);
    }
    transaction.commit().await?;
    let item = fetch_playlist(&state, id).await?;
    Ok(Json(json!(build_admin_playlist_item(item)?)))
}

async fn admin_delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<StatusCode, PlaylistError> {
    require_admin(&state, &headers)?;
    let mut transaction = state.pool().begin().await?;
    lock_site_settings(&mut transaction).await?;
    let exists = sqlx::query_scalar::<_, i64>("SELECT id FROM playlists WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *transaction)
        .await?;
    if exists.is_none() {
        return Err(PlaylistError::NotFound);
    }
    if playlist_is_scheduled(&mut transaction, id).await? {
        return Err(PlaylistError::Conflict(
            "该歌单仍被灵衣时间段引用，请先取消对应背景音乐".to_owned(),
        ));
    }
    let deleted = sqlx::query("DELETE FROM playlists WHERE id = $1")
        .bind(id)
        .execute(&mut *transaction)
        .await?
        .rows_affected();
    if deleted == 0 {
        return Err(PlaylistError::NotFound);
    }
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn admin_add_track(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(request): Json<AddTrackRequest>,
) -> Result<(StatusCode, Json<Value>), PlaylistError> {
    require_admin(&state, &headers)?;
    if request.asset_id <= 0 || request.duration_s < 0 {
        return Err(PlaylistError::validation("素材 ID 和时长必须为非负整数"));
    }
    let mut transaction = state.pool().begin().await?;
    let source_kind: Option<String> =
        sqlx::query_scalar("SELECT source_kind FROM playlists WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *transaction)
            .await?;
    match source_kind.as_deref() {
        Some("local") => {}
        Some(_) => return Err(PlaylistError::validation("外部歌单的曲目由音乐平台维护")),
        None => return Err(PlaylistError::NotFound),
    }
    let asset: Option<(String, String)> = sqlx::query_as(
        "SELECT asset.name, upload.object_key
         FROM assets asset
         JOIN uploads upload ON upload.id = asset.upload_id
         WHERE asset.id = $1 AND asset.status = 'active' AND asset.media_type = 'audio'",
    )
    .bind(request.asset_id)
    .fetch_optional(&mut *transaction)
    .await?;
    let (asset_name, object_key) =
        asset.ok_or_else(|| PlaylistError::validation("只能引用素材库中处于 active 状态的音频"))?;
    let title = normalize_text(
        "歌曲名称",
        request.title.as_deref().unwrap_or(&asset_name),
        200,
        false,
    )?;
    let artist = normalize_text("艺人", &request.artist, 200, true)?;
    let next_order: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sort_order), -1) + 1
         FROM playlist_tracks WHERE playlist_id = $1",
    )
    .bind(id)
    .fetch_one(&mut *transaction)
    .await?;
    let track_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO playlist_tracks (
            playlist_id, title, artist, file_key, duration_s, sort_order, file_asset_id
         ) VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id",
    )
    .bind(id)
    .bind(title)
    .bind(artist)
    .bind(object_key)
    .bind(request.duration_s)
    .bind(next_order)
    .bind(request.asset_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(map_unique_conflict)?;
    transaction.commit().await?;
    let track = fetch_local_track(&state, id, track_id).await?;
    Ok((StatusCode::CREATED, Json(json!(track))))
}

async fn admin_delete_track(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, track_id)): Path<(i64, i64)>,
) -> Result<StatusCode, PlaylistError> {
    require_admin(&state, &headers)?;
    let deleted = sqlx::query("DELETE FROM playlist_tracks WHERE id = $1 AND playlist_id = $2")
        .bind(track_id)
        .bind(id)
        .execute(state.pool())
        .await?
        .rows_affected();
    if deleted == 0 {
        return Err(PlaylistError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn admin_update_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<OrderRequest>,
) -> Result<Json<Value>, PlaylistError> {
    require_admin(&state, &headers)?;
    if request.order.len() > 100
        || request.order.iter().any(|id| *id <= 0)
        || request.order.iter().copied().collect::<HashSet<_>>().len() != request.order.len()
    {
        return Err(PlaylistError::validation("歌单顺序包含无效或重复的 ID"));
    }
    let mut transaction = state.pool().begin().await?;
    let existing = sqlx::query_scalar::<_, i64>("SELECT id FROM playlists FOR UPDATE")
        .fetch_all(&mut *transaction)
        .await?;
    if existing.iter().copied().collect::<HashSet<_>>()
        != request.order.iter().copied().collect::<HashSet<_>>()
    {
        return Err(PlaylistError::validation(
            "歌单顺序必须包含全部歌单且不能包含未知 ID",
        ));
    }
    for (index, id) in request.order.iter().enumerate() {
        sqlx::query("UPDATE playlists SET sort_order = $1 WHERE id = $2")
            .bind(index as i32)
            .bind(id)
            .execute(&mut *transaction)
            .await?;
    }
    transaction.commit().await?;
    admin_payload(&state).await.map(Json)
}

async fn fetch_playlist(state: &AppState, id: i64) -> Result<PlaylistRow, PlaylistError> {
    sqlx::query_as::<_, PlaylistRow>(
        "SELECT playlist.id, playlist.name, playlist.description, playlist.source_kind,
                playlist.external_id, playlist.external_url, playlist.enabled,
                playlist.sort_order,
                CASE WHEN playlist.source_kind = 'local' THEN (
                    SELECT COUNT(*)
                    FROM playlist_tracks track
                    JOIN assets asset ON asset.id = track.file_asset_id
                                      AND asset.status = 'active'
                                      AND asset.media_type = 'audio'
                    WHERE track.playlist_id = playlist.id
                ) END AS track_count,
                playlist.created_at, playlist.updated_at
         FROM playlists playlist WHERE playlist.id = $1",
    )
    .bind(id)
    .fetch_optional(state.pool())
    .await?
    .ok_or(PlaylistError::NotFound)
}

async fn fetch_local_track(
    state: &AppState,
    playlist_id: i64,
    track_id: i64,
) -> Result<PlaylistTrack, PlaylistError> {
    let row = sqlx::query_as::<_, LocalTrackRow>(
        "SELECT track.id, track.title, track.artist, track.duration_s,
                track.sort_order, track.file_asset_id, upload.object_key
         FROM playlist_tracks track
         JOIN assets asset ON asset.id = track.file_asset_id
         JOIN uploads upload ON upload.id = asset.upload_id
         WHERE track.playlist_id = $1 AND track.id = $2",
    )
    .bind(playlist_id)
    .bind(track_id)
    .fetch_optional(state.pool())
    .await?
    .ok_or(PlaylistError::NotFound)?;
    Ok(PlaylistTrack {
        id: row.id.to_string(),
        title: row.title,
        artist: row.artist,
        url: state.object_storage().public_url(&row.object_key),
        cover_url: None,
        source_kind: "local".to_owned(),
        duration_s: row.duration_s,
        sort_order: row.sort_order,
        asset_id: row.file_asset_id,
    })
}

async fn music_feature_enabled(state: &AppState) -> Result<bool, PlaylistError> {
    Ok(sqlx::query_scalar::<_, Option<bool>>(
        "SELECT (settings #>> '{features,music}')::boolean FROM site_settings WHERE id = 1",
    )
    .fetch_optional(state.pool())
    .await?
    .flatten()
    .unwrap_or(true))
}

async fn lock_site_settings(
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<(), PlaylistError> {
    sqlx::query_scalar::<_, i32>("SELECT 1 FROM site_settings WHERE id = 1 FOR UPDATE")
        .fetch_one(&mut **transaction)
        .await?;
    Ok(())
}

async fn playlist_is_scheduled(
    transaction: &mut Transaction<'_, Postgres>,
    playlist_id: i64,
) -> Result<bool, PlaylistError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1
            FROM site_settings settings,
                 jsonb_array_elements(
                    COALESCE(settings.settings #> '{raiment_schedule,periods}', '[]'::jsonb)
                 ) period
            WHERE settings.id = 1
              AND period ->> 'playlist_id' = $1::text
        )",
    )
    .bind(playlist_id)
    .fetch_one(&mut **transaction)
    .await?)
}

fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<(), PlaylistError> {
    if auth::has_valid_admin_session(state, headers) {
        Ok(())
    } else {
        Err(PlaylistError::Unauthorized)
    }
}

fn validate_source(source: &str) -> Result<(), PlaylistError> {
    if matches!(source, "local" | "netease" | "qq") {
        Ok(())
    } else {
        Err(PlaylistError::validation(
            "歌单来源只能是 local、netease 或 qq",
        ))
    }
}

fn meting_url(
    state: &AppState,
    source: &str,
    request_type: &str,
    id: &str,
) -> Result<Url, PlaylistError> {
    validate_source(source)?;
    if source == "local" {
        return Err(PlaylistError::validation("本地歌曲不使用外部解析"));
    }
    let base = state
        .meting_api_url()
        .ok_or_else(|| PlaylistError::Upstream("未配置外部歌单解析服务".to_owned()))?;
    let mut url = Url::parse(base)
        .map_err(|_| PlaylistError::Upstream("外部歌单解析服务地址无效".to_owned()))?;
    url.query_pairs_mut()
        .append_pair(
            "server",
            if source == "netease" {
                "netease"
            } else {
                "tencent"
            },
        )
        .append_pair("type", request_type)
        .append_pair("id", id);
    Ok(url)
}

fn parse_external_reference(
    source: &str,
    reference: &str,
) -> Result<(String, String), PlaylistError> {
    let reference = reference.trim();
    if reference.is_empty() {
        return Err(PlaylistError::validation("请填写外部歌单链接或 ID"));
    }
    let id = if reference
        .chars()
        .all(|character| character.is_ascii_digit())
    {
        reference.to_owned()
    } else {
        let url =
            Url::parse(reference).map_err(|_| PlaylistError::validation("外部歌单链接格式无效"))?;
        let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
        let host_matches = if source == "netease" {
            host == "music.163.com" || host.ends_with(".music.163.com")
        } else {
            host == "y.qq.com" || host.ends_with(".y.qq.com") || host == "i.y.qq.com"
        };
        if !host_matches {
            return Err(PlaylistError::validation("链接域名与选择的音乐平台不一致"));
        }
        url.query_pairs()
            .find(|(key, _)| key == "id")
            .map(|(_, value)| value.into_owned())
            .or_else(|| digits_after(reference, "id="))
            .or_else(|| {
                if source == "qq" {
                    digits_after(reference, "/playlist/")
                } else {
                    None
                }
            })
            .ok_or_else(|| PlaylistError::validation("链接中没有找到歌单 ID"))?
    };
    if id.is_empty() || id.len() > 32 || !id.chars().all(|character| character.is_ascii_digit()) {
        return Err(PlaylistError::validation("歌单 ID 必须是 1 到 32 位数字"));
    }
    let canonical = if source == "netease" {
        format!("https://music.163.com/#/playlist?id={id}")
    } else {
        format!("https://y.qq.com/n/ryqq/playlist/{id}")
    };
    Ok((id, canonical))
}

async fn resolve_external_reference(
    state: &AppState,
    source: &str,
    reference: &str,
) -> Result<(String, String), PlaylistError> {
    let reference = reference.trim();
    let Ok(mut current) = Url::parse(reference) else {
        return parse_external_reference(source, reference);
    };
    if source != "qq" || !is_qq_share_url(&current) {
        return parse_external_reference(source, reference);
    }

    for _ in 0..MAX_QQ_SHARE_REDIRECTS {
        let response = state
            .meting_http_client()
            .get(current.clone())
            .send()
            .await
            .map_err(|error| PlaylistError::Upstream(format!("QQ 音乐短链请求失败：{error}")))?;
        if !response.status().is_redirection() {
            return Err(PlaylistError::Upstream(format!(
                "QQ 音乐短链返回 HTTP {}",
                response.status()
            )));
        }
        let location = response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| PlaylistError::Upstream("QQ 音乐短链没有返回跳转地址".to_owned()))?;
        current = validated_qq_redirect(&current, location)?;
        if let Ok(parsed) = parse_external_reference(source, current.as_str()) {
            return Ok(parsed);
        }
    }

    Err(PlaylistError::Upstream(
        "QQ 音乐短链跳转次数过多或没有包含歌单 ID".to_owned(),
    ))
}

fn is_qq_share_url(url: &Url) -> bool {
    is_https_qq_music_url(url)
        && url.path() == "/base/fcgi-bin/u"
        && url
            .query_pairs()
            .any(|(key, value)| key == "__" && !value.is_empty())
}

fn validated_qq_redirect(current: &Url, location: &str) -> Result<Url, PlaylistError> {
    let target = current
        .join(location)
        .map_err(|_| PlaylistError::Upstream("QQ 音乐短链跳转地址无效".to_owned()))?;
    if !is_https_qq_music_url(&target) {
        return Err(PlaylistError::Upstream(
            "QQ 音乐短链跳转到了不受信任的地址".to_owned(),
        ));
    }
    Ok(target)
}

fn is_https_qq_music_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url
            .host_str()
            .is_some_and(|host| host == "y.qq.com" || host.ends_with(".y.qq.com"))
}

fn digits_after(value: &str, marker: &str) -> Option<String> {
    let start = value.find(marker)? + marker.len();
    let digits = value[start..]
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    (!digits.is_empty()).then_some(digits)
}

fn external_track_id(value: &str) -> Option<String> {
    Url::parse(value)
        .ok()?
        .query_pairs()
        .find(|(key, _)| key == "id")
        .map(|(_, value)| value.into_owned())
        .filter(|id| valid_external_track_id(id))
}

fn valid_external_track_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

fn validate_media_redirect(source: &str, location: &str) -> Result<(), PlaylistError> {
    let url = Url::parse(location)
        .map_err(|_| PlaylistError::Upstream("音乐平台播放地址无效".to_owned()))?;
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let allowed = matches!(url.scheme(), "http" | "https")
        && if source == "netease" {
            host == "music.126.net" || host.ends_with(".music.126.net")
        } else {
            host == "qq.com" || host.ends_with(".qq.com")
        };
    if allowed {
        Ok(())
    } else {
        Err(PlaylistError::Upstream(
            "音乐平台返回了不受信任的播放地址".to_owned(),
        ))
    }
}

fn normalize_text(
    label: &str,
    value: &str,
    max_chars: usize,
    allow_empty: bool,
) -> Result<String, PlaylistError> {
    let value = value.trim();
    if (!allow_empty && value.is_empty()) || value.chars().count() > max_chars {
        return Err(PlaylistError::validation(format!(
            "{label}{}且不能超过 {max_chars} 个字符",
            if allow_empty {
                "可以留空"
            } else {
                "不能为空"
            }
        )));
    }
    Ok(value.to_owned())
}

fn percent_encode_segment(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

fn map_unique_conflict(error: sqlx::Error) -> PlaylistError {
    if error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .is_some_and(|code| code == "23505")
    {
        PlaylistError::Conflict("这个歌单或素材已经被引用".to_owned())
    } else {
        PlaylistError::Database(error)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_EXTERNAL_TRACKS, TrackListQuery, external_track_id, is_qq_share_url,
        parse_external_reference, track_page_values, validate_media_redirect,
        validated_qq_redirect,
    };
    use reqwest::Url;

    #[test]
    fn external_playlists_support_up_to_two_thousand_tracks() {
        assert_eq!(MAX_EXTERNAL_TRACKS, 2_000);
    }

    #[test]
    fn validates_admin_track_pagination() {
        assert_eq!(
            track_page_values(TrackListQuery {
                page: Some(3),
                per_page: Some(10),
            })
            .unwrap(),
            (3, 10, 20)
        );
        assert!(
            track_page_values(TrackListQuery {
                page: Some(0),
                per_page: Some(20),
            })
            .is_err()
        );
        assert!(
            track_page_values(TrackListQuery {
                page: Some(1),
                per_page: Some(101),
            })
            .is_err()
        );
    }

    #[test]
    fn parses_supported_playlist_links_and_ids() {
        assert_eq!(
            parse_external_reference("netease", "https://music.163.com/#/playlist?id=60198")
                .unwrap()
                .0,
            "60198"
        );
        assert_eq!(
            parse_external_reference("qq", "https://y.qq.com/n/ryqq/playlist/123456")
                .unwrap()
                .0,
            "123456"
        );
        assert!(parse_external_reference("qq", "https://music.163.com/playlist?id=1").is_err());
    }

    #[test]
    fn recognizes_qq_share_links_and_validates_their_redirects() {
        let short = Url::parse("https://c6.y.qq.com/base/fcgi-bin/u?__=LUuvBV0J08cu").unwrap();
        assert!(is_qq_share_url(&short));

        let resolved = validated_qq_redirect(
            &short,
            "https://i.y.qq.com/n2/m/share/details/taoge.html?id=3571068057",
        )
        .unwrap();
        assert_eq!(
            parse_external_reference("qq", resolved.as_str()).unwrap().0,
            "3571068057"
        );
        assert!(validated_qq_redirect(&short, "http://127.0.0.1/private").is_err());
        assert!(validated_qq_redirect(&short, "https://example.com/playlist/1").is_err());
    }

    #[test]
    fn extracts_track_id_from_meting_url() {
        assert_eq!(
            external_track_id("http://meting:3000/api?server=netease&type=url&id=22704470")
                .as_deref(),
            Some("22704470")
        );
    }

    #[test]
    fn only_redirects_to_provider_media_hosts() {
        assert!(validate_media_redirect("netease", "https://m701.music.126.net/file.mp3").is_ok());
        assert!(
            validate_media_redirect("qq", "https://isure.stream.qqmusic.qq.com/file.m4a").is_ok()
        );
        assert!(validate_media_redirect("qq", "http://127.0.0.1/secret").is_err());
    }
}
