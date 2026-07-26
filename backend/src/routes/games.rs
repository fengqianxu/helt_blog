use std::{collections::HashSet, time::Duration};

use anyhow::{Context, Result, bail};
use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::FromRow;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    error::{ErrorBody, ErrorEnvelope},
    routes::contract::HttpMethod,
    state::AppState,
};

const STEAM_OWNED_GAMES_API: &str =
    "https://api.steampowered.com/IPlayerService/GetOwnedGames/v0001/";
const MAX_PUBLIC_PAGE_SIZE: i64 = 100;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/v1/games", get(list_games))
}

pub fn implements(method: HttpMethod, path: &str) -> bool {
    matches!((method, path), (HttpMethod::Get, "/api/v1/games"))
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    status: Option<String>,
    recent: Option<bool>,
    sort: Option<String>,
    page: Option<i64>,
    per_page: Option<i64>,
}

#[derive(Debug, FromRow)]
struct GameRow {
    id: i64,
    steam_app_id: i64,
    title: String,
    status: String,
    icon_hash: String,
    playtime_2weeks_minutes: i32,
    playtime_forever_minutes: i32,
    playtime_windows_minutes: i32,
    playtime_mac_minutes: i32,
    playtime_linux_minutes: i32,
    last_played_at: Option<DateTime<Utc>>,
    synced_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct GameItem {
    id: i64,
    steam_app_id: i64,
    title: String,
    status: String,
    cover_url: String,
    icon_url: Option<String>,
    playtime_2weeks_minutes: i32,
    playtime_forever_minutes: i32,
    playtime_windows_minutes: i32,
    playtime_mac_minutes: i32,
    playtime_linux_minutes: i32,
    last_played_at: Option<DateTime<Utc>>,
    synced_at: DateTime<Utc>,
    steam_url: String,
}

#[derive(Debug, Serialize)]
struct GameCounts {
    total: i64,
    recent: i64,
}

#[derive(Debug, Serialize)]
struct GameMeta {
    counts: GameCounts,
    synced_at: Option<DateTime<Utc>>,
    configured: bool,
    sync_status: String,
}

#[derive(Debug, Serialize)]
struct GameListResponse {
    page: i64,
    per_page: i64,
    total: i64,
    items: Vec<GameItem>,
    meta: GameMeta,
}

async fn list_games(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<GameListResponse>, GameError> {
    let status = match query.status.as_deref() {
        None | Some("") => None,
        Some("playing") => Some("playing"),
        Some("finished") => Some("finished"),
        Some("shelved") => Some("shelved"),
        Some(_) => {
            return Err(GameError::Validation(
                "游戏状态只能是 playing、finished 或 shelved",
            ));
        }
    };
    let recent_only = query.recent.unwrap_or(false);
    let sort = match query.sort.as_deref() {
        None | Some("") | Some("recent") => "recent",
        Some("playtime") => "playtime",
        Some(_) => {
            return Err(GameError::Validation("排序方式只能是 recent 或 playtime"));
        }
    };
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(10);
    if page < 1 {
        return Err(GameError::Validation("page 必须大于等于 1"));
    }
    if !(1..=MAX_PUBLIC_PAGE_SIZE).contains(&per_page) {
        return Err(GameError::Validation("per_page 必须在 1 到 100 之间"));
    }
    let offset = pagination_offset(page, per_page).ok_or(GameError::Validation("page 数值过大"))?;

    let (total, library_total, recent, synced_at, configured, sync_status): (
        i64,
        i64,
        i64,
        Option<DateTime<Utc>>,
        bool,
        String,
    ) = sqlx::query_as(
        "SELECT
             COUNT(*) FILTER (
                 WHERE steam_app_id IS NOT NULL
                   AND ($1::text IS NULL OR status = $1)
                   AND ($2::bool IS NOT TRUE OR playtime_2weeks_minutes > 0)
             ),
             COUNT(*) FILTER (WHERE steam_app_id IS NOT NULL),
             COUNT(*) FILTER (WHERE steam_app_id IS NOT NULL AND playtime_2weeks_minutes > 0),
             MAX(synced_at) FILTER (WHERE steam_app_id IS NOT NULL),
             COALESCE((
                 SELECT steam_web_api_key_ciphertext IS NOT NULL
                    AND COALESCE(btrim(settings #>> '{steam_sync,steam_id64}'), '') <> ''
                 FROM site_settings WHERE id = 1
             ), false),
             COALESCE((
                 SELECT CASE
                     WHEN steam_web_api_key_ciphertext IS NULL
                       OR COALESCE(btrim(settings #>> '{steam_sync,steam_id64}'), '') = ''
                         THEN 'disabled'
                     WHEN settings #>> '{steam_sync,last_status}' IN ('ok','queued','disabled')
                         THEN settings #>> '{steam_sync,last_status}'
                     WHEN settings #>> '{steam_sync,last_status}' IS NULL
                         THEN 'queued'
                     ELSE 'error'
                 END
                 FROM site_settings WHERE id = 1
             ), 'disabled')
         FROM games",
    )
    .bind(status)
    .bind(recent_only)
    .fetch_one(state.pool())
    .await?;

    let rows = sqlx::query_as::<_, GameRow>(
        "SELECT id, steam_app_id, title, status, icon_hash,
                playtime_2weeks_minutes, playtime_forever_minutes,
                playtime_windows_minutes, playtime_mac_minutes, playtime_linux_minutes,
                last_played_at, synced_at
         FROM games
         WHERE steam_app_id IS NOT NULL
           AND ($1::text IS NULL OR status = $1)
           AND ($2::bool IS NOT TRUE OR playtime_2weeks_minutes > 0)
         ORDER BY
             CASE WHEN $3::text = 'playtime' THEN playtime_forever_minutes END DESC,
             CASE WHEN $3::text = 'recent' AND playtime_2weeks_minutes > 0 THEN 0 ELSE 1 END,
             CASE WHEN $3::text = 'recent' THEN playtime_2weeks_minutes END DESC,
             CASE WHEN $3::text = 'recent' THEN last_played_at END DESC NULLS LAST,
             playtime_forever_minutes DESC,
             steam_app_id
         LIMIT $4 OFFSET $5",
    )
    .bind(status)
    .bind(recent_only)
    .bind(sort)
    .bind(per_page)
    .bind(offset)
    .fetch_all(state.pool())
    .await?;

    Ok(Json(GameListResponse {
        page,
        per_page,
        total,
        items: rows.into_iter().map(game_item).collect(),
        meta: GameMeta {
            counts: GameCounts {
                total: library_total,
                recent,
            },
            synced_at,
            configured,
            sync_status,
        },
    }))
}

fn pagination_offset(page: i64, per_page: i64) -> Option<i64> {
    (page - 1).checked_mul(per_page)
}

fn game_item(row: GameRow) -> GameItem {
    let app_id = row.steam_app_id;
    GameItem {
        id: row.id,
        steam_app_id: app_id,
        title: row.title,
        status: row.status,
        cover_url: format!(
            "https://shared.cloudflare.steamstatic.com/store_item_assets/steam/apps/{app_id}/library_600x900.jpg"
        ),
        icon_url: (!row.icon_hash.is_empty()).then(|| {
            format!(
                "https://media.steampowered.com/steamcommunity/public/images/apps/{app_id}/{}.jpg",
                row.icon_hash
            )
        }),
        playtime_2weeks_minutes: row.playtime_2weeks_minutes,
        playtime_forever_minutes: row.playtime_forever_minutes,
        playtime_windows_minutes: row.playtime_windows_minutes,
        playtime_mac_minutes: row.playtime_mac_minutes,
        playtime_linux_minutes: row.playtime_linux_minutes,
        last_played_at: row.last_played_at,
        synced_at: row.synced_at,
        steam_url: format!("https://store.steampowered.com/app/{app_id}"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SteamCredentials {
    web_api_key: String,
    steam_id64: String,
    ciphertext: Vec<u8>,
    nonce: Vec<u8>,
    key_version: i32,
}

#[derive(Debug, FromRow)]
struct SteamCredentialRow {
    steam_web_api_key_ciphertext: Option<Vec<u8>>,
    steam_web_api_key_nonce: Option<Vec<u8>>,
    steam_encryption_key_version: Option<i32>,
    steam_id64: String,
}

async fn configured_credentials(state: &AppState) -> Result<Option<SteamCredentials>> {
    let credentials = sqlx::query_as::<_, SteamCredentialRow>(
        "SELECT
             steam_web_api_key_ciphertext,
             steam_web_api_key_nonce,
             steam_encryption_key_version,
             COALESCE(settings #>> '{steam_sync,steam_id64}', '') AS steam_id64
         FROM site_settings WHERE id = 1",
    )
    .fetch_optional(state.pool())
    .await?;
    let Some(credentials) = credentials else {
        return Ok(None);
    };
    let Some(web_api_key) = state
        .llm_keyring()
        .decrypt_optional(
            credentials.steam_encryption_key_version,
            credentials.steam_web_api_key_ciphertext.as_deref(),
            credentials.steam_web_api_key_nonce.as_deref(),
        )
        .context("Steam Web API key could not be decrypted")?
    else {
        return Ok(None);
    };
    let steam_id64 = credentials.steam_id64.trim().to_owned();
    if web_api_key.trim().is_empty() || steam_id64.is_empty() {
        return Ok(None);
    }
    Ok(Some(SteamCredentials {
        web_api_key,
        steam_id64,
        ciphertext: credentials
            .steam_web_api_key_ciphertext
            .expect("decrypted Steam key has ciphertext"),
        nonce: credentials
            .steam_web_api_key_nonce
            .expect("decrypted Steam key has nonce"),
        key_version: credentials
            .steam_encryption_key_version
            .expect("decrypted Steam key has a version"),
    }))
}

async fn sync_interval_hours(state: &AppState) -> Result<i64, sqlx::Error> {
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT settings #>> '{steam_sync,interval_hours}' FROM site_settings WHERE id = 1",
    )
    .fetch_optional(state.pool())
    .await?
    .flatten();
    Ok(raw
        .and_then(|value| value.parse().ok())
        .unwrap_or(6)
        .clamp(1, 168))
}

pub fn trigger_sync(state: AppState) -> Option<Uuid> {
    if !state.try_begin_steam_sync() {
        return None;
    }
    let job_id = Uuid::now_v7();
    tokio::spawn(async move {
        let result = sync_configured(&state).await;
        let credentials_changed = result
            .as_ref()
            .err()
            .is_some_and(|sync_error| sync_error.to_string().contains("credentials changed"));
        if let Err(sync_error) = &result {
            if credentials_changed {
                info!("Steam credentials changed during sync; scheduling the current profile");
            } else {
                error!(error = %sync_error, "Steam game sync failed");
                record_sync_failure(&state, sync_error).await;
            }
        }
        state.finish_steam_sync();
        if credentials_changed {
            let _ = trigger_sync(state);
        }
    });
    Some(job_id)
}

pub async fn run_scheduler(state: AppState) {
    loop {
        match configured_credentials(&state).await {
            Ok(Some(_)) => {
                let _ = trigger_sync(state.clone());
            }
            Ok(None) => {}
            Err(load_error) => warn!(error = %load_error, "Steam credentials could not be loaded"),
        }
        let hours = sync_interval_hours(&state).await.unwrap_or(6);
        tokio::time::sleep(Duration::from_secs(hours as u64 * 60 * 60)).await;
    }
}

#[derive(Debug, Deserialize)]
struct SteamEnvelope {
    response: SteamOwnedGames,
}

#[derive(Debug, Deserialize)]
struct SteamOwnedGames {
    game_count: Option<i64>,
    games: Option<Vec<SteamGame>>,
}

#[derive(Debug, Deserialize)]
struct SteamGame {
    appid: i64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    img_icon_url: String,
    #[serde(default)]
    playtime_2weeks: i32,
    #[serde(default)]
    playtime_forever: i32,
    #[serde(default)]
    playtime_windows_forever: i32,
    #[serde(default)]
    playtime_mac_forever: i32,
    #[serde(default)]
    playtime_linux_forever: i32,
    #[serde(default)]
    rtime_last_played: i64,
}

async fn fetch_owned_games(
    state: &AppState,
    credentials: &SteamCredentials,
) -> Result<Vec<SteamGame>> {
    let response = state
        .http_client()
        .get(STEAM_OWNED_GAMES_API)
        .query(&[
            ("key", credentials.web_api_key.as_str()),
            ("steamid", credentials.steam_id64.as_str()),
            ("include_appinfo", "true"),
            ("include_played_free_games", "true"),
            ("format", "json"),
        ])
        .send()
        .await
        .context("Steam 游戏库请求失败")?;
    let status = response.status();
    if !status.is_success() {
        bail!("Steam Web API 返回 HTTP {status}，请检查 API Key 与 SteamID64");
    }
    let envelope = response
        .json::<SteamEnvelope>()
        .await
        .context("Steam 游戏库响应格式无效")?;
    validated_owned_games(envelope.response)
}

fn validated_owned_games(response: SteamOwnedGames) -> Result<Vec<SteamGame>> {
    let declared_count = response
        .game_count
        .context("Steam 未返回游戏数量，请确认“游戏详情”已设为公开")?;
    if declared_count < 0 {
        bail!("Steam 返回了无效的游戏数量");
    }
    let games = response.games.unwrap_or_default();
    if games.len() as i64 != declared_count {
        bail!(
            "Steam 游戏库不完整：声明 {declared_count} 款，实际返回 {} 款",
            games.len()
        );
    }
    let mut app_ids = HashSet::with_capacity(games.len());
    if games.iter().any(|game| !app_ids.insert(game.appid)) {
        bail!("Steam 游戏库返回了重复的 AppID");
    }
    Ok(games)
}

async fn sync_configured(state: &AppState) -> Result<()> {
    let credentials = configured_credentials(state)
        .await
        .context("Steam credentials could not be read")?
        .context("Steam credentials are not configured")?;
    let mut games = fetch_owned_games(state, &credentials).await?;
    games.retain(|game| game.appid > 0);
    games.sort_by(|left, right| {
        right
            .playtime_2weeks
            .cmp(&left.playtime_2weeks)
            .then_with(|| right.rtime_last_played.cmp(&left.rtime_last_played))
            .then_with(|| right.playtime_forever.cmp(&left.playtime_forever))
            .then_with(|| left.appid.cmp(&right.appid))
    });

    let mut transaction = state.pool().begin().await?;
    let current = sqlx::query_as::<_, SteamCredentialRow>(
        "SELECT
             steam_web_api_key_ciphertext,
             steam_web_api_key_nonce,
             steam_encryption_key_version,
             COALESCE(settings #>> '{steam_sync,steam_id64}', '') AS steam_id64
         FROM site_settings WHERE id = 1 FOR SHARE",
    )
    .fetch_optional(&mut *transaction)
    .await?;
    let credentials_unchanged = current
        .as_ref()
        .is_some_and(|current| {
            current.steam_web_api_key_ciphertext.as_deref()
                == Some(credentials.ciphertext.as_slice())
                && current.steam_web_api_key_nonce.as_deref() == Some(credentials.nonce.as_slice())
                && current.steam_encryption_key_version == Some(credentials.key_version)
                && current.steam_id64.trim() == credentials.steam_id64
        });
    if !credentials_unchanged {
        transaction.rollback().await?;
        bail!("Steam credentials changed while a sync was running");
    }

    let synced_at = Utc::now();
    for (sort_order, game) in games.iter().enumerate() {
        let title = if game.name.trim().is_empty() {
            format!("Steam App {}", game.appid)
        } else {
            game.name.trim().to_owned()
        };
        let recent_minutes = game.playtime_2weeks.max(0);
        let total_minutes = game.playtime_forever.max(0);
        let last_played_at = (game.rtime_last_played > 0)
            .then(|| DateTime::from_timestamp(game.rtime_last_played, 0))
            .flatten();
        sqlx::query(
            "INSERT INTO games (
                 title, status, play_hours, comment, sort_order,
                 steam_app_id, icon_hash, playtime_2weeks_minutes,
                 playtime_forever_minutes, playtime_windows_minutes,
                 playtime_mac_minutes, playtime_linux_minutes,
                 last_played_at, synced_at
             )
             VALUES ($1,$2,$3,'',$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
             ON CONFLICT (steam_app_id) WHERE steam_app_id IS NOT NULL DO UPDATE
             SET title = EXCLUDED.title,
                 status = EXCLUDED.status,
                 play_hours = EXCLUDED.play_hours,
                 sort_order = EXCLUDED.sort_order,
                 icon_hash = EXCLUDED.icon_hash,
                 playtime_2weeks_minutes = EXCLUDED.playtime_2weeks_minutes,
                 playtime_forever_minutes = EXCLUDED.playtime_forever_minutes,
                 playtime_windows_minutes = EXCLUDED.playtime_windows_minutes,
                 playtime_mac_minutes = EXCLUDED.playtime_mac_minutes,
                 playtime_linux_minutes = EXCLUDED.playtime_linux_minutes,
                 last_played_at = EXCLUDED.last_played_at,
                 synced_at = EXCLUDED.synced_at",
        )
        .bind(&title)
        .bind(steam_library_status(total_minutes))
        .bind(total_minutes as f32 / 60.0)
        .bind(sort_order as i32)
        .bind(game.appid)
        .bind(&game.img_icon_url)
        .bind(recent_minutes)
        .bind(total_minutes)
        .bind(game.playtime_windows_forever.max(0))
        .bind(game.playtime_mac_forever.max(0))
        .bind(game.playtime_linux_forever.max(0))
        .bind(last_played_at)
        .bind(synced_at)
        .execute(&mut *transaction)
        .await?;
    }

    let app_ids = games.iter().map(|game| game.appid).collect::<Vec<_>>();
    sqlx::query(
        "DELETE FROM games
         WHERE steam_app_id IS NOT NULL AND NOT (steam_app_id = ANY($1))",
    )
    .bind(&app_ids)
    .execute(&mut *transaction)
    .await?;

    let total = games.len() as i64;
    let recent = games.iter().filter(|game| game.playtime_2weeks > 0).count() as i64;
    sqlx::query(
        "UPDATE site_settings
         SET settings = jsonb_set(
             jsonb_set(
                 jsonb_set(settings, '{steam_sync,last_sync_at}', to_jsonb($1::timestamptz), true),
                 '{steam_sync,last_status}', to_jsonb('ok'::text), true
             ),
             '{steam_sync,last_counts}', $2::jsonb, true
         ), updated_at = now()
         WHERE id = 1",
    )
    .bind(synced_at)
    .bind(json!({ "total": total, "recent": recent }))
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    info!(steam_id64 = %credentials.steam_id64, total, recent, "Steam game mirror synchronized");
    Ok(())
}

fn steam_library_status(total_minutes: i32) -> &'static str {
    if total_minutes > 0 {
        "playing"
    } else {
        "shelved"
    }
}

async fn record_sync_failure(state: &AppState, sync_error: &anyhow::Error) {
    let message = sync_error.to_string();
    if let Err(database_error) = sqlx::query(
        "UPDATE site_settings
         SET settings = jsonb_set(settings, '{steam_sync,last_status}', to_jsonb(left($1, 500)::text), true),
             updated_at = now()
         WHERE id = 1",
    )
    .bind(message)
    .execute(state.pool())
    .await
    {
        warn!(error = %database_error, "Steam sync failure status could not be stored");
    }
}

#[derive(Debug)]
enum GameError {
    Validation(&'static str),
    Database(sqlx::Error),
}

impl From<sqlx::Error> for GameError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl IntoResponse for GameError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Validation(message) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_error",
                message.to_owned(),
            ),
            Self::Database(database_error) => {
                error!(error = %database_error, "Steam game database operation failed");
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
        SteamEnvelope, game_item, pagination_offset, steam_library_status, validated_owned_games,
    };

    #[test]
    fn steam_owned_games_payload_accepts_optional_recent_playtime() {
        let payload: SteamEnvelope = serde_json::from_value(serde_json::json!({
            "response": {
                "game_count": 1,
                "games": [{
                    "appid": 367520,
                    "name": "Hollow Knight",
                    "img_icon_url": "icon-hash",
                    "playtime_forever": 601,
                    "rtime_last_played": 1_700_000_000
                }]
            }
        }))
        .unwrap();
        let game = &payload.response.games.unwrap()[0];
        assert_eq!(game.playtime_2weeks, 0);
        assert_eq!(game.playtime_forever, 601);
    }

    #[test]
    fn rejects_incomplete_or_duplicate_steam_libraries() {
        let incomplete: SteamEnvelope = serde_json::from_value(serde_json::json!({
            "response": { "game_count": 1 }
        }))
        .unwrap();
        assert!(validated_owned_games(incomplete.response).is_err());

        let duplicate: SteamEnvelope = serde_json::from_value(serde_json::json!({
            "response": {
                "game_count": 2,
                "games": [{ "appid": 10 }, { "appid": 10 }]
            }
        }))
        .unwrap();
        assert!(validated_owned_games(duplicate.response).is_err());
        assert_eq!(pagination_offset(2, 8), Some(8));
        assert_eq!(pagination_offset(i64::MAX, 100), None);
    }

    #[test]
    fn inactivity_never_marks_a_steam_game_finished() {
        assert_eq!(steam_library_status(600), "playing");
        assert_eq!(steam_library_status(0), "shelved");
        assert_ne!(steam_library_status(600), "finished");
        assert_ne!(steam_library_status(0), "finished");
    }

    #[test]
    fn game_item_uses_https_steam_links() {
        let item = game_item(super::GameRow {
            id: 1,
            steam_app_id: 367520,
            title: "Hollow Knight".to_owned(),
            status: "playing".to_owned(),
            icon_hash: "abc".to_owned(),
            playtime_2weeks_minutes: 10,
            playtime_forever_minutes: 600,
            playtime_windows_minutes: 600,
            playtime_mac_minutes: 0,
            playtime_linux_minutes: 0,
            last_played_at: None,
            synced_at: chrono::Utc::now(),
        });
        assert!(item.cover_url.starts_with("https://"));
        assert_eq!(item.steam_url, "https://store.steampowered.com/app/367520");
    }
}
