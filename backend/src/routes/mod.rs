mod articles;
mod assets;
pub mod bangumi;
pub mod contract;
pub mod games;
mod health;
mod llm;
mod raiments;

use axum::{Json, Router, routing::get};
use serde::Serialize;

use crate::{auth, state::AppState};

#[derive(Serialize)]
struct ApiIndex {
    name: &'static str,
    version: &'static str,
    status: &'static str,
    health: HealthLinks,
}

#[derive(Serialize)]
struct HealthLinks {
    live: &'static str,
    ready: &'static str,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(auth::router())
        .merge(assets::router())
        .merge(articles::router())
        .merge(bangumi::router())
        .merge(games::router())
        .merge(llm::router())
        .merge(raiments::router())
        // 尚未实现的业务处理器继续注册契约占位路由；每个占位端点返回统一 501。
        // 已实现的业务域会在 contract::router 中自动排除，契约测试仍校验路径和方法不漂移。
        .merge(contract::router())
        .route("/", get(index))
        .route("/api/v1", get(index))
        .route("/health/live", get(health::live))
        .route("/health/ready", get(health::ready))
}

async fn index() -> Json<ApiIndex> {
    Json(ApiIndex {
        name: "helt-blog-api",
        version: env!("CARGO_PKG_VERSION"),
        status: "ok",
        health: HealthLinks {
            live: "/health/live",
            ready: "/health/ready",
        },
    })
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    use crate::{build_app, config::Config, state::AppState};

    fn test_config() -> Config {
        Config {
            environment: "test".to_owned(),
            host: "127.0.0.1".parse().unwrap(),
            port: 3000,
            database_url: "postgres://test:test@localhost/test".to_owned(),
            db_max_connections: 1,
            db_min_connections: 0,
            run_migrations: false,
            minio_endpoint: "http://localhost:9000".to_owned(),
            minio_access_key: "test".to_owned(),
            minio_secret_key: "test".to_owned(),
            minio_public_bucket: "blog-public".to_owned(),
            minio_private_bucket: "blog-private".to_owned(),
            admin_username: "test".to_owned(),
            admin_initial_password: Some("test".to_owned()),
            auth_jwt_secret: "test-secret-at-least-32-bytes-long".to_owned(),
            artalk_internal_url: None,
            artalk_site_name: "helt.".to_owned(),
            artalk_admin_name: "test".to_owned(),
            artalk_admin_email: "test@example.com".to_owned(),
            artalk_admin_password: "test".to_owned(),
            llm_encryption_key_version: 1,
            llm_encryption_secret: "test-llm-encryption-secret-at-least-32-bytes".to_owned(),
            llm_encryption_previous_key_version: None,
            llm_encryption_previous_secret: None,
            llm_private_host_allowlist: Vec::new(),
            public_origin: "http://localhost".to_owned(),
            cors_allowed_origins: vec!["http://localhost:5173".to_owned()],
            request_timeout_secs: 5,
        }
    }

    #[tokio::test]
    async fn liveness_does_not_require_dependencies() {
        let config = test_config();
        let pool = PgPoolOptions::new()
            .connect_lazy(&config.database_url)
            .unwrap();
        let state = AppState::new(pool, &config).unwrap();
        let app = build_app(state, &config).unwrap();

        let response = app
            .oneshot(Request::get("/health/live").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().contains_key("x-request-id"));
    }

    #[tokio::test]
    async fn unknown_routes_use_json_404() {
        let config = test_config();
        let pool = PgPoolOptions::new()
            .connect_lazy(&config.database_url)
            .unwrap();
        let state = AppState::new(pool, &config).unwrap();
        let app = build_app(state, &config).unwrap();

        let response = app
            .oneshot(Request::get("/missing").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/json"
        );
    }
}
