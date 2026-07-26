use std::{env, net::IpAddr, str::FromStr};

use thiserror::Error;

#[derive(Clone)]
pub struct Config {
    pub environment: String,
    pub host: IpAddr,
    pub port: u16,
    pub database_url: String,
    pub db_max_connections: u32,
    pub db_min_connections: u32,
    pub run_migrations: bool,
    pub minio_endpoint: String,
    pub minio_access_key: String,
    pub minio_secret_key: String,
    pub minio_public_bucket: String,
    pub minio_private_bucket: String,
    pub admin_username: String,
    pub admin_initial_password: Option<String>,
    pub auth_jwt_secret: String,
    pub llm_encryption_key_version: i32,
    pub llm_encryption_secret: String,
    pub llm_encryption_previous_key_version: Option<i32>,
    pub llm_encryption_previous_secret: Option<String>,
    pub llm_private_host_allowlist: Vec<String>,
    pub public_origin: String,
    pub cors_allowed_origins: Vec<String>,
    pub request_timeout_secs: u64,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("required environment variable {0} is missing")]
    Missing(&'static str),
    #[error("environment variable {name} has invalid value {value:?}")]
    Invalid { name: &'static str, value: String },
    #[error("DB_MIN_CONNECTIONS cannot be greater than DB_MAX_CONNECTIONS")]
    InvalidPoolSize,
    #[error("AUTH_JWT_SECRET must contain at least 32 characters")]
    WeakAuthSecret,
    #[error("LLM_ENCRYPTION_KEY must contain at least 32 characters")]
    WeakLlmEncryptionSecret,
    #[error("LLM_ENCRYPTION_PREVIOUS_KEY must contain at least 32 characters")]
    WeakPreviousLlmEncryptionSecret,
    #[error(
        "LLM_ENCRYPTION_PREVIOUS_KEY and LLM_ENCRYPTION_PREVIOUS_KEY_VERSION must be configured together"
    )]
    IncompletePreviousLlmEncryptionKey,
    #[error("LLM encryption key versions must be positive and distinct")]
    InvalidLlmEncryptionKeyVersions,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let db_max_connections = parse_or("DB_MAX_CONNECTIONS", 10_u32)?;
        let db_min_connections = parse_or("DB_MIN_CONNECTIONS", 1_u32)?;
        if db_min_connections > db_max_connections {
            return Err(ConfigError::InvalidPoolSize);
        }

        let cors_allowed_origins = env::var("CORS_ALLOWED_ORIGINS")
            .unwrap_or_else(|_| "http://localhost:3000,http://localhost:5173".to_owned())
            .split(',')
            .map(str::trim)
            .filter(|origin| !origin.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();

        if cors_allowed_origins.is_empty() {
            return Err(ConfigError::Invalid {
                name: "CORS_ALLOWED_ORIGINS",
                value: String::new(),
            });
        }

        let auth_jwt_secret = required("AUTH_JWT_SECRET")?;
        if auth_jwt_secret.chars().count() < 32 {
            return Err(ConfigError::WeakAuthSecret);
        }
        let llm_encryption_secret = required("LLM_ENCRYPTION_KEY")?;
        if llm_encryption_secret.chars().count() < 32 {
            return Err(ConfigError::WeakLlmEncryptionSecret);
        }
        let llm_encryption_key_version = parse_or("LLM_ENCRYPTION_KEY_VERSION", 1_i32)?;
        let llm_encryption_previous_secret = optional("LLM_ENCRYPTION_PREVIOUS_KEY");
        let llm_encryption_previous_key_version =
            optional_parse("LLM_ENCRYPTION_PREVIOUS_KEY_VERSION")?;
        if llm_encryption_previous_secret.is_some() != llm_encryption_previous_key_version.is_some()
        {
            return Err(ConfigError::IncompletePreviousLlmEncryptionKey);
        }
        if llm_encryption_previous_secret
            .as_ref()
            .is_some_and(|secret| secret.chars().count() < 32)
        {
            return Err(ConfigError::WeakPreviousLlmEncryptionSecret);
        }
        if llm_encryption_key_version <= 0
            || llm_encryption_previous_key_version
                .is_some_and(|version| version <= 0 || version == llm_encryption_key_version)
        {
            return Err(ConfigError::InvalidLlmEncryptionKeyVersions);
        }
        let llm_private_host_allowlist = parse_host_allowlist()?;

        Ok(Self {
            environment: env::var("APP_ENV").unwrap_or_else(|_| "development".to_owned()),
            host: parse_or("APP_HOST", IpAddr::from_str("0.0.0.0").unwrap())?,
            port: parse_or("APP_PORT", 3000_u16)?,
            database_url: required("DATABASE_URL")?,
            db_max_connections,
            db_min_connections,
            run_migrations: parse_or("RUN_MIGRATIONS", true)?,
            minio_endpoint: required("MINIO_ENDPOINT")?.trim_end_matches('/').to_owned(),
            minio_access_key: required("MINIO_ACCESS_KEY")?,
            minio_secret_key: required("MINIO_SECRET_KEY")?,
            minio_public_bucket: env::var("MINIO_PUBLIC_BUCKET")
                .unwrap_or_else(|_| "blog-public".to_owned()),
            minio_private_bucket: env::var("MINIO_PRIVATE_BUCKET")
                .unwrap_or_else(|_| "blog-private".to_owned()),
            admin_username: env::var("ADMIN_USERNAME").unwrap_or_else(|_| "helt".to_owned()),
            admin_initial_password: env::var("ADMIN_INITIAL_PASSWORD")
                .ok()
                .filter(|password| !password.trim().is_empty()),
            auth_jwt_secret,
            llm_encryption_key_version,
            llm_encryption_secret,
            llm_encryption_previous_key_version,
            llm_encryption_previous_secret,
            llm_private_host_allowlist,
            public_origin: env::var("PUBLIC_ORIGIN")
                .unwrap_or_else(|_| "http://localhost".to_owned()),
            cors_allowed_origins,
            request_timeout_secs: parse_or("REQUEST_TIMEOUT_SECS", 30_u64)?,
        })
    }
}

fn optional(name: &'static str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn optional_parse<T>(name: &'static str) -> Result<Option<T>, ConfigError>
where
    T: FromStr,
{
    optional(name)
        .map(|value| {
            value
                .parse()
                .map_err(|_| ConfigError::Invalid { name, value })
        })
        .transpose()
}

fn parse_host_allowlist() -> Result<Vec<String>, ConfigError> {
    let raw = env::var("LLM_PRIVATE_HOST_ALLOWLIST").unwrap_or_default();
    raw.split(',')
        .map(str::trim)
        .filter(|host| !host.is_empty())
        .map(|host| {
            let host = host.trim_matches(['[', ']']).trim_end_matches('.');
            if host.is_empty()
                || host.contains(char::is_whitespace)
                || host.contains("://")
                || host.contains(['/', '\\', '@', '?', '#'])
                || (host.parse::<IpAddr>().is_err()
                    && host.split('.').any(|label| {
                        label.is_empty()
                            || label.len() > 63
                            || label.starts_with('-')
                            || label.ends_with('-')
                            || !label
                                .chars()
                                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
                    }))
            {
                return Err(ConfigError::Invalid {
                    name: "LLM_PRIVATE_HOST_ALLOWLIST",
                    value: raw.clone(),
                });
            }
            Ok(host.to_ascii_lowercase())
        })
        .collect()
}

fn required(name: &'static str) -> Result<String, ConfigError> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or(ConfigError::Missing(name))
}

fn parse_or<T>(name: &'static str, default: T) -> Result<T, ConfigError>
where
    T: FromStr,
{
    match env::var(name) {
        Ok(value) => value
            .parse()
            .map_err(|_| ConfigError::Invalid { name, value }),
        Err(_) => Ok(default),
    }
}
