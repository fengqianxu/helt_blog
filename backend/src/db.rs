use std::{str::FromStr, time::Duration};

use anyhow::{Context, Result};
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
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .context("sqlx migrations failed")?;
    info!("database migrations are current");
    Ok(())
}
