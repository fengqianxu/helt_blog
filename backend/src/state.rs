use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use sqlx::PgPool;
use webauthn_rs::prelude::{PasskeyRegistration, Url, Webauthn, WebauthnBuilder};

use crate::{config::Config, storage::ObjectStorage};

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

struct Inner {
    pub pool: PgPool,
    pub http_client: Client,
    pub minio_health_url: String,
    pub object_storage: ObjectStorage,
    pub started_at: DateTime<Utc>,
    pub auth_jwt_secret: String,
    pub secure_cookies: bool,
    pub auth_failures: Mutex<HashMap<String, Vec<Instant>>>,
    pub webauthn: Webauthn,
    pub passkey_registrations: Mutex<HashMap<i64, (Instant, PasskeyRegistration)>>,
}

impl AppState {
    pub fn new(pool: PgPool, config: &Config) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .context("failed to construct internal HTTP client")?;
        let rp_origin = Url::parse(&config.public_origin)
            .context("PUBLIC_ORIGIN must be an absolute URL for Passkey support")?;
        let rp_id = rp_origin
            .domain()
            .context("PUBLIC_ORIGIN must contain a domain for Passkey support")?;
        let webauthn = WebauthnBuilder::new(rp_id, &rp_origin)
            .map_err(|error| {
                anyhow::anyhow!("invalid Passkey relying-party configuration: {error}")
            })?
            .rp_name("helt. Admin")
            .build()
            .map_err(|error| anyhow::anyhow!("failed to initialize Passkey support: {error}"))?;

        Ok(Self(Arc::new(Inner {
            pool,
            http_client,
            minio_health_url: format!("{}/minio/health/ready", config.minio_endpoint),
            object_storage: ObjectStorage::new(
                config.minio_endpoint.clone(),
                config.minio_access_key.clone(),
                config.minio_secret_key.clone(),
                config.minio_public_bucket.clone(),
            ),
            started_at: Utc::now(),
            auth_jwt_secret: config.auth_jwt_secret.clone(),
            secure_cookies: config.public_origin.starts_with("https://"),
            auth_failures: Mutex::new(HashMap::new()),
            webauthn,
            passkey_registrations: Mutex::new(HashMap::new()),
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

    pub fn object_storage(&self) -> &ObjectStorage {
        &self.0.object_storage
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

    pub fn webauthn(&self) -> &Webauthn {
        &self.0.webauthn
    }

    pub fn store_passkey_registration(&self, user_id: i64, registration: PasskeyRegistration) {
        let mut registrations = self
            .0
            .passkey_registrations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        registrations
            .retain(|_, (created_at, _)| created_at.elapsed() < Duration::from_secs(5 * 60));
        registrations.insert(user_id, (Instant::now(), registration));
    }

    pub fn take_passkey_registration(&self, user_id: i64) -> Option<PasskeyRegistration> {
        let mut registrations = self
            .0
            .passkey_registrations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        registrations
            .retain(|_, (created_at, _)| created_at.elapsed() < Duration::from_secs(5 * 60));
        registrations.remove(&user_id).map(|(_, state)| state)
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
