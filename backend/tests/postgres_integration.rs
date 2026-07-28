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
        meting_api_url: None,
        llm_encryption_key_version: 2,
        llm_encryption_secret: CURRENT_LLM_SECRET.to_owned(),
        llm_encryption_previous_key_version: Some(1),
        llm_encryption_previous_secret: Some(PREVIOUS_LLM_SECRET.to_owned()),
        llm_private_host_allowlist: Vec::new(),
        public_origin: "http://localhost".to_owned(),
        cors_allowed_origins: vec!["http://localhost".to_owned()],
        request_timeout_secs: 10,
        asset_request_timeout_secs: 300,
        upstream_request_timeout_secs: 15,
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
    let payload = if bytes.iter().all(u8::is_ascii_whitespace) {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or_else(|error| {
            panic!(
                "JSON response for {status} could not be decoded: {error}; body={:?}",
                String::from_utf8_lossy(&bytes)
            )
        })
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
async fn friend_applications_are_private_until_reviewed_and_rate_limited() {
    let database = TestDatabase::create().await;
    let app = test_app(database.pool.clone());

    let application = json!({
        "name": "Integration Notes",
        "url": "https://integration.example",
        "avatar_url": "https://integration.example/avatar.png",
        "contact_email": "owner@integration.example",
        "description": "A database-backed friend application."
    });
    let (status, created) = json_request(
        &app,
        Method::POST,
        "/api/v1/friends",
        Some(application.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["status"], "pending");
    let friend_id = created["id"].as_i64().expect("friend id");

    let (status, public_before_review) =
        json_request(&app, Method::GET, "/api/v1/friends", None).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "public friend list failed: {public_before_review}"
    );
    assert_eq!(public_before_review["total"], 0);

    let (status, duplicate) =
        json_request(&app, Method::POST, "/api/v1/friends", Some(application)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(duplicate["error"]["code"], "conflict");

    let (status, pending) = json_request(
        &app,
        Method::GET,
        "/api/v1/admin/friends?status=pending",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(pending["counts"]["pending"], 1);
    assert_eq!(
        pending["items"][0]["contact_email"],
        "owner@integration.example"
    );

    let (status, missing_avatar) = json_request(
        &app,
        Method::PATCH,
        &format!("/api/v1/admin/friends/{friend_id}"),
        Some(json!({ "status": "approved" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(missing_avatar["error"]["code"], "validation_error");

    let avatar_asset_id: i64 = sqlx::query_scalar(
        "SELECT id
         FROM assets
         WHERE status = 'active'
           AND media_type = 'image'
           AND upload_id IS NOT NULL
         ORDER BY id
         LIMIT 1",
    )
    .fetch_one(&database.pool)
    .await
    .expect("seeded image asset");
    let (status, approved) = json_request(
        &app,
        Method::PATCH,
        &format!("/api/v1/admin/friends/{friend_id}"),
        Some(json!({
            "status": "approved",
            "avatar_asset_id": avatar_asset_id
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(approved["status"], "approved");
    assert!(approved["reviewed_at"].is_string());

    let (status, public_after_review) =
        json_request(&app, Method::GET, "/api/v1/friends", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(public_after_review["total"], 1);
    assert_eq!(public_after_review["items"][0]["name"], "Integration Notes");
    assert!(
        public_after_review["items"][0]["avatar_url"]
            .as_str()
            .expect("public avatar URL")
            .starts_with("/storage/")
    );
    assert!(
        public_after_review["items"][0]
            .get("contact_email")
            .is_none()
    );
    assert!(public_after_review["items"][0].get("id").is_none());

    let (status, second) = json_request(
        &app,
        Method::POST,
        "/api/v1/friends",
        Some(json!({
            "name": "Second Site",
            "url": "https://second.example",
            "contact_email": "owner@second.example"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let second_id = second["id"].as_i64().expect("second friend id");
    let (status, rejected) = json_request(
        &app,
        Method::PATCH,
        &format!("/api/v1/admin/friends/{second_id}"),
        Some(json!({ "status": "rejected" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rejected["status"], "rejected");

    let (status, limited) = json_request(
        &app,
        Method::POST,
        "/api/v1/friends",
        Some(json!({
            "name": "Third Site",
            "url": "https://third.example",
            "contact_email": "owner@third.example"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(limited["error"]["code"], "rate_limited");

    database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL pointing at PostgreSQL"]
async fn raiments_and_site_schedule_support_crud_and_revision_conflicts() {
    let database = TestDatabase::create().await;
    let app = test_app(database.pool.clone());

    let (status, public) = json_request(&app, Method::GET, "/api/v1/raiments", None).await;
    assert_eq!(status, StatusCode::OK);
    let public_items = public["items"].as_array().expect("public items");
    assert_eq!(public_items.len(), 2);
    let public_saber = public_items
        .iter()
        .find(|item| item["id"] == "saber")
        .expect("public Saber raiment");
    let public_alter = public_items
        .iter()
        .find(|item| item["id"] == "alter-saber")
        .expect("public Alter Saber raiment");
    assert_eq!(public_saber["name"], "日间模式");
    assert_eq!(public_alter["color_scheme"], "night");
    assert_eq!(public_saber["cover_character_name"], "Saber");
    assert!(public_saber["cover_voice_url"].is_string());
    assert_eq!(
        public["schedule"]["periods"]
            .as_array()
            .expect("periods")
            .len(),
        2
    );
    assert_eq!(public["schedule"]["periods"][0]["start_at"], "07:00");
    assert_eq!(public["schedule"]["periods"][1]["end_at"], "07:00");
    assert!(
        public["items"]
            .as_array()
            .expect("public items")
            .iter()
            .all(|item| item["cover_url"]
                .as_str()
                .expect("cover URL")
                .starts_with("/storage/raiments/"))
    );
    assert!(public["items"][0].get("revision").is_none());

    let (status, admin) = json_request(&app, Method::GET, "/api/v1/admin/raiments", None).await;
    assert_eq!(status, StatusCode::OK);
    let saber = admin["items"]
        .as_array()
        .expect("admin items")
        .iter()
        .find(|item| item["id"] == "saber")
        .expect("seed saber");
    let revision = saber["revision"].as_i64().expect("revision");
    let cover_asset_id = saber["cover_asset_id"].as_i64().expect("cover asset");
    let voice_asset_id = saber["cover_voice_asset_id"].as_i64().expect("voice asset");
    let success_voice_asset_id = saber["login_success_voice_asset_id"]
        .as_i64()
        .expect("login success voice asset");
    let theme = saber["theme"].clone();

    let update = json!({
        "revision": revision,
        "name": "Saber Lily",
        "cover_asset_id": cover_asset_id,
        "theme": theme,
        "enabled": true,
        "sort_order": 0,
        "is_default": true,
        "color_scheme": "day",
        "cover_title": "Saber Lily\n新的封面",
        "cover_subtitle": "测试副标题",
        "cover_character_name": "Lily",
        "cover_dialogue": "欢迎回来。",
        "cover_voice_label": "播放 Lily 语音",
        "cover_voice_asset_id": voice_asset_id,
        "login_success_voice_asset_id": success_voice_asset_id,
        "kanban_asset_id": null
    });
    let (status, saved) = json_request(
        &app,
        Method::PUT,
        "/api/v1/admin/raiments/saber",
        Some(update.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["name"], "Saber Lily");
    assert_eq!(saved["revision"], revision + 1);

    let (status, conflict) = json_request(
        &app,
        Method::PUT,
        "/api/v1/admin/raiments/saber",
        Some(update),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["error"]["code"], "conflict");

    let create = json!({
        "name": "午后模式",
        "cover_asset_id": cover_asset_id,
        "theme": saved["theme"].clone(),
        "enabled": true,
        "sort_order": 10,
        "is_default": false,
        "color_scheme": "day",
        "cover_title": "午后的标题",
        "cover_subtitle": "午后副标题",
        "cover_character_name": "Lily",
        "cover_dialogue": "午后好。",
        "cover_voice_label": "播放午后语音",
        "cover_voice_asset_id": voice_asset_id,
        "login_success_voice_asset_id": null,
        "kanban_asset_id": null
    });
    let (status, created) =
        json_request(&app, Method::POST, "/api/v1/admin/raiments", Some(create)).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["name"], "午后模式");
    assert_eq!(created["is_builtin"], false);
    assert!(
        created["id"]
            .as_str()
            .expect("created id")
            .starts_with("raiment-")
    );
    let created_revision = created["revision"].as_i64().expect("created revision");
    let mut created_update = created.clone();
    let created_update = created_update
        .as_object_mut()
        .expect("created raiment object");
    created_update.remove("id");
    created_update.remove("cover_asset");
    created_update.remove("cover_voice_asset");
    created_update.remove("login_success_voice_asset");
    created_update.remove("is_builtin");
    created_update.remove("created_at");
    created_update.remove("updated_at");
    let (status, edited_created) = json_request(
        &app,
        Method::PUT,
        &format!(
            "/api/v1/admin/raiments/{}",
            created["id"].as_str().expect("created id")
        ),
        Some(Value::Object(created_update.clone())),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(edited_created["revision"], created_revision + 1);

    let created_id = created["id"].as_str().expect("created id");
    let (status, stale_delete) = json_request(
        &app,
        Method::DELETE,
        &format!("/api/v1/admin/raiments/{created_id}"),
        Some(json!({ "revision": created_revision })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(stale_delete["error"]["code"], "conflict");

    let (status, public) = json_request(&app, Method::GET, "/api/v1/raiments", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(public["items"].as_array().expect("public items").len(), 3);
    assert!(
        public["items"]
            .as_array()
            .expect("public items")
            .iter()
            .all(|item| item.get("switch_at").is_none())
    );
    assert!(
        public["items"]
            .as_array()
            .expect("public items")
            .iter()
            .find(|item| item["id"] == "saber")
            .expect("public saber")
            .get("revision")
            .is_none()
    );

    let referenced_asset: i64 = sqlx::query_scalar(
        "SELECT asset_id FROM asset_references
         WHERE source_type = 'raiment_cover' AND source_key = 'saber'",
    )
    .fetch_one(&database.pool)
    .await
    .expect("raiment cover reference");
    assert_eq!(referenced_asset, cover_asset_id);

    let (status, schedule) = json_request(
        &app,
        Method::GET,
        "/api/v1/admin/site/raiment-schedule",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let schedule_revision = schedule["revision"].as_i64().expect("schedule revision");
    let (status, playlists) =
        json_request(&app, Method::GET, "/api/v1/admin/playlists", None).await;
    assert_eq!(status, StatusCode::OK);
    let playlist = playlists["items"]
        .as_array()
        .expect("playlists")
        .first()
        .expect("seed playlist");
    let playlist_id = playlist["id"].as_i64().expect("playlist id");
    assert!(playlist.get("tracks").is_none());
    assert!(playlist.get("track_count").is_some());
    let (status, playlist_tracks) = json_request(
        &app,
        Method::GET,
        &format!("/api/v1/admin/playlists/{playlist_id}/tracks?page=1&per_page=10"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(playlist_tracks["page"], 1);
    assert_eq!(playlist_tracks["per_page"], 10);
    let playlist_track_page = playlist_tracks["items"]
        .as_array()
        .expect("playlist tracks");
    assert!(playlist_track_page.len() <= 10);
    assert!(
        playlist_tracks["total"].as_i64().expect("playlist total")
            >= playlist_track_page.len() as i64
    );
    let (status, saved_schedule) = json_request(
        &app,
        Method::PUT,
        "/api/v1/admin/site/raiment-schedule",
        Some(json!({
            "revision": schedule_revision,
            "periods": [
                {"id":"morning","start_at":"06:00","end_at":"12:00","raiment_id":"saber","playlist_id":playlist_id},
                {"id":"afternoon","start_at":"12:00","end_at":"18:00","raiment_id":created_id,"playlist_id":null},
                {"id":"night","start_at":"18:00","end_at":"06:00","raiment_id":"alter-saber","playlist_id":null}
            ]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved_schedule["revision"], schedule_revision + 1);
    assert_eq!(saved_schedule["periods"][0]["playlist_id"], playlist_id);

    let (status, public) = json_request(&app, Method::GET, "/api/v1/raiments", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(public["schedule"]["periods"][0]["playlist_id"], playlist_id);

    let (status, playlist_conflict) = json_request(
        &app,
        Method::PUT,
        &format!("/api/v1/admin/playlists/{playlist_id}"),
        Some(json!({
            "name": playlist["name"],
            "description": playlist["description"],
            "enabled": false
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(playlist_conflict["error"]["code"], "conflict");

    let (status, playlist_delete_conflict) = json_request(
        &app,
        Method::DELETE,
        &format!("/api/v1/admin/playlists/{playlist_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(playlist_delete_conflict["error"]["code"], "conflict");

    let (status, conflict) = json_request(
        &app,
        Method::DELETE,
        &format!("/api/v1/admin/raiments/{created_id}"),
        Some(json!({ "revision": edited_created["revision"] })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["error"]["code"], "conflict");

    let (status, _) = json_request(
        &app,
        Method::PUT,
        "/api/v1/admin/site/raiment-schedule",
        Some(json!({
            "revision": saved_schedule["revision"],
            "periods": [
                {"id":"day","start_at":"06:00","end_at":"18:00","raiment_id":"saber","playlist_id":null},
                {"id":"night","start_at":"18:00","end_at":"06:00","raiment_id":"alter-saber","playlist_id":null}
            ]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = json_request(
        &app,
        Method::DELETE,
        &format!("/api/v1/admin/raiments/{created_id}"),
        Some(json!({ "revision": edited_created["revision"] })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(body.is_null());

    sqlx::query("UPDATE site_settings SET settings = settings - 'raiment_schedule' WHERE id = 1")
        .execute(&database.pool)
        .await
        .expect("remove schedule to exercise legacy fallback");
    let (status, fallback_schedule) = json_request(
        &app,
        Method::GET,
        "/api/v1/admin/site/raiment-schedule",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fallback_schedule["revision"], 1);
    assert_eq!(fallback_schedule["periods"], json!([]));
    let (status, initialized_schedule) = json_request(
        &app,
        Method::PUT,
        "/api/v1/admin/site/raiment-schedule",
        Some(json!({ "revision": 1, "periods": [] })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(initialized_schedule["revision"], 2);

    database.cleanup().await;
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

    let (status, synchronized) = json_request(
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
    let synchronized_updated_at = synchronized["updated_at"]
        .as_str()
        .expect("synchronized timestamp");
    let synchronized_timestamp = synchronized_updated_at
        .parse::<chrono::DateTime<Utc>>()
        .expect("parse synchronized timestamp");
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
    let timestamp_after_view: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT updated_at FROM articles WHERE id=$1")
            .bind(article_id)
            .fetch_one(&database.pool)
            .await
            .expect("timestamp after public view");
    assert_eq!(timestamp_after_view, synchronized_timestamp);

    let (status, saved_after_view) = json_request(
        &app,
        Method::PUT,
        &format!("/api/v1/admin/articles/{article_id}"),
        Some(article_payload(
            category_id,
            &tag_ids[1..],
            synchronized_updated_at,
            "Saved after public view",
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved_after_view["status"], "published");

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
async fn moments_crud_public_timeline_and_likes_use_postgres() {
    let database = TestDatabase::create().await;
    let app = test_app(database.pool.clone());
    let created_at = Utc::now();

    let (status, created) = json_request(
        &app,
        Method::POST,
        "/api/v1/admin/moments",
        Some(json!({
            "content": "第一条数据库说说",
            "asset_ids": [],
            "created_at": created_at
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let moment_id = created["id"].as_i64().expect("moment id");
    assert_eq!(created["content"], "第一条数据库说说");
    assert_eq!(created["images"], json!([]));

    let (status, public_list) = json_request(
        &app,
        Method::GET,
        "/api/v1/moments?page=1&per_page=10&visitor_id=integration-visitor",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(public_list["total"], 1);
    assert_eq!(public_list["items"][0]["id"], moment_id);
    assert_eq!(public_list["items"][0]["liked_by_me"], false);

    let (status, liked) = json_request(
        &app,
        Method::POST,
        &format!("/api/v1/moments/{moment_id}/like"),
        Some(json!({ "visitor_id": "integration-visitor" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(liked["liked"], true);
    assert_eq!(liked["like_count"], 1);

    let (status, updated) = json_request(
        &app,
        Method::PUT,
        &format!("/api/v1/admin/moments/{moment_id}"),
        Some(json!({
            "content": "编辑后的数据库说说",
            "asset_ids": [],
            "created_at": created_at
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["content"], "编辑后的数据库说说");
    assert_eq!(updated["like_count"], 1);

    let (status, admin_list) = json_request(
        &app,
        Method::GET,
        "/api/v1/admin/moments?page=1&per_page=10&search=%E7%BC%96%E8%BE%91",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(admin_list["total"], 1);

    let (status, _) = json_request(
        &app,
        Method::DELETE,
        &format!("/api/v1/admin/moments/{moment_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let attempts_remain: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM moment_like_attempts WHERE moment_id=$1)")
            .bind(moment_id)
            .fetch_one(&database.pool)
            .await
            .expect("check like attempt cascade");
    assert!(!attempts_remain);

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
    let keyring = LlmKeyring::new(2, CURRENT_LLM_SECRET, Some((1, PREVIOUS_LLM_SECRET)))
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
