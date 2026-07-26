use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use std::collections::HashSet;

use chrono::{DateTime, NaiveTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, Postgres, Transaction};
use tracing::error;
use uuid::Uuid;

use crate::{
    auth,
    error::{ErrorBody, ErrorEnvelope},
    routes::contract::HttpMethod,
    state::AppState,
};

const RAIMENT_SELECT: &str = "
    SELECT r.id, r.name, r.cover_asset_id, r.theme, r.enabled,
           r.sort_order, r.is_default,
           r.color_scheme, r.kanban_asset_id, r.cover_title, r.cover_subtitle,
           r.cover_character_name, r.cover_dialogue, r.cover_voice_label,
           r.cover_voice_asset_id, r.login_success_voice_asset_id,
           r.is_builtin, r.revision,
           r.created_at, r.updated_at,
           a.name AS cover_name, u.object_key AS cover_object_key,
           u.mime AS cover_mime, u.size_bytes AS cover_size_bytes,
           u.original_filename AS cover_original_filename,
           voice.name AS voice_name, voice_upload.object_key AS voice_object_key,
           voice_upload.mime AS voice_mime, voice_upload.size_bytes AS voice_size_bytes,
           voice_upload.original_filename AS voice_original_filename,
           success_voice.name AS success_voice_name,
           success_voice_upload.object_key AS success_voice_object_key,
           success_voice_upload.mime AS success_voice_mime,
           success_voice_upload.size_bytes AS success_voice_size_bytes,
           success_voice_upload.original_filename AS success_voice_original_filename
    FROM raiments r
    JOIN assets a ON a.id = r.cover_asset_id
    JOIN uploads u ON u.id = a.upload_id
    LEFT JOIN assets voice ON voice.id = r.cover_voice_asset_id
    LEFT JOIN uploads voice_upload ON voice_upload.id = voice.upload_id
    LEFT JOIN assets success_voice ON success_voice.id = r.login_success_voice_asset_id
    LEFT JOIN uploads success_voice_upload ON success_voice_upload.id = success_voice.upload_id
";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/raiments", get(public_list))
        .route("/api/v1/admin/raiments", get(admin_list).post(admin_create))
        .route(
            "/api/v1/admin/raiments/{id}",
            axum::routing::put(admin_update).delete(admin_delete),
        )
        .route(
            "/api/v1/admin/site/raiment-schedule",
            get(admin_get_schedule).put(admin_update_schedule),
        )
}

pub fn implements(method: HttpMethod, path: &str) -> bool {
    matches!(
        (method, path),
        (HttpMethod::Get, "/api/v1/raiments")
            | (HttpMethod::Get, "/api/v1/admin/raiments")
            | (HttpMethod::Post, "/api/v1/admin/raiments")
            | (HttpMethod::Put, "/api/v1/admin/raiments/{id}")
            | (HttpMethod::Delete, "/api/v1/admin/raiments/{id}")
            | (HttpMethod::Get, "/api/v1/admin/site/raiment-schedule")
            | (HttpMethod::Put, "/api/v1/admin/site/raiment-schedule")
    )
}

#[derive(Debug, thiserror::Error)]
enum RaimentError {
    #[error("需要有效的管理员会话")]
    Unauthorized,
    #[error("灵衣不存在")]
    NotFound,
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Conflict(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("{0}")]
    CorruptData(String),
}

impl RaimentError {
    fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }
}

impl IntoResponse for RaimentError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
                "需要有效的管理员会话".to_owned(),
            ),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found", "灵衣不存在".to_owned()),
            Self::Validation(message) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_error",
                message,
            ),
            Self::Conflict(message) => (StatusCode::CONFLICT, "conflict", message),
            Self::Database(error) => {
                error!(%error, "raiment database operation failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "灵衣操作失败".to_owned(),
                )
            }
            Self::CorruptData(message) => {
                error!(%message, "persisted raiment data is invalid");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "灵衣配置损坏".to_owned(),
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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct ThemeTokens {
    primary: String,
    secondary: String,
    background: String,
    surface: String,
    surface_alt: String,
    text: String,
    text_secondary: String,
    muted: String,
    faint: String,
    border: String,
    danger: String,
    success: String,
}

impl ThemeTokens {
    fn validate(&self) -> Result<(), RaimentError> {
        for (label, value) in [
            ("主色", &self.primary),
            ("辅色", &self.secondary),
            ("背景色", &self.background),
            ("卡片色", &self.surface),
            ("次级卡片色", &self.surface_alt),
            ("正文色", &self.text),
            ("次级正文色", &self.text_secondary),
            ("弱化文字色", &self.muted),
            ("辅助文字色", &self.faint),
            ("边框色", &self.border),
            ("危险色", &self.danger),
            ("成功色", &self.success),
        ] {
            if !is_hex_color(value) {
                return Err(RaimentError::validation(format!(
                    "{label}必须使用 #RRGGBB 格式"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum ColorScheme {
    Day,
    Night,
}

impl ColorScheme {
    fn parse(value: &str) -> Result<Self, RaimentError> {
        match value {
            "day" => Ok(Self::Day),
            "night" => Ok(Self::Night),
            _ => Err(RaimentError::validation("外观基调只能是 day 或 night")),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Night => "night",
        }
    }
}

#[derive(Debug, FromRow)]
struct RaimentRow {
    id: String,
    name: String,
    cover_asset_id: i64,
    theme: Value,
    enabled: bool,
    sort_order: i32,
    is_default: bool,
    color_scheme: String,
    kanban_asset_id: Option<i64>,
    cover_title: String,
    cover_subtitle: String,
    cover_character_name: String,
    cover_dialogue: String,
    cover_voice_label: String,
    cover_voice_asset_id: Option<i64>,
    login_success_voice_asset_id: Option<i64>,
    is_builtin: bool,
    revision: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    cover_name: String,
    cover_object_key: String,
    cover_mime: String,
    cover_size_bytes: i64,
    cover_original_filename: Option<String>,
    voice_name: Option<String>,
    voice_object_key: Option<String>,
    voice_mime: Option<String>,
    voice_size_bytes: Option<i64>,
    voice_original_filename: Option<String>,
    success_voice_name: Option<String>,
    success_voice_object_key: Option<String>,
    success_voice_mime: Option<String>,
    success_voice_size_bytes: Option<i64>,
    success_voice_original_filename: Option<String>,
}

#[derive(Debug, Serialize)]
struct PublicRaiment {
    id: String,
    name: String,
    cover_url: String,
    theme: ThemeTokens,
    color_scheme: ColorScheme,
    cover_title: String,
    cover_subtitle: String,
    cover_character_name: String,
    cover_dialogue: String,
    cover_voice_label: String,
    cover_voice_url: Option<String>,
    login_success_voice_url: Option<String>,
    kanban_configured: bool,
}

#[derive(Debug, Serialize)]
struct AssetFile {
    url: String,
    mime: String,
    size_bytes: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    original_filename: Option<String>,
}

#[derive(Debug, Serialize)]
struct LinkedAsset {
    id: i64,
    name: String,
    media_type: &'static str,
    file: AssetFile,
}

#[derive(Debug, Serialize)]
struct AdminRaiment {
    id: String,
    name: String,
    cover_asset_id: i64,
    cover_asset: LinkedAsset,
    theme: ThemeTokens,
    enabled: bool,
    sort_order: i32,
    is_default: bool,
    color_scheme: ColorScheme,
    cover_title: String,
    cover_subtitle: String,
    cover_character_name: String,
    cover_dialogue: String,
    cover_voice_label: String,
    cover_voice_asset_id: Option<i64>,
    cover_voice_asset: Option<LinkedAsset>,
    login_success_voice_asset_id: Option<i64>,
    login_success_voice_asset: Option<LinkedAsset>,
    kanban_asset_id: Option<i64>,
    is_builtin: bool,
    revision: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateRaimentRequest {
    name: String,
    cover_asset_id: i64,
    theme: ThemeTokens,
    enabled: bool,
    sort_order: i32,
    is_default: bool,
    color_scheme: String,
    cover_title: String,
    cover_subtitle: String,
    cover_character_name: String,
    cover_dialogue: String,
    cover_voice_label: String,
    cover_voice_asset_id: Option<i64>,
    login_success_voice_asset_id: Option<i64>,
    kanban_asset_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateRaimentRequest {
    revision: i64,
    name: String,
    cover_asset_id: i64,
    theme: ThemeTokens,
    enabled: bool,
    sort_order: i32,
    is_default: bool,
    color_scheme: String,
    cover_title: String,
    cover_subtitle: String,
    cover_character_name: String,
    cover_dialogue: String,
    cover_voice_label: String,
    cover_voice_asset_id: Option<i64>,
    login_success_voice_asset_id: Option<i64>,
    kanban_asset_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct SchedulePeriod {
    id: String,
    start_at: String,
    end_at: String,
    raiment_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RaimentSchedule {
    revision: i64,
    periods: Vec<SchedulePeriod>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateScheduleRequest {
    revision: i64,
    periods: Vec<SchedulePeriod>,
}

struct RaimentFields<'a> {
    name: &'a str,
    cover_asset_id: i64,
    theme: &'a ThemeTokens,
    enabled: bool,
    sort_order: i32,
    is_default: bool,
    color_scheme: &'a str,
    cover_title: &'a str,
    cover_subtitle: &'a str,
    cover_character_name: &'a str,
    cover_dialogue: &'a str,
    cover_voice_label: &'a str,
    cover_voice_asset_id: Option<i64>,
    login_success_voice_asset_id: Option<i64>,
    kanban_asset_id: Option<i64>,
}

async fn public_list(State(state): State<AppState>) -> Result<Json<Value>, RaimentError> {
    let rows = fetch_raiments(&state, true).await?;
    let default_raiment_id = rows
        .iter()
        .find(|row| row.is_default)
        .map(|row| row.id.clone())
        .ok_or_else(|| RaimentError::CorruptData("未配置已启用的默认灵衣".to_owned()))?;
    let items = rows
        .into_iter()
        .map(|row| public_item(&state, row))
        .collect::<Result<Vec<_>, _>>()?;
    let schedule = load_schedule(&state).await?;
    Ok(Json(json!({
        "items": items,
        "schedule": schedule,
        "default_raiment_id": default_raiment_id,
    })))
}

async fn admin_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, RaimentError> {
    require_admin(&state, &headers)?;
    let rows = fetch_raiments(&state, false).await?;
    let items = rows
        .into_iter()
        .map(|row| admin_item(&state, row))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(json!({ "items": items })))
}

async fn admin_get_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<RaimentSchedule>, RaimentError> {
    require_admin(&state, &headers)?;
    Ok(Json(load_schedule(&state).await?))
}

async fn admin_update_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut request): Json<UpdateScheduleRequest>,
) -> Result<Json<RaimentSchedule>, RaimentError> {
    require_admin(&state, &headers)?;
    if request.revision < 1 {
        return Err(RaimentError::validation("revision 必须为正整数"));
    }
    normalize_and_validate_schedule(&mut request.periods)?;

    let mut transaction = state.pool().begin().await?;
    lock_site_settings(&mut transaction).await?;
    let referenced_ids = request
        .periods
        .iter()
        .map(|period| period.raiment_id.as_str())
        .collect::<HashSet<_>>();
    let available_ids =
        sqlx::query_scalar::<_, String>("SELECT id FROM raiments WHERE enabled = true FOR SHARE")
            .fetch_all(&mut *transaction)
            .await?
            .into_iter()
            .collect::<HashSet<_>>();
    if let Some(missing) = referenced_ids
        .iter()
        .find(|id| !available_ids.contains(**id))
    {
        return Err(RaimentError::validation(format!(
            "时间段引用了不存在或未启用的灵衣：{missing}"
        )));
    }

    let next = RaimentSchedule {
        revision: next_revision(request.revision)?,
        periods: request.periods,
    };
    let next_json = serde_json::to_value(&next)
        .map_err(|error| RaimentError::CorruptData(error.to_string()))?;
    let changed = sqlx::query(
        "UPDATE site_settings
         SET settings = jsonb_set(settings, '{raiment_schedule}', $1, true)
         WHERE id = 1
           AND COALESCE((settings #>> '{raiment_schedule,revision}')::bigint, 0) = $2",
    )
    .bind(next_json)
    .bind(request.revision)
    .execute(&mut *transaction)
    .await?
    .rows_affected();
    if changed == 0 {
        return Err(RaimentError::Conflict(
            "灵衣时间段已被其他页面更新，请刷新后再保存".to_owned(),
        ));
    }
    transaction.commit().await?;
    Ok(Json(next))
}

async fn admin_create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateRaimentRequest>,
) -> Result<(StatusCode, Json<AdminRaiment>), RaimentError> {
    require_admin(&state, &headers)?;
    let color_scheme = validate_create(&request)?;
    validate_assets(
        &state,
        request.cover_asset_id,
        request.cover_voice_asset_id,
        request.login_success_voice_asset_id,
        request.kanban_asset_id,
    )
    .await?;

    let id = format!("raiment-{}", Uuid::now_v7().simple());
    let theme = serde_json::to_value(&request.theme)
        .map_err(|error| RaimentError::CorruptData(error.to_string()))?;
    let mut transaction = state.pool().begin().await?;
    if request.is_default {
        sqlx::query("SELECT id FROM raiments FOR UPDATE")
            .fetch_all(&mut *transaction)
            .await?;
        sqlx::query(
            "UPDATE raiments
             SET is_default = false, revision = revision + 1
             WHERE is_default = true",
        )
        .execute(&mut *transaction)
        .await?;
    }
    sqlx::query(
        "INSERT INTO raiments (
            id, name, cover_asset_id, theme, enabled, sort_order, is_default, color_scheme,
            cover_title, cover_subtitle, cover_character_name, cover_dialogue,
            cover_voice_label, cover_voice_asset_id, login_success_voice_asset_id,
            kanban_asset_id, is_builtin
         ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
            $11, $12, $13, $14, $15, $16, false
         )",
    )
    .bind(&id)
    .bind(request.name.trim())
    .bind(request.cover_asset_id)
    .bind(theme)
    .bind(request.enabled)
    .bind(request.sort_order)
    .bind(request.is_default)
    .bind(color_scheme.as_str())
    .bind(request.cover_title.trim())
    .bind(request.cover_subtitle.trim())
    .bind(request.cover_character_name.trim())
    .bind(request.cover_dialogue.trim())
    .bind(request.cover_voice_label.trim())
    .bind(request.cover_voice_asset_id)
    .bind(request.login_success_voice_asset_id)
    .bind(request.kanban_asset_id)
    .execute(&mut *transaction)
    .await?;
    sync_asset_references(
        &mut transaction,
        &id,
        request.name.trim(),
        request.cover_asset_id,
        request.cover_voice_asset_id,
        request.login_success_voice_asset_id,
        request.kanban_asset_id,
    )
    .await?;
    transaction.commit().await?;

    let row = fetch_raiment(&state, &id).await?;
    Ok((StatusCode::CREATED, Json(admin_item(&state, row)?)))
}

async fn admin_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<UpdateRaimentRequest>,
) -> Result<Json<AdminRaiment>, RaimentError> {
    require_admin(&state, &headers)?;
    let color_scheme = validate_update(&request)?;
    validate_assets(
        &state,
        request.cover_asset_id,
        request.cover_voice_asset_id,
        request.login_success_voice_asset_id,
        request.kanban_asset_id,
    )
    .await?;

    let theme = serde_json::to_value(&request.theme)
        .map_err(|error| RaimentError::CorruptData(error.to_string()))?;
    let mut transaction = state.pool().begin().await?;
    lock_site_settings(&mut transaction).await?;
    sqlx::query("SELECT id FROM raiments FOR UPDATE")
        .fetch_all(&mut *transaction)
        .await?;
    let current = sqlx::query_as::<_, (bool, bool, i64)>(
        "SELECT enabled, is_default, revision FROM raiments WHERE id = $1",
    )
    .bind(&id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(RaimentError::NotFound)?;
    let (current_enabled, current_is_default, current_revision) = current;
    if current_revision != request.revision {
        return Err(RaimentError::Conflict(
            "灵衣已被其他页面更新，请刷新后再保存".to_owned(),
        ));
    }
    if current_is_default && !request.is_default {
        return Err(RaimentError::Conflict(
            "请直接将另一套已启用灵衣设为默认，默认灵衣不能留空".to_owned(),
        ));
    }
    if !request.enabled {
        if raiment_is_scheduled(&mut transaction, &id).await? {
            return Err(RaimentError::Conflict(
                "该灵衣仍被站点时间段引用，请先移除对应时间段再停用".to_owned(),
            ));
        }
        if current_enabled {
            let other_enabled: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM raiments WHERE enabled = true AND id <> $1",
            )
            .bind(&id)
            .fetch_one(&mut *transaction)
            .await?;
            if other_enabled == 0 {
                return Err(RaimentError::Conflict(
                    "至少需要保留一套已启用灵衣供博客展示".to_owned(),
                ));
            }
        }
    }
    if request.is_default && !current_is_default {
        sqlx::query(
            "UPDATE raiments
             SET is_default = false, revision = revision + 1
             WHERE is_default = true AND id <> $1",
        )
        .bind(&id)
        .execute(&mut *transaction)
        .await?;
    }
    let changed = sqlx::query(
        "UPDATE raiments
         SET name = $1, cover_asset_id = $2, theme = $3,
             enabled = $4, sort_order = $5, is_default = $6, color_scheme = $7,
             cover_title = $8, cover_subtitle = $9, cover_character_name = $10,
             cover_dialogue = $11, cover_voice_label = $12,
             cover_voice_asset_id = $13, login_success_voice_asset_id = $14,
             kanban_asset_id = $15,
             revision = revision + 1
         WHERE id = $16 AND revision = $17",
    )
    .bind(request.name.trim())
    .bind(request.cover_asset_id)
    .bind(theme)
    .bind(request.enabled)
    .bind(request.sort_order)
    .bind(request.is_default)
    .bind(color_scheme.as_str())
    .bind(request.cover_title.trim())
    .bind(request.cover_subtitle.trim())
    .bind(request.cover_character_name.trim())
    .bind(request.cover_dialogue.trim())
    .bind(request.cover_voice_label.trim())
    .bind(request.cover_voice_asset_id)
    .bind(request.login_success_voice_asset_id)
    .bind(request.kanban_asset_id)
    .bind(&id)
    .bind(request.revision)
    .execute(&mut *transaction)
    .await?
    .rows_affected();
    if changed == 0 {
        return Err(RaimentError::Conflict(
            "灵衣已被其他页面更新，请刷新后再保存".to_owned(),
        ));
    }

    sync_asset_references(
        &mut transaction,
        &id,
        request.name.trim(),
        request.cover_asset_id,
        request.cover_voice_asset_id,
        request.login_success_voice_asset_id,
        request.kanban_asset_id,
    )
    .await?;
    transaction.commit().await?;

    let row = fetch_raiment(&state, &id).await?;
    Ok(Json(admin_item(&state, row)?))
}

async fn admin_delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, RaimentError> {
    require_admin(&state, &headers)?;
    let mut transaction = state.pool().begin().await?;
    lock_site_settings(&mut transaction).await?;
    let rows = sqlx::query_as::<_, (String, bool, bool)>(
        "SELECT id, enabled, is_default FROM raiments FOR UPDATE",
    )
    .fetch_all(&mut *transaction)
    .await?;
    let (_, target_enabled, target_is_default) = rows
        .iter()
        .find(|(candidate, _, _)| candidate == &id)
        .ok_or(RaimentError::NotFound)?;
    if *target_is_default {
        return Err(RaimentError::Conflict(
            "默认灵衣不能删除，请先将另一套已启用灵衣设为默认".to_owned(),
        ));
    }
    if *target_enabled && rows.iter().filter(|(_, enabled, _)| *enabled).count() == 1 {
        return Err(RaimentError::Conflict(
            "至少需要保留一套已启用灵衣供博客展示".to_owned(),
        ));
    }
    if raiment_is_scheduled(&mut transaction, &id).await? {
        return Err(RaimentError::Conflict(
            "该灵衣仍被站点时间段引用，请先在站点设置中移除对应时间段".to_owned(),
        ));
    }

    sqlx::query(
        "DELETE FROM asset_references
         WHERE source_key = $1
           AND source_type IN (
               'raiment_cover', 'raiment_voice', 'raiment_success_voice', 'raiment_kanban'
           )",
    )
    .bind(&id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query("DELETE FROM raiments WHERE id = $1")
        .bind(&id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<(), RaimentError> {
    if auth::has_valid_admin_session(state, headers) {
        Ok(())
    } else {
        Err(RaimentError::Unauthorized)
    }
}

async fn lock_site_settings(
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<(), RaimentError> {
    let exists: Option<i32> =
        sqlx::query_scalar("SELECT 1 FROM site_settings WHERE id = 1 FOR UPDATE")
            .fetch_optional(&mut **transaction)
            .await?;
    if exists.is_none() {
        return Err(RaimentError::CorruptData("站点设置记录不存在".to_owned()));
    }
    Ok(())
}

async fn raiment_is_scheduled(
    transaction: &mut Transaction<'_, Postgres>,
    id: &str,
) -> Result<bool, RaimentError> {
    Ok(sqlx::query_scalar(
        "SELECT COALESCE(
            settings #> '{raiment_schedule,periods}' @> jsonb_build_array(
                jsonb_build_object('raiment_id', $1::text)
            ),
            false
         )
         FROM site_settings WHERE id = 1",
    )
    .bind(id)
    .fetch_one(&mut **transaction)
    .await?)
}

async fn validate_assets(
    state: &AppState,
    cover_asset_id: i64,
    cover_voice_asset_id: Option<i64>,
    login_success_voice_asset_id: Option<i64>,
    kanban_asset_id: Option<i64>,
) -> Result<(), RaimentError> {
    let cover_is_valid: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM assets
            WHERE id = $1 AND status = 'active' AND media_type = 'image'
        )",
    )
    .bind(cover_asset_id)
    .fetch_one(state.pool())
    .await?;
    if !cover_is_valid {
        return Err(RaimentError::validation(
            "封面必须引用素材库中处于 active 状态的图片",
        ));
    }

    if let Some(asset_id) = cover_voice_asset_id {
        let voice_is_valid: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM assets
                WHERE id = $1 AND status = 'active' AND media_type = 'audio'
            )",
        )
        .bind(asset_id)
        .fetch_one(state.pool())
        .await?;
        if !voice_is_valid {
            return Err(RaimentError::validation(
                "封面语音必须引用素材库中处于 active 状态的音频",
            ));
        }
    }

    if let Some(asset_id) = login_success_voice_asset_id {
        let voice_is_valid: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM assets
                WHERE id = $1 AND status = 'active' AND media_type = 'audio'
            )",
        )
        .bind(asset_id)
        .fetch_one(state.pool())
        .await?;
        if !voice_is_valid {
            return Err(RaimentError::validation(
                "登录成功语音必须引用素材库中处于 active 状态的音频",
            ));
        }
    }

    if let Some(asset_id) = kanban_asset_id {
        let kanban_is_valid: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM assets
                WHERE id = $1 AND status = 'active' AND media_type = 'live2d'
            )",
        )
        .bind(asset_id)
        .fetch_one(state.pool())
        .await?;
        if !kanban_is_valid {
            return Err(RaimentError::validation(
                "看板娘必须引用素材库中处于 active 状态的 Live2D 素材",
            ));
        }
    }
    Ok(())
}

async fn sync_asset_references(
    transaction: &mut Transaction<'_, Postgres>,
    id: &str,
    name: &str,
    cover_asset_id: i64,
    cover_voice_asset_id: Option<i64>,
    login_success_voice_asset_id: Option<i64>,
    kanban_asset_id: Option<i64>,
) -> Result<(), RaimentError> {
    sqlx::query(
        "INSERT INTO asset_references (
            asset_id, source_type, source_key, source_label, admin_path
         ) VALUES ($1, 'raiment_cover', $2, $3, '/admin/raiments')
         ON CONFLICT (source_type, source_key) DO UPDATE
         SET asset_id = EXCLUDED.asset_id,
             source_label = EXCLUDED.source_label,
             admin_path = EXCLUDED.admin_path",
    )
    .bind(cover_asset_id)
    .bind(id)
    .bind(format!("{name} 灵衣封面"))
    .execute(&mut **transaction)
    .await?;

    if let Some(asset_id) = cover_voice_asset_id {
        sqlx::query(
            "INSERT INTO asset_references (
                asset_id, source_type, source_key, source_label, admin_path
             ) VALUES ($1, 'raiment_voice', $2, $3, '/admin/raiments')
             ON CONFLICT (source_type, source_key) DO UPDATE
             SET asset_id = EXCLUDED.asset_id,
                 source_label = EXCLUDED.source_label,
                 admin_path = EXCLUDED.admin_path",
        )
        .bind(asset_id)
        .bind(id)
        .bind(format!("{name} 灵衣封面语音"))
        .execute(&mut **transaction)
        .await?;
    } else {
        sqlx::query(
            "DELETE FROM asset_references
             WHERE source_type = 'raiment_voice' AND source_key = $1",
        )
        .bind(id)
        .execute(&mut **transaction)
        .await?;
    }

    if let Some(asset_id) = login_success_voice_asset_id {
        sqlx::query(
            "INSERT INTO asset_references (
                asset_id, source_type, source_key, source_label, admin_path
             ) VALUES ($1, 'raiment_success_voice', $2, $3, '/admin/raiments')
             ON CONFLICT (source_type, source_key) DO UPDATE
             SET asset_id = EXCLUDED.asset_id,
                 source_label = EXCLUDED.source_label,
                 admin_path = EXCLUDED.admin_path",
        )
        .bind(asset_id)
        .bind(id)
        .bind(format!("{name} 登录成功语音"))
        .execute(&mut **transaction)
        .await?;
    } else {
        sqlx::query(
            "DELETE FROM asset_references
             WHERE source_type = 'raiment_success_voice' AND source_key = $1",
        )
        .bind(id)
        .execute(&mut **transaction)
        .await?;
    }

    if let Some(asset_id) = kanban_asset_id {
        sqlx::query(
            "INSERT INTO asset_references (
                asset_id, source_type, source_key, source_label, admin_path
             ) VALUES ($1, 'raiment_kanban', $2, $3, '/admin/raiments')
             ON CONFLICT (source_type, source_key) DO UPDATE
             SET asset_id = EXCLUDED.asset_id,
                 source_label = EXCLUDED.source_label,
                 admin_path = EXCLUDED.admin_path",
        )
        .bind(asset_id)
        .bind(id)
        .bind(format!("{name} 看板娘"))
        .execute(&mut **transaction)
        .await?;
    } else {
        sqlx::query(
            "DELETE FROM asset_references
             WHERE source_type = 'raiment_kanban' AND source_key = $1",
        )
        .bind(id)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

async fn fetch_raiments(
    state: &AppState,
    enabled_only: bool,
) -> Result<Vec<RaimentRow>, RaimentError> {
    let filter = if enabled_only {
        " WHERE r.enabled = true"
    } else {
        ""
    };
    let sql = format!("{RAIMENT_SELECT}{filter} ORDER BY r.sort_order, r.created_at, r.id");
    Ok(sqlx::query_as::<_, RaimentRow>(&sql)
        .fetch_all(state.pool())
        .await?)
}

async fn fetch_raiment(state: &AppState, id: &str) -> Result<RaimentRow, RaimentError> {
    let sql = format!("{RAIMENT_SELECT} WHERE r.id = $1");
    sqlx::query_as::<_, RaimentRow>(&sql)
        .bind(id)
        .fetch_optional(state.pool())
        .await?
        .ok_or(RaimentError::NotFound)
}

fn public_item(state: &AppState, row: RaimentRow) -> Result<PublicRaiment, RaimentError> {
    let cover_voice_url = row
        .voice_object_key
        .as_deref()
        .map(|key| state.object_storage().public_url(key));
    let login_success_voice_url = row
        .success_voice_object_key
        .as_deref()
        .map(|key| state.object_storage().public_url(key));
    Ok(PublicRaiment {
        id: row.id,
        name: row.name,
        cover_url: state.object_storage().public_url(&row.cover_object_key),
        theme: parse_theme(row.theme)?,
        color_scheme: ColorScheme::parse(&row.color_scheme)?,
        cover_title: row.cover_title,
        cover_subtitle: row.cover_subtitle,
        cover_character_name: row.cover_character_name,
        cover_dialogue: row.cover_dialogue,
        cover_voice_label: row.cover_voice_label,
        cover_voice_url,
        login_success_voice_url,
        kanban_configured: row.kanban_asset_id.is_some(),
    })
}

fn admin_item(state: &AppState, row: RaimentRow) -> Result<AdminRaiment, RaimentError> {
    let cover_url = state.object_storage().public_url(&row.cover_object_key);
    let cover_voice_asset = match (
        row.cover_voice_asset_id,
        row.voice_name,
        row.voice_object_key,
        row.voice_mime,
        row.voice_size_bytes,
    ) {
        (Some(id), Some(name), Some(object_key), Some(mime), Some(size_bytes)) => {
            Some(LinkedAsset {
                id,
                name,
                media_type: "audio",
                file: AssetFile {
                    url: state.object_storage().public_url(&object_key),
                    mime,
                    size_bytes,
                    original_filename: row.voice_original_filename,
                },
            })
        }
        (None, None, None, None, None) => None,
        _ => {
            return Err(RaimentError::CorruptData(
                "灵衣封面语音素材关联不完整".to_owned(),
            ));
        }
    };
    let login_success_voice_asset = match (
        row.login_success_voice_asset_id,
        row.success_voice_name,
        row.success_voice_object_key,
        row.success_voice_mime,
        row.success_voice_size_bytes,
    ) {
        (Some(id), Some(name), Some(object_key), Some(mime), Some(size_bytes)) => {
            Some(LinkedAsset {
                id,
                name,
                media_type: "audio",
                file: AssetFile {
                    url: state.object_storage().public_url(&object_key),
                    mime,
                    size_bytes,
                    original_filename: row.success_voice_original_filename,
                },
            })
        }
        (None, None, None, None, None) => None,
        _ => {
            return Err(RaimentError::CorruptData(
                "灵衣登录成功语音素材关联不完整".to_owned(),
            ));
        }
    };
    Ok(AdminRaiment {
        id: row.id,
        name: row.name,
        cover_asset_id: row.cover_asset_id,
        cover_asset: LinkedAsset {
            id: row.cover_asset_id,
            name: row.cover_name,
            media_type: "image",
            file: AssetFile {
                url: cover_url,
                mime: row.cover_mime,
                size_bytes: row.cover_size_bytes,
                original_filename: row.cover_original_filename,
            },
        },
        theme: parse_theme(row.theme)?,
        enabled: row.enabled,
        sort_order: row.sort_order,
        is_default: row.is_default,
        color_scheme: ColorScheme::parse(&row.color_scheme)?,
        cover_title: row.cover_title,
        cover_subtitle: row.cover_subtitle,
        cover_character_name: row.cover_character_name,
        cover_dialogue: row.cover_dialogue,
        cover_voice_label: row.cover_voice_label,
        cover_voice_asset_id: row.cover_voice_asset_id,
        cover_voice_asset,
        login_success_voice_asset_id: row.login_success_voice_asset_id,
        login_success_voice_asset,
        kanban_asset_id: row.kanban_asset_id,
        is_builtin: row.is_builtin,
        revision: row.revision,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn parse_theme(value: Value) -> Result<ThemeTokens, RaimentError> {
    let theme: ThemeTokens = serde_json::from_value(value)
        .map_err(|error| RaimentError::CorruptData(error.to_string()))?;
    theme.validate()?;
    Ok(theme)
}

async fn load_schedule(state: &AppState) -> Result<RaimentSchedule, RaimentError> {
    let value: Option<Value> =
        sqlx::query_scalar("SELECT settings -> 'raiment_schedule' FROM site_settings WHERE id = 1")
            .fetch_optional(state.pool())
            .await?
            .flatten();
    let Some(value) = value else {
        return Ok(RaimentSchedule {
            revision: 1,
            periods: Vec::new(),
        });
    };
    let mut schedule: RaimentSchedule = serde_json::from_value(value)
        .map_err(|error| RaimentError::CorruptData(error.to_string()))?;
    if schedule.revision < 1 {
        return Err(RaimentError::CorruptData(
            "灵衣时间段 revision 无效".to_owned(),
        ));
    }
    normalize_and_validate_schedule(&mut schedule.periods).map_err(|error| match error {
        RaimentError::Validation(message) => RaimentError::CorruptData(message),
        other => other,
    })?;
    Ok(schedule)
}

fn normalize_and_validate_schedule(periods: &mut [SchedulePeriod]) -> Result<(), RaimentError> {
    if periods.len() > 48 {
        return Err(RaimentError::validation("灵衣时间段不能超过 48 个"));
    }
    let mut ids = HashSet::new();
    let mut segments: Vec<(u16, u16, String)> = Vec::new();
    for period in periods.iter_mut() {
        period.id = period.id.trim().to_owned();
        period.raiment_id = period.raiment_id.trim().to_owned();
        if period.id.is_empty() || period.id.chars().count() > 100 {
            return Err(RaimentError::validation(
                "每个时间段都需要有效且不超过 100 个字符的 id",
            ));
        }
        if !ids.insert(period.id.clone()) {
            return Err(RaimentError::validation("时间段 id 不能重复"));
        }
        if period.raiment_id.is_empty() {
            return Err(RaimentError::validation("每个时间段都必须选择灵衣"));
        }
        let start = parse_time(&period.start_at)?;
        let end = parse_time(&period.end_at)?;
        period.start_at = format_time(start);
        period.end_at = format_time(end);
        let start_minutes = start.hour() as u16 * 60 + start.minute() as u16;
        let end_minutes = end.hour() as u16 * 60 + end.minute() as u16;
        if start_minutes == end_minutes {
            return Err(RaimentError::validation(
                "时间段的开始时间和结束时间不能相同",
            ));
        }
        if start_minutes < end_minutes {
            segments.push((start_minutes, end_minutes, period.id.clone()));
        } else {
            segments.push((start_minutes, 24 * 60, period.id.clone()));
            segments.push((0, end_minutes, period.id.clone()));
        }
    }
    for left in 0..segments.len() {
        for right in (left + 1)..segments.len() {
            let (left_start, left_end, left_id) = &segments[left];
            let (right_start, right_end, right_id) = &segments[right];
            if left_id != right_id && left_start < right_end && right_start < left_end {
                return Err(RaimentError::validation(format!(
                    "时间段 {left_id} 与 {right_id} 重叠"
                )));
            }
        }
    }
    periods.sort_by(|left, right| {
        left.start_at
            .cmp(&right.start_at)
            .then_with(|| left.end_at.cmp(&right.end_at))
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(())
}

fn validate_create(request: &CreateRaimentRequest) -> Result<ColorScheme, RaimentError> {
    validate_fields(RaimentFields {
        name: &request.name,
        cover_asset_id: request.cover_asset_id,
        theme: &request.theme,
        enabled: request.enabled,
        sort_order: request.sort_order,
        is_default: request.is_default,
        color_scheme: &request.color_scheme,
        cover_title: &request.cover_title,
        cover_subtitle: &request.cover_subtitle,
        cover_character_name: &request.cover_character_name,
        cover_dialogue: &request.cover_dialogue,
        cover_voice_label: &request.cover_voice_label,
        cover_voice_asset_id: request.cover_voice_asset_id,
        login_success_voice_asset_id: request.login_success_voice_asset_id,
        kanban_asset_id: request.kanban_asset_id,
    })
}

fn validate_update(request: &UpdateRaimentRequest) -> Result<ColorScheme, RaimentError> {
    if request.revision < 1 {
        return Err(RaimentError::validation("revision 必须为正整数"));
    }
    next_revision(request.revision)?;
    validate_fields(RaimentFields {
        name: &request.name,
        cover_asset_id: request.cover_asset_id,
        theme: &request.theme,
        enabled: request.enabled,
        sort_order: request.sort_order,
        is_default: request.is_default,
        color_scheme: &request.color_scheme,
        cover_title: &request.cover_title,
        cover_subtitle: &request.cover_subtitle,
        cover_character_name: &request.cover_character_name,
        cover_dialogue: &request.cover_dialogue,
        cover_voice_label: &request.cover_voice_label,
        cover_voice_asset_id: request.cover_voice_asset_id,
        login_success_voice_asset_id: request.login_success_voice_asset_id,
        kanban_asset_id: request.kanban_asset_id,
    })
}

fn validate_fields(fields: RaimentFields<'_>) -> Result<ColorScheme, RaimentError> {
    let name = fields.name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(RaimentError::validation(
            "灵衣名称不能为空且不能超过 80 个字符",
        ));
    }
    if fields.cover_asset_id < 1 {
        return Err(RaimentError::validation("请选择有效的封面素材"));
    }
    if fields.sort_order < 0 {
        return Err(RaimentError::validation("排序值不能为负数"));
    }
    if fields.is_default && !fields.enabled {
        return Err(RaimentError::validation("默认灵衣必须保持启用"));
    }
    for (label, value, max) in [
        ("封面标题", fields.cover_title, 240),
        ("封面副标题", fields.cover_subtitle, 240),
        ("封面对话角色名", fields.cover_character_name, 80),
        ("封面对话", fields.cover_dialogue, 500),
        ("封面语音标签", fields.cover_voice_label, 120),
    ] {
        if value.chars().count() > max {
            return Err(RaimentError::validation(format!(
                "{label}不能超过 {max} 个字符"
            )));
        }
    }
    if fields.cover_title.trim().is_empty() {
        return Err(RaimentError::validation("封面标题不能为空"));
    }
    if fields.cover_voice_asset_id.is_some_and(|id| id < 1) {
        return Err(RaimentError::validation("封面语音素材 ID 必须为正整数"));
    }
    if fields.login_success_voice_asset_id.is_some_and(|id| id < 1) {
        return Err(RaimentError::validation("登录成功语音素材 ID 必须为正整数"));
    }
    if fields.kanban_asset_id.is_some_and(|id| id < 1) {
        return Err(RaimentError::validation("看板娘素材 ID 必须为正整数"));
    }
    fields.theme.validate()?;
    ColorScheme::parse(fields.color_scheme)
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn parse_time(value: &str) -> Result<NaiveTime, RaimentError> {
    if value.len() != 5 {
        return Err(RaimentError::validation(
            "时间段必须使用 24 小时制 HH:MM 格式",
        ));
    }
    NaiveTime::parse_from_str(value, "%H:%M")
        .map_err(|_| RaimentError::validation("时间段必须使用 24 小时制 HH:MM 格式"))
}

fn format_time(value: NaiveTime) -> String {
    value.format("%H:%M").to_string()
}

fn next_revision(revision: i64) -> Result<i64, RaimentError> {
    revision
        .checked_add(1)
        .ok_or_else(|| RaimentError::validation("revision 已达到上限，无法继续更新"))
}

#[cfg(test)]
mod tests {
    use super::{
        ColorScheme, RaimentFields, SchedulePeriod, ThemeTokens, is_hex_color, next_revision,
        normalize_and_validate_schedule, parse_time, validate_fields,
    };

    fn theme() -> ThemeTokens {
        ThemeTokens {
            primary: "#2B5FB8".to_owned(),
            secondary: "#B99A3E".to_owned(),
            background: "#F5F7FB".to_owned(),
            surface: "#FFFFFF".to_owned(),
            surface_alt: "#F0EFE9".to_owned(),
            text: "#1F2534".to_owned(),
            text_secondary: "#3A4155".to_owned(),
            muted: "#6B7284".to_owned(),
            faint: "#9AA1B3".to_owned(),
            border: "#D9DCE3".to_owned(),
            danger: "#D84358".to_owned(),
            success: "#3D8455".to_owned(),
        }
    }

    #[test]
    fn theme_colors_require_full_hex_values() {
        assert!(is_hex_color("#2B5FB8"));
        assert!(is_hex_color("#ffffff"));
        assert!(!is_hex_color("2B5FB8"));
        assert!(!is_hex_color("#fff"));
        assert!(!is_hex_color("#GG0000"));
        assert!(theme().validate().is_ok());
    }

    #[test]
    fn raiment_fields_and_twenty_four_hour_times_are_validated() {
        assert!(parse_time("07:00").is_ok());
        assert!(parse_time("23:59").is_ok());
        assert!(parse_time("7:00").is_err());
        assert!(parse_time("24:00").is_err());
        let theme = theme();
        let valid = |name, color_scheme| RaimentFields {
            name,
            cover_asset_id: 1,
            theme: &theme,
            enabled: true,
            sort_order: 0,
            is_default: false,
            color_scheme,
            cover_title: "标题",
            cover_subtitle: "副标题",
            cover_character_name: "Saber",
            cover_dialogue: "你好",
            cover_voice_label: "播放",
            cover_voice_asset_id: None,
            login_success_voice_asset_id: None,
            kanban_asset_id: None,
        };
        let validated = validate_fields(valid("日间模式", "day")).expect("valid raiment");
        assert_eq!(validated, ColorScheme::Day);
        assert!(validate_fields(valid("", "day")).is_err());
        assert!(validate_fields(valid("灵衣", "sepia")).is_err());
    }

    #[test]
    fn schedule_accepts_overnight_ranges_and_rejects_overlap() {
        let mut valid = vec![
            SchedulePeriod {
                id: "day".to_owned(),
                start_at: "07:00".to_owned(),
                end_at: "19:00".to_owned(),
                raiment_id: "saber".to_owned(),
            },
            SchedulePeriod {
                id: "night".to_owned(),
                start_at: "19:00".to_owned(),
                end_at: "07:00".to_owned(),
                raiment_id: "alter-saber".to_owned(),
            },
        ];
        assert!(normalize_and_validate_schedule(&mut valid).is_ok());

        let mut overlap = valid.clone();
        overlap.push(SchedulePeriod {
            id: "late".to_owned(),
            start_at: "23:00".to_owned(),
            end_at: "23:30".to_owned(),
            raiment_id: "saber".to_owned(),
        });
        assert!(normalize_and_validate_schedule(&mut overlap).is_err());
    }

    #[test]
    fn schedule_revision_cannot_overflow() {
        assert_eq!(next_revision(1).expect("next revision"), 2);
        assert!(next_revision(i64::MAX).is_err());
    }
}
