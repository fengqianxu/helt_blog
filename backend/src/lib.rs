pub mod admin;
pub mod artalk;
pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod llm_crypto;
pub mod llm_network;
pub mod routes;
pub mod state;
pub mod storage;
pub mod storage_gc;
pub mod telemetry;

use std::time::Duration;

use anyhow::{Context, Result};
use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{
        HeaderName, HeaderValue, Method, StatusCode,
        header::{self, AUTHORIZATION, CONTENT_TYPE},
    },
};
use tower_http::{
    catch_panic::CatchPanicLayer,
    compression::CompressionLayer,
    cors::CorsLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    set_header::SetResponseHeaderLayer,
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

use crate::{config::Config, state::AppState};

pub fn build_app(state: AppState, config: &Config) -> Result<Router> {
    let allowed_origins = config
        .cors_allowed_origins
        .iter()
        .map(|origin| {
            origin
                .parse::<HeaderValue>()
                .with_context(|| format!("invalid CORS origin: {origin}"))
        })
        .collect::<Result<Vec<_>>>()?;

    let cors = CorsLayer::new()
        .allow_origin(allowed_origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            CONTENT_TYPE,
            AUTHORIZATION,
            HeaderName::from_static("x-request-id"),
        ])
        .allow_credentials(true);

    let request_id_header = HeaderName::from_static("x-request-id");

    Ok(Router::new()
        .merge(routes::router())
        .fallback(error::not_found)
        .layer(DefaultBodyLimit::max(
            routes::contract::DEFAULT_REQUEST_BODY_LIMIT_BYTES,
        ))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::GATEWAY_TIMEOUT,
            Duration::from_secs(config.request_timeout_secs),
        ))
        .layer(CatchPanicLayer::new())
        .layer(CompressionLayer::new())
        .layer(cors)
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::new(request_id_header.clone()))
        .layer(SetRequestIdLayer::new(request_id_header, MakeRequestUuid))
        .with_state(state))
}
