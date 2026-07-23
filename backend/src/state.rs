use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use sqlx::PgPool;

use crate::config::Config;

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

struct Inner {
    pub pool: PgPool,
    pub http_client: Client,
    pub minio_health_url: String,
    pub started_at: DateTime<Utc>,
    pub auth_jwt_secret: String,
    pub secure_cookies: bool,
    pub auth_failures: Mutex<HashMap<String, Vec<Instant>>>,
}

impl AppState {
    pub fn new(pool: PgPool, config: &Config) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .context("failed to construct internal HTTP client")?;

        Ok(Self(Arc::new(Inner {
            pool,
            http_client,
            minio_health_url: format!("{}/minio/health/ready", config.minio_endpoint),
            started_at: Utc::now(),
            auth_jwt_secret: config.auth_jwt_secret.clone(),
            secure_cookies: config.public_origin.starts_with("https://"),
            auth_failures: Mutex::new(HashMap::new()),
        })))
    }

    pub fn pool(&self) -> &PgPool {
        &self.0.pool
    }

    pub fn http_client(&self) -> &Client {
        &self.0.http_client
    }

    pub fn minio_health_url(&self) -> &str {
        &self.0.minio_health_url
    }

    pub fn started_at(&self) -> DateTime<Utc> {
        self.0.started_at
    }

    pub fn auth_jwt_secret(&self) -> &str {
        &self.0.auth_jwt_secret
    }

    pub fn secure_cookies(&self) -> bool {
        self.0.secure_cookies
    }

    pub fn auth_rate_limited(&self, key: &str) -> bool {
        let cutoff = Instant::now() - Duration::from_secs(15 * 60);
        let mut failures = self
            .0
            .auth_failures
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let attempts = failures.entry(key.to_owned()).or_default();
        attempts.retain(|attempt| *attempt >= cutoff);
        attempts.len() >= 5
    }

    pub fn record_auth_failure(&self, key: &str) {
        let mut failures = self
            .0
            .auth_failures
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        failures
            .entry(key.to_owned())
            .or_default()
            .push(Instant::now());
    }

    pub fn clear_auth_failures(&self, key: &str) {
        let mut failures = self
            .0
            .auth_failures
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        failures.remove(key);
    }
}
