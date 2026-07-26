use std::time::{Duration, Instant};

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use tracing::error;

use crate::{
    auth,
    error::{ErrorBody, ErrorEnvelope},
    llm_crypto::LlmKeyring,
    routes::contract::HttpMethod,
    state::AppState,
};

const MAX_NAME_CHARS: usize = 80;
const MAX_MODEL_CHARS: usize = 200;
const MAX_API_KEY_CHARS: usize = 4096;
const MAX_PROMPT_CHARS: usize = 12_000;
const MAX_ARTICLE_TITLE_CHARS: usize = 300;
const MAX_ARTICLE_SUMMARY_CHARS: usize = 120;
const MAX_ARTICLE_SOURCE_CHARS: usize = 200_000;
const MAX_CONNECTIONS: usize = 20;
const MAX_UPSTREAM_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const ARTICLE_POLISH_TIMEOUT_SECS: u64 = 25;
const SUMMARY_POLISH_SYSTEM_PROMPT: &str = "你是博客文章摘要编辑助手。当前任务只能润色摘要，不是续写、扩写或改写文章正文。严格保留原意与已有事实，不得虚构数据、经历、引用或来源。最终答案只能是一段可直接替换的纯文本摘要：不要标题、前缀、引号、解释、Markdown 或换行。包含中文、英文、数字、标点和空格在内，输出必须不超过 120 个字符，建议控制在 80–110 个字符。输出前请自行计数字符；如果超过上限，先压缩措辞再作答。此格式和长度要求高于用户的其他偏好。";
type EncryptedApiKey = (Option<Vec<u8>>, Option<Vec<u8>>, Option<i32>);

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/admin/llm", get(get_settings).put(update_settings))
        .route("/api/v1/admin/llm/connections", post(create_connection))
        .route("/api/v1/admin/llm/models", post(list_models))
        .route("/api/v1/admin/llm/test", post(test_connection))
        .route("/api/v1/admin/llm/polish", post(polish_article))
}

pub fn implements(method: HttpMethod, path: &str) -> bool {
    matches!(
        (method, path),
        (HttpMethod::Get, "/api/v1/admin/llm")
            | (HttpMethod::Put, "/api/v1/admin/llm")
            | (HttpMethod::Post, "/api/v1/admin/llm/connections")
            | (HttpMethod::Post, "/api/v1/admin/llm/models")
            | (HttpMethod::Post, "/api/v1/admin/llm/test")
            | (HttpMethod::Post, "/api/v1/admin/llm/polish")
    )
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct UseCaseConfig {
    enabled: bool,
    system_prompt: String,
    #[serde(default)]
    connection_id: Option<i64>,
    #[serde(default)]
    model: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct UseCases {
    kanban_chat: UseCaseConfig,
    comment_review: UseCaseConfig,
    article_assistant: UseCaseConfig,
}

#[derive(Debug, Deserialize)]
struct UpdateSettingsRequest {
    revision: i64,
    #[serde(default)]
    connections: Option<Vec<ConnectionInput>>,
    // Legacy singleton fields remain accepted so older admin clients can still
    // save a single connection while they roll forward to the collection UI.
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    base_url: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default)]
    clear_api_key: bool,
    #[serde(default = "default_temperature")]
    temperature: f32,
    #[serde(default = "default_max_tokens")]
    max_tokens: i32,
    #[serde(default)]
    enabled: bool,
    use_cases: UseCases,
}

#[derive(Debug, Deserialize)]
struct ConnectionInput {
    #[serde(default)]
    id: Option<i64>,
    display_name: String,
    base_url: String,
    model: String,
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default)]
    clear_api_key: bool,
    temperature: f32,
    max_tokens: i32,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct CreateConnectionRequest {
    revision: i64,
    display_name: String,
    base_url: String,
    api_key: String,
}

#[derive(Debug, Serialize)]
struct LlmConnectionResponse {
    id: i64,
    display_name: String,
    base_url: String,
    model: String,
    api_key_configured: bool,
    temperature: f32,
    max_tokens: i32,
    enabled: bool,
    status: ConnectionStatus,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct ListModelsRequest {
    #[serde(default)]
    connection_id: Option<i64>,
    #[serde(default)]
    base_url: String,
    #[serde(default)]
    api_key: Option<String>,
}

#[derive(Debug, Serialize)]
struct ListModelsResponse {
    items: Vec<ModelOption>,
}

#[derive(Debug, Serialize)]
struct ModelOption {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct TestConnectionRequest {
    #[serde(default)]
    connection_id: Option<i64>,
}

#[derive(Debug, Serialize)]
struct TestConnectionResponse {
    reply: String,
    latency_ms: i32,
}

#[derive(Debug, Deserialize)]
struct PolishArticleRequest {
    connection_id: i64,
    model: String,
    prompt: String,
    target: PolishTarget,
    #[serde(default)]
    title: String,
    #[serde(default)]
    summary: String,
    content_md: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PolishTarget {
    Summary,
    Content,
}

#[derive(Debug, Serialize)]
struct PolishArticleResponse {
    target: PolishTarget,
    text: String,
}

#[derive(Debug, Serialize)]
struct LlmSettingsResponse {
    revision: i64,
    connections: Vec<LlmConnectionResponse>,
    // These fields are retained as a compatibility projection for clients
    // written against the original singleton response.
    display_name: String,
    base_url: String,
    model: String,
    api_key_configured: bool,
    temperature: f32,
    max_tokens: i32,
    enabled: bool,
    use_cases: UseCases,
    status: ConnectionStatus,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct ConnectionStatus {
    state: String,
    tested_at: Option<DateTime<Utc>>,
    latency_ms: Option<i32>,
    error: Option<String>,
}

#[derive(Clone, Debug, FromRow)]
struct LlmSettingsRow {
    display_name: String,
    base_url: String,
    model: String,
    temperature: f32,
    max_tokens: i32,
    use_cases: Value,
    revision: i64,
    last_tested_at: Option<DateTime<Utc>>,
    last_test_status: String,
    last_test_latency_ms: Option<i32>,
    last_test_error: Option<String>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, FromRow)]
struct LlmConnectionRow {
    id: i64,
    display_name: String,
    base_url: String,
    model: String,
    api_key_ciphertext: Option<Vec<u8>>,
    api_key_nonce: Option<Vec<u8>>,
    encryption_key_version: Option<i32>,
    temperature: f32,
    max_tokens: i32,
    enabled: bool,
    last_tested_at: Option<DateTime<Utc>>,
    last_test_status: String,
    last_test_latency_ms: Option<i32>,
    last_test_error: Option<String>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
enum LlmError {
    #[error("需要有效的管理员会话")]
    Unauthorized,
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Upstream(String),
    #[error("{0}")]
    Internal(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl LlmError {
    fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }
}

impl IntoResponse for LlmError {
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
            Self::Upstream(message) => (StatusCode::BAD_GATEWAY, "upstream_unavailable", message),
            Self::Internal(message) => {
                error!(%message, "LLM settings operation failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "LLM 配置操作失败".to_owned(),
                )
            }
            Self::Database(database_error) => {
                error!(error = %database_error, "LLM settings database operation failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "LLM 配置操作失败".to_owned(),
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

async fn get_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<LlmSettingsResponse>, LlmError> {
    require_admin(&state, &headers)?;
    let row = load_settings(&state).await?;
    Ok(Json(to_response(&state, row).await?))
}

async fn create_connection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut request): Json<CreateConnectionRequest>,
) -> Result<Json<LlmSettingsResponse>, LlmError> {
    require_admin(&state, &headers)?;
    request.display_name = request.display_name.trim().to_owned();
    request.base_url = normalize_base_url(&request.base_url)?;
    validate_base_url_policy(&state, &request.base_url)?;
    request.api_key = request.api_key.trim().to_owned();
    if request.revision < 1 {
        return Err(LlmError::validation("revision 必须为正整数"));
    }
    validate_required_length(&request.display_name, "连接名称", MAX_NAME_CHARS)?;
    validate_required_length(&request.api_key, "API Key", MAX_API_KEY_CHARS)?;

    let current = load_settings(&state).await?;
    if current.revision != request.revision {
        return Err(LlmError::Conflict(
            "LLM 配置已在其他页面更新，请刷新后重试".to_owned(),
        ));
    }
    if load_connections(&state).await?.len() >= MAX_CONNECTIONS {
        return Err(LlmError::validation(format!(
            "最多保存 {MAX_CONNECTIONS} 条 LLM Key"
        )));
    }

    let started = Instant::now();
    let models = request_model_options(&state, &request.base_url, Some(&request.api_key)).await?;
    if models.is_empty() {
        return Err(LlmError::Upstream(
            "模型 API 未返回可用模型，Key 未保存".to_owned(),
        ));
    }
    let latency_ms = i32::try_from(started.elapsed().as_millis()).unwrap_or(i32::MAX);
    let (api_key_ciphertext, api_key_nonce, encryption_key_version) =
        encrypt_api_key(state.llm_keyring(), &request.api_key)?;

    let mut transaction = state.pool().begin().await?;
    let revision_updated = sqlx::query_scalar::<_, i64>(
        "UPDATE llm_settings
         SET revision = revision + 1
         WHERE id = 1 AND revision = $1
         RETURNING revision",
    )
    .bind(request.revision)
    .fetch_optional(&mut *transaction)
    .await?;
    if revision_updated.is_none() {
        return Err(LlmError::Conflict(
            "LLM 配置已在其他页面更新，请刷新后重试".to_owned(),
        ));
    }
    sqlx::query(
        "INSERT INTO llm_connections
         (display_name, base_url, model, api_key_ciphertext, api_key_nonce, encryption_key_version,
          temperature, max_tokens, enabled, last_tested_at, last_test_status,
          last_test_latency_ms, last_test_error)
         VALUES ($1, $2, '', $3, $4, $5, 0.7, 512, true, now(), 'online', $6, NULL)",
    )
    .bind(&request.display_name)
    .bind(&request.base_url)
    .bind(api_key_ciphertext)
    .bind(api_key_nonce)
    .bind(encryption_key_version)
    .bind(latency_ms)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;

    let row = load_settings(&state).await?;
    Ok(Json(to_response(&state, row).await?))
}

async fn update_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut request): Json<UpdateSettingsRequest>,
) -> Result<Json<LlmSettingsResponse>, LlmError> {
    require_admin(&state, &headers)?;
    normalize_and_validate(&mut request)?;
    if let Some(connections) = request.connections.as_ref() {
        for connection in connections {
            validate_base_url_policy(&state, &connection.base_url)?;
        }
    } else {
        validate_base_url_policy(&state, &request.base_url)?;
    }

    let current = load_settings(&state).await?;
    if current.revision != request.revision {
        return Err(LlmError::Conflict(
            "LLM 配置已在其他页面更新，请刷新后重试".to_owned(),
        ));
    }

    let existing_connections = load_connections(&state).await?;
    let collection_update = request.connections.is_some();
    let legacy_connection_id = existing_connections.first().map(|connection| connection.id);
    let connections = request.connections.take().unwrap_or_else(|| {
        vec![ConnectionInput {
            id: legacy_connection_id,
            display_name: request.display_name.clone(),
            base_url: request.base_url.clone(),
            model: request.model.clone(),
            api_key: request.api_key.clone(),
            clear_api_key: request.clear_api_key,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            enabled: request.enabled,
        }]
    });
    let existing_ids: std::collections::HashSet<i64> = existing_connections
        .iter()
        .map(|connection| connection.id)
        .collect();
    let requested_ids: std::collections::HashSet<i64> = connections
        .iter()
        .filter_map(|connection| connection.id)
        .collect();
    if requested_ids.iter().any(|id| !existing_ids.contains(id)) {
        return Err(LlmError::validation("连接记录不存在，请刷新后重试"));
    }
    if collection_update
        && connections.iter().any(|connection| {
            connection.id.and_then(|id| {
                existing_connections
                    .iter()
                    .find(|existing| existing.id == id)
                    .map(|existing| existing.base_url != connection.base_url)
            }) == Some(true)
        })
    {
        return Err(LlmError::validation(
            "更换 API 地址时请删除旧 Key，并通过“测试并保存”重新新增",
        ));
    }
    for use_case in [
        &request.use_cases.kanban_chat,
        &request.use_cases.comment_review,
        &request.use_cases.article_assistant,
    ] {
        if let Some(connection_id) = use_case.connection_id {
            if !requested_ids.contains(&connection_id) {
                return Err(LlmError::validation("场景引用了不存在或已删除的 Key"));
            }
            if use_case.enabled
                && connections
                    .iter()
                    .find(|connection| connection.id == Some(connection_id))
                    .is_some_and(|connection| !connection.enabled)
            {
                return Err(LlmError::validation("启用场景不能绑定已停用的 Key"));
            }
            if use_case.enabled {
                let connection = connections
                    .iter()
                    .find(|connection| connection.id == Some(connection_id))
                    .expect("requested connection id was validated above");
                let previous = existing_connections
                    .iter()
                    .find(|existing| existing.id == connection_id);
                let supplied_key = connection
                    .api_key
                    .as_deref()
                    .is_some_and(|key| !key.trim().is_empty());
                let keeps_saved_key = !connection.clear_api_key
                    && previous.is_some_and(|existing| existing.api_key_ciphertext.is_some());
                if !supplied_key && !keeps_saved_key {
                    return Err(LlmError::validation("启用场景不能绑定未配置凭据的 Key"));
                }
            }
            if use_case.enabled && use_case.model.trim().is_empty() {
                return Err(LlmError::validation("启用场景前必须选择模型"));
            }
        } else if use_case.enabled {
            return Err(LlmError::validation("启用场景前必须选择一个 Key 和模型"));
        }
    }

    let mut transaction = state.pool().begin().await?;
    for connection in &connections {
        let supplied_key = connection
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty());
        if connection.clear_api_key && supplied_key.is_some() {
            return Err(LlmError::validation(
                "不能同时填写新 API Key 和清除现有 API Key",
            ));
        }
        let previous = connection
            .id
            .and_then(|id| existing_connections.iter().find(|item| item.id == id));
        let credentials_changed = supplied_key.is_some() || connection.clear_api_key;
        let (api_key_ciphertext, api_key_nonce, encryption_key_version) =
            if let Some(api_key) = supplied_key {
                encrypt_api_key(state.llm_keyring(), api_key)?
            } else if connection.clear_api_key {
                (None, None, None)
            } else {
                // Preserve the database columns in-place. Copying the values
                // loaded above could overwrite a concurrent key rotation with
                // stale ciphertext after the row lock is released.
                (None, None, None)
            };
        let connection_test_invalidated = previous.is_none()
            || supplied_key.is_some()
            || connection.clear_api_key
            || previous.is_some_and(|item| item.base_url != connection.base_url);
        if let Some(id) = connection.id {
            sqlx::query(
                "UPDATE llm_connections
                 SET display_name = $1, base_url = $2, model = $3,
                     api_key_ciphertext = CASE WHEN $7 THEN $4 ELSE api_key_ciphertext END,
                     api_key_nonce = CASE WHEN $7 THEN $5 ELSE api_key_nonce END,
                     encryption_key_version = CASE WHEN $7 THEN $6 ELSE encryption_key_version END,
                     temperature = $8, max_tokens = $9, enabled = $10,
                     last_tested_at = CASE WHEN $11 THEN NULL ELSE last_tested_at END,
                     last_test_status = CASE WHEN $11 THEN 'untested' ELSE last_test_status END,
                     last_test_latency_ms = CASE WHEN $11 THEN NULL ELSE last_test_latency_ms END,
                     last_test_error = CASE WHEN $11 THEN NULL ELSE last_test_error END
                 WHERE id = $12",
            )
            .bind(&connection.display_name)
            .bind(&connection.base_url)
            .bind(&connection.model)
            .bind(api_key_ciphertext)
            .bind(api_key_nonce)
            .bind(encryption_key_version)
            .bind(credentials_changed)
            .bind(connection.temperature)
            .bind(connection.max_tokens)
            .bind(connection.enabled)
            .bind(connection_test_invalidated)
            .bind(id)
            .execute(&mut *transaction)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO llm_connections
                 (display_name, base_url, model, api_key_ciphertext, api_key_nonce,
                  encryption_key_version,
                  temperature, max_tokens, enabled)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            )
            .bind(&connection.display_name)
            .bind(&connection.base_url)
            .bind(&connection.model)
            .bind(api_key_ciphertext)
            .bind(api_key_nonce)
            .bind(encryption_key_version)
            .bind(connection.temperature)
            .bind(connection.max_tokens)
            .bind(connection.enabled)
            .execute(&mut *transaction)
            .await?;
        }
    }
    for id in existing_ids.difference(&requested_ids) {
        sqlx::query("DELETE FROM llm_connections WHERE id = $1")
            .bind(id)
            .execute(&mut *transaction)
            .await?;
    }
    let use_cases = serde_json::to_value(&request.use_cases)
        .map_err(|error| LlmError::Internal(error.to_string()))?;
    let row = sqlx::query_as::<_, LlmSettingsRow>(
        "UPDATE llm_settings
         SET use_cases = $1, revision = revision + 1
         WHERE id = 1 AND revision = $2
         RETURNING display_name, base_url, model, temperature, max_tokens, use_cases,
                   revision, last_tested_at, last_test_status,
                   last_test_latency_ms, last_test_error, updated_at",
    )
    .bind(use_cases)
    .bind(request.revision)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or_else(|| LlmError::Conflict("LLM 配置已在其他页面更新，请刷新后重试".to_owned()))?;
    transaction.commit().await?;

    Ok(Json(to_response(&state, row).await?))
}

async fn test_connection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<TestConnectionRequest>,
) -> Result<Json<TestConnectionResponse>, LlmError> {
    require_admin(&state, &headers)?;
    let connections = load_connections(&state).await?;
    let row = request
        .connection_id
        .and_then(|id| connections.iter().find(|connection| connection.id == id))
        .or_else(|| connections.first())
        .cloned()
        .ok_or_else(|| LlmError::validation("请先新增并保存一条 LLM Key"))?;

    let api_key = decrypt_api_key(
        state.llm_keyring(),
        row.encryption_key_version,
        row.api_key_ciphertext.as_deref(),
        row.api_key_nonce.as_deref(),
    )?;

    let started = Instant::now();
    let result = request_model_options(&state, &row.base_url, api_key.as_deref()).await;
    let latency_ms = i32::try_from(started.elapsed().as_millis()).unwrap_or(i32::MAX);

    match result {
        Ok(models) if !models.is_empty() => {
            record_test_result(&state, row.id, "online", latency_ms, None).await?;
            Ok(Json(TestConnectionResponse {
                reply: format!("连接成功，已读取 {} 个模型", models.len()),
                latency_ms,
            }))
        }
        Ok(_) => {
            let upstream_error = "模型 API 未返回可用模型".to_owned();
            record_test_result(&state, row.id, "error", latency_ms, Some(&upstream_error)).await?;
            Err(LlmError::Upstream(upstream_error))
        }
        Err(upstream_error) => {
            let stored_error = truncate(&upstream_error.to_string(), 500);
            record_test_result(&state, row.id, "error", latency_ms, Some(&stored_error)).await?;
            Err(upstream_error)
        }
    }
}

async fn polish_article(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut request): Json<PolishArticleRequest>,
) -> Result<Json<PolishArticleResponse>, LlmError> {
    require_admin(&state, &headers)?;
    request.model = request.model.trim().to_owned();
    request.prompt = request.prompt.trim().to_owned();
    request.title = request.title.trim().to_owned();
    request.summary = request.summary.trim().to_owned();

    validate_required_length(&request.model, "模型", MAX_MODEL_CHARS)?;
    validate_required_length(&request.prompt, "润色提示词", MAX_PROMPT_CHARS)?;
    validate_optional_length(&request.title, "文章标题", MAX_ARTICLE_TITLE_CHARS)?;
    validate_optional_length(&request.summary, "文章摘要", MAX_ARTICLE_SUMMARY_CHARS)?;
    validate_optional_length(&request.content_md, "文章正文", MAX_ARTICLE_SOURCE_CHARS)?;
    match request.target {
        PolishTarget::Summary if request.summary.trim().is_empty() => {
            return Err(LlmError::validation("请先填写需要润色的文章摘要"));
        }
        PolishTarget::Content if request.content_md.trim().is_empty() => {
            return Err(LlmError::validation("请先填写需要润色的文章正文"));
        }
        _ => {}
    }

    let connection = load_connections(&state)
        .await?
        .into_iter()
        .find(|connection| connection.id == request.connection_id)
        .ok_or_else(|| LlmError::validation("选择的 LLM Key 不存在，请重新选择"))?;
    if !connection.enabled {
        return Err(LlmError::validation("选择的 LLM Key 已停用，请重新选择"));
    }
    let api_key = decrypt_api_key(
        state.llm_keyring(),
        connection.encryption_key_version,
        connection.api_key_ciphertext.as_deref(),
        connection.api_key_nonce.as_deref(),
    )?
    .ok_or_else(|| LlmError::validation("选择的 LLM Key 尚未配置凭据"))?;

    let text = request_polished_text(&state, &connection, &api_key, &request).await?;
    Ok(Json(PolishArticleResponse {
        target: request.target,
        text,
    }))
}

async fn list_models(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ListModelsRequest>,
) -> Result<Json<ListModelsResponse>, LlmError> {
    require_admin(&state, &headers)?;
    let saved = if let Some(connection_id) = request.connection_id {
        load_connections(&state)
            .await?
            .into_iter()
            .find(|connection| connection.id == connection_id)
    } else {
        None
    };
    let base_url = if request.base_url.trim().is_empty() {
        saved
            .as_ref()
            .map(|connection| connection.base_url.clone())
            .ok_or_else(|| LlmError::validation("请先填写 API 地址"))?
    } else {
        request.base_url.clone()
    };
    let base_url = normalize_base_url(&base_url)?;
    validate_base_url_policy(&state, &base_url)?;
    let supplied_key = request
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty());
    let decrypted_key = if supplied_key.is_none()
        && saved
            .as_ref()
            .is_some_and(|connection| connection.base_url == base_url)
    {
        let saved = saved.as_ref().expect("saved connection checked above");
        decrypt_api_key(
            state.llm_keyring(),
            saved.encryption_key_version,
            saved.api_key_ciphertext.as_deref(),
            saved.api_key_nonce.as_deref(),
        )?
    } else {
        None
    };
    let api_key = supplied_key.or(decrypted_key.as_deref());
    if let Some(api_key) = api_key {
        if api_key.chars().count() > MAX_API_KEY_CHARS {
            return Err(LlmError::validation("API Key 不能超过 4096 个字符"));
        }
    }

    let items = request_model_options(&state, &base_url, api_key).await?;
    Ok(Json(ListModelsResponse { items }))
}

async fn request_model_options(
    state: &AppState,
    base_url: &str,
    api_key: Option<&str>,
) -> Result<Vec<ModelOption>, LlmError> {
    let endpoint = models_endpoint(base_url).map_err(LlmError::validation)?;
    let response = state
        .llm_http_client()
        .get(endpoint, api_key, Duration::from_secs(8))
        .await
        .map_err(|error| LlmError::Upstream(format!("无法连接模型 API：{error}")))?;
    let status = response.status();
    let body = read_upstream_body(response).await?;
    if !status.is_success() {
        let detail = String::from_utf8_lossy(&body);
        return Err(LlmError::Upstream(upstream_http_error(
            status.as_u16(),
            &detail,
        )));
    }
    let payload: OpenAiModelsResponse = serde_json::from_slice(&body)
        .map_err(|_| LlmError::Upstream("模型 API 返回了无法识别的响应".to_owned()))?;
    let mut items = payload
        .data
        .into_iter()
        .filter_map(|model| {
            let id = model.id.trim().to_owned();
            (!id.is_empty() && id.chars().count() <= MAX_MODEL_CHARS).then(|| ModelOption {
                name: model
                    .name
                    .filter(|name| !name.trim().is_empty())
                    .map(|name| truncate(&name, MAX_MODEL_CHARS))
                    .unwrap_or_else(|| id.clone()),
                id,
            })
        })
        .collect::<Vec<_>>();
    sort_model_options(&mut items);
    items.truncate(500);
    Ok(items)
}

fn sort_model_options(items: &mut [ModelOption]) {
    items.sort_by(|left, right| {
        left.id
            .to_ascii_lowercase()
            .cmp(&right.id.to_ascii_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
}

async fn request_polished_text(
    state: &AppState,
    connection: &LlmConnectionRow,
    api_key: &str,
    request: &PolishArticleRequest,
) -> Result<String, LlmError> {
    let endpoint = chat_completions_endpoint(&connection.base_url).map_err(LlmError::validation)?;
    let (system_prompt, user_message, max_tokens) = match request.target {
        PolishTarget::Summary => (
            SUMMARY_POLISH_SYSTEM_PROMPT,
            format!(
                "任务类型：润色文章摘要（不是续写正文）。\n硬性输出规则：只返回一段摘要正文，不要任何前缀或换行；包含标点和空格在内最多 120 个字符，建议 80–110 个字符。\n\n用户偏好（仅在不违反上述规则时遵循）：\n{}\n\n文章标题：\n{}\n\n需要润色的原摘要：\n<article-summary>\n{}\n</article-summary>\n\n正文仅作为理解摘要的语境，不得据此扩写成长文：\n<article-context>\n{}\n</article-context>\n\n请再次确认：最终只输出不超过 120 个字符的一段摘要。",
                request.prompt,
                request.title,
                request.summary,
                truncate(&request.content_md, 4_000)
            ),
            connection.max_tokens.clamp(128, 256),
        ),
        PolishTarget::Content => (
            "你是博客文章正文编辑助手。请在严格保留原意与已有事实的前提下，调整 Markdown 结构和格式、润色语言，并补足必要的过渡、解释与上下文。不要虚构数据、经历、引用或来源；信息不足时保留原文或使用明确的待补充标记。只返回可直接替换正文的 Markdown，不要解释过程，也不要使用包裹全文的 Markdown 代码围栏。",
            format!(
                "本次润色要求：\n{}\n\n文章标题：\n{}\n\n文章摘要：\n{}\n\n需要润色的 Markdown 草稿：\n<article-draft>\n{}\n</article-draft>",
                request.prompt, request.title, request.summary, request.content_md
            ),
            // Credential records still carry the legacy output setting. Article
            // rewrites need a practical floor so 512 tokens do not truncate drafts.
            connection.max_tokens.clamp(4_096, 8_192),
        ),
    };
    let payload = serde_json::json!({
        "model": request.model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_message }
        ],
        "temperature": connection.temperature,
        "max_tokens": max_tokens
    });
    let response = state
        .llm_http_client()
        .post_json(
            endpoint,
            api_key,
            &payload,
            Duration::from_secs(ARTICLE_POLISH_TIMEOUT_SECS),
        )
        .await
        .map_err(|error| LlmError::Upstream(format!("无法连接模型 API：{error}")))?;
    let status = response.status();
    let body = read_upstream_body(response).await?;
    if !status.is_success() {
        let detail = String::from_utf8_lossy(&body);
        return Err(LlmError::Upstream(upstream_http_error(
            status.as_u16(),
            &detail,
        )));
    }
    let payload: Value = serde_json::from_slice(&body)
        .map_err(|_| LlmError::Upstream("模型 API 返回了无法识别的响应".to_owned()))?;
    let mut text = extract_polished_content(&payload)
        .ok_or_else(|| LlmError::Upstream("模型未返回可用的润色结果".to_owned()))?;
    if request.target == PolishTarget::Summary {
        text = collapse_whitespace(&text);
    }
    if request.target == PolishTarget::Summary && text.chars().count() > MAX_ARTICLE_SUMMARY_CHARS {
        return Err(LlmError::Upstream(format!(
            "模型返回的摘要超过 {MAX_ARTICLE_SUMMARY_CHARS} 个字符，请调整提示词后重试"
        )));
    }
    Ok(text)
}

fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<(), LlmError> {
    if auth::has_valid_admin_session(state, headers) {
        Ok(())
    } else {
        Err(LlmError::Unauthorized)
    }
}

async fn load_settings(state: &AppState) -> Result<LlmSettingsRow, LlmError> {
    sqlx::query_as::<_, LlmSettingsRow>(
        "SELECT display_name, base_url, model, temperature, max_tokens, use_cases,
                revision, last_tested_at, last_test_status,
                last_test_latency_ms, last_test_error, updated_at
         FROM llm_settings WHERE id = 1",
    )
    .fetch_one(state.pool())
    .await
    .map_err(Into::into)
}

async fn load_connections(state: &AppState) -> Result<Vec<LlmConnectionRow>, LlmError> {
    sqlx::query_as::<_, LlmConnectionRow>(
        "SELECT id, display_name, base_url, model, api_key_ciphertext,
                api_key_nonce, encryption_key_version, temperature, max_tokens, enabled, last_tested_at,
                last_test_status, last_test_latency_ms, last_test_error, updated_at
         FROM llm_connections
         ORDER BY id",
    )
    .fetch_all(state.pool())
    .await
    .map_err(Into::into)
}

async fn to_response(
    state: &AppState,
    row: LlmSettingsRow,
) -> Result<LlmSettingsResponse, LlmError> {
    let use_cases: UseCases = serde_json::from_value(row.use_cases.clone())
        .map_err(|error| LlmError::Internal(error.to_string()))?;
    let connections = load_connections(state).await?;
    let connection_responses = connections
        .iter()
        .map(to_connection_response)
        .collect::<Vec<_>>();
    let primary = connections.first();

    Ok(LlmSettingsResponse {
        revision: row.revision,
        connections: connection_responses,
        display_name: primary
            .map(|connection| connection.display_name.clone())
            .unwrap_or(row.display_name),
        base_url: primary
            .map(|connection| connection.base_url.clone())
            .unwrap_or(row.base_url),
        model: primary
            .map(|connection| connection.model.clone())
            .unwrap_or(row.model),
        api_key_configured: primary
            .map(|connection| connection.api_key_ciphertext.is_some())
            .unwrap_or(false),
        temperature: primary
            .map(|connection| connection.temperature)
            .unwrap_or(row.temperature),
        max_tokens: primary
            .map(|connection| connection.max_tokens)
            .unwrap_or(row.max_tokens),
        enabled: primary
            .map(|connection| connection.enabled)
            .unwrap_or(false),
        use_cases,
        status: primary.map(connection_status).unwrap_or(ConnectionStatus {
            state: row.last_test_status,
            tested_at: row.last_tested_at,
            latency_ms: row.last_test_latency_ms,
            error: row.last_test_error,
        }),
        updated_at: row.updated_at,
    })
}

fn to_connection_response(row: &LlmConnectionRow) -> LlmConnectionResponse {
    LlmConnectionResponse {
        id: row.id,
        display_name: row.display_name.clone(),
        base_url: row.base_url.clone(),
        model: row.model.clone(),
        api_key_configured: row.api_key_ciphertext.is_some(),
        temperature: row.temperature,
        max_tokens: row.max_tokens,
        enabled: row.enabled,
        status: connection_status(row),
        updated_at: row.updated_at,
    }
}

fn connection_status(row: &LlmConnectionRow) -> ConnectionStatus {
    ConnectionStatus {
        state: row.last_test_status.clone(),
        tested_at: row.last_tested_at,
        latency_ms: row.last_test_latency_ms,
        error: row.last_test_error.clone(),
    }
}

fn normalize_and_validate(request: &mut UpdateSettingsRequest) -> Result<(), LlmError> {
    request.display_name = request.display_name.trim().to_owned();
    request.model = request.model.trim().to_owned();

    if request.revision < 1 {
        return Err(LlmError::validation("revision 必须为正整数"));
    }
    if let Some(connections) = request.connections.as_mut() {
        if connections.len() > MAX_CONNECTIONS {
            return Err(LlmError::validation(format!(
                "最多保存 {MAX_CONNECTIONS} 条 LLM Key"
            )));
        }
        let mut ids = std::collections::HashSet::with_capacity(connections.len());
        for connection in connections {
            let Some(id) = connection.id else {
                return Err(LlmError::validation(
                    "新增 Key 必须通过“测试并保存”接口完成",
                ));
            };
            if !ids.insert(id) {
                return Err(LlmError::validation("连接列表不能包含重复的 Key"));
            }
            if connection.api_key.is_some() || connection.clear_api_key {
                return Err(LlmError::validation(
                    "更换或清除凭据时请删除旧 Key，并通过“测试并保存”重新新增",
                ));
            }
            connection.display_name = connection.display_name.trim().to_owned();
            connection.model = connection.model.trim().to_owned();
            connection.base_url = normalize_base_url(&connection.base_url)?;
            validate_required_length(&connection.display_name, "连接名称", MAX_NAME_CHARS)?;
            if connection.model.chars().count() > MAX_MODEL_CHARS {
                return Err(LlmError::validation(format!(
                    "模型不能超过 {MAX_MODEL_CHARS} 个字符"
                )));
            }
            if let Some(api_key) = connection.api_key.as_deref() {
                if api_key.chars().count() > MAX_API_KEY_CHARS {
                    return Err(LlmError::validation("API Key 不能超过 4096 个字符"));
                }
            }
            if !(0.0..=2.0).contains(&connection.temperature) {
                return Err(LlmError::validation("Temperature 必须在 0 到 2 之间"));
            }
            if !(1..=8192).contains(&connection.max_tokens) {
                return Err(LlmError::validation("最大输出 Token 必须在 1 到 8192 之间"));
            }
        }
    } else {
        request.base_url = normalize_base_url(&request.base_url)?;
        validate_required_length(&request.display_name, "连接名称", MAX_NAME_CHARS)?;
        validate_required_length(&request.model, "模型", MAX_MODEL_CHARS)?;
        if !(0.0..=2.0).contains(&request.temperature) {
            return Err(LlmError::validation("Temperature 必须在 0 到 2 之间"));
        }
        if !(1..=8192).contains(&request.max_tokens) {
            return Err(LlmError::validation("最大输出 Token 必须在 1 到 8192 之间"));
        }
    }
    for (name, use_case) in [
        ("看板娘对话", &mut request.use_cases.kanban_chat),
        ("评论预审", &mut request.use_cases.comment_review),
        ("文章助手", &mut request.use_cases.article_assistant),
    ] {
        use_case.system_prompt = use_case.system_prompt.trim().to_owned();
        use_case.model = use_case.model.trim().to_owned();
        if use_case.model.chars().count() > MAX_MODEL_CHARS {
            return Err(LlmError::validation(format!(
                "{name}模型不能超过 {MAX_MODEL_CHARS} 个字符"
            )));
        }
        if use_case.enabled && use_case.system_prompt.is_empty() {
            return Err(LlmError::validation(format!(
                "启用{name}前必须填写系统提示词"
            )));
        }
        if use_case.system_prompt.chars().count() > MAX_PROMPT_CHARS {
            return Err(LlmError::validation(format!(
                "{name}系统提示词不能超过 {MAX_PROMPT_CHARS} 个字符"
            )));
        }
    }
    Ok(())
}

fn validate_required_length(value: &str, label: &str, max: usize) -> Result<(), LlmError> {
    let count = value.chars().count();
    if count == 0 || count > max {
        return Err(LlmError::validation(format!(
            "{label}长度必须在 1 到 {max} 个字符之间"
        )));
    }
    Ok(())
}

fn validate_optional_length(value: &str, label: &str, max: usize) -> Result<(), LlmError> {
    if value.chars().count() > max {
        return Err(LlmError::validation(format!(
            "{label}不能超过 {max} 个字符"
        )));
    }
    Ok(())
}

fn normalize_base_url(value: &str) -> Result<String, LlmError> {
    let mut url = Url::parse(value.trim())
        .map_err(|_| LlmError::validation("API 地址必须是完整的 HTTP(S) URL"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(LlmError::validation(
            "API 地址只能包含 http(s) 协议、主机、端口和路径",
        ));
    }
    let path = url.path().trim_end_matches('/').to_owned();
    if path.ends_with("/models") || path.ends_with("/chat/completions") {
        return Err(LlmError::validation(
            "API 地址请填写服务根地址（例如 https://api.example.com/v1），不要填写具体资源路径",
        ));
    }
    url.set_path(if path.is_empty() { "/" } else { &path });
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

fn validate_base_url_policy(state: &AppState, base_url: &str) -> Result<(), LlmError> {
    let url = Url::parse(base_url).map_err(|_| LlmError::validation("LLM API 地址无效"))?;
    state
        .llm_http_client()
        .validate_configured_url(&url)
        .map_err(|error| LlmError::validation(format!("LLM API 地址不符合网络策略：{error}")))
}

fn encrypt_api_key(keyring: &LlmKeyring, api_key: &str) -> Result<EncryptedApiKey, LlmError> {
    let encrypted = keyring
        .encrypt(api_key)
        .map_err(|error| LlmError::Internal(error.to_string()))?;
    Ok((
        Some(encrypted.ciphertext),
        Some(encrypted.nonce),
        Some(encrypted.key_version),
    ))
}

fn decrypt_api_key(
    keyring: &LlmKeyring,
    key_version: Option<i32>,
    ciphertext: Option<&[u8]>,
    nonce: Option<&[u8]>,
) -> Result<Option<String>, LlmError> {
    keyring
        .decrypt_optional(key_version, ciphertext, nonce)
        .map_err(|error| LlmError::Internal(error.to_string()))
}

fn models_endpoint(base_url: &str) -> Result<Url, String> {
    openai_endpoint(base_url, "models")
}

fn chat_completions_endpoint(base_url: &str) -> Result<Url, String> {
    openai_endpoint(base_url, "chat/completions")
}

fn openai_endpoint(base_url: &str, resource: &str) -> Result<Url, String> {
    let base = base_url.trim_end_matches('/');
    let parsed = Url::parse(base).map_err(|_| "LLM API 地址无效".to_owned())?;
    let endpoint = if base.ends_with(&format!("/{resource}")) {
        base.to_owned()
    } else if parsed.path().is_empty() || parsed.path() == "/" {
        format!("{base}/v1/{resource}")
    } else {
        format!("{base}/{resource}")
    };
    Url::parse(&endpoint).map_err(|_| "LLM API 地址无效".to_owned())
}

fn upstream_http_error(status: u16, detail: &str) -> String {
    let detail = detail.replace(['\r', '\n'], " ");
    let detail = truncate(detail.trim(), 300);
    if detail.is_empty() {
        format!("模型 API 返回 HTTP {status}")
    } else {
        format!("模型 API 返回 HTTP {status}：{detail}")
    }
}

async fn read_upstream_body(mut response: reqwest::Response) -> Result<Vec<u8>, LlmError> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| LlmError::Upstream(format!("读取模型 API 响应失败：{error}")))?
    {
        if body.len().saturating_add(chunk.len()) > MAX_UPSTREAM_RESPONSE_BYTES {
            return Err(LlmError::Upstream(format!(
                "模型 API 响应超过 {} MiB 限制",
                MAX_UPSTREAM_RESPONSE_BYTES / (1024 * 1024)
            )));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn extract_polished_content(payload: &Value) -> Option<String> {
    let content = payload.pointer("/choices/0/message/content")?;
    let raw = if let Some(text) = content.as_str() {
        text.to_owned()
    } else {
        content
            .as_array()?
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("")
    };
    let normalized = strip_outer_markdown_fence(&raw);
    (!normalized.is_empty()).then(|| normalized.to_owned())
}

fn strip_outer_markdown_fence(value: &str) -> &str {
    let trimmed = value.trim();
    let Some(after_opening) = trimmed.strip_prefix("```") else {
        return trimmed;
    };
    let Some((language, body)) = after_opening.split_once('\n') else {
        return trimmed;
    };
    if !matches!(
        language.trim().to_ascii_lowercase().as_str(),
        "" | "md" | "markdown"
    ) {
        return trimmed;
    }
    body.strip_suffix("```").map(str::trim).unwrap_or(trimmed)
}

async fn record_test_result(
    state: &AppState,
    connection_id: i64,
    status: &str,
    latency_ms: i32,
    error_message: Option<&str>,
) -> Result<(), LlmError> {
    sqlx::query(
        "UPDATE llm_connections
         SET last_tested_at = now(),
             last_test_status = $1,
             last_test_latency_ms = $2,
             last_test_error = $3
         WHERE id = $4",
    )
    .bind(status)
    .bind(latency_ms)
    .bind(error_message)
    .bind(connection_id)
    .execute(state.pool())
    .await?;
    Ok(())
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Debug, Deserialize)]
struct OpenAiModelsResponse {
    #[serde(default)]
    data: Vec<OpenAiModel>,
}

#[derive(Debug, Deserialize)]
struct OpenAiModel {
    id: String,
    #[serde(default)]
    name: Option<String>,
}

fn default_temperature() -> f32 {
    0.7
}

fn default_max_tokens() -> i32 {
    512
}

#[cfg(test)]
mod tests {
    use super::{
        ModelOption, SUMMARY_POLISH_SYSTEM_PROMPT, UpdateSettingsRequest,
        chat_completions_endpoint, collapse_whitespace, decrypt_api_key, encrypt_api_key,
        extract_polished_content, models_endpoint, normalize_and_validate, normalize_base_url,
        sort_model_options,
    };
    use crate::llm_crypto::LlmKeyring;
    use serde_json::json;

    #[test]
    fn api_key_encryption_round_trips_without_plaintext_storage() {
        let keyring = LlmKeyring::new(7, "test encryption secret", None).expect("keyring");
        let (ciphertext, nonce, key_version) =
            encrypt_api_key(&keyring, "secret-key").expect("encrypt");
        assert_ne!(ciphertext.as_deref(), Some("secret-key".as_bytes()));
        assert_eq!(
            decrypt_api_key(
                &keyring,
                key_version,
                ciphertext.as_deref(),
                nonce.as_deref()
            )
            .expect("decrypt"),
            Some("secret-key".to_owned())
        );
    }

    #[test]
    fn api_endpoints_preserve_versioned_base_paths() {
        assert_eq!(
            models_endpoint("https://api.openai.com/v1")
                .expect("models endpoint")
                .as_str(),
            "https://api.openai.com/v1/models"
        );
    }

    #[test]
    fn api_endpoints_add_v1_for_origin_only_urls() {
        assert_eq!(
            models_endpoint("https://cli.example.com")
                .expect("models endpoint")
                .as_str(),
            "https://cli.example.com/v1/models"
        );
    }

    #[test]
    fn chat_completion_endpoint_preserves_versioned_base_paths() {
        assert_eq!(
            chat_completions_endpoint("https://api.openai.com/v1")
                .expect("chat endpoint")
                .as_str(),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn polished_content_accepts_text_and_removes_an_outer_fence() {
        let payload = json!({
            "choices": [{"message": {"content": "```markdown\n# 标题\n\n正文\n```"}}]
        });
        assert_eq!(
            extract_polished_content(&payload).as_deref(),
            Some("# 标题\n\n正文")
        );
    }

    #[test]
    fn summary_prompt_makes_target_and_character_limit_explicit() {
        assert!(SUMMARY_POLISH_SYSTEM_PROMPT.contains("只能润色摘要"));
        assert!(SUMMARY_POLISH_SYSTEM_PROMPT.contains("不超过 120 个字符"));
        assert!(SUMMARY_POLISH_SYSTEM_PROMPT.contains("不是续写、扩写或改写文章正文"));
    }

    #[test]
    fn model_options_are_sorted_by_identifier() {
        let mut items = vec![
            ModelOption {
                id: "gpt-z".to_owned(),
                name: "Z".to_owned(),
            },
            ModelOption {
                id: "Claude-A".to_owned(),
                name: "A".to_owned(),
            },
            ModelOption {
                id: "gpt-a".to_owned(),
                name: "A".to_owned(),
            },
        ];
        sort_model_options(&mut items);
        assert_eq!(
            items.into_iter().map(|item| item.id).collect::<Vec<_>>(),
            ["Claude-A", "gpt-a", "gpt-z"]
        );
    }

    #[test]
    fn base_url_rejects_credentials_and_query_strings() {
        assert_eq!(
            normalize_base_url("https://example.com/v1/").expect("valid URL"),
            "https://example.com/v1"
        );
        assert!(normalize_base_url("https://user:secret@example.com/v1").is_err());
        assert!(normalize_base_url("https://example.com/v1?key=secret").is_err());
        assert!(normalize_base_url("https://example.com/v1/models").is_err());
        assert!(normalize_base_url("https://example.com/v1/chat/completions").is_err());
    }

    #[test]
    fn summary_output_is_collapsed_to_one_paragraph() {
        assert_eq!(
            collapse_whitespace("第一句。\n\n  第二句。\t第三句。"),
            "第一句。 第二句。 第三句。"
        );
    }

    #[test]
    fn collection_update_accepts_payload_without_legacy_singleton_fields() {
        let mut request: UpdateSettingsRequest = serde_json::from_value(json!({
            "revision": 1,
            "connections": [{
                "id": 3,
                "display_name": "Primary",
                "base_url": "https://api.example.com/v1",
                "model": "",
                "clear_api_key": false,
                "temperature": 0.7,
                "max_tokens": 512,
                "enabled": true
            }],
            "use_cases": {
                "kanban_chat": {
                    "enabled": false,
                    "system_prompt": "Chat",
                    "connection_id": null,
                    "model": ""
                },
                "comment_review": {
                    "enabled": false,
                    "system_prompt": "Review",
                    "connection_id": null,
                    "model": ""
                },
                "article_assistant": {
                    "enabled": false,
                    "system_prompt": "Write",
                    "connection_id": null,
                    "model": ""
                }
            }
        }))
        .expect("the collection UI payload should deserialize");

        assert_eq!(request.temperature, 0.7);
        assert_eq!(request.max_tokens, 512);
        assert!(!request.enabled);
        normalize_and_validate(&mut request).expect("the collection UI payload should validate");
    }

    #[test]
    fn collection_update_cannot_bypass_tested_connection_creation() {
        let mut request: UpdateSettingsRequest = serde_json::from_value(json!({
            "revision": 1,
            "connections": [{
                "display_name": "Untested",
                "base_url": "https://api.example.com/v1",
                "model": "",
                "api_key": "secret",
                "clear_api_key": false,
                "temperature": 0.7,
                "max_tokens": 512,
                "enabled": true
            }],
            "use_cases": {
                "kanban_chat": { "enabled": false, "system_prompt": "Chat" },
                "comment_review": { "enabled": false, "system_prompt": "Review" },
                "article_assistant": { "enabled": false, "system_prompt": "Write" }
            }
        }))
        .expect("payload should deserialize before semantic validation");

        assert!(normalize_and_validate(&mut request).is_err());
    }

    #[test]
    fn collection_update_rejects_duplicate_connection_ids() {
        let connection = json!({
            "id": 3,
            "display_name": "Primary",
            "base_url": "https://api.example.com/v1",
            "model": "",
            "clear_api_key": false,
            "temperature": 0.7,
            "max_tokens": 512,
            "enabled": true
        });
        let mut request: UpdateSettingsRequest = serde_json::from_value(json!({
            "revision": 1,
            "connections": [connection.clone(), connection],
            "use_cases": {
                "kanban_chat": { "enabled": false, "system_prompt": "Chat" },
                "comment_review": { "enabled": false, "system_prompt": "Review" },
                "article_assistant": { "enabled": false, "system_prompt": "Write" }
            }
        }))
        .expect("payload should deserialize before semantic validation");

        assert!(normalize_and_validate(&mut request).is_err());
    }
}
