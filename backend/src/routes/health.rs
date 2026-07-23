use axum::{Json, extract::State, http::StatusCode};
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
pub struct LiveResponse {
    status: &'static str,
    version: &'static str,
    started_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct ReadyResponse {
    status: &'static str,
    services: Services,
}

#[derive(Serialize)]
pub struct Services {
    postgres: &'static str,
    minio: &'static str,
}

pub async fn live(State(state): State<AppState>) -> Json<LiveResponse> {
    Json(LiveResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        started_at: state.started_at(),
    })
}

pub async fn ready(State(state): State<AppState>) -> (StatusCode, Json<ReadyResponse>) {
    let postgres_ok = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(state.pool())
        .await
        .is_ok();
    let minio_ok = state
        .http_client()
        .get(state.minio_health_url())
        .send()
        .await
        .is_ok_and(|response| response.status().is_success());

    let ready = postgres_ok && minio_ok;
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(ReadyResponse {
            status: if ready { "ok" } else { "degraded" },
            services: Services {
                postgres: if postgres_ok { "up" } else { "down" },
                minio: if minio_ok { "up" } else { "down" },
            },
        }),
    )
}
