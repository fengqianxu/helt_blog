use std::{sync::Arc, time::Duration};

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
}
