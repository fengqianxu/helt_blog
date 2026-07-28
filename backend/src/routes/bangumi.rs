use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use reqwest::{Url, header};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::FromRow;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    auth,
    error::{ErrorBody, ErrorEnvelope},
    routes::contract::HttpMethod,
    state::AppState,
    storage_gc,
};

const BILIBILI_FOLLOW_API: &str = "https://api.bilibili.com/x/space/bangumi/follow/list";
const BILIBILI_PAGE_SIZE: i64 = 30;
const MAX_PUBLIC_PAGE_SIZE: i64 = 100;
const MAX_COVER_BYTES: u64 = 5 * 1024 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/bangumi", get(list_bangumi))
        .route("/api/v1/admin/bangumi/sync", post(start_sync))
}

pub fn implements(method: HttpMethod, path: &str) -> bool {
    matches!(
        (method, path),
        (HttpMethod::Get, "/api/v1/bangumi") | (HttpMethod::Post, "/api/v1/admin/bangumi/sync")
    )
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    status: Option<String>,
    page: Option<i64>,
    per_page: Option<i64>,
}

#[derive(Debug, FromRow)]
struct BangumiRow {
    id: i64,
    bilibili_media_id: i64,
    season_id: Option<i64>,
    title: String,
    cover_key: Option<String>,
    status: String,
    ep_current: i32,
    ep_total: i32,
    synced_at: DateTime<Utc>,
    metadata: Value,
}

#[derive(Debug, Serialize)]
struct BangumiItem {
    id: i64,
    bilibili_media_id: i64,
    season_id: Option<i64>,
    title: String,
    cover_url: Option<String>,
    status: String,
    ep_current: i32,
    ep_total: i32,
    synced_at: DateTime<Utc>,
    season_type: String,
    summary: String,
    score: Option<f64>,
    url: String,
    latest_episode: String,
}

#[derive(Debug, Serialize)]
struct BangumiCounts {
    watching: i64,
    finished: i64,
}

#[derive(Debug, Serialize)]
struct BangumiMeta {
    counts: BangumiCounts,
    synced_at: Option<DateTime<Utc>>,
    configured: bool,
    sync_status: String,
}

#[derive(Debug, Serialize)]
struct BangumiListResponse {
    page: i64,
    per_page: i64,
    total: i64,
    items: Vec<BangumiItem>,
    meta: BangumiMeta,
}

async fn list_bangumi(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<BangumiListResponse>, BangumiError> {
    let status = match query.status.as_deref() {
        None | Some("") => None,
        Some("watching") => Some("watching"),
        Some("finished") => Some("finished"),
        Some(_) => {
            return Err(BangumiError::Validation(
                "追番状态只能是 watching 或 finished",
            ));
        }
    };
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(10);
    if page < 1 {
        return Err(BangumiError::Validation("page 必须大于等于 1"));
    }
    if !(1..=MAX_PUBLIC_PAGE_SIZE).contains(&per_page) {
        return Err(BangumiError::Validation("per_page 必须在 1 到 100 之间"));
    }

    let offset =
        pagination_offset(page, per_page).ok_or(BangumiError::Validation("page 数值过大"))?;

    let (total, watching, finished, synced_at, configured, sync_status): (
        i64,
        i64,
        i64,
        Option<DateTime<Utc>>,
        bool,
        String,
    ) = sqlx::query_as(
        "SELECT
             COUNT(*) FILTER (WHERE $1::text IS NULL OR status = $1),
             COUNT(*) FILTER (WHERE status = 'watching'),
             COUNT(*) FILTER (WHERE status = 'finished'),
             MAX(synced_at),
             COALESCE((SELECT btrim(settings #>> '{bangumi_sync,uid}') <> '' FROM site_settings WHERE id = 1), false),
             COALESCE((
                 SELECT CASE
                     WHEN COALESCE(btrim(settings #>> '{bangumi_sync,uid}'), '') = ''
                         THEN 'disabled'
                     WHEN settings #>> '{bangumi_sync,last_status}' IN ('ok','queued','disabled')
                         THEN settings #>> '{bangumi_sync,last_status}'
                     WHEN settings #>> '{bangumi_sync,last_status}' IS NULL
                         THEN 'queued'
                     ELSE 'error'
                 END
                 FROM site_settings WHERE id = 1
             ), 'disabled')
         FROM bangumi",
    )
    .bind(status)
    .fetch_one(state.pool())
    .await?;

    let rows = sqlx::query_as::<_, BangumiRow>(
        "SELECT id, bilibili_media_id, season_id, title, cover_key, status,
                ep_current, ep_total, synced_at, metadata
         FROM bangumi
         WHERE $1::text IS NULL OR status = $1
         ORDER BY CASE status WHEN 'watching' THEN 0 ELSE 1 END, sort_order, id
         LIMIT $2 OFFSET $3",
    )
    .bind(status)
    .bind(per_page)
    .bind(offset)
    .fetch_all(state.pool())
    .await?;

    let items = rows
        .into_iter()
        .map(|row| bangumi_item(&state, row))
        .collect();
    Ok(Json(BangumiListResponse {
        page,
        per_page,
        total,
        items,
        meta: BangumiMeta {
            counts: BangumiCounts { watching, finished },
            synced_at,
            configured,
            sync_status,
        },
    }))
}

fn pagination_offset(page: i64, per_page: i64) -> Option<i64> {
    (page - 1).checked_mul(per_page)
}

fn bangumi_item(state: &AppState, row: BangumiRow) -> BangumiItem {
    let text = |key: &str| {
        row.metadata
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let source_cover = text("source_cover");
    let source_url = text("url");
    let fallback_url = format!(
        "https://www.bilibili.com/bangumi/media/md{}",
        row.bilibili_media_id
    );
    BangumiItem {
        id: row.id,
        bilibili_media_id: row.bilibili_media_id,
        season_id: row.season_id,
        title: row.title,
        cover_url: row
            .cover_key
            .as_deref()
            .map(|key| state.object_storage().public_url(key))
            .or_else(|| validated_bilibili_cover_url(&source_cover).map(|url| url.to_string())),
        status: row.status,
        ep_current: row.ep_current,
        ep_total: row.ep_total,
        synced_at: row.synced_at,
        season_type: text("season_type"),
        summary: text("summary"),
        score: row.metadata.get("score").and_then(Value::as_f64),
        url: validated_bilibili_page_url(&source_url)
            .map(|url| url.to_string())
            .unwrap_or(fallback_url),
        latest_episode: text("latest_episode"),
    }
}

async fn start_sync(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(StatusCode, Json<Value>), BangumiError> {
    auth::authenticated_admin_id(&state, &headers)
        .await
        .ok_or(BangumiError::Unauthorized)?;
    if configured_uid(&state).await?.is_none() {
        return Err(BangumiError::Validation("请先在个人资料中填写 B 站 UID"));
    }
    let job_id = trigger_sync(state).ok_or(BangumiError::Conflict)?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "job_id": job_id, "status": "queued" })),
    ))
}

pub fn trigger_sync(state: AppState) -> Option<Uuid> {
    if !state.try_begin_bangumi_sync() {
        return None;
    }
    let job_id = Uuid::now_v7();
    tokio::spawn(async move {
        let result = sync_configured(&state).await;
        let uid_changed = result
            .as_ref()
            .err()
            .is_some_and(|sync_error| sync_error.to_string().contains("UID changed"));
        if let Err(sync_error) = &result {
            if uid_changed {
                info!("Bilibili UID changed during sync; scheduling the current profile");
            } else {
                error!(error = %sync_error, "Bilibili bangumi sync failed");
                record_sync_failure(&state, sync_error).await;
            }
        }
        state.finish_bangumi_sync();
        if uid_changed {
            let _ = trigger_sync(state);
        }
    });
    Some(job_id)
}

pub async fn run_scheduler(state: AppState) {
    loop {
        match configured_uid(&state).await {
            Ok(Some(_)) => {
                let _ = trigger_sync(state.clone());
            }
            Ok(None) => {}
            Err(load_error) => warn!(error = %load_error, "Bilibili UID could not be loaded"),
        }
        let hours = sync_interval_hours(&state).await.unwrap_or(6);
        tokio::time::sleep(Duration::from_secs(hours as u64 * 60 * 60)).await;
    }
}

async fn sync_interval_hours(state: &AppState) -> Result<i64, sqlx::Error> {
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT settings #>> '{bangumi_sync,interval_hours}' FROM site_settings WHERE id = 1",
    )
    .fetch_optional(state.pool())
    .await?
    .flatten();
    Ok(raw
        .and_then(|value| value.parse().ok())
        .unwrap_or(6)
        .clamp(1, 168))
}

async fn configured_uid(state: &AppState) -> Result<Option<String>, sqlx::Error> {
    let uid: Option<String> = sqlx::query_scalar(
        "SELECT NULLIF(btrim(settings #>> '{bangumi_sync,uid}'), '') FROM site_settings WHERE id = 1",
    )
    .fetch_optional(state.pool())
    .await?
    .flatten();
    Ok(uid)
}

#[derive(Debug)]
struct SyncedBangumi {
    media_id: i64,
    season_id: i64,
    title: String,
    cover: String,
    status: &'static str,
    ep_current: i32,
    ep_total: i32,
    sort_order: i32,
    metadata: Value,
    cover_key: Option<String>,
}

async fn sync_configured(state: &AppState) -> Result<()> {
    let uid = configured_uid(state)
        .await
        .context("Bilibili UID could not be read")?
        .context("Bilibili UID is not configured")?;
    let mut items = fetch_follow_list(state, &uid, 2, "watching").await?;
    items.extend(fetch_follow_list(state, &uid, 3, "finished").await?);
    let mut media_ids = HashSet::with_capacity(items.len());
    if items.iter().any(|item| !media_ids.insert(item.media_id)) {
        bail!("Bilibili follow-list returned duplicate media IDs");
    }

    let existing = sqlx::query_as::<_, ExistingCover>(
        "SELECT bilibili_media_id, cover_key, metadata #>> '{source_cover}' AS source_cover FROM bangumi",
    )
    .fetch_all(state.pool())
    .await?
    .into_iter()
    .map(|row| (row.bilibili_media_id, row))
    .collect::<HashMap<_, _>>();

    for item in &mut items {
        let previous = existing.get(&item.media_id);
        if previous.is_some_and(|row| {
            row.cover_key.is_some() && row.source_cover.as_deref() == Some(item.cover.as_str())
        }) {
            item.cover_key = previous.and_then(|row| row.cover_key.clone());
            continue;
        }
        match cache_cover(state, item.media_id, &item.cover).await {
            Ok(key) => item.cover_key = Some(key),
            Err(cover_error) => {
                warn!(
                    error = %cover_error,
                    media_id = item.media_id,
                    "Bilibili cover could not be cached"
                );
                item.cover_key = previous.and_then(|row| row.cover_key.clone());
                if item.cover_key.is_some()
                    && let Some(previous_source) = previous.and_then(|row| row.source_cover.clone())
                    && let Some(metadata) = item.metadata.as_object_mut()
                {
                    // Keep retrying a changed source URL on later syncs while serving the
                    // last successfully cached cover in the meantime.
                    metadata.insert("source_cover".to_owned(), json!(previous_source));
                }
            }
        }
    }

    let mut transaction = state.pool().begin().await?;
    let current_uid: Option<String> = sqlx::query_scalar(
        "SELECT NULLIF(btrim(settings #>> '{bangumi_sync,uid}'), '') FROM site_settings WHERE id = 1 FOR SHARE",
    )
    .fetch_optional(&mut *transaction)
    .await?
    .flatten();
    if current_uid.as_deref() != Some(uid.as_str()) {
        transaction.rollback().await?;
        bail!("Bilibili UID changed while a sync was running");
    }

    let synced_at = Utc::now();
    for item in &items {
        sqlx::query(
            "INSERT INTO bangumi (
                 bilibili_media_id, season_id, title, cover_key, status,
                 ep_current, ep_total, sort_order, synced_at, metadata
             )
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
             ON CONFLICT (bilibili_media_id) DO UPDATE
             SET season_id = EXCLUDED.season_id,
                 title = EXCLUDED.title,
                 cover_key = EXCLUDED.cover_key,
                 status = EXCLUDED.status,
                 ep_current = EXCLUDED.ep_current,
                 ep_total = EXCLUDED.ep_total,
                 sort_order = EXCLUDED.sort_order,
                 synced_at = EXCLUDED.synced_at,
                 metadata = EXCLUDED.metadata",
        )
        .bind(item.media_id)
        .bind(item.season_id)
        .bind(&item.title)
        .bind(&item.cover_key)
        .bind(item.status)
        .bind(item.ep_current)
        .bind(item.ep_total)
        .bind(item.sort_order)
        .bind(synced_at)
        .bind(&item.metadata)
        .execute(&mut *transaction)
        .await?;
        if let Some(cover_key) = &item.cover_key {
            // A UID change or unfollow may already have queued this deterministic key.
            // Once it is referenced again it must no longer be garbage-collected.
            sqlx::query("DELETE FROM storage_gc_jobs WHERE object_key = $1")
                .bind(cover_key)
                .execute(&mut *transaction)
                .await?;
        }
        if let Some(previous_key) = existing
            .get(&item.media_id)
            .and_then(|row| row.cover_key.as_deref())
            .filter(|previous_key| Some(*previous_key) != item.cover_key.as_deref())
        {
            storage_gc::enqueue(&mut transaction, previous_key, "bangumi_cover_replaced").await?;
        }
    }

    let media_ids = items.iter().map(|item| item.media_id).collect::<Vec<_>>();
    let removed_cover_keys = sqlx::query_scalar::<_, Option<String>>(
        "DELETE FROM bangumi
         WHERE NOT (bilibili_media_id = ANY($1))
         RETURNING cover_key",
    )
    .bind(&media_ids)
    .fetch_all(&mut *transaction)
    .await?;
    for cover_key in removed_cover_keys.into_iter().flatten() {
        storage_gc::enqueue(&mut transaction, &cover_key, "bangumi_unfollowed").await?;
    }

    let watching = items
        .iter()
        .filter(|item| item.status == "watching")
        .count() as i64;
    let finished = items
        .iter()
        .filter(|item| item.status == "finished")
        .count() as i64;
    let counts = json!({ "watching": watching, "finished": finished });
    sqlx::query(
        "UPDATE site_settings
         SET settings = jsonb_set(
             jsonb_set(
                 jsonb_set(settings, '{bangumi_sync,last_sync_at}', to_jsonb($1::timestamptz), true),
                 '{bangumi_sync,last_status}', to_jsonb('ok'::text), true
             ),
             '{bangumi_sync,last_counts}', $2::jsonb, true
         ), updated_at = now()
         WHERE id = 1",
    )
    .bind(synced_at)
    .bind(counts)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    info!(
        uid,
        watching, finished, "Bilibili bangumi mirror synchronized"
    );
    Ok(())
}

#[derive(Debug, FromRow)]
struct ExistingCover {
    bilibili_media_id: i64,
    cover_key: Option<String>,
    source_cover: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BilibiliEnvelope {
    code: i64,
    message: String,
    data: Option<BilibiliPage>,
}

#[derive(Debug, Deserialize)]
struct BilibiliPage {
    #[serde(default)]
    list: Option<Vec<BilibiliItem>>,
    total: i64,
}

#[derive(Debug, Deserialize)]
struct BilibiliItem {
    media_id: i64,
    season_id: i64,
    title: String,
    cover: String,
    #[serde(default)]
    total_count: i32,
    #[serde(default)]
    progress: String,
    #[serde(default)]
    season_type_name: String,
    #[serde(default)]
    evaluate: String,
    rating: Option<BilibiliRating>,
    new_ep: Option<BilibiliEpisode>,
    #[serde(default)]
    url: String,
    #[serde(default)]
    badge: String,
}

#[derive(Debug, Deserialize)]
struct BilibiliRating {
    score: f64,
}

#[derive(Debug, Deserialize)]
struct BilibiliEpisode {
    #[serde(default)]
    index_show: String,
}

async fn fetch_follow_list(
    state: &AppState,
    uid: &str,
    follow_status: i32,
    status: &'static str,
) -> Result<Vec<SyncedBangumi>> {
    let mut page = 1_i64;
    let mut output = Vec::new();
    loop {
        let response = state
            .http_client()
            .get(BILIBILI_FOLLOW_API)
            .header(header::USER_AGENT, "helt-blog/0.1 (+https://github.com/)")
            .header(
                header::REFERER,
                format!("https://space.bilibili.com/{uid}/bangumi"),
            )
            .query(&[
                ("type", "1".to_owned()),
                ("follow_status", follow_status.to_string()),
                ("pn", page.to_string()),
                ("ps", BILIBILI_PAGE_SIZE.to_string()),
                ("vmid", uid.to_owned()),
            ])
            .send()
            .await
            .context("Bilibili follow-list request failed")?;
        let http_status = response.status();
        if !http_status.is_success() {
            bail!("Bilibili follow-list returned HTTP {http_status}");
        }
        let envelope = response
            .json::<BilibiliEnvelope>()
            .await
            .context("Bilibili follow-list response was invalid")?;
        if envelope.code != 0 {
            bail!(
                "Bilibili rejected UID {uid}: {} ({})",
                envelope.message,
                envelope.code
            );
        }
        let data = envelope
            .data
            .context("Bilibili follow-list response had no data")?;
        let total = data.total.max(0) as usize;
        let page_items = data.list.unwrap_or_default();
        let page_len = page_items.len();
        for (index, item) in page_items.into_iter().enumerate() {
            let cover = normalized_cover_url(&item.cover);
            let latest_episode = item
                .new_ep
                .as_ref()
                .map(|episode| episode.index_show.clone())
                .unwrap_or_default();
            let score = item.rating.as_ref().map(|rating| rating.score);
            let fallback_url =
                format!("https://www.bilibili.com/bangumi/play/ss{}", item.season_id);
            let url = validated_bilibili_page_url(&item.url)
                .map(|url| url.to_string())
                .unwrap_or(fallback_url);
            output.push(SyncedBangumi {
                media_id: item.media_id,
                season_id: item.season_id,
                title: item.title,
                cover: cover.clone(),
                status,
                ep_current: episode_number(&item.progress),
                ep_total: item.total_count.max(0),
                sort_order: ((page - 1) * BILIBILI_PAGE_SIZE + index as i64) as i32,
                metadata: json!({
                    "season_type": item.season_type_name,
                    "summary": item.evaluate,
                    "score": score,
                    "url": url,
                    "latest_episode": latest_episode,
                    "badge": item.badge,
                    "source_cover": cover,
                }),
                cover_key: None,
            });
        }
        if output.len() >= total {
            break;
        }
        if page_len == 0 || page_len < BILIBILI_PAGE_SIZE as usize {
            bail!(
                "Bilibili follow-list ended after {} of {total} items",
                output.len()
            );
        }
        if page >= 200 {
            bail!("Bilibili follow-list exceeded the 200-page safety limit");
        }
        page += 1;
    }
    Ok(output)
}

fn normalized_cover_url(raw: &str) -> String {
    raw.trim().replacen("http://", "https://", 1)
}

fn validated_bilibili_cover_url(raw: &str) -> Option<Url> {
    let url = Url::parse(&normalized_cover_url(raw)).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    (url.scheme() == "https" && (host == "hdslb.com" || host.ends_with(".hdslb.com")))
        .then_some(url)
}

fn validated_bilibili_page_url(raw: &str) -> Option<Url> {
    let url = Url::parse(raw.trim()).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    (url.scheme() == "https" && (host == "bilibili.com" || host.ends_with(".bilibili.com")))
        .then_some(url)
}

fn episode_number(progress: &str) -> i32 {
    progress
        .split(|character: char| !character.is_ascii_digit())
        .find(|part| !part.is_empty())
        .and_then(|part| part.parse().ok())
        .unwrap_or(0)
}

async fn cache_cover(state: &AppState, media_id: i64, source: &str) -> Result<String> {
    let url = validated_bilibili_cover_url(source)
        .context("Bilibili cover URL used an unexpected host")?;
    let response = state
        .http_client()
        .get(url)
        .header(header::USER_AGENT, "helt-blog/0.1")
        .header(header::REFERER, "https://www.bilibili.com/")
        .send()
        .await
        .context("Bilibili cover download failed")?;
    if !response.status().is_success() {
        bail!("Bilibili cover returned HTTP {}", response.status());
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_COVER_BYTES)
    {
        bail!("Bilibili cover exceeded 5 MB");
    }
    let bytes = response
        .bytes()
        .await
        .context("Bilibili cover body could not be read")?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_COVER_BYTES {
        bail!("Bilibili cover size was invalid");
    }
    let content_type = raster_image_content_type(&bytes)
        .context("Bilibili cover was not a supported raster image")?;
    let object_key = format!("bangumi/covers/{media_id}/{}", Uuid::now_v7());
    state
        .object_storage()
        .put_public_object(
            state.storage_http_client(),
            &object_key,
            content_type,
            bytes.to_vec(),
        )
        .await
        .context("Bilibili cover could not be stored")?;
    if let Err(staging_error) = sqlx::query(
        "INSERT INTO storage_gc_jobs (object_key, reason, next_attempt_at)
         VALUES ($1, 'bangumi_cover_uncommitted', now() + interval '1 hour')
         ON CONFLICT (object_key) DO UPDATE
         SET reason = EXCLUDED.reason,
             next_attempt_at = EXCLUDED.next_attempt_at,
             locked_at = NULL",
    )
    .bind(&object_key)
    .execute(state.pool())
    .await
    {
        if let Err(cleanup_error) = state
            .object_storage()
            .delete_public_object(state.storage_http_client(), &object_key)
            .await
        {
            warn!(
                object_key,
                error = %cleanup_error,
                "untracked staged Bilibili cover could not be removed"
            );
        }
        return Err(staging_error).context("Bilibili cover cleanup could not be staged");
    }
    Ok(object_key)
}

fn raster_image_content_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("image/webp")
    } else {
        None
    }
}

async fn record_sync_failure(state: &AppState, sync_error: &anyhow::Error) {
    let message = sync_error.to_string();
    if let Err(database_error) = sqlx::query(
        "UPDATE site_settings
         SET settings = jsonb_set(settings, '{bangumi_sync,last_status}', to_jsonb(left($1, 500)::text), true),
             updated_at = now()
         WHERE id = 1",
    )
    .bind(message)
    .execute(state.pool())
    .await
    {
        warn!(error = %database_error, "Bilibili sync failure status could not be stored");
    }
}

#[derive(Debug)]
enum BangumiError {
    Validation(&'static str),
    Unauthorized,
    Conflict,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for BangumiError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl IntoResponse for BangumiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Validation(message) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_error",
                message.to_owned(),
            ),
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
                "需要有效的管理员会话".to_owned(),
            ),
            Self::Conflict => (
                StatusCode::CONFLICT,
                "bangumi_sync_running",
                "追番同步任务正在运行".to_owned(),
            ),
            Self::Database(database_error) => {
                error!(error = %database_error, "bangumi database operation failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "服务器内部错误".to_owned(),
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
        episode_number, normalized_cover_url, pagination_offset, raster_image_content_type,
        validated_bilibili_cover_url, validated_bilibili_page_url,
    };

    #[test]
    fn normalizes_cover_urls_and_extracts_progress() {
        assert_eq!(
            normalized_cover_url("http://i0.hdslb.com/a.jpg"),
            "https://i0.hdslb.com/a.jpg"
        );
        assert_eq!(episode_number("看到第12话"), 12);
        assert_eq!(episode_number(""), 0);
    }

    #[test]
    fn rejects_untrusted_public_media_urls_and_oversized_pages() {
        assert!(validated_bilibili_cover_url("https://i0.hdslb.com/a.jpg").is_some());
        assert!(validated_bilibili_cover_url("https://example.com/a.jpg").is_none());
        assert!(validated_bilibili_page_url("https://www.bilibili.com/bangumi/play/ss1").is_some());
        assert!(validated_bilibili_page_url("javascript:alert(1)").is_none());
        assert!(validated_bilibili_page_url("https://bilibili.com.example.org/").is_none());
        assert_eq!(
            raster_image_content_type(b"\xff\xd8\xffbody"),
            Some("image/jpeg")
        );
        assert_eq!(
            raster_image_content_type(b"\x89PNG\r\n\x1a\nbody"),
            Some("image/png")
        );
        assert_eq!(raster_image_content_type(b"not an image"), None);
        assert_eq!(pagination_offset(2, 8), Some(8));
        assert_eq!(pagination_offset(i64::MAX, 100), None);
    }
}
