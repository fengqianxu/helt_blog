use std::time::{SystemTime, UNIX_EPOCH};

use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CONTENT_TYPE, COOKIE, SET_COOKIE},
    },
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
};
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rand::{
    RngCore,
    distr::{Alphanumeric, SampleString},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use sqlx::FromRow;
use tracing::{error, info, warn};
use webauthn_rs::prelude::{CredentialID, RegisterPublicKeyCredential, Uuid};

use crate::{
    error::{ErrorBody, ErrorEnvelope},
    routes::{bangumi, contract::HttpMethod, games},
    state::AppState,
    storage_gc,
};

const ACCESS_COOKIE: &str = "helt_admin_access";
const REFRESH_COOKIE: &str = "helt_admin_refresh";
const ACCESS_TTL_SECONDS: i64 = 2 * 60 * 60;
const REFRESH_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;
const JWT_ISSUER: &str = "helt-blog";
const MAX_AVATAR_BYTES: usize = 512 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/profile", get(public_profile))
        .route("/api/v1/admin/auth/login", post(login))
        .route("/api/v1/admin/auth/refresh", post(refresh))
        .route("/api/v1/admin/auth/logout", post(logout))
        .route("/api/v1/admin/auth/me", get(me))
        .route("/api/v1/admin/auth/profile", patch(update_profile))
        .route(
            "/api/v1/admin/auth/avatar",
            post(upload_avatar).delete(remove_avatar),
        )
        .route("/api/v1/admin/auth/change-password", post(change_password))
        .route(
            "/api/v1/admin/auth/passkeys",
            get(list_passkeys).post(register_passkey),
        )
        .route(
            "/api/v1/admin/auth/passkeys/options",
            post(passkey_registration_options),
        )
        .route("/api/v1/admin/auth/passkeys/{id}", delete(delete_passkey))
        .route("/api/v1/admin/auth/forgot-password", post(forgot_password))
}

pub fn implements(method: HttpMethod, path: &str) -> bool {
    matches!(
        (method, path),
        (HttpMethod::Get, "/api/v1/profile")
            | (HttpMethod::Post, "/api/v1/admin/auth/login")
            | (HttpMethod::Post, "/api/v1/admin/auth/refresh")
            | (HttpMethod::Post, "/api/v1/admin/auth/logout")
            | (HttpMethod::Get, "/api/v1/admin/auth/me")
            | (HttpMethod::Patch, "/api/v1/admin/auth/profile")
            | (HttpMethod::Post, "/api/v1/admin/auth/avatar")
            | (HttpMethod::Delete, "/api/v1/admin/auth/avatar")
            | (HttpMethod::Post, "/api/v1/admin/auth/change-password")
            | (HttpMethod::Get, "/api/v1/admin/auth/passkeys")
            | (HttpMethod::Post, "/api/v1/admin/auth/passkeys/options")
            | (HttpMethod::Post, "/api/v1/admin/auth/passkeys")
            | (HttpMethod::Delete, "/api/v1/admin/auth/passkeys/{id}")
            | (HttpMethod::Post, "/api/v1/admin/auth/forgot-password")
    )
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
    #[serde(default)]
    remember: bool,
    totp_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ForgotPasswordRequest {
    username: String,
}

#[derive(Debug, Deserialize)]
struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
    #[serde(default)]
    revoke_other_sessions: bool,
}

#[derive(Debug, Deserialize)]
struct UpdateProfileRequest {
    email: String,
    bilibili_uid: String,
    #[serde(default)]
    steam_web_api_key: String,
    #[serde(default)]
    clear_steam_web_api_key: bool,
    #[serde(default)]
    steam_id64: String,
    avatar_asset_id: Option<i64>,
    #[serde(default)]
    avatar_crop_x: f32,
    #[serde(default)]
    avatar_crop_y: f32,
    #[serde(default = "default_avatar_zoom")]
    avatar_crop_zoom: f32,
}

#[derive(Debug, Deserialize)]
struct RegisterPasskeyRequest {
    #[serde(flatten)]
    credential: RegisterPublicKeyCredential,
    label: Option<String>,
}

#[derive(Debug, FromRow)]
struct LoginUser {
    id: i64,
    username: String,
    password_hash: String,
    totp_secret: Option<String>,
}

#[derive(Debug, Serialize)]
struct AdminIdentity {
    username: String,
    email: String,
    avatar_url: Option<String>,
    avatar_asset_id: Option<i64>,
    avatar_crop_x: f32,
    avatar_crop_y: f32,
    avatar_crop_zoom: f32,
    bilibili_uid: String,
    steam_web_api_key_configured: bool,
    steam_web_api_key_masked: String,
    steam_id64: String,
}

#[derive(Debug, FromRow, Serialize)]
struct PublicProfile {
    username: String,
    email: String,
    avatar_url: Option<String>,
}

#[derive(Debug, FromRow)]
struct AdminProfileRow {
    username: String,
    email: String,
    avatar_url: Option<String>,
    avatar_asset_id: Option<i64>,
    avatar_crop_x: f32,
    avatar_crop_y: f32,
    avatar_crop_zoom: f32,
    bilibili_uid: String,
    steam_web_api_key_configured: bool,
    steam_id64: String,
}

#[derive(Debug, Default, FromRow)]
struct StoredProfileSyncSettings {
    bilibili_uid: String,
    steam_web_api_key_ciphertext: Option<Vec<u8>>,
    steam_web_api_key_nonce: Option<Vec<u8>>,
    steam_encryption_key_version: Option<i32>,
    steam_id64: String,
}

struct AvatarMedia {
    mime: &'static str,
    extension: &'static str,
}

#[derive(Debug, Serialize)]
struct LoginResponse {
    admin: AdminIdentity,
}

#[derive(Debug, Serialize)]
struct MessageResponse {
    message: &'static str,
}

#[derive(Debug, FromRow, Serialize)]
struct PasskeyItem {
    id: i64,
    label: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct PasskeyListResponse {
    items: Vec<PasskeyItem>,
}

async fn public_profile(State(state): State<AppState>) -> Result<Json<PublicProfile>, ApiError> {
    let profile = sqlx::query_as::<_, PublicProfile>(
        "SELECT username, email, avatar_url
         FROM admin_users
         ORDER BY id
         LIMIT 1",
    )
    .fetch_optional(state.pool())
    .await
    .map_err(|err| {
        error!(error = %err, "public profile could not be loaded");
        ApiError::Internal
    })?
    .ok_or(ApiError::Internal)?;

    Ok(Json(profile))
}

#[derive(Debug, Serialize, Deserialize)]
struct AccessClaims {
    sub: i64,
    username: String,
    iss: String,
    iat: usize,
    exp: usize,
}

#[derive(Debug)]
enum ApiError {
    Unauthorized,
    CurrentPasswordInvalid,
    Validation(&'static str),
    NotFound(&'static str),
    Conflict(&'static str),
    RateLimited,
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
                "账号、密码或动态验证码不正确",
            ),
            Self::CurrentPasswordInvalid => (
                StatusCode::UNAUTHORIZED,
                "current_password_invalid",
                "当前密码不正确",
            ),
            Self::Validation(message) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_error",
                message,
            ),
            Self::NotFound(message) => (StatusCode::NOT_FOUND, "not_found", message),
            Self::Conflict(message) => (StatusCode::CONFLICT, "conflict", message),
            Self::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_attempts",
                "认证失败次数过多，请十五分钟后再试",
            ),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "服务暂时不可用，请稍后重试",
            ),
        };

        (
            status,
            Json(ErrorEnvelope {
                error: ErrorBody {
                    code,
                    message: message.to_owned(),
                },
            }),
        )
            .into_response()
    }
}

async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Result<Response, ApiError> {
    let username = request.username.trim();
    if username.is_empty() || username.chars().count() > 64 {
        return Err(ApiError::Validation("请输入有效账号"));
    }
    if request.password.is_empty() || request.password.chars().count() > 512 {
        return Err(ApiError::Validation("请输入有效密码"));
    }

    let rate_key = auth_rate_key(&headers, username);
    if state.auth_rate_limited(&rate_key) {
        return Err(ApiError::RateLimited);
    }

    let user = sqlx::query_as::<_, LoginUser>(
        "SELECT id, username, password_hash, totp_secret FROM admin_users WHERE username = $1",
    )
    .bind(username)
    .fetch_optional(state.pool())
    .await
    .map_err(|err| {
        error!(error = %err, "administrator lookup failed");
        ApiError::Internal
    })?;

    let supplied_password = request.password;
    let stored_hash = user
        .as_ref()
        .map(|candidate| candidate.password_hash.clone());
    let password_valid = tokio::task::spawn_blocking(move || {
        verify_password_or_run_dummy(stored_hash.as_deref(), &supplied_password)
    })
    .await
    .map_err(|err| {
        error!(error = %err, "password verification task failed");
        ApiError::Internal
    })?;

    let totp_valid = user
        .as_ref()
        .and_then(|candidate| candidate.totp_secret.as_deref())
        .is_none_or(|secret| {
            request
                .totp_code
                .as_deref()
                .is_some_and(|code| verify_totp(secret, code))
        });

    let Some(user) = user.filter(|_| password_valid && totp_valid) else {
        state.record_auth_failure(&rate_key);
        return Err(ApiError::Unauthorized);
    };

    state.clear_auth_failures(&rate_key);
    let access_token = create_access_token(&state, &user)?;
    let mut response_headers = HeaderMap::new();
    append_cookie(&mut response_headers, access_cookie(&state, &access_token))?;

    if request.remember {
        let refresh_token = random_token();
        let refresh_hash = hash_token(&refresh_token);
        let expires_at = Utc::now() + Duration::seconds(REFRESH_TTL_SECONDS);
        sqlx::query(
            "INSERT INTO refresh_tokens (user_id, token_hash, expires_at) VALUES ($1, $2, $3)",
        )
        .bind(user.id)
        .bind(refresh_hash)
        .bind(expires_at)
        .execute(state.pool())
        .await
        .map_err(|err| {
            error!(error = %err, "refresh token persistence failed");
            ApiError::Internal
        })?;
        append_cookie(
            &mut response_headers,
            refresh_cookie(&state, &refresh_token),
        )?;
    } else {
        append_cookie(&mut response_headers, clear_cookie(&state, REFRESH_COOKIE))?;
    }

    let admin = load_admin_identity(&state, user.id, &user.username).await?;
    info!(username = %user.username, "administrator signed in");
    Ok((
        StatusCode::OK,
        response_headers,
        Json(LoginResponse { admin }),
    )
        .into_response())
}

async fn refresh(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, ApiError> {
    let Some(old_token) = read_cookie(&headers, REFRESH_COOKIE) else {
        return Err(ApiError::Unauthorized);
    };
    let old_hash = hash_token(&old_token);
    let mut transaction = state.pool().begin().await.map_err(|err| {
        error!(error = %err, "refresh transaction could not start");
        ApiError::Internal
    })?;
    let row = sqlx::query_as::<_, (i64, i64, String)>(
        "SELECT rt.id, au.id, au.username
         FROM refresh_tokens rt
         JOIN admin_users au ON au.id = rt.user_id
         WHERE rt.token_hash = $1 AND rt.expires_at > now()
         FOR UPDATE",
    )
    .bind(&old_hash)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|err| {
        error!(error = %err, "refresh token lookup failed");
        ApiError::Internal
    })?;

    let Some((refresh_id, user_id, username)) = row else {
        let _ = transaction.rollback().await;
        return Err(ApiError::Unauthorized);
    };

    sqlx::query("DELETE FROM refresh_tokens WHERE id = $1")
        .bind(refresh_id)
        .execute(&mut *transaction)
        .await
        .map_err(|err| {
            error!(error = %err, "old refresh token could not be revoked");
            ApiError::Internal
        })?;

    let next_refresh = random_token();
    sqlx::query("INSERT INTO refresh_tokens (user_id, token_hash, expires_at) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(hash_token(&next_refresh))
        .bind(Utc::now() + Duration::seconds(REFRESH_TTL_SECONDS))
        .execute(&mut *transaction)
        .await
        .map_err(|err| {
            error!(error = %err, "rotated refresh token could not be persisted");
            ApiError::Internal
        })?;

    transaction.commit().await.map_err(|err| {
        error!(error = %err, "refresh token rotation could not commit");
        ApiError::Internal
    })?;

    let user = LoginUser {
        id: user_id,
        username: username.clone(),
        password_hash: String::new(),
        totp_secret: None,
    };
    let access_token = create_access_token(&state, &user)?;
    let mut response_headers = HeaderMap::new();
    append_cookie(&mut response_headers, access_cookie(&state, &access_token))?;
    append_cookie(&mut response_headers, refresh_cookie(&state, &next_refresh))?;

    let admin = load_admin_identity(&state, user_id, &username).await?;
    Ok((
        StatusCode::OK,
        response_headers,
        Json(LoginResponse { admin }),
    )
        .into_response())
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, ApiError> {
    authenticate(&state, &headers)?;
    if let Some(refresh_token) = read_cookie(&headers, REFRESH_COOKIE) {
        sqlx::query("DELETE FROM refresh_tokens WHERE token_hash = $1")
            .bind(hash_token(&refresh_token))
            .execute(state.pool())
            .await
            .map_err(|err| {
                error!(error = %err, "refresh token revocation failed");
                ApiError::Internal
            })?;
    }

    let mut response_headers = HeaderMap::new();
    append_cookie(&mut response_headers, clear_cookie(&state, ACCESS_COOKIE))?;
    append_cookie(&mut response_headers, clear_cookie(&state, REFRESH_COOKIE))?;
    Ok((StatusCode::NO_CONTENT, response_headers).into_response())
}

async fn me(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, ApiError> {
    let claims = authenticate(&state, &headers)?;
    Ok(Json(load_admin_identity(&state, claims.sub, &claims.username).await?).into_response())
}

async fn update_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<UpdateProfileRequest>,
) -> Result<Response, ApiError> {
    let claims = authenticate(&state, &headers)?;
    let email = normalized_email(&request.email)?;
    let bilibili_uid = normalized_bilibili_uid(&request.bilibili_uid)?;
    if !(-1.0..=1.0).contains(&request.avatar_crop_x)
        || !(-1.0..=1.0).contains(&request.avatar_crop_y)
        || !(1.0..=3.0).contains(&request.avatar_crop_zoom)
    {
        return Err(ApiError::Validation("头像裁剪参数无效"));
    }
    let mut transaction = state.pool().begin().await.map_err(|err| {
        error!(error = %err, "profile update transaction could not start");
        ApiError::Internal
    })?;
    let previous = sqlx::query_as::<_, StoredProfileSyncSettings>(
        "SELECT
             COALESCE(settings #>> '{bangumi_sync,uid}', '') AS bilibili_uid,
             steam_web_api_key_ciphertext,
             steam_web_api_key_nonce,
             steam_encryption_key_version,
             COALESCE(settings #>> '{steam_sync,steam_id64}', '') AS steam_id64
         FROM site_settings
         WHERE id = 1
         FOR UPDATE",
    )
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|err| {
        error!(error = %err, "previous profile sync credentials could not be read");
        ApiError::Internal
    })?
    .unwrap_or_default();
    let previous_steam_web_api_key = state
        .llm_keyring()
        .decrypt_optional(
            previous.steam_encryption_key_version,
            previous.steam_web_api_key_ciphertext.as_deref(),
            previous.steam_web_api_key_nonce.as_deref(),
        )
        .map_err(|err| {
            error!(error = %err, "stored Steam Web API key could not be decrypted");
            ApiError::Internal
        })?;
    let (supplied_steam_web_api_key, steam_id64, steam_key_configured) = normalized_steam_update(
        &request.steam_web_api_key,
        &request.steam_id64,
        request.clear_steam_web_api_key,
        previous_steam_web_api_key.is_some(),
    )?;
    let encrypted_steam_web_api_key = supplied_steam_web_api_key
        .as_deref()
        .map(|api_key| state.llm_keyring().encrypt(api_key))
        .transpose()
        .map_err(|err| {
            error!(error = %err, "Steam Web API key could not be encrypted");
            ApiError::Internal
        })?;
    let steam_key_mutated = supplied_steam_web_api_key.is_some() || request.clear_steam_web_api_key;

    let avatar_url = if let Some(asset_id) = request.avatar_asset_id {
        Some(
            sqlx::query_scalar::<_, String>(
                "SELECT '/storage/' || upload.object_key
             FROM assets asset
             JOIN uploads upload ON upload.id = asset.upload_id
             WHERE asset.id = $1
               AND asset.status = 'active'
               AND asset.media_type = 'image'",
            )
            .bind(asset_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|err| {
                error!(error = %err, "profile avatar asset could not be validated");
                ApiError::Internal
            })?
            .ok_or(ApiError::Validation("请选择有效的图片素材作为头像"))?,
        )
    } else {
        None
    };

    let updated = sqlx::query(
        "UPDATE admin_users
         SET email = $1, avatar_url = $2, avatar_asset_id = $3,
             avatar_crop_x = $4, avatar_crop_y = $5, avatar_crop_zoom = $6
         WHERE id = $7 AND username = $8",
    )
    .bind(&email)
    .bind(&avatar_url)
    .bind(request.avatar_asset_id)
    .bind(request.avatar_crop_x)
    .bind(request.avatar_crop_y)
    .bind(request.avatar_crop_zoom)
    .bind(claims.sub)
    .bind(&claims.username)
    .execute(&mut *transaction)
    .await
    .map_err(|err| {
        error!(error = %err, "administrator profile could not be updated");
        ApiError::Internal
    })?;
    if updated.rows_affected() != 1 {
        let _ = transaction.rollback().await;
        return Err(ApiError::Unauthorized);
    }

    sqlx::query(
        "DELETE FROM asset_references
         WHERE source_type = 'admin_avatar' AND source_key = $1",
    )
    .bind(format!("admin:{}", claims.sub))
    .execute(&mut *transaction)
    .await
    .map_err(|err| {
        error!(error = %err, "previous profile avatar reference could not be removed");
        ApiError::Internal
    })?;
    if let Some(asset_id) = request.avatar_asset_id {
        sqlx::query(
            "INSERT INTO asset_references (
                 asset_id, source_type, source_key, source_label, admin_path
             )
             VALUES ($1, 'admin_avatar', $2, $3, '/admin/profile')
             ON CONFLICT (source_type, source_key) DO UPDATE
             SET asset_id = EXCLUDED.asset_id,
                 source_label = EXCLUDED.source_label,
                 admin_path = EXCLUDED.admin_path",
        )
        .bind(asset_id)
        .bind(format!("admin:{}", claims.sub))
        .bind(format!("{} 的管理员头像", claims.username))
        .execute(&mut *transaction)
        .await
        .map_err(|err| {
            error!(error = %err, "profile avatar reference could not be created");
            ApiError::Internal
        })?;
    }

    sqlx::query(
        "UPDATE site_settings
         SET settings = jsonb_set(
             settings,
             '{bangumi_sync}',
             COALESCE(settings -> 'bangumi_sync', '{}'::jsonb)
                 || jsonb_build_object('uid', $1::text),
             true
         )
         WHERE id = 1",
    )
    .bind(&bilibili_uid)
    .execute(&mut *transaction)
    .await
    .map_err(|err| {
        error!(error = %err, "Bilibili UID could not be moved into the administrator profile");
        ApiError::Internal
    })?;

    sqlx::query(
        "UPDATE site_settings
         SET settings = jsonb_set(
                 settings #- '{steam_sync,web_api_key}'::text[],
                 '{steam_sync}',
                 COALESCE(
                     (settings #- '{steam_sync,web_api_key}'::text[]) -> 'steam_sync',
                     '{}'::jsonb
                 ) || jsonb_build_object('steam_id64', $1::text),
                 true
             ),
             steam_web_api_key_ciphertext = CASE
                 WHEN $2::bool THEN $3
                 ELSE steam_web_api_key_ciphertext
             END,
             steam_web_api_key_nonce = CASE
                 WHEN $2::bool THEN $4
                 ELSE steam_web_api_key_nonce
             END,
             steam_encryption_key_version = CASE
                 WHEN $2::bool THEN $5
                 ELSE steam_encryption_key_version
             END,
             updated_at = now()
         WHERE id = 1",
    )
    .bind(&steam_id64)
    .bind(steam_key_mutated)
    .bind(
        encrypted_steam_web_api_key
            .as_ref()
            .map(|secret| secret.ciphertext.as_slice()),
    )
    .bind(
        encrypted_steam_web_api_key
            .as_ref()
            .map(|secret| secret.nonce.as_slice()),
    )
    .bind(
        encrypted_steam_web_api_key
            .as_ref()
            .map(|secret| secret.key_version),
    )
    .execute(&mut *transaction)
    .await
    .map_err(|err| {
        error!(error = %err, "Steam credentials could not be stored in the administrator profile");
        ApiError::Internal
    })?;

    if previous.bilibili_uid != bilibili_uid {
        let cover_keys =
            sqlx::query_scalar::<_, Option<String>>("DELETE FROM bangumi RETURNING cover_key")
                .fetch_all(&mut *transaction)
                .await
                .map_err(|err| {
                    error!(error = %err, "previous Bilibili mirror could not be cleared");
                    ApiError::Internal
                })?;
        for cover_key in cover_keys.into_iter().flatten() {
            storage_gc::enqueue(&mut transaction, &cover_key, "bangumi_uid_changed")
                .await
                .map_err(|err| {
                    error!(error = %err, "previous Bilibili cover could not be queued for cleanup");
                    ApiError::Internal
                })?;
        }
        sqlx::query(
            "UPDATE site_settings
             SET settings = jsonb_set(
                 jsonb_set(
                     jsonb_set(settings, '{bangumi_sync,last_sync_at}', 'null'::jsonb, true),
                     '{bangumi_sync,last_status}', to_jsonb($1::text), true
                 ),
                 '{bangumi_sync,last_counts}', '{\"watching\":0,\"finished\":0}'::jsonb, true
             )
             WHERE id = 1",
        )
        .bind(if bilibili_uid.is_empty() {
            "disabled"
        } else {
            "queued"
        })
        .execute(&mut *transaction)
        .await
        .map_err(|err| {
            error!(error = %err, "Bilibili sync status could not be reset");
            ApiError::Internal
        })?;
    }

    let steam_credentials_changed = steam_key_mutated || previous.steam_id64 != steam_id64;
    if steam_credentials_changed {
        sqlx::query("DELETE FROM games WHERE steam_app_id IS NOT NULL")
            .execute(&mut *transaction)
            .await
            .map_err(|err| {
                error!(error = %err, "previous Steam game mirror could not be cleared");
                ApiError::Internal
            })?;
        sqlx::query(
            "UPDATE site_settings
             SET settings = jsonb_set(
                 jsonb_set(
                     jsonb_set(settings, '{steam_sync,last_sync_at}', 'null'::jsonb, true),
                     '{steam_sync,last_status}', to_jsonb($1::text), true
                 ),
                 '{steam_sync,last_counts}', '{\"total\":0,\"recent\":0}'::jsonb, true
             )
             WHERE id = 1",
        )
        .bind(if steam_key_configured {
            "queued"
        } else {
            "disabled"
        })
        .execute(&mut *transaction)
        .await
        .map_err(|err| {
            error!(error = %err, "Steam sync status could not be reset");
            ApiError::Internal
        })?;
    }

    transaction.commit().await.map_err(|err| {
        error!(error = %err, "profile update transaction could not commit");
        ApiError::Internal
    })?;
    info!(username = %claims.username, "administrator profile updated");

    if !bilibili_uid.is_empty() {
        let _ = bangumi::trigger_sync(state.clone());
    }
    if steam_key_configured {
        let _ = games::trigger_sync(state.clone());
    }

    Ok(Json(load_admin_identity(&state, claims.sub, &claims.username).await?).into_response())
}

async fn upload_avatar(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let claims = authenticate(&state, &headers)?;
    let media = validate_avatar_upload(&headers, &body)?;
    ensure_admin_exists(&state, claims.sub, &claims.username).await?;
    let object_id = Uuid::now_v7();
    let object_key = format!(
        "avatars/admin/{}/{}.{}",
        claims.sub, object_id, media.extension
    );
    let bytes = body.to_vec();
    state
        .object_storage()
        .put_public_object(state.http_client(), &object_key, media.mime, bytes.clone())
        .await
        .map_err(|err| {
            error!(error = %err, object_key, "administrator avatar upload failed");
            ApiError::Internal
        })?;

    let avatar_url = state.object_storage().public_url(&object_key);
    let persisted = persist_avatar_asset(
        &state,
        claims.sub,
        &claims.username,
        &object_key,
        &avatar_url,
        media.mime,
        &bytes,
    )
    .await;
    if let Err(error) = persisted {
        if let Err(cleanup_error) = state
            .object_storage()
            .delete_public_object(state.http_client(), &object_key)
            .await
        {
            warn!(error = %cleanup_error, object_key, "orphaned avatar object could not be cleaned up");
        }
        return Err(error);
    }

    info!(username = %claims.username, object_key, "administrator avatar uploaded");
    Ok(Json(load_admin_identity(&state, claims.sub, &claims.username).await?).into_response())
}

async fn remove_avatar(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let claims = authenticate(&state, &headers)?;
    let mut transaction = state.pool().begin().await.map_err(|err| {
        error!(error = %err, "avatar removal transaction could not start");
        ApiError::Internal
    })?;
    let current = sqlx::query_as::<_, (Option<i64>,)>(
        "SELECT avatar_asset_id
         FROM admin_users
         WHERE id = $1 AND username = $2
         FOR UPDATE",
    )
    .bind(claims.sub)
    .bind(&claims.username)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|err| {
        error!(error = %err, "administrator avatar binding could not be read");
        ApiError::Internal
    })?
    .ok_or(ApiError::Unauthorized)?
    .0;

    sqlx::query(
        "UPDATE admin_users
         SET avatar_url = NULL, avatar_asset_id = NULL
         WHERE id = $1",
    )
    .bind(claims.sub)
    .execute(&mut *transaction)
    .await
    .map_err(|err| {
        error!(error = %err, "administrator avatar could not be removed");
        ApiError::Internal
    })?;
    sqlx::query(
        "DELETE FROM asset_references
         WHERE source_type = 'admin_avatar' AND source_key = $1",
    )
    .bind(format!("admin:{}", claims.sub))
    .execute(&mut *transaction)
    .await
    .map_err(|err| {
        error!(error = %err, "administrator avatar reference could not be removed");
        ApiError::Internal
    })?;
    if let Some(asset_id) = current {
        sqlx::query("UPDATE assets SET status = 'archived' WHERE id = $1")
            .bind(asset_id)
            .execute(&mut *transaction)
            .await
            .map_err(|err| {
                error!(error = %err, "administrator avatar asset could not be archived");
                ApiError::Internal
            })?;
    }
    transaction.commit().await.map_err(|err| {
        error!(error = %err, "avatar removal transaction could not commit");
        ApiError::Internal
    })?;

    info!(username = %claims.username, "administrator avatar removed");
    Ok(Json(load_admin_identity(&state, claims.sub, &claims.username).await?).into_response())
}

async fn change_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ChangePasswordRequest>,
) -> Result<Response, ApiError> {
    let claims = authenticate(&state, &headers)?;
    validate_password_change(&request)?;

    let stored_hash = sqlx::query_scalar::<_, String>(
        "SELECT password_hash FROM admin_users WHERE id = $1 AND username = $2",
    )
    .bind(claims.sub)
    .bind(&claims.username)
    .fetch_optional(state.pool())
    .await
    .map_err(|err| {
        error!(error = %err, "administrator password lookup failed");
        ApiError::Internal
    })?
    .ok_or(ApiError::Unauthorized)?;

    let current_password = request.current_password;
    let current_valid = tokio::task::spawn_blocking(move || {
        verify_password_or_run_dummy(Some(&stored_hash), &current_password)
    })
    .await
    .map_err(|err| {
        error!(error = %err, "current password verification task failed");
        ApiError::Internal
    })?;
    if !current_valid {
        return Err(ApiError::CurrentPasswordInvalid);
    }

    let new_password = request.new_password;
    let password_hash = tokio::task::spawn_blocking(move || hash_password(&new_password))
        .await
        .map_err(|err| {
            error!(error = %err, "new password hashing task failed");
            ApiError::Internal
        })??;

    let mut transaction = state.pool().begin().await.map_err(|err| {
        error!(error = %err, "password change transaction could not start");
        ApiError::Internal
    })?;
    sqlx::query("UPDATE admin_users SET password_hash = $1 WHERE id = $2")
        .bind(password_hash)
        .bind(claims.sub)
        .execute(&mut *transaction)
        .await
        .map_err(|err| {
            error!(error = %err, "administrator password could not be updated");
            ApiError::Internal
        })?;
    if request.revoke_other_sessions {
        sqlx::query("DELETE FROM refresh_tokens WHERE user_id = $1")
            .bind(claims.sub)
            .execute(&mut *transaction)
            .await
            .map_err(|err| {
                error!(error = %err, "administrator refresh tokens could not be revoked");
                ApiError::Internal
            })?;
    } else if let Some(current_refresh) = read_cookie(&headers, REFRESH_COOKIE) {
        sqlx::query("DELETE FROM refresh_tokens WHERE user_id = $1 AND token_hash = $2")
            .bind(claims.sub)
            .bind(hash_token(&current_refresh))
            .execute(&mut *transaction)
            .await
            .map_err(|err| {
                error!(error = %err, "current administrator refresh token could not be revoked");
                ApiError::Internal
            })?;
    }
    transaction.commit().await.map_err(|err| {
        error!(error = %err, "password change transaction could not commit");
        ApiError::Internal
    })?;

    let mut response_headers = HeaderMap::new();
    append_cookie(&mut response_headers, clear_cookie(&state, ACCESS_COOKIE))?;
    append_cookie(&mut response_headers, clear_cookie(&state, REFRESH_COOKIE))?;
    info!(username = %claims.username, "administrator password changed");
    Ok((StatusCode::NO_CONTENT, response_headers).into_response())
}

async fn list_passkeys(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let claims = authenticate(&state, &headers)?;
    let items = sqlx::query_as::<_, PasskeyItem>(
        "SELECT id, label, created_at
         FROM passkeys
         WHERE user_id = $1
         ORDER BY created_at DESC, id DESC",
    )
    .bind(claims.sub)
    .fetch_all(state.pool())
    .await
    .map_err(|err| {
        error!(error = %err, "administrator passkeys could not be listed");
        ApiError::Internal
    })?;

    Ok(Json(PasskeyListResponse { items }).into_response())
}

async fn passkey_registration_options(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let claims = authenticate(&state, &headers)?;
    let username = sqlx::query_scalar::<_, String>(
        "SELECT username FROM admin_users WHERE id = $1 AND username = $2",
    )
    .bind(claims.sub)
    .bind(&claims.username)
    .fetch_optional(state.pool())
    .await
    .map_err(|err| {
        error!(error = %err, "administrator lookup for passkey registration failed");
        ApiError::Internal
    })?
    .ok_or(ApiError::Unauthorized)?;
    let credential_ids =
        sqlx::query_scalar::<_, Vec<u8>>("SELECT credential_id FROM passkeys WHERE user_id = $1")
            .bind(claims.sub)
            .fetch_all(state.pool())
            .await
            .map_err(|err| {
                error!(error = %err, "existing passkeys could not be loaded");
                ApiError::Internal
            })?;
    let excluded = (!credential_ids.is_empty()).then(|| {
        credential_ids
            .into_iter()
            .map(CredentialID::from)
            .collect::<Vec<_>>()
    });
    let user_handle = Uuid::from_u128(claims.sub as u128);
    let (options, registration) = state
        .webauthn()
        .start_passkey_registration(user_handle, &username, &username, excluded)
        .map_err(|err| {
            error!(error = %err, "passkey registration challenge could not be created");
            ApiError::Internal
        })?;
    state.store_passkey_registration(claims.sub, registration);

    Ok(Json(options).into_response())
}

async fn register_passkey(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RegisterPasskeyRequest>,
) -> Result<Response, ApiError> {
    let claims = authenticate(&state, &headers)?;
    let label = normalized_passkey_label(request.label.as_deref())?;
    let registration = state
        .take_passkey_registration(claims.sub)
        .ok_or(ApiError::Validation("通行密钥验证已失效，请重新发起保存"))?;
    let passkey = state
        .webauthn()
        .finish_passkey_registration(&request.credential, &registration)
        .map_err(|err| {
            warn!(error = %err, username = %claims.username, "passkey registration was rejected");
            ApiError::Validation("通行密钥验证失败，请重试")
        })?;
    let credential_id = passkey.cred_id().as_slice().to_vec();
    let serialized_passkey = serde_json::to_vec(&passkey).map_err(|err| {
        error!(error = %err, "passkey could not be serialized");
        ApiError::Internal
    })?;

    let item = sqlx::query_as::<_, PasskeyItem>(
        "INSERT INTO passkeys (user_id, credential_id, public_key, sign_count, label)
         VALUES ($1, $2, $3, 0, $4)
         RETURNING id, label, created_at",
    )
    .bind(claims.sub)
    .bind(credential_id)
    .bind(serialized_passkey)
    .bind(label)
    .fetch_one(state.pool())
    .await
    .map_err(|err| {
        if err
            .as_database_error()
            .is_some_and(|database_error| database_error.is_unique_violation())
        {
            ApiError::Conflict("这枚通行密钥已经保存")
        } else {
            error!(error = %err, "passkey could not be persisted");
            ApiError::Internal
        }
    })?;

    info!(username = %claims.username, passkey_id = item.id, "administrator passkey registered");
    Ok((StatusCode::CREATED, Json(item)).into_response())
}

async fn delete_passkey(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(passkey_id): Path<i64>,
) -> Result<Response, ApiError> {
    let claims = authenticate(&state, &headers)?;
    if passkey_id <= 0 {
        return Err(ApiError::Validation("通行密钥编号无效"));
    }
    let deleted = sqlx::query("DELETE FROM passkeys WHERE id = $1 AND user_id = $2")
        .bind(passkey_id)
        .bind(claims.sub)
        .execute(state.pool())
        .await
        .map_err(|err| {
            error!(error = %err, "passkey could not be deleted");
            ApiError::Internal
        })?
        .rows_affected();
    if deleted == 0 {
        return Err(ApiError::NotFound("未找到这枚通行密钥"));
    }

    info!(username = %claims.username, passkey_id, "administrator passkey removed");
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn forgot_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ForgotPasswordRequest>,
) -> Result<Response, ApiError> {
    let username = request.username.trim();
    if username.is_empty() || username.chars().count() > 64 {
        return Err(ApiError::Validation("请输入有效账号"));
    }

    let source = client_address(&headers);
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM admin_users WHERE username = $1)",
    )
    .bind(username)
    .fetch_one(state.pool())
    .await
    .map_err(|err| {
        error!(error = %err, "forgot-password lookup failed");
        ApiError::Internal
    })?;

    if exists {
        warn!(username, source, "administrator password reset requested");
    } else {
        warn!(
            source,
            "password reset requested for an unknown administrator"
        );
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(MessageResponse {
            message: "如账号存在，请在服务器中运行 blog-admin reset-password 完成重置",
        }),
    )
        .into_response())
}

fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<AccessClaims, ApiError> {
    let token = read_cookie(headers, ACCESS_COOKIE).ok_or(ApiError::Unauthorized)?;
    let claims =
        decode_access_token(state.auth_jwt_secret(), &token).map_err(|_| ApiError::Unauthorized)?;
    if claims.sub <= 0 || claims.username.trim().is_empty() {
        return Err(ApiError::Unauthorized);
    }
    Ok(claims)
}

pub(crate) fn has_valid_admin_session(state: &AppState, headers: &HeaderMap) -> bool {
    authenticate(state, headers).is_ok()
}

pub(crate) fn authenticated_admin_id(state: &AppState, headers: &HeaderMap) -> Option<i64> {
    authenticate(state, headers).ok().map(|claims| claims.sub)
}

fn create_access_token(state: &AppState, user: &LoginUser) -> Result<String, ApiError> {
    let now = Utc::now().timestamp();
    let claims = AccessClaims {
        sub: user.id,
        username: user.username.clone(),
        iss: JWT_ISSUER.to_owned(),
        iat: now as usize,
        exp: (now + ACCESS_TTL_SECONDS) as usize,
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(state.auth_jwt_secret().as_bytes()),
    )
    .map_err(|err| {
        error!(error = %err, "access token creation failed");
        ApiError::Internal
    })
}

fn decode_access_token(
    secret: &str,
    token: &str,
) -> Result<AccessClaims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[JWT_ISSUER]);
    decode::<AccessClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
}

fn verify_password_or_run_dummy(stored_hash: Option<&str>, supplied_password: &str) -> bool {
    match stored_hash.and_then(|hash| PasswordHash::new(hash).ok()) {
        Some(parsed) => Argon2::default()
            .verify_password(supplied_password.as_bytes(), &parsed)
            .is_ok(),
        None => {
            let salt = SaltString::encode_b64(&[0_u8; 16]).expect("fixed salt is valid");
            let _ = Argon2::default().hash_password(supplied_password.as_bytes(), &salt);
            false
        }
    }
}

fn validate_password_change(request: &ChangePasswordRequest) -> Result<(), ApiError> {
    let current_length = request.current_password.chars().count();
    let new_length = request.new_password.chars().count();
    if current_length == 0 || current_length > 512 {
        return Err(ApiError::Validation("请输入有效的当前密码"));
    }
    if !(12..=128).contains(&new_length) {
        return Err(ApiError::Validation("新密码长度须为 12–128 个字符"));
    }
    if request.current_password == request.new_password {
        return Err(ApiError::Validation("新密码不能与当前密码相同"));
    }
    Ok(())
}

fn hash_password(password: &str) -> Result<String, ApiError> {
    let mut salt_bytes = [0_u8; 16];
    rand::rng().fill_bytes(&mut salt_bytes);
    let salt = SaltString::encode_b64(&salt_bytes).map_err(|err| {
        error!(error = %err, "password salt could not be encoded");
        ApiError::Internal
    })?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| {
            error!(error = %err, "password could not be hashed");
            ApiError::Internal
        })
}

fn normalized_passkey_label(label: Option<&str>) -> Result<String, ApiError> {
    let label = label.unwrap_or_default().trim();
    if label.chars().count() > 80 {
        return Err(ApiError::Validation("通行密钥名称不能超过 80 个字符"));
    }
    Ok(if label.is_empty() {
        "未命名通行密钥".to_owned()
    } else {
        label.to_owned()
    })
}

fn verify_totp(secret: &str, supplied_code: &str) -> bool {
    if supplied_code.len() != 6 || !supplied_code.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    let Some(secret_bytes) = decode_base32(secret) else {
        return false;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() / 30);

    (-1_i64..=1).any(|offset| {
        let counter = now.saturating_add_signed(offset);
        let Ok(mut hmac) = Hmac::<Sha1>::new_from_slice(&secret_bytes) else {
            return false;
        };
        hmac.update(&counter.to_be_bytes());
        let digest = hmac.finalize().into_bytes();
        let start = usize::from(digest[19] & 0x0f);
        let number = (u32::from(digest[start] & 0x7f) << 24)
            | (u32::from(digest[start + 1]) << 16)
            | (u32::from(digest[start + 2]) << 8)
            | u32::from(digest[start + 3]);
        format!("{:06}", number % 1_000_000) == supplied_code
    })
}

fn decode_base32(value: &str) -> Option<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = 0_u32;
    let mut bits = 0_u8;

    for character in value.chars() {
        let normalized = character.to_ascii_uppercase();
        if normalized == '=' || normalized == ' ' || normalized == '-' {
            continue;
        }
        let digit = match normalized {
            'A'..='Z' => u32::from(normalized as u8 - b'A'),
            '2'..='7' => u32::from(normalized as u8 - b'2' + 26),
            _ => return None,
        };
        buffer = (buffer << 5) | digit;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            output.push(((buffer >> bits) & 0xff) as u8);
        }
    }

    (!output.is_empty()).then_some(output)
}

fn random_token() -> String {
    Alphanumeric.sample_string(&mut rand::rng(), 64)
}

fn hash_token(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

async fn load_admin_identity(
    state: &AppState,
    user_id: i64,
    username: &str,
) -> Result<AdminIdentity, ApiError> {
    let profile = sqlx::query_as::<_, AdminProfileRow>(
        "SELECT
             au.username,
             au.email,
             au.avatar_url,
             au.avatar_asset_id,
             au.avatar_crop_x,
             au.avatar_crop_y,
             au.avatar_crop_zoom,
             COALESCE(ss.settings #>> '{bangumi_sync,uid}', '') AS bilibili_uid,
             COALESCE(
                 ss.steam_web_api_key_ciphertext IS NOT NULL
                 AND ss.steam_web_api_key_nonce IS NOT NULL
                 AND ss.steam_encryption_key_version IS NOT NULL,
                 false
             ) AS steam_web_api_key_configured,
             COALESCE(ss.settings #>> '{steam_sync,steam_id64}', '') AS steam_id64
         FROM admin_users au
         LEFT JOIN site_settings ss ON ss.id = 1
         WHERE au.id = $1 AND au.username = $2",
    )
    .bind(user_id)
    .bind(username)
    .fetch_optional(state.pool())
    .await
    .map_err(|err| {
        error!(error = %err, "administrator profile lookup failed");
        ApiError::Internal
    })?
    .ok_or(ApiError::Unauthorized)?;

    Ok(AdminIdentity {
        username: profile.username,
        email: profile.email,
        avatar_url: profile.avatar_url,
        avatar_asset_id: profile.avatar_asset_id,
        avatar_crop_x: profile.avatar_crop_x,
        avatar_crop_y: profile.avatar_crop_y,
        avatar_crop_zoom: profile.avatar_crop_zoom,
        bilibili_uid: profile.bilibili_uid,
        steam_web_api_key_configured: profile.steam_web_api_key_configured,
        steam_web_api_key_masked: if profile.steam_web_api_key_configured {
            "********".to_owned()
        } else {
            String::new()
        },
        steam_id64: profile.steam_id64,
    })
}

fn default_avatar_zoom() -> f32 {
    1.0
}

fn normalized_email(value: &str) -> Result<String, ApiError> {
    let email = value.trim().to_lowercase();
    if email.is_empty() {
        return Ok(email);
    }
    if email.len() > 254
        || email.chars().any(char::is_whitespace)
        || !email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.ends_with('.')
        })
    {
        return Err(ApiError::Validation("请输入有效的邮箱地址"));
    }
    Ok(email)
}

fn normalized_bilibili_uid(value: &str) -> Result<String, ApiError> {
    let uid = value.trim();
    if uid.is_empty() {
        return Ok(String::new());
    }
    if uid.len() > 20 || !uid.bytes().all(|character| character.is_ascii_digit()) {
        return Err(ApiError::Validation(
            "B 站 UID 只能包含数字，且不能超过 20 位",
        ));
    }
    Ok(uid.to_owned())
}

fn normalized_steam_web_api_key(value: &str) -> Result<String, ApiError> {
    let key = value.trim();
    if key.is_empty() {
        return Ok(String::new());
    }
    if key.len() != 32 || !key.bytes().all(|character| character.is_ascii_hexdigit()) {
        return Err(ApiError::Validation(
            "Steam Web API Key 应为 32 位十六进制字符",
        ));
    }
    Ok(key.to_ascii_uppercase())
}

fn normalized_steam_id64(value: &str) -> Result<String, ApiError> {
    const INDIVIDUAL_ACCOUNT_BASE: u64 = 76_561_197_960_265_728;
    let steam_id = value.trim();
    if steam_id.is_empty() {
        return Ok(String::new());
    }
    if steam_id.len() != 17
        || !steam_id.bytes().all(|character| character.is_ascii_digit())
        || steam_id
            .parse::<u64>()
            .map_or(true, |value| value < INDIVIDUAL_ACCOUNT_BASE)
    {
        return Err(ApiError::Validation("请输入有效的 17 位 SteamID64"));
    }
    Ok(steam_id.to_owned())
}

fn normalized_steam_update(
    web_api_key: &str,
    steam_id64: &str,
    clear_web_api_key: bool,
    previously_configured: bool,
) -> Result<(Option<String>, String, bool), ApiError> {
    let web_api_key = normalized_steam_web_api_key(web_api_key)?;
    let steam_id64 = normalized_steam_id64(steam_id64)?;
    if clear_web_api_key {
        if !web_api_key.is_empty() {
            return Err(ApiError::Validation(
                "清除 Steam Web API Key 时不能同时提交新 Key",
            ));
        }
        return Ok((None, String::new(), false));
    }
    if !web_api_key.is_empty() && steam_id64.is_empty() {
        return Err(ApiError::Validation(
            "提交 Steam Web API Key 时必须同时填写 SteamID64",
        ));
    }
    if web_api_key.is_empty() && !steam_id64.is_empty() && !previously_configured {
        return Err(ApiError::Validation(
            "首次配置 Steam 同步时必须填写 Steam Web API Key",
        ));
    }
    if web_api_key.is_empty() && steam_id64.is_empty() && previously_configured {
        return Err(ApiError::Validation(
            "如需删除 Steam 凭据，请显式选择清除 Steam Web API Key",
        ));
    }
    let supplied_key = (!web_api_key.is_empty()).then_some(web_api_key);
    let configured = (previously_configured || supplied_key.is_some()) && !steam_id64.is_empty();
    Ok((supplied_key, steam_id64, configured))
}

fn validate_avatar_upload(headers: &HeaderMap, body: &[u8]) -> Result<AvatarMedia, ApiError> {
    if body.is_empty() || body.len() > MAX_AVATAR_BYTES {
        return Err(ApiError::Validation("头像文件大小须在 1 B–512 KB 之间"));
    }
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim);
    let media = match content_type {
        Some("image/png") if body.starts_with(b"\x89PNG\r\n\x1a\n") => AvatarMedia {
            mime: "image/png",
            extension: "png",
        },
        Some("image/jpeg") if body.starts_with(b"\xff\xd8\xff") => AvatarMedia {
            mime: "image/jpeg",
            extension: "jpg",
        },
        Some("image/webp")
            if body.len() >= 12 && body.starts_with(b"RIFF") && &body[8..12] == b"WEBP" =>
        {
            AvatarMedia {
                mime: "image/webp",
                extension: "webp",
            }
        }
        _ => {
            return Err(ApiError::Validation(
                "头像必须是内容有效的 PNG、JPEG 或 WebP 图片",
            ));
        }
    };
    Ok(media)
}

async fn ensure_admin_exists(
    state: &AppState,
    user_id: i64,
    username: &str,
) -> Result<(), ApiError> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
             SELECT 1 FROM admin_users WHERE id = $1 AND username = $2
         )",
    )
    .bind(user_id)
    .bind(username)
    .fetch_one(state.pool())
    .await
    .map_err(|err| {
        error!(error = %err, "administrator lookup before avatar upload failed");
        ApiError::Internal
    })?;
    if exists {
        Ok(())
    } else {
        Err(ApiError::Unauthorized)
    }
}

async fn persist_avatar_asset(
    state: &AppState,
    user_id: i64,
    username: &str,
    object_key: &str,
    avatar_url: &str,
    mime: &str,
    bytes: &[u8],
) -> Result<(), ApiError> {
    let mut transaction = state.pool().begin().await.map_err(|err| {
        error!(error = %err, "avatar asset transaction could not start");
        ApiError::Internal
    })?;
    let current_asset_id = sqlx::query_as::<_, (Option<i64>,)>(
        "SELECT avatar_asset_id
         FROM admin_users
         WHERE id = $1 AND username = $2
         FOR UPDATE",
    )
    .bind(user_id)
    .bind(username)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|err| {
        error!(error = %err, "administrator avatar asset binding could not be locked");
        ApiError::Internal
    })?
    .ok_or(ApiError::Unauthorized)?
    .0;
    let checksum = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let upload_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO uploads (
             object_key, bucket, mime, size_bytes, kind,
             original_filename, checksum_sha256, metadata
         )
         VALUES ($1, $2, $3, $4, 'avatar', $5, $6, $7)
         RETURNING id",
    )
    .bind(object_key)
    .bind(state.object_storage().public_bucket())
    .bind(mime)
    .bind(i64::try_from(bytes.len()).map_err(|_| ApiError::Internal)?)
    .bind(format!(
        "{}-avatar.{}",
        username,
        mime.rsplit('/').next().unwrap_or("image")
    ))
    .bind(checksum)
    .bind(json!({ "owner": username, "source": "admin_profile" }))
    .fetch_one(&mut *transaction)
    .await
    .map_err(|err| {
        error!(error = %err, "avatar upload ledger could not be created");
        ApiError::Internal
    })?;

    let asset_id = if let Some(asset_id) = current_asset_id {
        let old_upload: (i64, String) = sqlx::query_as(
            "SELECT upload.id, upload.object_key
             FROM assets asset
             JOIN uploads upload ON upload.id = asset.upload_id
             WHERE asset.id = $1
             FOR UPDATE OF asset",
        )
        .bind(asset_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|err| {
            error!(error = %err, "avatar asset could not be locked");
            ApiError::Internal
        })?;
        sqlx::query("UPDATE assets SET status = 'active', upload_id = $1 WHERE id = $2")
            .bind(upload_id)
            .bind(asset_id)
            .execute(&mut *transaction)
            .await
            .map_err(|err| {
                error!(error = %err, "avatar asset could not be reactivated");
                ApiError::Internal
            })?;
        storage_gc::enqueue(&mut transaction, &old_upload.1, "avatar_replaced")
            .await
            .map_err(|err| {
                error!(error = %err, "previous avatar cleanup could not be queued");
                ApiError::Internal
            })?;
        sqlx::query("DELETE FROM uploads WHERE id = $1")
            .bind(old_upload.0)
            .execute(&mut *transaction)
            .await
            .map_err(|err| {
                error!(error = %err, "previous avatar upload ledger could not be removed");
                ApiError::Internal
            })?;
        asset_id
    } else {
        sqlx::query_scalar::<_, i64>(
            "INSERT INTO assets (name, media_type, upload_id)
             VALUES ($1, 'image', $2)
             RETURNING id",
        )
        .bind(format!("{username} 的管理员头像"))
        .bind(upload_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|err| {
            error!(error = %err, "administrator avatar asset could not be created");
            ApiError::Internal
        })?
    };
    sqlx::query(
        "UPDATE admin_users
         SET avatar_url = $1, avatar_asset_id = $2
         WHERE id = $3",
    )
    .bind(avatar_url)
    .bind(asset_id)
    .bind(user_id)
    .execute(&mut *transaction)
    .await
    .map_err(|err| {
        error!(error = %err, "administrator avatar binding could not be updated");
        ApiError::Internal
    })?;
    sqlx::query(
        "INSERT INTO asset_references (
             asset_id, source_type, source_key, source_label, admin_path
         )
         VALUES ($1, 'admin_avatar', $2, $3, '/admin')
         ON CONFLICT (source_type, source_key) DO UPDATE
         SET asset_id = EXCLUDED.asset_id,
             source_label = EXCLUDED.source_label,
             admin_path = EXCLUDED.admin_path",
    )
    .bind(asset_id)
    .bind(format!("admin:{user_id}"))
    .bind(format!("{username} 的管理员头像"))
    .execute(&mut *transaction)
    .await
    .map_err(|err| {
        error!(error = %err, "administrator avatar reference could not be updated");
        ApiError::Internal
    })?;
    transaction.commit().await.map_err(|err| {
        error!(error = %err, "avatar asset transaction could not commit");
        ApiError::Internal
    })
}

fn auth_rate_key(headers: &HeaderMap, username: &str) -> String {
    format!("{}:{}", client_address(headers), username.to_lowercase())
}

fn client_address(headers: &HeaderMap) -> &str {
    headers
        .get("x-real-ip")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            headers
                .get("x-forwarded-for")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.split(',').next())
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or("unknown")
}

fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get_all(COOKIE).iter().find_map(|value| {
        value.to_str().ok()?.split(';').find_map(|pair| {
            let (cookie_name, cookie_value) = pair.trim().split_once('=')?;
            (cookie_name == name).then(|| cookie_value.to_owned())
        })
    })
}

fn access_cookie(state: &AppState, token: &str) -> String {
    cookie_value(state, ACCESS_COOKIE, token, Some(ACCESS_TTL_SECONDS))
}

fn refresh_cookie(state: &AppState, token: &str) -> String {
    cookie_value(state, REFRESH_COOKIE, token, Some(REFRESH_TTL_SECONDS))
}

fn clear_cookie(state: &AppState, name: &str) -> String {
    cookie_value(state, name, "", Some(0))
}

fn cookie_value(state: &AppState, name: &str, value: &str, max_age: Option<i64>) -> String {
    let mut cookie = format!("{name}={value}; Path=/; HttpOnly; SameSite=Lax");
    if let Some(seconds) = max_age {
        cookie.push_str(&format!("; Max-Age={seconds}"));
    }
    if state.secure_cookies() {
        cookie.push_str("; Secure");
    }
    cookie
}

fn append_cookie(headers: &mut HeaderMap, cookie: String) -> Result<(), ApiError> {
    let value = HeaderValue::from_str(&cookie).map_err(|err| {
        error!(error = %err, "authentication cookie could not be encoded");
        ApiError::Internal
    })?;
    headers.append(SET_COOKIE, value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ACCESS_COOKIE, AccessClaims, ChangePasswordRequest, JWT_ISSUER, authenticate,
        decode_access_token, decode_base32, hash_token, normalized_bilibili_uid, normalized_email,
        normalized_steam_id64, normalized_steam_update, normalized_steam_web_api_key,
        validate_avatar_upload,
    };
    use axum::http::{HeaderMap, HeaderValue, header::CONTENT_TYPE};
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    use sqlx::postgres::PgPoolOptions;

    use crate::{config::Config, state::AppState};

    fn test_state() -> AppState {
        let config = Config {
            environment: "test".to_owned(),
            host: "127.0.0.1".parse().unwrap(),
            port: 3000,
            database_url: "postgres://test:test@localhost/test".to_owned(),
            db_max_connections: 1,
            db_min_connections: 0,
            run_migrations: false,
            minio_endpoint: "http://localhost:9000".to_owned(),
            minio_access_key: "test".to_owned(),
            minio_secret_key: "test".to_owned(),
            minio_public_bucket: "blog-public".to_owned(),
            minio_private_bucket: "blog-private".to_owned(),
            admin_username: "test".to_owned(),
            admin_initial_password: Some("test".to_owned()),
            auth_jwt_secret: "test-secret-at-least-32-bytes-long".to_owned(),
            artalk_internal_url: None,
            artalk_site_name: "helt.".to_owned(),
            artalk_admin_name: "test".to_owned(),
            artalk_admin_email: "test@example.com".to_owned(),
            artalk_admin_password: "test".to_owned(),
            llm_encryption_key_version: 1,
            llm_encryption_secret: "test-llm-encryption-secret-at-least-32-bytes".to_owned(),
            llm_encryption_previous_key_version: None,
            llm_encryption_previous_secret: None,
            llm_private_host_allowlist: Vec::new(),
            public_origin: "http://localhost".to_owned(),
            cors_allowed_origins: vec!["http://localhost:5173".to_owned()],
            request_timeout_secs: 5,
        };
        AppState::new(
            PgPoolOptions::new()
                .connect_lazy(&config.database_url)
                .unwrap(),
            &config,
        )
        .unwrap()
    }

    fn access_cookie(claims: AccessClaims) -> HeaderMap {
        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(b"test-secret-at-least-32-bytes-long"),
        )
        .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            HeaderValue::from_str(&format!("{ACCESS_COOKIE}={token}")).unwrap(),
        );
        headers
    }

    #[test]
    fn base32_decoder_accepts_standard_totp_secrets() {
        assert_eq!(
            decode_base32("JBSW Y3DP-EHPK3PXP"),
            Some(b"Hello!\xde\xad\xbe\xef".to_vec())
        );
    }

    #[test]
    fn token_hash_is_stable_and_does_not_store_the_token() {
        let hash = hash_token("refresh-secret");
        assert_eq!(hash.len(), 64);
        assert_ne!(hash, "refresh-secret");
        assert_eq!(hash, hash_token("refresh-secret"));
    }

    #[test]
    fn access_token_rejects_the_wrong_secret() {
        let claims = AccessClaims {
            sub: 1,
            username: "helt".to_owned(),
            iss: JWT_ISSUER.to_owned(),
            iat: 1,
            exp: usize::MAX / 2,
        };
        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(b"correct-secret"),
        )
        .unwrap();
        assert!(decode_access_token("correct-secret", &token).is_ok());
        assert!(decode_access_token("wrong-secret", &token).is_err());
    }

    #[tokio::test]
    async fn single_administrator_authentication_rejects_malformed_identity_claims() {
        let state = test_state();
        let valid = AccessClaims {
            sub: 1,
            username: "helt".to_owned(),
            iss: JWT_ISSUER.to_owned(),
            iat: 1,
            exp: usize::MAX / 2,
        };
        assert!(authenticate(&state, &access_cookie(valid)).is_ok());

        for claims in [
            AccessClaims {
                sub: 0,
                username: "helt".to_owned(),
                iss: JWT_ISSUER.to_owned(),
                iat: 1,
                exp: usize::MAX / 2,
            },
            AccessClaims {
                sub: 1,
                username: " ".to_owned(),
                iss: JWT_ISSUER.to_owned(),
                iat: 1,
                exp: usize::MAX / 2,
            },
        ] {
            assert!(authenticate(&state, &access_cookie(claims)).is_err());
        }
    }

    #[test]
    fn profile_fields_are_normalized_and_reject_invalid_values() {
        assert_eq!(
            normalized_email(" Admin@Example.COM ").unwrap(),
            "admin@example.com"
        );
        assert!(normalized_email("not-an-email").is_err());
        assert_eq!(normalized_bilibili_uid(" 123456 ").unwrap(), "123456");
        assert!(normalized_bilibili_uid("uid-123").is_err());
        assert_eq!(
            normalized_steam_web_api_key(" 0123456789abcdef0123456789abcdef ").unwrap(),
            "0123456789ABCDEF0123456789ABCDEF"
        );
        assert!(normalized_steam_web_api_key("not-a-key").is_err());
        assert_eq!(
            normalized_steam_id64(" 76561198000000000 ").unwrap(),
            "76561198000000000"
        );
        assert!(normalized_steam_id64("123456").is_err());
        assert!(normalized_steam_update("", "", false, false).is_ok());
        assert!(
            normalized_steam_update(
                "0123456789abcdef0123456789abcdef",
                "76561198000000000",
                false,
                false
            )
            .is_ok()
        );
        assert!(
            normalized_steam_update("0123456789abcdef0123456789abcdef", "", false, false).is_err()
        );
        assert!(normalized_steam_update("", "76561198000000000", false, false).is_err());
        let retained =
            normalized_steam_update("", "76561198000000000", false, true).expect("retain key");
        assert!(retained.0.is_none());
        assert!(retained.2);
        let cleared =
            normalized_steam_update("", "76561198000000000", true, true).expect("explicit clear");
        assert_eq!(cleared, (None, String::new(), false));
    }

    #[test]
    fn avatar_upload_requires_matching_image_content_and_mime() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("image/png"));
        let png = b"\x89PNG\r\n\x1a\nvalid-enough-for-sniffing";
        assert_eq!(
            validate_avatar_upload(&headers, png).unwrap().extension,
            "png"
        );
        assert!(validate_avatar_upload(&headers, b"not-an-image").is_err());
    }

    #[test]
    fn password_change_requires_an_explicit_choice_to_revoke_other_sessions() {
        let default_request: ChangePasswordRequest = serde_json::from_value(serde_json::json!({
            "current_password": "old-password",
            "new_password": "new-password-long-enough"
        }))
        .unwrap();
        assert!(!default_request.revoke_other_sessions);

        let revoke_request: ChangePasswordRequest = serde_json::from_value(serde_json::json!({
            "current_password": "old-password",
            "new_password": "new-password-long-enough",
            "revoke_other_sessions": true
        }))
        .unwrap();
        assert!(revoke_request.revoke_other_sessions);
    }
}
