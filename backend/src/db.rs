use std::{str::FromStr, time::Duration};

use anyhow::{Context, Result, bail};
use sqlx::{
    ConnectOptions, PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use tracing::info;

use crate::config::Config;

pub async fn connect(config: &Config) -> Result<PgPool> {
    let options = PgConnectOptions::from_str(&config.database_url)
        .context("DATABASE_URL is not a valid PostgreSQL URL")?
        .application_name("helt-blog-backend")
        .disable_statement_logging();

    PgPoolOptions::new()
        .min_connections(config.db_min_connections)
        .max_connections(config.db_max_connections)
        .acquire_timeout(Duration::from_secs(10))
        .idle_timeout(Duration::from_secs(600))
        .max_lifetime(Duration::from_secs(1800))
        .after_connect(|connection, _metadata| {
            Box::pin(async move {
                sqlx::query("SET TIME ZONE 'Asia/Shanghai'")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(options)
        .await
        .context("could not establish PostgreSQL connection pool")
}

pub async fn migrate(pool: &PgPool) -> Result<()> {
    info!("running database migrations");
    protect_existing_artalk_data(pool).await?;
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .context("sqlx migrations failed")?;
    info!("database migrations are current");
    Ok(())
}

/// Migrations 27 and 28 predate the data-retention policy and delete rows from
/// Artalk-owned tables. They are immutable once published, so an upgrade with
/// either migration pending must use `scripts/preserve-artalk-migrations-0027-0028.sql`
/// to snapshot the rows and record the retired migrations without executing
/// their DELETE statements. A fresh database has no SQLx ledger yet and is safe:
/// Compose starts the backend before Artalk creates its schema.
async fn protect_existing_artalk_data(pool: &PgPool) -> Result<()> {
    let ledger_exists =
        sqlx::query_scalar::<_, bool>("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(pool)
            .await
            .context("failed to inspect SQLx migration ledger")?;
    if !ledger_exists {
        return Ok(());
    }

    let pending_retired_migration = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (
             SELECT 1
             FROM (VALUES (27::BIGINT), (28::BIGINT)) AS retired(version)
             WHERE NOT EXISTS (
                 SELECT 1 FROM _sqlx_migrations applied
                 WHERE applied.version = retired.version AND applied.success
             )
         )",
    )
    .fetch_one(pool)
    .await
    .context("failed to inspect retired Artalk migrations")?;
    if !pending_retired_migration {
        return Ok(());
    }

    for table in [
        "artalk_notifies",
        "artalk_votes",
        "artalk_comments",
        "artalk_auth_identities",
        "artalk_user_email_verifies",
        "artalk_pages",
        "artalk_users",
    ] {
        let table_exists =
            sqlx::query_scalar::<_, bool>(&format!("SELECT to_regclass('{table}') IS NOT NULL"))
                .fetch_one(pool)
                .await
                .with_context(|| format!("failed to inspect {table} before migration"))?;
        if !table_exists {
            continue;
        }
        let table_has_rows = sqlx::query_scalar::<_, bool>(&format!(
            "SELECT EXISTS (SELECT 1 FROM {table} LIMIT 1)"
        ))
        .fetch_one(pool)
        .await
        .with_context(|| format!("failed to inspect {table} before migration"))?;
        if table_has_rows {
            bail!(
                "refusing to run retired destructive Artalk migrations 27/28 while {table} contains data; follow DEPLOY.md 'Artalk 数据保留升级' and run scripts/preserve-artalk-migrations-0027-0028.sql before starting this backend"
            );
        }
    }

    Ok(())
}
