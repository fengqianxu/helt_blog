use std::{env, str::FromStr};

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use blog_backend::{
    build_app,
    config::Config,
    db,
    llm_crypto::{LlmKeyring, rotate_llm_encryption_keys},
    state::AppState,
};
use chrono::Utc;
use http_body_util::BodyExt;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use tower::ServiceExt;
use uuid::Uuid;

const JWT_SECRET: &str = "postgres-integration-jwt-secret-32-bytes";
const CURRENT_LLM_SECRET: &str = "postgres-integration-current-llm-key";
const PREVIOUS_LLM_SECRET: &str = "postgres-integration-previous-llm-key";

struct TestDatabase {
    admin_pool: PgPool,
    pool: PgPool,
    schema: String,
}

impl TestDatabase {
    async fn create() -> Self {
        let database_url =
            env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL is required for ignored tests");
        let admin_pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&database_url)
            .await
            .expect("connect to integration PostgreSQL");
        let schema = format!("test_{}", Uuid::now_v7().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&admin_pool)
            .await
            .expect("create isolated test schema");

        let search_path = format!("{schema},public");
        let options = PgConnectOptions::from_str(&database_url)
            .expect("valid TEST_DATABASE_URL")
            .options([("search_path", search_path.as_str())]);
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .expect("connect to isolated schema");
        db::migrate(&pool).await.expect("apply migrations");

        Self {
            admin_pool,
            pool,
            schema,
        }
    }

    async fn cleanup(self) {
        self.pool.close().await;
        sqlx::query(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.admin_pool)
            .await
            .expect("drop isolated test schema");
        self.admin_pool.close().await;
    }
}

fn test_config() -> Config {
    Config {
        environment: "test".to_owned(),
        host: "127.0.0.1".parse().expect("test host"),
        port: 3000,
        database_url: "postgres://unused".to_owned(),
        db_max_connections: 5,
        db_min_connections: 0,
        run_migrations: false,
        minio_endpoint: "http://minio:9000".to_owned(),
        minio_access_key: "test".to_owned(),
        minio_secret_key: "test".to_owned(),
        minio_public_bucket: "blog-public".to_owned(),
        minio_private_bucket: "blog-private".to_owned(),
        admin_username: "test".to_owned(),
        admin_initial_password: None,
        auth_jwt_secret: JWT_SECRET.to_owned(),
        artalk_internal_url: None,
        artalk_site_name: "helt.".to_owned(),
        artalk_admin_name: "test".to_owned(),
        artalk_admin_email: "test@example.com".to_owned(),
        artalk_admin_password: "test".to_owned(),
        llm_encryption_key_version: 2,
        llm_encryption_secret: CURRENT_LLM_SECRET.to_owned(),
        llm_encryption_previous_key_version: Some(1),
        llm_encryption_previous_secret: Some(PREVIOUS_LLM_SECRET.to_owned()),
        llm_private_host_allowlist: Vec::new(),
        public_origin: "http://localhost".to_owned(),
        cors_allowed_origins: vec!["http://localhost".to_owned()],
        request_timeout_secs: 10,
    }
}

fn test_app(pool: PgPool) -> Router {
    let config = test_config();
    let state = AppState::new(pool, &config).expect("build integration app state");
    build_app(state, &config).expect("build integration router")
}

#[derive(Serialize)]
struct TestClaims {
    sub: i64,
    username: String,
    iss: String,
    iat: usize,
    exp: usize,
}

fn admin_cookie() -> String {
    let now = Utc::now().timestamp() as usize;
    let claims = TestClaims {
        sub: 1,
        username: "integration-admin".to_owned(),
        iss: "helt-blog".to_owned(),
        iat: now,
        exp: now + 3_600,
    };
    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(JWT_SECRET.as_bytes()),
    )
    .expect("encode admin JWT");
    format!("helt_admin_access={token}")
}

async fn json_request(
    app: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::COOKIE, admin_cookie());
    let body = if let Some(value) = body {
        request = request.header(header::CONTENT_TYPE, "application/json");
        Body::from(serde_json::to_vec(&value).expect("serialize request"))
    } else {
        Body::empty()
    };
    let response = app
        .clone()
        .oneshot(request.body(body).expect("build request"))
        .await
        .expect("router response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("read response")
        .to_bytes();
    let payload = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("JSON response")
    };
    (status, payload)
}

fn article_payload(
    category_id: i64,
    tag_ids: &[i64],
    expected_updated_at: &str,
    title: &str,
) -> Value {
    json!({
        "expected_updated_at": expected_updated_at,
        "title": title,
        "summary": "",
        "content_md": "# Integration\n\nDatabase-backed article.",
        "category_id": category_id,
        "tag_ids": tag_ids,
        "cover_asset_id": null,
        "content_asset_ids": [],
        "is_pinned": false,
        "allow_comment": true,
        "kanban_ref": true,
        "status": "published"
    })
}

fn use_cases(connection_id: Option<i64>, enabled: bool) -> Value {
    json!({
        "kanban_chat": {
            "enabled": enabled,
            "system_prompt": "Chat safely.",
            "connection_id": connection_id,
            "model": if enabled { "test-model" } else { "" }
        },
        "article_assistant": {
            "enabled": false,
            "system_prompt": "Edit safely.",
            "connection_id": null,
            "model": ""
        }
    })
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL pointing at PostgreSQL"]
async fn article_crud_publish_conflict_and_tag_sync_use_postgres() {
    let database = TestDatabase::create().await;
    let legacy_comments_removed: bool = sqlx::query_scalar(
        "SELECT NOT EXISTS (
            SELECT 1 FROM information_schema.tables
            WHERE table_schema = $1 AND table_name = 'comments'
        )",
    )
    .bind(&database.schema)
    .fetch_one(&database.pool)
    .await
    .expect("check legacy comments table");
    assert!(legacy_comments_removed);
    let app = test_app(database.pool.clone());
    let category_id: i64 = sqlx::query_scalar("SELECT id FROM categories ORDER BY id LIMIT 1")
        .fetch_one(&database.pool)
        .await
        .expect("seed category");
    let tag_ids = sqlx::query_scalar::<_, i64>(
        "INSERT INTO tags(name) VALUES ('integration-a'), ('integration-b') RETURNING id",
    )
    .fetch_all(&database.pool)
    .await
    .expect("insert tags");

    let (status, created) = json_request(
        &app,
        Method::POST,
        "/api/v1/admin/articles",
        Some(json!({ "title": "Integration draft" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let article_id = created["id"].as_i64().expect("article id");
    let initial_updated_at = created["updated_at"].as_str().expect("creation timestamp");

    let (status, published) = json_request(
        &app,
        Method::PUT,
        &format!("/api/v1/admin/articles/{article_id}"),
        Some(article_payload(
            category_id,
            &tag_ids,
            initial_updated_at,
            "Published integration article",
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(published["status"], "published");
    let published_updated_at = published["updated_at"]
        .as_str()
        .expect("published timestamp");
    let stored_published_at: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT published_at FROM articles WHERE id=$1")
            .bind(article_id)
            .fetch_one(&database.pool)
            .await
            .expect("published row");
    assert!(stored_published_at.is_some());
    let stored_tags: Vec<i64> =
        sqlx::query_scalar("SELECT tag_id FROM article_tags WHERE article_id=$1 ORDER BY tag_id")
            .bind(article_id)
            .fetch_all(&database.pool)
            .await
            .expect("article tags");
    assert_eq!(stored_tags, tag_ids);

    let (status, _) = json_request(
        &app,
        Method::PUT,
        &format!("/api/v1/admin/articles/{article_id}"),
        Some(article_payload(
            category_id,
            &tag_ids,
            initial_updated_at,
            "Stale overwrite",
        )),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, _) = json_request(
        &app,
        Method::PUT,
        &format!("/api/v1/admin/articles/{article_id}"),
        Some(article_payload(
            category_id,
            &tag_ids[1..],
            published_updated_at,
            "Tag synchronization",
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let stored_tags: Vec<i64> =
        sqlx::query_scalar("SELECT tag_id FROM article_tags WHERE article_id=$1")
            .bind(article_id)
            .fetch_all(&database.pool)
            .await
            .expect("synchronized tags");
    assert_eq!(stored_tags, tag_ids[1..]);

    let (status, public_article) = json_request(
        &app,
        Method::GET,
        &format!(
            "/api/v1/articles/{}",
            created["slug"].as_str().expect("slug")
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(public_article["article"]["title"], "Tag synchronization");
    assert!(public_article["article"].get("comment_count").is_none());

    let (status, _) = json_request(
        &app,
        Method::DELETE,
        &format!("/api/v1/admin/articles/{article_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let remains: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM articles WHERE id=$1)")
        .bind(article_id)
        .fetch_one(&database.pool)
        .await
        .expect("article deletion");
    assert!(!remains);

    database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL pointing at PostgreSQL"]
async fn bangumi_mirror_is_public_paginated_and_filterable() {
    let database = TestDatabase::create().await;
    sqlx::query(
        "UPDATE site_settings
         SET settings = jsonb_set(settings, '{bangumi_sync,uid}', to_jsonb('123456'::text), true)",
    )
    .execute(&database.pool)
    .await
    .expect("configure Bilibili UID");
    sqlx::query(
        "INSERT INTO bangumi (
             bilibili_media_id, season_id, title, cover_key, status,
             ep_current, ep_total, sort_order, metadata
         ) VALUES
         (1001, 2001, 'Watching fixture', 'bangumi/covers/1001', 'watching', 3, 12, 0,
          '{\"season_type\":\"番剧\",\"summary\":\"Watching summary\",\"score\":9.5,\"url\":\"https://www.bilibili.com/bangumi/play/ss2001\",\"latest_episode\":\"更新至第4话\"}'::jsonb),
         (1002, 2002, 'Finished fixture', NULL, 'finished', 12, 12, 0,
          '{\"season_type\":\"国创\",\"summary\":\"Finished summary\",\"source_cover\":\"https://i0.hdslb.com/test.png\"}'::jsonb)",
    )
    .execute(&database.pool)
    .await
    .expect("insert bangumi mirror fixtures");
    let app = test_app(database.pool.clone());

    let (status, all) = json_request(&app, Method::GET, "/api/v1/bangumi?per_page=100", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(all["total"], 2);
    assert_eq!(all["meta"]["counts"]["watching"], 1);
    assert_eq!(all["meta"]["counts"]["finished"], 1);
    assert_eq!(all["meta"]["configured"], true);
    assert_eq!(all["meta"]["sync_status"], "queued");
    assert_eq!(all["items"][0]["title"], "Watching fixture");
    assert_eq!(all["items"][0]["cover_url"], "/storage/bangumi/covers/1001");
    assert_eq!(
        all["items"][1]["cover_url"],
        "https://i0.hdslb.com/test.png"
    );

    let (status, finished) = json_request(
        &app,
        Method::GET,
        "/api/v1/bangumi?status=finished&page=1&per_page=10",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(finished["total"], 1);
    assert_eq!(finished["items"][0]["status"], "finished");

    let (status, _) = json_request(&app, Method::GET, "/api/v1/bangumi?status=invalid", None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL pointing at PostgreSQL"]
async fn steam_game_mirror_is_public_paginated_and_reports_progress() {
    let database = TestDatabase::create().await;
    let keyring = LlmKeyring::new(
        2,
        CURRENT_LLM_SECRET,
        Some((1, PREVIOUS_LLM_SECRET)),
    )
    .expect("Steam test keyring");
    let encrypted_steam_key = keyring
        .encrypt("0123456789ABCDEF0123456789ABCDEF")
        .expect("encrypt Steam fixture key");
    sqlx::query(
        "UPDATE site_settings
         SET settings = jsonb_set(
                 settings #- '{steam_sync,web_api_key}'::text[],
                 '{steam_sync,steam_id64}',
                 to_jsonb('76561198000000000'::text),
                 true
             ),
             steam_web_api_key_ciphertext = $1,
             steam_web_api_key_nonce = $2,
             steam_encryption_key_version = $3",
    )
    .bind(encrypted_steam_key.ciphertext)
    .bind(encrypted_steam_key.nonce)
    .bind(encrypted_steam_key.key_version)
    .execute(&database.pool)
    .await
    .expect("configure Steam credentials");
    sqlx::query(
        "INSERT INTO games (
             title, status, play_hours, sort_order, steam_app_id, icon_hash,
             playtime_2weeks_minutes, playtime_forever_minutes,
             playtime_windows_minutes, last_played_at, synced_at
         ) VALUES
         ('Recent fixture', 'playing', 12.5, 0, 1001, 'recent-icon', 90, 750, 750, now(), now()),
         ('Library fixture', 'finished', 40, 1, 1002, '', 0, 2400, 2400, NULL, now())",
    )
    .execute(&database.pool)
    .await
    .expect("insert Steam game mirror fixtures");
    let app = test_app(database.pool.clone());

    let (status, all) = json_request(&app, Method::GET, "/api/v1/games?per_page=100", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(all["total"], 2);
    assert_eq!(all["meta"]["counts"]["total"], 2);
    assert_eq!(all["meta"]["counts"]["recent"], 1);
    assert_eq!(all["meta"]["configured"], true);
    assert_eq!(all["meta"]["sync_status"], "queued");
    assert_eq!(all["items"][0]["title"], "Recent fixture");
    assert_eq!(all["items"][0]["playtime_2weeks_minutes"], 90);
    assert_eq!(all["items"][0]["playtime_forever_minutes"], 750);
    assert_eq!(
        all["items"][0]["steam_url"],
        "https://store.steampowered.com/app/1001"
    );

    let (status, recent) = json_request(
        &app,
        Method::GET,
        "/api/v1/games?status=playing&page=1&per_page=10",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(recent["total"], 1);
    assert_eq!(recent["items"][0]["steam_app_id"], 1001);

    let (status, by_playtime) = json_request(
        &app,
        Method::GET,
        "/api/v1/games?sort=playtime&page=1&per_page=10",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(by_playtime["items"][0]["steam_app_id"], 1002);

    let (status, _) = json_request(&app, Method::GET, "/api/v1/games?status=invalid", None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = json_request(&app, Method::GET, "/api/v1/games?sort=invalid", None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL pointing at PostgreSQL"]
async fn llm_rotation_key_delete_and_use_case_references_use_postgres() {
    let database = TestDatabase::create().await;
    let previous = LlmKeyring::new(1, PREVIOUS_LLM_SECRET, None).expect("previous keyring");
    let legacy = previous
        .encrypt("legacy-api-key")
        .expect("legacy ciphertext");
    let connection_id: i64 = sqlx::query_scalar(
        "INSERT INTO llm_connections
         (display_name, base_url, model, api_key_ciphertext, api_key_nonce,
          encryption_key_version, enabled)
         VALUES ('Integration', 'https://api.example.com/v1', '', $1, $2, $3, true)
         RETURNING id",
    )
    .bind(&legacy.ciphertext)
    .bind(&legacy.nonce)
    .bind(legacy.key_version)
    .fetch_one(&database.pool)
    .await
    .expect("insert legacy connection");

    let unsupported_key = LlmKeyring::new(9, "unsupported-integration-key-material", None)
        .expect("unsupported keyring");
    let unsupported = unsupported_key
        .encrypt("unsupported-api-key")
        .expect("unsupported ciphertext");
    let unsupported_id: i64 = sqlx::query_scalar(
        "INSERT INTO llm_connections
         (display_name, base_url, model, api_key_ciphertext, api_key_nonce,
          encryption_key_version, enabled)
         VALUES ('Unsupported', 'https://api.example.com/v1', '', $1, $2, $3, true)
         RETURNING id",
    )
    .bind(&unsupported.ciphertext)
    .bind(&unsupported.nonce)
    .bind(unsupported.key_version)
    .fetch_one(&database.pool)
    .await
    .expect("insert unsupported connection");

    let rotating = LlmKeyring::new(2, CURRENT_LLM_SECRET, Some((1, PREVIOUS_LLM_SECRET)))
        .expect("rotating keyring");
    assert!(
        rotate_llm_encryption_keys(&database.pool, &rotating)
            .await
            .is_err()
    );
    let version_after_failed_rotation: i32 =
        sqlx::query_scalar("SELECT encryption_key_version FROM llm_connections WHERE id=$1")
            .bind(connection_id)
            .fetch_one(&database.pool)
            .await
            .expect("version after rollback");
    assert_eq!(version_after_failed_rotation, 1);

    sqlx::query("DELETE FROM llm_connections WHERE id=$1")
        .bind(unsupported_id)
        .execute(&database.pool)
        .await
        .expect("remove unsupported fixture");
    assert_eq!(
        rotate_llm_encryption_keys(&database.pool, &rotating)
            .await
            .expect("atomic rotation"),
        1
    );
    let (ciphertext, nonce, version): (Vec<u8>, Vec<u8>, i32) = sqlx::query_as(
        "SELECT api_key_ciphertext, api_key_nonce, encryption_key_version
         FROM llm_connections WHERE id=$1",
    )
    .bind(connection_id)
    .fetch_one(&database.pool)
    .await
    .expect("rotated credential");
    assert_eq!(version, 2);
    assert_eq!(
        rotating
            .decrypt(version, &ciphertext, &nonce)
            .expect("decrypt rotated credential"),
        "legacy-api-key"
    );

    let app = test_app(database.pool.clone());
    let (status, settings) = json_request(&app, Method::GET, "/api/v1/admin/llm", None).await;
    assert_eq!(status, StatusCode::OK);
    let revision = settings["revision"].as_i64().expect("LLM revision");
    let preserve_credentials = json!({
        "revision": revision,
        "connections": [{
            "id": connection_id,
            "display_name": "Integration renamed",
            "base_url": "https://api.example.com/v1",
            "model": "",
            "temperature": 0.7,
            "max_tokens": 512,
            "enabled": true
        }],
        "use_cases": use_cases(None, false)
    });
    let (status, preserved) = json_request(
        &app,
        Method::PUT,
        "/api/v1/admin/llm",
        Some(preserve_credentials),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let revision = preserved["revision"]
        .as_i64()
        .expect("updated LLM revision");
    let (preserved_ciphertext, preserved_version): (Vec<u8>, i32) = sqlx::query_as(
        "SELECT api_key_ciphertext, encryption_key_version
         FROM llm_connections WHERE id=$1",
    )
    .bind(connection_id)
    .fetch_one(&database.pool)
    .await
    .expect("credential after non-secret settings update");
    assert_eq!(preserved_ciphertext, ciphertext);
    assert_eq!(preserved_version, 2);

    let rejected_delete = json!({
        "revision": revision,
        "connections": [],
        "use_cases": use_cases(Some(connection_id), true)
    });
    let (status, _) = json_request(
        &app,
        Method::PUT,
        "/api/v1/admin/llm",
        Some(rejected_delete),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let still_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM llm_connections WHERE id=$1)")
            .bind(connection_id)
            .fetch_one(&database.pool)
            .await
            .expect("connection retained after rejected delete");
    assert!(still_exists);

    let accepted_delete = json!({
        "revision": revision,
        "connections": [],
        "use_cases": use_cases(None, false)
    });
    let (status, updated) = json_request(
        &app,
        Method::PUT,
        "/api/v1/admin/llm",
        Some(accepted_delete),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["connections"], json!([]));
    let remains: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM llm_connections WHERE id=$1)")
            .bind(connection_id)
            .fetch_one(&database.pool)
            .await
            .expect("connection deleted");
    assert!(!remains);

    database.cleanup().await;
}
