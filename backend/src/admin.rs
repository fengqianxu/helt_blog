use anyhow::{Context, Result};
use argon2::{Argon2, PasswordHasher, password_hash::SaltString};
use rand::{
    RngCore,
    distr::{Alphanumeric, SampleString},
};
use sqlx::PgPool;
use tracing::{info, warn};

use crate::config::Config;

pub async fn ensure_initial_admin(pool: &PgPool, config: &Config) -> Result<()> {
    let existing_users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM admin_users")
        .fetch_one(pool)
        .await
        .context("failed to inspect administrator table")?;

    if existing_users > 0 {
        return Ok(());
    }

    let generated = config.admin_initial_password.is_none();
    let (password, password_hash) = password_and_hash(config.admin_initial_password.clone())?;

    let inserted = sqlx::query(
        "INSERT INTO admin_users (username, password_hash) VALUES ($1, $2) ON CONFLICT (username) DO NOTHING",
    )
    .bind(&config.admin_username)
    .bind(password_hash)
    .execute(pool)
    .await
    .context("failed to create initial administrator")?
    .rows_affected();

    if inserted == 1 {
        if generated {
            warn!(
                username = %config.admin_username,
                initial_password = %password,
                "initial administrator created; store this password now because it will not be shown again"
            );
        } else {
            info!(username = %config.admin_username, "initial administrator created");
        }
    }

    Ok(())
}

pub async fn reset_password(pool: &PgPool, config: &Config) -> Result<()> {
    let (password, password_hash) = password_and_hash(config.admin_initial_password.clone())?;
    let updated = sqlx::query("UPDATE admin_users SET password_hash = $1 WHERE username = $2")
        .bind(password_hash)
        .bind(&config.admin_username)
        .execute(pool)
        .await
        .context("failed to reset administrator password")?
        .rows_affected();

    if updated == 0 {
        anyhow::bail!("administrator {:?} does not exist", config.admin_username);
    }

    warn!(
        username = %config.admin_username,
        new_password = %password,
        "administrator password reset; store this password now"
    );
    Ok(())
}

fn password_and_hash(explicit_password: Option<String>) -> Result<(String, String)> {
    let password =
        explicit_password.unwrap_or_else(|| Alphanumeric.sample_string(&mut rand::rng(), 24));
    let mut salt_bytes = [0_u8; 16];
    rand::rng().fill_bytes(&mut salt_bytes);
    let salt = SaltString::encode_b64(&salt_bytes)
        .map_err(|error| anyhow::anyhow!("failed to encode password salt: {error}"))?;
    let password_hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?
        .to_string();

    Ok((password, password_hash))
}
