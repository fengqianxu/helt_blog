use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sqlx::{FromRow, Postgres, Transaction};
use tracing::error;

use crate::{
    auth,
    error::{ErrorBody, ErrorEnvelope},
    routes::contract::HttpMethod,
    state::AppState,
};

const DEFAULT_SITE_NAME: &str = "helt.";
const DEFAULT_TAGLINE: &str = "记录技术、生活与热爱";
const DEFAULT_FOOTER_TEXT: &str = "记录技术、生活与热爱";
const DEFAULT_FOOTER_COPYRIGHT: &str = "© 2020—{year} {site_name} · POWERED BY REACT";
const DEFAULT_HERO_EYEBROW: &str = "SINCE 2020 · HELT'S BLOG";

fn default_hero_eyebrow() -> String {
    DEFAULT_HERO_EYEBROW.to_owned()
}

fn default_footer_text() -> String {
    DEFAULT_FOOTER_TEXT.to_owned()
}

fn default_footer_copyright() -> String {
    DEFAULT_FOOTER_COPYRIGHT.to_owned()
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/site", get(public_site))
        .route("/api/v1/stats/visit", post(record_visit))
        .route("/api/v1/admin/stats/overview", get(admin_overview))
        .route("/api/v1/admin/stats/pv-uv", get(admin_pv_uv))
        .route(
            "/api/v1/admin/site/settings",
            get(admin_settings)
                .put(update_settings)
                .patch(patch_setting),
        )
}

pub fn implements(method: HttpMethod, path: &str) -> bool {
    matches!(
        (method, path),
        (HttpMethod::Get, "/api/v1/site")
            | (HttpMethod::Post, "/api/v1/stats/visit")
            | (HttpMethod::Get, "/api/v1/admin/stats/overview")
            | (HttpMethod::Get, "/api/v1/admin/stats/pv-uv")
            | (HttpMethod::Get, "/api/v1/admin/site/settings")
            | (HttpMethod::Put, "/api/v1/admin/site/settings")
            | (HttpMethod::Patch, "/api/v1/admin/site/settings")
    )
}

#[derive(Debug, FromRow)]
struct SiteSettingsRow {
    settings: Value,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
struct SitePayload {
    basic: SiteBasic,
    features: SiteFeatures,
    stats: SiteStats,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
struct SiteBasic {
    name: String,
    tagline: String,
    footer_text: String,
    footer_copyright: String,
    hero_eyebrow: String,
    domain: String,
    icp: String,
    founded_at: String,
    logo_asset_id: Option<i64>,
    logo_url: Option<String>,
    favicon_asset_id: Option<i64>,
    favicon_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SiteFeatures {
    splash: bool,
    comments: bool,
    kanban: bool,
    music: bool,
    stats: bool,
    easter_egg: bool,
}

#[derive(Debug, Clone, Serialize)]
struct SiteStats {
    article_count: i64,
    total_words: i64,
    total_visits: i64,
    uptime_days: i64,
}

#[derive(Debug, FromRow, Serialize)]
struct AdminOverviewRow {
    today_pv: i64,
    today_uv: i64,
    yesterday_pv: i64,
    yesterday_uv: i64,
    article_count: i64,
    published_count: i64,
    draft_count: i64,
    total_visits: i64,
}

#[derive(Debug, Serialize)]
struct AdminOverview {
    #[serde(flatten)]
    counts: AdminOverviewRow,
    uptime_days: i64,
}

#[derive(Debug, Deserialize)]
struct StatsQuery {
    #[serde(default = "default_stats_days")]
    days: i32,
}

#[derive(Debug, FromRow, Serialize)]
struct DailyStat {
    date: NaiveDate,
    pv: i64,
    uv: i64,
}

#[derive(Debug, Serialize)]
struct DailyStatsPayload {
    items: Vec<DailyStat>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateSiteRequest {
    basic: EditableBasic,
    features: SiteFeatures,
    updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EditableBasic {
    name: String,
    tagline: String,
    #[serde(default = "default_footer_text")]
    footer_text: String,
    #[serde(default = "default_footer_copyright")]
    footer_copyright: String,
    #[serde(default = "default_hero_eyebrow")]
    hero_eyebrow: String,
    icp: String,
    logo_asset_id: Option<i64>,
    favicon_asset_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PatchSettingRequest {
    path: String,
    value: Value,
    updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VisitRequest {
    visitor_id: String,
    path: String,
}

async fn public_site(State(state): State<AppState>) -> Result<Json<SitePayload>, SiteError> {
    Ok(Json(load_payload(&state).await?))
}

async fn admin_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SitePayload>, SiteError> {
    require_admin(&state, &headers)?;
    Ok(Json(load_payload(&state).await?))
}

async fn update_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<UpdateSiteRequest>,
) -> Result<Json<SitePayload>, SiteError> {
    require_admin(&state, &headers)?;
    persist_settings(&state, request).await?;
    Ok(Json(load_payload(&state).await?))
}

async fn patch_setting(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(patch): Json<PatchSettingRequest>,
) -> Result<Json<SitePayload>, SiteError> {
    require_admin(&state, &headers)?;
    let current = load_site_row(&state).await?;
    let mut request = editable_request(&current.settings);
    request.updated_at = patch.updated_at.or(Some(current.updated_at));
    apply_patch(&mut request, &patch.path, patch.value)?;
    persist_settings(&state, request).await?;
    Ok(Json(load_payload(&state).await?))
}

async fn admin_overview(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AdminOverview>, SiteError> {
    require_admin(&state, &headers)?;
    let counts = sqlx::query_as::<_, AdminOverviewRow>(
        "SELECT
            COALESCE((SELECT pv FROM daily_stats WHERE day = CURRENT_DATE), 0)::bigint AS today_pv,
            COALESCE((SELECT uv FROM daily_stats WHERE day = CURRENT_DATE), 0)::bigint AS today_uv,
            COALESCE((SELECT pv FROM daily_stats WHERE day = CURRENT_DATE - 1), 0)::bigint AS yesterday_pv,
            COALESCE((SELECT uv FROM daily_stats WHERE day = CURRENT_DATE - 1), 0)::bigint AS yesterday_uv,
            (SELECT COUNT(*) FROM articles)::bigint AS article_count,
            (SELECT COUNT(*) FROM articles WHERE status = 'published')::bigint AS published_count,
            (SELECT COUNT(*) FROM articles WHERE status = 'draft')::bigint AS draft_count,
            COALESCE((SELECT SUM(pv) FROM daily_stats), 0)::bigint AS total_visits",
    )
    .fetch_one(state.pool())
    .await?;
    let settings = load_site_row(&state).await?.settings;
    let today: NaiveDate = sqlx::query_scalar("SELECT CURRENT_DATE")
        .fetch_one(state.pool())
        .await?;
    Ok(Json(AdminOverview {
        counts,
        uptime_days: uptime_days(&settings, today),
    }))
}

async fn admin_pv_uv(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<StatsQuery>,
) -> Result<Json<DailyStatsPayload>, SiteError> {
    require_admin(&state, &headers)?;
    if !(1..=90).contains(&query.days) {
        return Err(SiteError::validation("days 必须在 1 到 90 之间"));
    }
    let items = sqlx::query_as::<_, DailyStat>(
        "SELECT series.day::date AS date,
                COALESCE(stats.pv, 0)::bigint AS pv,
                COALESCE(stats.uv, 0)::bigint AS uv
         FROM generate_series(
             CURRENT_DATE - ($1::integer - 1),
             CURRENT_DATE,
             interval '1 day'
         ) AS series(day)
         LEFT JOIN daily_stats stats ON stats.day = series.day::date
         ORDER BY series.day",
    )
    .bind(query.days)
    .fetch_all(state.pool())
    .await?;
    Ok(Json(DailyStatsPayload { items }))
}

async fn record_visit(
    State(state): State<AppState>,
    Json(request): Json<VisitRequest>,
) -> Result<StatusCode, SiteError> {
    let visitor_id = normalized_required(&request.visitor_id, "visitor_id", 128)?;
    let path = request.path.trim();
    if path.is_empty() || path.chars().count() > 2048 || !path.starts_with('/') {
        return Err(SiteError::validation(
            "path 必须是长度不超过 2048 的站内绝对路径",
        ));
    }
    let enabled: bool = sqlx::query_scalar(
        "SELECT COALESCE((settings #>> '{features,stats}')::boolean, true)
         FROM site_settings WHERE id = 1",
    )
    .fetch_optional(state.pool())
    .await?
    .ok_or_else(|| SiteError::CorruptData("站点设置记录不存在".to_owned()))?;
    if !enabled {
        return Ok(StatusCode::NO_CONTENT);
    }

    let mut transaction = state.pool().begin().await?;
    let day: NaiveDate = sqlx::query_scalar("SELECT CURRENT_DATE")
        .fetch_one(&mut *transaction)
        .await?;
    let is_new_visitor = sqlx::query(
        "INSERT INTO daily_visitors (day, visitor_id) VALUES ($1, $2)
         ON CONFLICT (day, visitor_id) DO NOTHING",
    )
    .bind(day)
    .bind(visitor_id)
    .execute(&mut *transaction)
    .await?
    .rows_affected()
        == 1;
    sqlx::query(
        "INSERT INTO daily_stats (day, pv, uv) VALUES ($1, 1, $2)
         ON CONFLICT (day) DO UPDATE
         SET pv = daily_stats.pv + 1,
             uv = daily_stats.uv + EXCLUDED.uv",
    )
    .bind(day)
    .bind(i64::from(is_new_visitor))
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn persist_settings(
    state: &AppState,
    mut request: UpdateSiteRequest,
) -> Result<(), SiteError> {
    request.basic.name = normalized_required(&request.basic.name, "站点文字名称", 100)?;
    request.basic.tagline = normalized_optional(&request.basic.tagline, "站点简介", 300)?;
    request.basic.footer_text = normalized_optional(&request.basic.footer_text, "页脚介绍", 500)?;
    request.basic.footer_copyright =
        normalized_optional(&request.basic.footer_copyright, "页脚底部文字", 300)?;
    request.basic.hero_eyebrow =
        normalized_required(&request.basic.hero_eyebrow, "封面标识文字", 120)?;
    request.basic.icp = normalized_optional(&request.basic.icp, "备案号", 100)?;
    validate_asset_id(request.basic.logo_asset_id, "站点 Logo")?;
    validate_asset_id(request.basic.favicon_asset_id, "浏览器图标")?;

    let mut transaction = state.pool().begin().await?;
    let current = sqlx::query_as::<_, SiteSettingsRow>(
        "SELECT settings, updated_at FROM site_settings WHERE id = 1 FOR UPDATE",
    )
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or_else(|| SiteError::CorruptData("站点设置记录不存在".to_owned()))?;
    if request
        .updated_at
        .is_some_and(|expected| expected != current.updated_at)
    {
        return Err(SiteError::Conflict(
            "站点设置已在其他页面更新，请刷新后重试".to_owned(),
        ));
    }

    ensure_image_asset(&mut transaction, request.basic.logo_asset_id, "站点 Logo").await?;
    ensure_image_asset(
        &mut transaction,
        request.basic.favicon_asset_id,
        "浏览器图标",
    )
    .await?;

    let mut settings = current.settings;
    set_setting(&mut settings, "basic", "name", json!(request.basic.name))?;
    set_setting(
        &mut settings,
        "basic",
        "tagline",
        json!(request.basic.tagline),
    )?;
    set_setting(
        &mut settings,
        "basic",
        "footer_text",
        json!(request.basic.footer_text),
    )?;
    set_setting(
        &mut settings,
        "basic",
        "footer_copyright",
        json!(request.basic.footer_copyright),
    )?;
    set_setting(
        &mut settings,
        "basic",
        "hero_eyebrow",
        json!(request.basic.hero_eyebrow),
    )?;
    set_setting(&mut settings, "basic", "icp", json!(request.basic.icp))?;
    set_setting(
        &mut settings,
        "basic",
        "logo_asset_id",
        json!(request.basic.logo_asset_id),
    )?;
    set_setting(
        &mut settings,
        "basic",
        "favicon_asset_id",
        json!(request.basic.favicon_asset_id),
    )?;
    for (key, value) in [
        ("splash", request.features.splash),
        ("comments", request.features.comments),
        ("kanban", request.features.kanban),
        ("music", request.features.music),
        ("stats", request.features.stats),
        ("easter_egg", request.features.easter_egg),
    ] {
        set_setting(&mut settings, "features", key, json!(value))?;
    }
    // Keep the legacy location in sync for rolling upgrades.
    set_setting(
        &mut settings,
        "theme",
        "splash_enabled",
        json!(request.features.splash),
    )?;

    sqlx::query("UPDATE site_settings SET settings = $1 WHERE id = 1")
        .bind(settings)
        .execute(&mut *transaction)
        .await?;
    sync_asset_reference(
        &mut transaction,
        request.basic.logo_asset_id,
        "site:branding:logo",
        "站点 Logo",
    )
    .await?;
    sync_asset_reference(
        &mut transaction,
        request.basic.favicon_asset_id,
        "site:branding:favicon",
        "浏览器图标",
    )
    .await?;
    transaction.commit().await?;
    Ok(())
}

async fn load_payload(state: &AppState) -> Result<SitePayload, SiteError> {
    let row = load_site_row(state).await?;
    let logo_asset_id = optional_i64(&row.settings, "basic", "logo_asset_id");
    let favicon_asset_id = optional_i64(&row.settings, "basic", "favicon_asset_id");
    let (logo_url, favicon_url, stats) = tokio::try_join!(
        asset_url(state, logo_asset_id),
        asset_url(state, favicon_asset_id),
        load_stats(state, &row.settings),
    )?;
    Ok(SitePayload {
        basic: SiteBasic {
            name: text_setting(&row.settings, "basic", "name", DEFAULT_SITE_NAME),
            tagline: text_setting(&row.settings, "basic", "tagline", DEFAULT_TAGLINE),
            footer_text: text_setting(&row.settings, "basic", "footer_text", DEFAULT_FOOTER_TEXT),
            footer_copyright: text_setting(
                &row.settings,
                "basic",
                "footer_copyright",
                DEFAULT_FOOTER_COPYRIGHT,
            ),
            hero_eyebrow: text_setting(
                &row.settings,
                "basic",
                "hero_eyebrow",
                DEFAULT_HERO_EYEBROW,
            ),
            domain: state.public_origin().to_owned(),
            icp: text_setting(&row.settings, "basic", "icp", ""),
            founded_at: text_setting(&row.settings, "basic", "founded_at", "2026-07-23"),
            logo_asset_id,
            logo_url,
            favicon_asset_id,
            favicon_url,
        },
        features: features_from_document(&row.settings),
        stats,
        updated_at: row.updated_at,
    })
}

async fn load_site_row(state: &AppState) -> Result<SiteSettingsRow, SiteError> {
    sqlx::query_as("SELECT settings, updated_at FROM site_settings WHERE id = 1")
        .fetch_optional(state.pool())
        .await?
        .ok_or_else(|| SiteError::CorruptData("站点设置记录不存在".to_owned()))
}

async fn asset_url(state: &AppState, asset_id: Option<i64>) -> Result<Option<String>, SiteError> {
    let Some(asset_id) = asset_id else {
        return Ok(None);
    };
    let object_key: Option<String> = sqlx::query_scalar(
        "SELECT u.object_key
         FROM assets a JOIN uploads u ON u.id = a.upload_id
         WHERE a.id = $1 AND a.status = 'active' AND a.media_type = 'image'",
    )
    .bind(asset_id)
    .fetch_optional(state.pool())
    .await?;
    Ok(object_key.map(|key| state.object_storage().public_url(&key)))
}

async fn load_stats(state: &AppState, settings: &Value) -> Result<SiteStats, SiteError> {
    let (article_count, total_words, today): (i64, i64, NaiveDate) = sqlx::query_as(
        "SELECT COUNT(*)::bigint, COALESCE(SUM(word_count), 0)::bigint, CURRENT_DATE
         FROM articles WHERE status = 'published'",
    )
    .fetch_one(state.pool())
    .await?;
    let total_visits: i64 =
        sqlx::query_scalar("SELECT COALESCE(SUM(pv), 0)::bigint FROM daily_stats")
            .fetch_one(state.pool())
            .await?;
    Ok(SiteStats {
        article_count,
        total_words,
        total_visits,
        uptime_days: uptime_days(settings, today),
    })
}

fn uptime_days(settings: &Value, today: NaiveDate) -> i64 {
    let founded_at = text_setting(settings, "basic", "founded_at", "2026-07-23");
    NaiveDate::parse_from_str(&founded_at, "%Y-%m-%d")
        .map(|date| (today - date).num_days().saturating_add(1))
        .unwrap_or(1)
        .max(1)
}

fn default_stats_days() -> i32 {
    14
}

fn editable_request(settings: &Value) -> UpdateSiteRequest {
    UpdateSiteRequest {
        basic: EditableBasic {
            name: text_setting(settings, "basic", "name", DEFAULT_SITE_NAME),
            tagline: text_setting(settings, "basic", "tagline", DEFAULT_TAGLINE),
            footer_text: text_setting(settings, "basic", "footer_text", DEFAULT_FOOTER_TEXT),
            footer_copyright: text_setting(
                settings,
                "basic",
                "footer_copyright",
                DEFAULT_FOOTER_COPYRIGHT,
            ),
            hero_eyebrow: text_setting(settings, "basic", "hero_eyebrow", DEFAULT_HERO_EYEBROW),
            icp: text_setting(settings, "basic", "icp", ""),
            logo_asset_id: optional_i64(settings, "basic", "logo_asset_id"),
            favicon_asset_id: optional_i64(settings, "basic", "favicon_asset_id"),
        },
        features: features_from_document(settings),
        updated_at: None,
    }
}

fn apply_patch(request: &mut UpdateSiteRequest, path: &str, value: Value) -> Result<(), SiteError> {
    match path {
        "basic.name" => request.basic.name = json_string(value, path)?,
        "basic.tagline" => request.basic.tagline = json_string(value, path)?,
        "basic.footer_text" => request.basic.footer_text = json_string(value, path)?,
        "basic.footer_copyright" => request.basic.footer_copyright = json_string(value, path)?,
        "basic.hero_eyebrow" => request.basic.hero_eyebrow = json_string(value, path)?,
        "basic.icp" => request.basic.icp = json_string(value, path)?,
        "basic.logo_asset_id" => request.basic.logo_asset_id = json_optional_i64(value, path)?,
        "basic.favicon_asset_id" => {
            request.basic.favicon_asset_id = json_optional_i64(value, path)?
        }
        "features.splash" => request.features.splash = json_bool(value, path)?,
        "features.comments" => request.features.comments = json_bool(value, path)?,
        "features.kanban" => request.features.kanban = json_bool(value, path)?,
        "features.music" => request.features.music = json_bool(value, path)?,
        "features.stats" => request.features.stats = json_bool(value, path)?,
        "features.easter_egg" => request.features.easter_egg = json_bool(value, path)?,
        _ => return Err(SiteError::validation("不支持的站点设置路径")),
    }
    Ok(())
}

async fn ensure_image_asset(
    transaction: &mut Transaction<'_, Postgres>,
    asset_id: Option<i64>,
    label: &str,
) -> Result<(), SiteError> {
    let Some(asset_id) = asset_id else {
        return Ok(());
    };
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM assets
            WHERE id = $1 AND status = 'active' AND media_type = 'image'
         )",
    )
    .bind(asset_id)
    .fetch_one(&mut **transaction)
    .await?;
    if !exists {
        return Err(SiteError::validation(format!(
            "{label}必须引用素材库中有效的图片"
        )));
    }
    Ok(())
}

async fn sync_asset_reference(
    transaction: &mut Transaction<'_, Postgres>,
    asset_id: Option<i64>,
    source_key: &str,
    source_label: &str,
) -> Result<(), SiteError> {
    if let Some(asset_id) = asset_id {
        sqlx::query(
            "INSERT INTO asset_references
                 (asset_id, source_type, source_key, source_label, admin_path)
             VALUES ($1, 'site_branding', $2, $3, '/admin/settings')
             ON CONFLICT (source_type, source_key) DO UPDATE
             SET asset_id = EXCLUDED.asset_id,
                 source_label = EXCLUDED.source_label,
                 admin_path = EXCLUDED.admin_path",
        )
        .bind(asset_id)
        .bind(source_key)
        .bind(source_label)
        .execute(&mut **transaction)
        .await?;
    } else {
        sqlx::query(
            "DELETE FROM asset_references
             WHERE source_type = 'site_branding' AND source_key = $1",
        )
        .bind(source_key)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

fn set_setting(
    settings: &mut Value,
    group: &str,
    key: &str,
    value: Value,
) -> Result<(), SiteError> {
    let root = settings
        .as_object_mut()
        .ok_or_else(|| SiteError::CorruptData("站点设置不是 JSON 对象".to_owned()))?;
    let group_value = root
        .entry(group.to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    let group_object = group_value
        .as_object_mut()
        .ok_or_else(|| SiteError::CorruptData(format!("站点设置 {group} 分组不是对象")))?;
    group_object.insert(key.to_owned(), value);
    Ok(())
}

fn features_from_document(settings: &Value) -> SiteFeatures {
    SiteFeatures {
        splash: bool_setting_with_fallback(
            settings,
            ("features", "splash"),
            ("theme", "splash_enabled"),
            true,
        ),
        comments: bool_setting(settings, "features", "comments", true),
        kanban: bool_setting(settings, "features", "kanban", true),
        music: bool_setting(settings, "features", "music", true),
        stats: bool_setting(settings, "features", "stats", true),
        easter_egg: bool_setting(settings, "features", "easter_egg", true),
    }
}

fn text_setting(settings: &Value, group: &str, key: &str, default: &str) -> String {
    settings
        .get(group)
        .and_then(|value| value.get(key))
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_owned()
}

fn optional_i64(settings: &Value, group: &str, key: &str) -> Option<i64> {
    settings
        .get(group)
        .and_then(|value| value.get(key))
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
}

fn bool_setting(settings: &Value, group: &str, key: &str, default: bool) -> bool {
    settings
        .get(group)
        .and_then(|value| value.get(key))
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

fn bool_setting_with_fallback(
    settings: &Value,
    primary: (&str, &str),
    fallback: (&str, &str),
    default: bool,
) -> bool {
    settings
        .get(primary.0)
        .and_then(|value| value.get(primary.1))
        .and_then(Value::as_bool)
        .or_else(|| {
            settings
                .get(fallback.0)
                .and_then(|value| value.get(fallback.1))
                .and_then(Value::as_bool)
        })
        .unwrap_or(default)
}

fn normalized_required(value: &str, label: &str, max_chars: usize) -> Result<String, SiteError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max_chars {
        return Err(SiteError::validation(format!(
            "{label}不能为空且不能超过 {max_chars} 个字符"
        )));
    }
    Ok(value.to_owned())
}

fn normalized_optional(value: &str, label: &str, max_chars: usize) -> Result<String, SiteError> {
    let value = value.trim();
    if value.chars().count() > max_chars {
        return Err(SiteError::validation(format!(
            "{label}不能超过 {max_chars} 个字符"
        )));
    }
    Ok(value.to_owned())
}

fn validate_asset_id(asset_id: Option<i64>, label: &str) -> Result<(), SiteError> {
    if asset_id.is_some_and(|asset_id| asset_id <= 0) {
        return Err(SiteError::validation(format!("{label}素材 ID 无效")));
    }
    Ok(())
}

fn json_string(value: Value, path: &str) -> Result<String, SiteError> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| SiteError::validation(format!("{path} 必须是字符串")))
}

fn json_optional_i64(value: Value, path: &str) -> Result<Option<i64>, SiteError> {
    if value.is_null() {
        return Ok(None);
    }
    value
        .as_i64()
        .map(Some)
        .ok_or_else(|| SiteError::validation(format!("{path} 必须是整数或 null")))
}

fn json_bool(value: Value, path: &str) -> Result<bool, SiteError> {
    value
        .as_bool()
        .ok_or_else(|| SiteError::validation(format!("{path} 必须是布尔值")))
}

fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<(), SiteError> {
    if auth::has_valid_admin_session(state, headers) {
        Ok(())
    } else {
        Err(SiteError::Unauthorized)
    }
}

#[derive(Debug, thiserror::Error)]
enum SiteError {
    #[error("需要有效的管理员会话")]
    Unauthorized,
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Conflict(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("{0}")]
    CorruptData(String),
}

impl SiteError {
    fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }
}

impl IntoResponse for SiteError {
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
            Self::Conflict(message) => (StatusCode::CONFLICT, "conflict", message),
            Self::Database(error) => {
                error!(%error, "site settings database operation failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "站点设置操作失败".to_owned(),
                )
            }
            Self::CorruptData(message) => {
                error!(%message, "persisted site settings are invalid");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "站点设置数据损坏".to_owned(),
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
    use chrono::NaiveDate;
    use serde_json::json;

    use super::{apply_patch, editable_request, features_from_document, uptime_days};

    #[test]
    fn uptime_uses_the_database_calendar_day() {
        let settings = json!({"basic": {"founded_at": "2026-07-23"}});
        let today = NaiveDate::from_ymd_opt(2026, 7, 28).expect("valid date");
        assert_eq!(uptime_days(&settings, today), 6);
    }

    #[test]
    fn legacy_splash_setting_is_preserved() {
        let settings = json!({
            "basic": {"name": "helt."},
            "features": {"comments": false},
            "theme": {"splash_enabled": false}
        });
        let features = features_from_document(&settings);
        assert!(!features.splash);
        assert!(!features.comments);
        assert!(features.kanban);
    }

    #[test]
    fn patch_only_accepts_known_typed_leaf_fields() {
        let mut request = editable_request(&json!({}));
        apply_patch(&mut request, "basic.logo_asset_id", json!(42)).unwrap();
        apply_patch(&mut request, "basic.hero_eyebrow", json!("MY BLOG")).unwrap();
        apply_patch(&mut request, "basic.footer_text", json!("FOOTER COPY")).unwrap();
        apply_patch(
            &mut request,
            "basic.footer_copyright",
            json!("© {year} {site_name}"),
        )
        .unwrap();
        apply_patch(&mut request, "features.music", json!(false)).unwrap();
        assert_eq!(request.basic.logo_asset_id, Some(42));
        assert_eq!(request.basic.hero_eyebrow, "MY BLOG");
        assert_eq!(request.basic.footer_text, "FOOTER COPY");
        assert_eq!(request.basic.footer_copyright, "© {year} {site_name}");
        assert!(!request.features.music);
        assert!(apply_patch(&mut request, "features.music", json!("no")).is_err());
        assert!(apply_patch(&mut request, "secrets.api_key", json!("x")).is_err());
    }

    #[test]
    fn every_public_feature_can_be_disabled() {
        let settings = json!({
            "features": {
                "splash": false,
                "comments": false,
                "kanban": false,
                "music": false,
                "stats": false,
                "easter_egg": false
            }
        });
        let features = features_from_document(&settings);
        assert!(!features.splash);
        assert!(!features.comments);
        assert!(!features.kanban);
        assert!(!features.music);
        assert!(!features.stats);
        assert!(!features.easter_egg);

        let mut request = editable_request(&json!({}));
        for path in [
            "features.splash",
            "features.comments",
            "features.kanban",
            "features.music",
            "features.stats",
            "features.easter_egg",
        ] {
            apply_patch(&mut request, path, json!(false)).unwrap();
        }
        assert!(!request.features.splash);
        assert!(!request.features.comments);
        assert!(!request.features.kanban);
        assert!(!request.features.music);
        assert!(!request.features.stats);
        assert!(!request.features.easter_egg);
    }
}
