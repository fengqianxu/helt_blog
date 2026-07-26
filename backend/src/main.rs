use std::{net::SocketAddr, process::ExitCode, time::Duration};

use anyhow::{Context, Result};
use blog_backend::{
    admin, build_app,
    config::Config,
    db,
    llm_crypto::{LlmKeyring, rotate_llm_encryption_keys},
    state::AppState,
    storage_gc, telemetry,
};
use tokio::{net::TcpListener, signal};
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<ExitCode> {
    dotenvy::dotenv().ok();
    telemetry::init();

    let command = std::env::args().nth(1);
    if command.as_deref() == Some("healthcheck") {
        return healthcheck().await;
    }

    let config = Config::from_env().context("failed to load application configuration")?;
    info!(
        environment = %config.environment,
        host = %config.host,
        port = config.port,
        "starting backend"
    );

    let pool = db::connect(&config)
        .await
        .context("database connection failed")?;
    if config.run_migrations {
        db::migrate(&pool)
            .await
            .context("database migration failed")?;
    }

    if command.as_deref() == Some("reset-password") {
        admin::reset_password(&pool, &config).await?;
        return Ok(ExitCode::SUCCESS);
    }
    if command.as_deref() == Some("rotate-llm-encryption-key") {
        let keyring = LlmKeyring::new(
            config.llm_encryption_key_version,
            &config.llm_encryption_secret,
            config
                .llm_encryption_previous_key_version
                .zip(config.llm_encryption_previous_secret.as_deref()),
        )
        .context("invalid LLM encryption keyring")?;
        let rotated = rotate_llm_encryption_keys(&pool, &keyring)
            .await
            .context("LLM encryption key rotation failed")?;
        info!(
            rotated,
            current_version = keyring.current_version(),
            "LLM encryption key rotation complete"
        );
        return Ok(ExitCode::SUCCESS);
    }
    if let Some(command) = command {
        anyhow::bail!(
            "unknown command {command:?}; supported commands: reset-password, rotate-llm-encryption-key"
        );
    }

    admin::ensure_initial_admin(&pool, &config)
        .await
        .context("administrator bootstrap failed")?;

    let state = AppState::new(pool, &config).context("failed to build application state")?;
    tokio::spawn(storage_gc::run(state.clone()));
    let app = build_app(state, &config).context("failed to build router")?;
    let address = SocketAddr::new(config.host, config.port);
    let listener = TcpListener::bind(address)
        .await
        .with_context(|| format!("failed to bind {address}"))?;

    info!(address = %address, "backend listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("HTTP server failed")?;

    Ok(ExitCode::SUCCESS)
}

async fn healthcheck() -> Result<ExitCode> {
    let port = std::env::var("APP_PORT").unwrap_or_else(|_| "3000".to_owned());
    let url = format!("http://127.0.0.1:{port}/health/ready");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?;

    match client.get(url).send().await {
        Ok(response) if response.status().is_success() => Ok(ExitCode::SUCCESS),
        Ok(response) => {
            error!(status = %response.status(), "healthcheck returned an unhealthy status");
            Ok(ExitCode::FAILURE)
        }
        Err(error) => {
            error!(%error, "healthcheck request failed");
            Ok(ExitCode::FAILURE)
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }

    info!("shutdown signal received");
}
