use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use sqlx::PgPool;
use webauthn_rs::prelude::{PasskeyRegistration, Url, Webauthn, WebauthnBuilder};

use crate::{
    artalk::ArtalkClient, config::Config, llm_crypto::LlmKeyring, llm_network::LlmHttpClient,
    storage::ObjectStorage,
};

const AUTH_FAILURE_WINDOW: Duration = Duration::from_secs(15 * 60);
const MAX_AUTH_FAILURE_KEYS: usize = 4_096;

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

struct Inner {
    pub pool: PgPool,
    pub http_client: Client,
    pub storage_http_client: Client,
    pub minio_health_url: String,
    pub object_storage: ObjectStorage,
    pub artalk: ArtalkClient,
    pub started_at: DateTime<Utc>,
    pub auth_jwt_secret: String,
    pub llm_keyring: LlmKeyring,
    pub llm_http_client: LlmHttpClient,
    pub secure_cookies: bool,
    pub auth_failures: Mutex<HashMap<String, Vec<Instant>>>,
    pub webauthn: Webauthn,
    pub passkey_registrations: Mutex<HashMap<i64, (Instant, PasskeyRegistration)>>,
    pub bangumi_syncing: AtomicBool,
    pub steam_syncing: AtomicBool,
}

impl AppState {
    pub fn new(pool: PgPool, config: &Config) -> Result<Self> {
        let http_client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(config.upstream_request_timeout_secs))
            .build()
            .context("failed to construct upstream HTTP client")?;
        let storage_http_client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(config.asset_request_timeout_secs))
            .build()
            .context("failed to construct object-storage HTTP client")?;
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
        let llm_keyring = LlmKeyring::new(
            config.llm_encryption_key_version,
            &config.llm_encryption_secret,
            config
                .llm_encryption_previous_key_version
                .zip(config.llm_encryption_previous_secret.as_deref()),
        )
        .context("invalid LLM encryption keyring")?;
        let llm_http_client =
            LlmHttpClient::new(&config.environment, &config.llm_private_host_allowlist);
        let artalk = ArtalkClient::new(http_client.clone(), config)?;

        Ok(Self(Arc::new(Inner {
            pool,
            http_client,
            storage_http_client,
            minio_health_url: format!("{}/minio/health/ready", config.minio_endpoint),
            object_storage: ObjectStorage::new(
                config.minio_endpoint.clone(),
                config.minio_access_key.clone(),
                config.minio_secret_key.clone(),
                config.minio_public_bucket.clone(),
            ),
            artalk,
            started_at: Utc::now(),
            auth_jwt_secret: config.auth_jwt_secret.clone(),
            llm_keyring,
            llm_http_client,
            secure_cookies: config.public_origin.starts_with("https://"),
            auth_failures: Mutex::new(HashMap::new()),
            webauthn,
            passkey_registrations: Mutex::new(HashMap::new()),
            bangumi_syncing: AtomicBool::new(false),
            steam_syncing: AtomicBool::new(false),
        })))
    }

    pub fn pool(&self) -> &PgPool {
        &self.0.pool
    }

    pub fn http_client(&self) -> &Client {
        &self.0.http_client
    }

    pub fn storage_http_client(&self) -> &Client {
        &self.0.storage_http_client
    }

    pub fn minio_health_url(&self) -> &str {
        &self.0.minio_health_url
    }

    pub fn object_storage(&self) -> &ObjectStorage {
        &self.0.object_storage
    }

    pub fn artalk(&self) -> &ArtalkClient {
        &self.0.artalk
    }

    pub fn started_at(&self) -> DateTime<Utc> {
        self.0.started_at
    }

    pub fn auth_jwt_secret(&self) -> &str {
        &self.0.auth_jwt_secret
    }

    pub fn llm_keyring(&self) -> &LlmKeyring {
        &self.0.llm_keyring
    }

    pub fn llm_http_client(&self) -> &LlmHttpClient {
        &self.0.llm_http_client
    }

    pub fn secure_cookies(&self) -> bool {
        self.0.secure_cookies
    }

    pub fn webauthn(&self) -> &Webauthn {
        &self.0.webauthn
    }

    pub fn try_begin_bangumi_sync(&self) -> bool {
        self.0
            .bangumi_syncing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub fn finish_bangumi_sync(&self) {
        self.0.bangumi_syncing.store(false, Ordering::Release);
    }

    pub fn try_begin_steam_sync(&self) -> bool {
        self.0
            .steam_syncing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub fn finish_steam_sync(&self) {
        self.0.steam_syncing.store(false, Ordering::Release);
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
        let mut failures = self
            .0
            .auth_failures
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        prune_auth_failures(&mut failures, Instant::now());
        failures
            .get(key)
            .is_some_and(|attempts| attempts.len() >= 5)
    }

    pub fn record_auth_failure(&self, key: &str) {
        let mut failures = self
            .0
            .auth_failures
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = Instant::now();
        prune_auth_failures(&mut failures, now);
        if failures.len() >= MAX_AUTH_FAILURE_KEYS
            && !failures.contains_key(key)
            && let Some(oldest) = failures
                .iter()
                .min_by_key(|(_, attempts)| attempts.last().copied())
                .map(|(key, _)| key.clone())
        {
            failures.remove(&oldest);
        }
        failures.entry(key.to_owned()).or_default().push(now);
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

fn prune_auth_failures(failures: &mut HashMap<String, Vec<Instant>>, now: Instant) {
    failures.retain(|_, attempts| {
        attempts.retain(|attempt| now.saturating_duration_since(*attempt) < AUTH_FAILURE_WINDOW);
        !attempts.is_empty()
    });
}

#[cfg(test)]
mod tests {
    use super::{AUTH_FAILURE_WINDOW, prune_auth_failures};
    use std::{collections::HashMap, time::Instant};

    #[test]
    fn expired_auth_failure_keys_are_removed() {
        let now = Instant::now();
        let expired = now.checked_sub(AUTH_FAILURE_WINDOW).expect("test instant");
        let recent = now;
        let mut failures = HashMap::from([
            ("expired".to_owned(), vec![expired]),
            ("mixed".to_owned(), vec![expired, recent]),
        ]);

        prune_auth_failures(&mut failures, now);

        assert!(!failures.contains_key("expired"));
        assert_eq!(failures["mixed"], [recent]);
    }
}
