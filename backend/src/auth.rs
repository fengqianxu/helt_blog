use std::time::{SystemTime, UNIX_EPOCH};

use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use axum::{
    Json, Router,
    extract::State,
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{COOKIE, SET_COOKIE},
    },
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{Duration, Utc};
use hmac::{Hmac, Mac};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rand::distr::{Alphanumeric, SampleString};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use sqlx::FromRow;
use tracing::{error, info, warn};

use crate::{
    error::{ErrorBody, ErrorEnvelope},
    routes::contract::HttpMethod,
    state::AppState,
};

const ACCESS_COOKIE: &str = "helt_admin_access";
const REFRESH_COOKIE: &str = "helt_admin_refresh";
const ACCESS_TTL_SECONDS: i64 = 2 * 60 * 60;
const REFRESH_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;
const JWT_ISSUER: &str = "helt-blog";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/admin/auth/login", post(login))
        .route("/api/v1/admin/auth/refresh", post(refresh))
        .route("/api/v1/admin/auth/logout", post(logout))
        .route("/api/v1/admin/auth/me", get(me))
        .route("/api/v1/admin/auth/forgot-password", post(forgot_password))
}

pub fn implements(method: HttpMethod, path: &str) -> bool {
    matches!(
        (method, path),
        (HttpMethod::Post, "/api/v1/admin/auth/login")
            | (HttpMethod::Post, "/api/v1/admin/auth/refresh")
            | (HttpMethod::Post, "/api/v1/admin/auth/logout")
            | (HttpMethod::Get, "/api/v1/admin/auth/me")
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
    role: &'static str,
    avatar_url: Option<String>,
}

#[derive(Debug, Serialize)]
struct LoginResponse {
    admin: AdminIdentity,
}

#[derive(Debug, Serialize)]
struct MessageResponse {
    message: &'static str,
}

#[derive(Debug, Serialize, Deserialize)]
struct AccessClaims {
    sub: i64,
    username: String,
    role: String,
    iss: String,
    iat: usize,
    exp: usize,
}

#[derive(Debug)]
enum ApiError {
    Unauthorized,
    Validation(&'static str),
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
            Self::Validation(message) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_error",
                message,
            ),
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

    info!(username = %user.username, "administrator signed in");
    Ok((
        StatusCode::OK,
        response_headers,
        Json(LoginResponse {
            admin: admin_identity(user.username),
        }),
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

    Ok((
        StatusCode::OK,
        response_headers,
        Json(LoginResponse {
            admin: admin_identity(username),
        }),
    )
        .into_response())
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, ApiError> {
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
    let username = sqlx::query_scalar::<_, String>(
        "SELECT username FROM admin_users WHERE id = $1 AND username = $2",
    )
    .bind(claims.sub)
    .bind(&claims.username)
    .fetch_optional(state.pool())
    .await
    .map_err(|err| {
        error!(error = %err, "administrator session lookup failed");
        ApiError::Internal
    })?
    .ok_or(ApiError::Unauthorized)?;

    Ok(Json(admin_identity(username)).into_response())
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
    decode_access_token(state.auth_jwt_secret(), &token).map_err(|_| ApiError::Unauthorized)
}

fn create_access_token(state: &AppState, user: &LoginUser) -> Result<String, ApiError> {
    let now = Utc::now().timestamp();
    let claims = AccessClaims {
        sub: user.id,
        username: user.username.clone(),
        role: "administrator".to_owned(),
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

fn admin_identity(username: String) -> AdminIdentity {
    AdminIdentity {
        username,
        role: "administrator",
        avatar_url: None,
    }
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
    use super::{AccessClaims, JWT_ISSUER, decode_access_token, decode_base32, hash_token};
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

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
            role: "administrator".to_owned(),
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
}
