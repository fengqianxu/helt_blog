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
            public_origin: env::var("PUBLIC_ORIGIN")
                .unwrap_or_else(|_| "http://localhost".to_owned()),
            cors_allowed_origins,
            request_timeout_secs: parse_or("REQUEST_TIMEOUT_SECS", 30_u64)?,
        })
    }
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
