use std::collections::{HashMap, HashSet};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, Postgres, Transaction};
use tracing::error;
use uuid::Uuid;

use crate::{
    artalk::{ArtalkError, article_page_key},
    auth,
    error::{ErrorBody, ErrorEnvelope},
    routes::contract::HttpMethod,
    state::AppState,
};

const MAX_BATCH: usize = 100;
const MAX_TITLE_CHARS: usize = 200;
const MAX_SUMMARY_CHARS: usize = 280;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/articles", get(public_list))
        .route("/api/v1/articles/{slug}", get(public_detail))
        .route("/api/v1/categories", get(public_categories))
        .route("/api/v1/tags", get(public_tags))
        .route("/api/v1/admin/articles", get(admin_list).post(admin_create))
        .route("/api/v1/admin/articles/batch", post(admin_batch))
        .route(
            "/api/v1/admin/articles/{id}",
            get(admin_detail).put(admin_update).delete(admin_delete),
        )
        .route(
            "/api/v1/admin/categories",
            get(admin_categories).post(admin_create_category),
        )
        .route(
            "/api/v1/admin/categories/{id}",
            patch(admin_update_category).delete(admin_delete_category),
        )
        .route("/api/v1/admin/tags", get(admin_tags).post(admin_create_tag))
        .route(
            "/api/v1/admin/tags/{id}",
            patch(admin_update_tag).delete(admin_delete_tag),
        )
}

pub fn implements(method: HttpMethod, path: &str) -> bool {
    matches!(
        (method, path),
        (HttpMethod::Get, "/api/v1/articles")
            | (HttpMethod::Get, "/api/v1/articles/{slug}")
            | (HttpMethod::Get, "/api/v1/categories")
            | (HttpMethod::Get, "/api/v1/tags")
            | (HttpMethod::Get, "/api/v1/admin/articles")
            | (HttpMethod::Post, "/api/v1/admin/articles")
            | (HttpMethod::Get, "/api/v1/admin/articles/{id}")
            | (HttpMethod::Put, "/api/v1/admin/articles/{id}")
            | (HttpMethod::Delete, "/api/v1/admin/articles/{id}")
            | (HttpMethod::Post, "/api/v1/admin/articles/batch")
            | (HttpMethod::Get, "/api/v1/admin/categories")
            | (HttpMethod::Post, "/api/v1/admin/categories")
            | (HttpMethod::Patch, "/api/v1/admin/categories/{id}")
            | (HttpMethod::Delete, "/api/v1/admin/categories/{id}")
            | (HttpMethod::Get, "/api/v1/admin/tags")
            | (HttpMethod::Post, "/api/v1/admin/tags")
            | (HttpMethod::Patch, "/api/v1/admin/tags/{id}")
            | (HttpMethod::Delete, "/api/v1/admin/tags/{id}")
    )
}

#[derive(Debug, thiserror::Error)]
enum ArticleError {
    #[error("需要有效的管理员会话")]
    Unauthorized,
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Conflict(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Artalk(#[from] ArtalkError),
}

impl ArticleError {
    fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }
}

impl IntoResponse for ArticleError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
                "需要有效的管理员会话".to_owned(),
            ),
            Self::NotFound(message) => (StatusCode::NOT_FOUND, "not_found", message),
            Self::Validation(message) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_error",
                message,
            ),
            Self::Conflict(message) => (StatusCode::CONFLICT, "conflict", message),
            Self::Database(error) => {
                if let Some(db) = error.as_database_error() {
                    if db.code().as_deref() == Some("23505") {
                        return (
                            StatusCode::CONFLICT,
                            Json(ErrorEnvelope {
                                error: ErrorBody {
                                    code: "conflict",
                                    message: "名称或 slug 已存在".to_owned(),
                                },
                            }),
                        )
                            .into_response();
                    }
                }
                error!(%error, "article database operation failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "文章操作失败".to_owned(),
                )
            }
            Self::Artalk(error) => {
                error!(%error, "article synchronization with Artalk failed");
                (
                    StatusCode::BAD_GATEWAY,
                    "artalk_unavailable",
                    "评论服务同步失败，文章操作未完成".to_owned(),
                )
            }
        };
        (
            status,
            Json(ErrorEnvelope {
                error: ErrorBody { code, message },
            }),
        )
            .into_response()
    }
}

fn default_page() -> i64 {
    1
}

fn default_per_page() -> i64 {
    10
}

fn page_values(page: i64, per_page: i64) -> Result<(i64, i64, i64), ArticleError> {
    if page < 1 || !(1..=50).contains(&per_page) {
        return Err(ArticleError::validation(
            "page 必须 >= 1，per_page 必须为 1..50",
        ));
    }
    Ok((page, per_page, (page - 1) * per_page))
}

#[derive(Debug, Deserialize)]
struct PublicListQuery {
    #[serde(default = "default_page")]
    page: i64,
    #[serde(default = "default_per_page")]
    per_page: i64,
    category: Option<String>,
    tag: Option<String>,
    group_by: Option<String>,
    search: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AdminListQuery {
    #[serde(default = "default_page")]
    page: i64,
    #[serde(default = "default_per_page")]
    per_page: i64,
    status: Option<String>,
    is_pinned: Option<bool>,
    sort: Option<String>,
    search: Option<String>,
}

#[derive(Debug, Serialize, FromRow, Clone)]
struct CategorySummary {
    id: i64,
    name: String,
    slug: String,
    color: String,
    sort_order: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    article_count: Option<i64>,
}

#[derive(Debug, Serialize, FromRow, Clone)]
struct TagSummary {
    id: i64,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    article_count: Option<i64>,
}

#[derive(Debug, FromRow)]
struct ArticleTagRow {
    article_id: i64,
    id: i64,
    name: String,
}

#[derive(Debug, Serialize, FromRow, Clone)]
struct ArticleRow {
    id: i64,
    slug: String,
    title: String,
    summary: String,
    content_md: String,
    status: String,
    is_pinned: bool,
    allow_comment: bool,
    kanban_ref: bool,
    word_count: i32,
    read_minutes: i32,
    view_count: i64,
    published_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    category_id: Option<i64>,
    category_name: Option<String>,
    category_slug: Option<String>,
    category_color: Option<String>,
    cover_url: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
struct ArticleItem {
    id: i64,
    slug: String,
    title: String,
    summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_md: Option<String>,
    status: String,
    is_pinned: bool,
    allow_comment: bool,
    kanban_ref: bool,
    word_count: i32,
    read_minutes: i32,
    view_count: i64,
    published_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    category: Option<CategoryRef>,
    cover_url: Option<String>,
    tags: Vec<TagSummary>,
}

#[derive(Debug, Serialize, FromRow)]
struct RelatedArticleItem {
    id: i64,
    slug: String,
    title: String,
}

#[derive(Debug, Serialize, Clone)]
struct CategoryRef {
    id: i64,
    name: String,
    slug: String,
    color: String,
}

fn article_item(row: ArticleRow, tags: Vec<TagSummary>, include_content: bool) -> ArticleItem {
    ArticleItem {
        id: row.id,
        slug: row.slug,
        title: row.title,
        summary: row.summary,
        content_md: include_content.then_some(row.content_md),
        status: row.status,
        is_pinned: row.is_pinned,
        allow_comment: row.allow_comment,
        kanban_ref: row.kanban_ref,
        word_count: row.word_count,
        read_minutes: row.read_minutes,
        view_count: row.view_count,
        published_at: row.published_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
        category: row
            .category_id
            .zip(row.category_name)
            .zip(row.category_slug)
            .zip(row.category_color)
            .map(|(((id, name), slug), color)| CategoryRef {
                id,
                name,
                slug,
                color,
            }),
        cover_url: row.cover_url,
        tags,
    }
}

async fn tags_for_article(
    pool: &sqlx::PgPool,
    article_id: i64,
) -> Result<Vec<TagSummary>, ArticleError> {
    Ok(sqlx::query_as::<_, TagSummary>(
        "SELECT t.id, t.name, NULL::BIGINT AS article_count
         FROM tags t JOIN article_tags at ON at.tag_id = t.id
         WHERE at.article_id = $1 ORDER BY t.name",
    )
    .bind(article_id)
    .fetch_all(pool)
    .await?)
}

async fn tags_for_articles(
    pool: &sqlx::PgPool,
    article_ids: &[i64],
) -> Result<HashMap<i64, Vec<TagSummary>>, ArticleError> {
    if article_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query_as::<_, ArticleTagRow>(
        "SELECT at.article_id, t.id, t.name
         FROM article_tags at JOIN tags t ON t.id = at.tag_id
         WHERE at.article_id = ANY($1)
         ORDER BY at.article_id, t.name",
    )
    .bind(article_ids)
    .fetch_all(pool)
    .await?;
    let mut tags = HashMap::<i64, Vec<TagSummary>>::new();
    for row in rows {
        tags.entry(row.article_id).or_default().push(TagSummary {
            id: row.id,
            name: row.name,
            article_count: None,
        });
    }
    Ok(tags)
}

async fn fetch_article(
    pool: &sqlx::PgPool,
    condition: &str,
    bind: &str,
    include_unpublished: bool,
) -> Result<Option<ArticleRow>, ArticleError> {
    let status_clause = if include_unpublished {
        ""
    } else {
        " AND a.status = 'published'"
    };
    let sql = format!(
        "SELECT a.id, a.slug, a.title, a.summary, a.content_md, a.status, a.is_pinned,
                a.allow_comment, a.kanban_ref, a.word_count, a.read_minutes, a.view_count,
                a.published_at, a.created_at, a.updated_at, c.id AS category_id,
                c.name AS category_name, c.slug AS category_slug, c.color AS category_color,
                CASE WHEN asset.id IS NULL THEN NULL ELSE '/storage/' || upload.object_key END AS cover_url
         FROM articles a
         LEFT JOIN categories c ON c.id = a.category_id
         LEFT JOIN assets asset ON asset.id = a.cover_asset_id AND asset.status = 'active'
         LEFT JOIN uploads upload ON upload.id = asset.upload_id
         WHERE {condition}{status_clause}",
    );
    Ok(sqlx::query_as::<_, ArticleRow>(&sql)
        .bind(bind)
        .fetch_optional(pool)
        .await?)
}

#[allow(clippy::too_many_arguments)]
async fn list_articles(
    pool: &sqlx::PgPool,
    offset: i64,
    per_page: i64,
    status: Option<&str>,
    pinned: Option<bool>,
    search: Option<&str>,
    category: Option<&str>,
    tag: Option<&str>,
    sort: Option<&str>,
) -> Result<(Vec<ArticleItem>, i64), ArticleError> {
    let mut filters = vec!["1=1".to_owned()];
    if status.is_some() {
        filters.push("a.status = $1".to_owned());
    }
    if pinned.is_some() {
        filters.push(format!(
            "a.is_pinned = ${}",
            if status.is_some() { 2 } else { 1 }
        ));
    }
    let mut next = 1 + i64::from(status.is_some()) + i64::from(pinned.is_some());
    if search.is_some() {
        filters.push(format!(
            "(a.title ILIKE ${0} OR a.summary ILIKE ${0} OR a.content_md ILIKE ${0})",
            next
        ));
        next += 1;
    }
    if category.is_some() {
        filters.push(format!("c.slug = ${}", next));
        next += 1;
    }
    if tag.is_some() {
        filters.push(format!(
            "EXISTS (SELECT 1 FROM article_tags filter_at JOIN tags filter_t ON filter_t.id = filter_at.tag_id
             WHERE filter_at.article_id = a.id
               AND (filter_t.name = ${0} OR regexp_replace(lower(filter_t.name), '\\s+', '-', 'g') = lower(${0})))",
            next
        ));
        next += 1;
    }
    let order = match sort {
        Some("published_at") => "a.is_pinned DESC, a.published_at DESC NULLS LAST, a.id DESC",
        Some("created_at") => "a.created_at DESC, a.id DESC",
        Some("updated_at") => "a.updated_at DESC, a.id DESC",
        Some(_) => "a.updated_at DESC, a.id DESC",
        // 后台文章管理默认将草稿置顶，其次是手动置顶文章，最后按文章日期倒序。
        // 草稿通常没有 published_at，因此以 updated_at / created_at 作为日期回退。
        None => {
            "CASE WHEN a.status = 'draft' THEN 0 ELSE 1 END ASC, a.is_pinned DESC, COALESCE(a.published_at, a.updated_at, a.created_at) DESC, a.id DESC"
        }
    };
    let sql = format!(
        "SELECT a.id, a.slug, a.title, a.summary, a.content_md, a.status, a.is_pinned,
                a.allow_comment, a.kanban_ref, a.word_count, a.read_minutes, a.view_count,
                a.published_at, a.created_at, a.updated_at, c.id AS category_id,
                c.name AS category_name, c.slug AS category_slug, c.color AS category_color,
                CASE WHEN asset.id IS NULL THEN NULL ELSE '/storage/' || upload.object_key END AS cover_url
         FROM articles a
         LEFT JOIN categories c ON c.id = a.category_id
         LEFT JOIN assets asset ON asset.id = a.cover_asset_id AND asset.status = 'active'
         LEFT JOIN uploads upload ON upload.id = asset.upload_id
         WHERE {}
         ORDER BY {}
         LIMIT ${} OFFSET ${}",
        filters.join(" AND "),
        order,
        next,
        next + 1
    );
    let count_sql = format!(
        "SELECT COUNT(*) FROM articles a LEFT JOIN categories c ON c.id = a.category_id WHERE {}",
        filters.join(" AND ")
    );
    let mut rows = sqlx::query_as::<_, ArticleRow>(&sql);
    let mut count = sqlx::query_scalar::<_, i64>(&count_sql);
    if let Some(value) = status {
        rows = rows.bind(value);
        count = count.bind(value);
    }
    if let Some(value) = pinned {
        rows = rows.bind(value);
        count = count.bind(value);
    }
    if let Some(value) = search {
        let value = format!("%{}%", value.trim());
        rows = rows.bind(value.clone());
        count = count.bind(value);
    }
    if let Some(value) = category {
        rows = rows.bind(value);
        count = count.bind(value);
    }
    if let Some(value) = tag {
        rows = rows.bind(value);
        count = count.bind(value);
    }
    let rows = rows.bind(per_page).bind(offset).fetch_all(pool).await?;
    let total = count.fetch_one(pool).await?;
    let article_ids = rows.iter().map(|row| row.id).collect::<Vec<_>>();
    let mut tags_by_article = tags_for_articles(pool, &article_ids).await?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let tags = tags_by_article.remove(&row.id).unwrap_or_default();
        items.push(article_item(row, tags, false));
    }
    Ok((items, total))
}

async fn public_list(
    State(state): State<AppState>,
    Query(query): Query<PublicListQuery>,
) -> Result<Json<Value>, ArticleError> {
    let (page, per_page, offset) = page_values(query.page, query.per_page)?;
    if query
        .group_by
        .as_deref()
        .is_some_and(|value| value != "year")
    {
        return Err(ArticleError::validation("group_by 只支持 year"));
    }
    let (items, total) = list_articles(
        state.pool(),
        offset,
        per_page,
        Some("published"),
        None,
        query.search.as_deref(),
        query.category.as_deref(),
        query.tag.as_deref(),
        Some("published_at"),
    )
    .await?;
    if query.group_by.as_deref() == Some("year") {
        let mut groups: HashMap<i32, Vec<ArticleItem>> = HashMap::new();
        for item in items {
            let year = item
                .published_at
                .map(|date| date.year())
                .unwrap_or_else(|| item.created_at.year());
            groups.entry(year).or_default().push(item);
        }
        let mut grouped = groups
            .into_iter()
            .map(|(year, articles)| serde_json::json!({ "year": year, "count": articles.len(), "items": articles }))
            .collect::<Vec<_>>();
        grouped.sort_by(|left, right| right["year"].as_i64().cmp(&left["year"].as_i64()));
        return Ok(Json(serde_json::json!({
            "page": page, "per_page": per_page, "total": total, "items": grouped
        })));
    }
    Ok(Json(serde_json::json!({
        "page": page, "per_page": per_page, "total": total, "items": items
    })))
}

async fn public_detail(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, ArticleError> {
    let slug = validate_slug(&slug)?;
    let article = fetch_article(state.pool(), "a.slug = $1", &slug, false)
        .await?
        .ok_or_else(|| ArticleError::NotFound("文章不存在".to_owned()))?;
    sqlx::query("UPDATE articles SET view_count = view_count + 1 WHERE id = $1")
        .bind(article.id)
        .execute(state.pool())
        .await?;
    let tags = tags_for_article(state.pool(), article.id).await?;
    let mut item = article_item(article.clone(), tags.clone(), true);
    item.view_count += 1;
    let previous = sqlx::query_as::<_, ArticleRow>(
        "SELECT a.id, a.slug, a.title, a.summary, a.content_md, a.status, a.is_pinned,
                a.allow_comment, a.kanban_ref, a.word_count, a.read_minutes, a.view_count,
                a.published_at, a.created_at, a.updated_at, c.id AS category_id,
                c.name AS category_name, c.slug AS category_slug, c.color AS category_color,
                NULL::TEXT AS cover_url
         FROM articles a LEFT JOIN categories c ON c.id = a.category_id
         WHERE a.status='published' AND (a.published_at, a.id) < (
             SELECT published_at, id FROM articles WHERE id = $1)
         ORDER BY a.published_at DESC, a.id DESC LIMIT 1",
    )
    .bind(article.id)
    .fetch_optional(state.pool())
    .await?
    .map(|row| article_item(row, Vec::new(), false));
    let next = sqlx::query_as::<_, ArticleRow>(
        "SELECT a.id, a.slug, a.title, a.summary, a.content_md, a.status, a.is_pinned,
                a.allow_comment, a.kanban_ref, a.word_count, a.read_minutes, a.view_count,
                a.published_at, a.created_at, a.updated_at, c.id AS category_id,
                c.name AS category_name, c.slug AS category_slug, c.color AS category_color,
                NULL::TEXT AS cover_url
         FROM articles a LEFT JOIN categories c ON c.id = a.category_id
         WHERE a.status='published' AND (a.published_at, a.id) > (
             SELECT published_at, id FROM articles WHERE id = $1)
         ORDER BY a.published_at ASC, a.id ASC LIMIT 1",
    )
    .bind(article.id)
    .fetch_optional(state.pool())
    .await?
    .map(|row| article_item(row, Vec::new(), false));
    let related_rows = sqlx::query_as::<_, RelatedArticleItem>(
        "SELECT a.id, a.slug, a.title
         FROM articles a
         WHERE a.status='published' AND a.id <> $1
           AND (a.category_id = $2 OR EXISTS (
             SELECT 1
             FROM article_tags related_tag
             WHERE related_tag.article_id = a.id
               AND related_tag.tag_id IN (
                 SELECT article_tags.tag_id
                 FROM article_tags
                 WHERE article_tags.article_id = $1
               )
           ))
         ORDER BY a.published_at DESC NULLS LAST, a.id DESC LIMIT 4",
    )
    .bind(article.id)
    .bind(article.category_id)
    .fetch_all(state.pool())
    .await?;
    Ok(Json(serde_json::json!({
        "article": item,
        "previous": previous,
        "next": next,
        "related": related_rows,
        "allow_comment": article.allow_comment
    })))
}

async fn public_categories(State(state): State<AppState>) -> Result<Json<Value>, ArticleError> {
    let items = sqlx::query_as::<_, CategorySummary>(
        "SELECT c.id, c.name, c.slug, c.color, c.sort_order,
                COUNT(a.id) FILTER (WHERE a.status='published') AS article_count
         FROM categories c LEFT JOIN articles a ON a.category_id = c.id
         GROUP BY c.id ORDER BY c.sort_order, c.id",
    )
    .fetch_all(state.pool())
    .await?;
    Ok(Json(serde_json::json!({ "items": items })))
}

async fn public_tags(State(state): State<AppState>) -> Result<Json<Value>, ArticleError> {
    let items = sqlx::query_as::<_, TagSummary>(
        "SELECT t.id, t.name, COUNT(at.article_id) FILTER (WHERE a.status='published') AS article_count
         FROM tags t LEFT JOIN article_tags at ON at.tag_id=t.id
         LEFT JOIN articles a ON a.id=at.article_id
         GROUP BY t.id ORDER BY article_count DESC, t.name",
    )
    .fetch_all(state.pool())
    .await?;
    Ok(Json(serde_json::json!({ "items": items })))
}

async fn admin_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AdminListQuery>,
) -> Result<Json<Value>, ArticleError> {
    require_admin(&state, &headers)?;
    if let Some(status) = query.status.as_deref() {
        validate_status(status)?;
    }
    if query
        .sort
        .as_deref()
        .is_some_and(|sort| !matches!(sort, "updated_at" | "created_at" | "published_at"))
    {
        return Err(ArticleError::validation("sort 无效"));
    }
    let (page, per_page, offset) = page_values(query.page, query.per_page)?;
    let (items, total) = list_articles(
        state.pool(),
        offset,
        per_page,
        query.status.as_deref(),
        query.is_pinned,
        query.search.as_deref(),
        None,
        None,
        query.sort.as_deref(),
    )
    .await?;
    Ok(Json(serde_json::json!({
        "page": page, "per_page": per_page, "total": total, "items": items
    })))
}

#[derive(Debug, Deserialize)]
struct CreateArticleRequest {
    title: Option<String>,
}

async fn admin_create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateArticleRequest>,
) -> Result<(StatusCode, Json<Value>), ArticleError> {
    require_admin(&state, &headers)?;
    let title = normalize_title(request.title.as_deref())?;
    let temporary_slug = format!("draft-{}", Uuid::now_v7());
    let mut tx = state.pool().begin().await?;
    let id: i64 =
        sqlx::query_scalar("INSERT INTO articles (slug, title) VALUES ($1, $2) RETURNING id")
            .bind(&temporary_slug)
            .bind(&title)
            .fetch_one(&mut *tx)
            .await?;
    let slug = format!("p{id}");
    let updated_at = sqlx::query_scalar::<_, DateTime<Utc>>(
        "UPDATE articles SET slug = $1 WHERE id = $2 RETURNING updated_at",
    )
    .bind(&slug)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": id,
            "slug": slug,
            "status": "draft",
            "updated_at": updated_at
        })),
    ))
}

async fn admin_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ArticleError> {
    require_admin(&state, &headers)?;
    let article = fetch_article_by_id(state.pool(), id, true)
        .await?
        .ok_or_else(|| ArticleError::NotFound("文章不存在".to_owned()))?;
    let tag_ids = sqlx::query_scalar::<_, i64>(
        "SELECT tag_id FROM article_tags WHERE article_id=$1 ORDER BY tag_id",
    )
    .bind(id)
    .fetch_all(state.pool())
    .await?;
    let content_asset_ids = sqlx::query_scalar::<_, i64>(
        "SELECT asset_id FROM article_assets WHERE article_id=$1 AND role='content' ORDER BY sort_order, asset_id",
    )
    .bind(id)
    .fetch_all(state.pool())
    .await?;
    let item = article_item(
        article.clone(),
        tags_for_article(state.pool(), id).await?,
        true,
    );
    Ok(Json(serde_json::json!({
        "article": item,
        "title": article.title,
        "summary": article.summary,
        "content_md": article.content_md,
        "category_id": article.category_id,
        "tag_ids": tag_ids,
        "cover_asset_id": cover_asset_id(state.pool(), id).await?,
        "content_asset_ids": content_asset_ids,
        "is_pinned": article.is_pinned,
        "allow_comment": article.allow_comment,
        "kanban_ref": article.kanban_ref,
        "status": article.status,
        "slug": article.slug,
        "updated_at": article.updated_at
    })))
}

#[derive(Debug, Deserialize, Default)]
struct UpdateArticleRequest {
    expected_updated_at: Option<DateTime<Utc>>,
    title: Option<String>,
    summary: Option<String>,
    content_md: Option<String>,
    category_id: Option<Option<i64>>,
    tag_ids: Option<Vec<i64>>,
    cover_asset_id: Option<Option<i64>>,
    content_asset_ids: Option<Vec<i64>>,
    is_pinned: Option<bool>,
    allow_comment: Option<bool>,
    kanban_ref: Option<bool>,
    status: Option<String>,
}

async fn admin_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(request): Json<UpdateArticleRequest>,
) -> Result<Json<Value>, ArticleError> {
    require_admin(&state, &headers)?;
    let existing = fetch_article_by_id(state.pool(), id, true)
        .await?
        .ok_or_else(|| ArticleError::NotFound("文章不存在".to_owned()))?;
    let expected_updated_at = request.expected_updated_at.ok_or_else(|| {
        ArticleError::validation("expected_updated_at 不能为空，请刷新文章后重试")
    })?;
    let title = normalize_title(request.title.as_deref().or(Some(existing.title.as_str())))?;
    let content_md = request.content_md.unwrap_or(existing.content_md);
    let raw_summary = request.summary.unwrap_or(existing.summary);
    let summary = normalize_summary(&raw_summary, &content_md)?;
    let category_id = request.category_id.unwrap_or(existing.category_id);
    let tag_ids = request.tag_ids.unwrap_or_default();
    let cover_asset_id = request
        .cover_asset_id
        .unwrap_or(cover_asset_id(state.pool(), id).await?);
    let content_asset_ids = request.content_asset_ids.unwrap_or_default();
    let status = request.status.unwrap_or(existing.status);
    let allow_comment = request.allow_comment.unwrap_or(existing.allow_comment);
    validate_status(&status)?;
    if status == "published" {
        if title == "未命名草稿" {
            return Err(ArticleError::validation("发布文章必须填写标题"));
        }
        if content_md.trim().is_empty() {
            return Err(ArticleError::validation("发布文章必须填写正文"));
        }
        if category_id.is_none() {
            return Err(ArticleError::validation("发布文章必须选择分类"));
        }
    }
    validate_category(state.pool(), category_id).await?;
    validate_tags(state.pool(), &tag_ids).await?;
    validate_assets(state.pool(), cover_asset_id, &content_asset_ids).await?;

    let word_count = content_md.chars().filter(|ch| !ch.is_whitespace()).count() as i32;
    let read_minutes = if word_count == 0 {
        0
    } else {
        ((word_count as f32 / 450.0).ceil() as i32).max(1)
    };
    let mut tx = state.pool().begin().await?;
    let published_at = if status == "published" {
        Some(existing.published_at.unwrap_or_else(Utc::now))
    } else {
        existing.published_at
    };
    let result = sqlx::query(
        "UPDATE articles SET title=$1, summary=$2, content_md=$3, category_id=$4,
            cover_asset_id=$5, is_pinned=$6, allow_comment=$7, kanban_ref=$8,
            status=$9, word_count=$10, read_minutes=$11, published_at=$12
         WHERE id=$13 AND updated_at=$14",
    )
    .bind(&title)
    .bind(&summary)
    .bind(&content_md)
    .bind(category_id)
    .bind(cover_asset_id)
    .bind(request.is_pinned.unwrap_or(existing.is_pinned))
    .bind(allow_comment)
    .bind(request.kanban_ref.unwrap_or(existing.kanban_ref))
    .bind(&status)
    .bind(word_count)
    .bind(read_minutes)
    .bind(published_at)
    .bind(id)
    .bind(expected_updated_at)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        tx.rollback().await?;
        return Err(ArticleError::Conflict(
            "文章已在其他页面更新，请刷新后重试".to_owned(),
        ));
    }
    sync_article_relations(&mut tx, id, &tag_ids, &content_asset_ids).await?;
    if allow_comment != existing.allow_comment {
        state
            .artalk()
            .set_page_commenting(&article_page_key(&existing.slug), &title, allow_comment)
            .await?;
    }
    tx.commit().await?;
    let updated = fetch_article_by_id(state.pool(), id, true)
        .await?
        .ok_or_else(|| ArticleError::NotFound("文章不存在".to_owned()))?;
    Ok(Json(serde_json::json!({
        "id": updated.id,
        "slug": updated.slug,
        "status": updated.status,
        "word_count": updated.word_count,
        "read_minutes": updated.read_minutes,
        "updated_at": updated.updated_at
    })))
}

async fn admin_delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<StatusCode, ArticleError> {
    require_admin(&state, &headers)?;
    let mut tx = state.pool().begin().await?;
    let slug = sqlx::query_scalar::<_, String>("SELECT slug FROM articles WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| ArticleError::NotFound("文章不存在".to_owned()))?;
    let result = sqlx::query("DELETE FROM articles WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    debug_assert_eq!(result.rows_affected(), 1);
    let page_key = article_page_key(&slug);
    state.artalk().delete_pages([page_key.as_str()]).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct BatchArticleRequest {
    article_ids: Vec<i64>,
    action: String,
}

async fn admin_batch(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<BatchArticleRequest>,
) -> Result<Json<Value>, ArticleError> {
    require_admin(&state, &headers)?;
    let ids = unique_ids(&request.article_ids)?;
    if !matches!(
        request.action.as_str(),
        "publish" | "unpublish" | "delete" | "pin" | "unpin"
    ) {
        return Err(ArticleError::validation("action 无效"));
    }
    let mut tx = state.pool().begin().await?;
    let mut affected = 0_i64;
    let mut failed_ids = Vec::new();
    let mut deleted_page_keys = Vec::new();
    for id in ids {
        let article = sqlx::query_as::<_, (String, String, String, Option<i64>, String)>(
            "SELECT status, title, content_md, category_id, slug FROM articles WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((status, title, content_md, category_id, slug)) = article else {
            failed_ids.push(id);
            continue;
        };
        let result = match request.action.as_str() {
            "publish" => {
                if title.trim().is_empty()
                    || title == "未命名草稿"
                    || content_md.trim().is_empty()
                    || category_id.is_none()
                {
                    failed_ids.push(id);
                    continue;
                }
                sqlx::query("UPDATE articles SET status='published', published_at=COALESCE(published_at, now()) WHERE id=$1")
                    .bind(id).execute(&mut *tx).await?
            }
            "unpublish" => {
                sqlx::query("UPDATE articles SET status='draft' WHERE id=$1")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?
            }
            "pin" => {
                sqlx::query("UPDATE articles SET is_pinned=true WHERE id=$1")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?
            }
            "unpin" => {
                sqlx::query("UPDATE articles SET is_pinned=false WHERE id=$1")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?
            }
            "delete" => {
                deleted_page_keys.push(article_page_key(&slug));
                sqlx::query("DELETE FROM articles WHERE id=$1")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?
            }
            _ => unreachable!(),
        };
        if result.rows_affected() == 0 || (request.action == "unpublish" && status == "draft") {
            failed_ids.push(id);
        } else {
            affected += result.rows_affected() as i64;
        }
    }
    state
        .artalk()
        .delete_pages(deleted_page_keys.iter().map(String::as_str))
        .await?;
    tx.commit().await?;
    Ok(Json(
        serde_json::json!({ "affected": affected, "failed_ids": failed_ids }),
    ))
}

async fn fetch_article_by_id(
    pool: &sqlx::PgPool,
    id: i64,
    include_unpublished: bool,
) -> Result<Option<ArticleRow>, ArticleError> {
    fetch_article(
        pool,
        "a.id::TEXT = $1",
        &id.to_string(),
        include_unpublished,
    )
    .await
}

async fn cover_asset_id(pool: &sqlx::PgPool, id: i64) -> Result<Option<i64>, ArticleError> {
    Ok(
        sqlx::query_scalar("SELECT cover_asset_id FROM articles WHERE id=$1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .flatten(),
    )
}

fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ArticleError> {
    if auth::has_valid_admin_session(state, headers) {
        Ok(())
    } else {
        Err(ArticleError::Unauthorized)
    }
}

fn normalize_title(title: Option<&str>) -> Result<String, ArticleError> {
    let title = title.unwrap_or("").trim();
    if title.is_empty() {
        return Ok("未命名草稿".to_owned());
    }
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(ArticleError::validation(format!(
            "文章标题不能超过 {MAX_TITLE_CHARS} 个字符"
        )));
    }
    Ok(title.to_owned())
}

fn normalize_summary(summary: &str, content_md: &str) -> Result<String, ArticleError> {
    let summary = summary.trim();
    if !summary.is_empty() && summary.chars().count() > MAX_SUMMARY_CHARS {
        return Err(ArticleError::validation(format!(
            "文章摘要不能超过 {MAX_SUMMARY_CHARS} 个字符"
        )));
    }
    let source = if summary.is_empty() {
        content_md
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('>'))
            .unwrap_or("")
    } else {
        summary
    };
    Ok(source.chars().take(MAX_SUMMARY_CHARS).collect())
}

fn validate_slug(slug: &str) -> Result<String, ArticleError> {
    let value = slug.trim();
    if value.is_empty()
        || value.len() > 120
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "-_".contains(ch))
    {
        return Err(ArticleError::validation("slug 格式无效"));
    }
    Ok(value.to_owned())
}

fn validate_status(status: &str) -> Result<(), ArticleError> {
    if matches!(status, "draft" | "published" | "hidden") {
        Ok(())
    } else {
        Err(ArticleError::validation(
            "status 必须为 draft、published 或 hidden",
        ))
    }
}

async fn validate_category(
    pool: &sqlx::PgPool,
    category_id: Option<i64>,
) -> Result<(), ArticleError> {
    if let Some(id) = category_id {
        let exists =
            sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM categories WHERE id=$1)")
                .bind(id)
                .fetch_one(pool)
                .await?;
        if !exists {
            return Err(ArticleError::validation("分类不存在"));
        }
    }
    Ok(())
}

async fn validate_tags(pool: &sqlx::PgPool, tag_ids: &[i64]) -> Result<(), ArticleError> {
    if tag_ids.iter().any(|id| *id <= 0) || tag_ids.len() > 50 {
        return Err(ArticleError::validation("tag_ids 无效"));
    }
    let unique = tag_ids.iter().copied().collect::<HashSet<_>>();
    if unique.len() != tag_ids.len() {
        return Err(ArticleError::validation("tag_ids 不能重复"));
    }
    if tag_ids.is_empty() {
        return Ok(());
    }
    let found = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tags WHERE id = ANY($1)")
        .bind(tag_ids)
        .fetch_one(pool)
        .await?;
    if found != tag_ids.len() as i64 {
        return Err(ArticleError::validation("存在不存在的标签"));
    }
    Ok(())
}

async fn validate_assets(
    pool: &sqlx::PgPool,
    cover_asset_id: Option<i64>,
    content_asset_ids: &[i64],
) -> Result<(), ArticleError> {
    if let Some(id) = cover_asset_id {
        let valid = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM assets WHERE id=$1 AND status='active' AND media_type='image')",
        )
        .bind(id)
        .fetch_one(pool)
        .await?;
        if !valid {
            return Err(ArticleError::validation("文章封面必须是可用图片素材"));
        }
    }
    let unique = content_asset_ids.iter().copied().collect::<HashSet<_>>();
    if unique.len() != content_asset_ids.len() || content_asset_ids.len() > 100 {
        return Err(ArticleError::validation("正文素材引用无效"));
    }
    if content_asset_ids.is_empty() {
        return Ok(());
    }
    let found = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM assets WHERE id = ANY($1) AND status='active' AND media_type IN ('image','audio','video')",
    )
    .bind(content_asset_ids)
    .fetch_one(pool)
    .await?;
    if found != content_asset_ids.len() as i64 {
        return Err(ArticleError::validation(
            "正文素材必须是可用图片、音频或视频",
        ));
    }
    Ok(())
}

async fn sync_article_relations(
    tx: &mut Transaction<'_, Postgres>,
    article_id: i64,
    tag_ids: &[i64],
    content_asset_ids: &[i64],
) -> Result<(), ArticleError> {
    sqlx::query("DELETE FROM article_tags WHERE article_id=$1")
        .bind(article_id)
        .execute(&mut **tx)
        .await?;
    for tag_id in tag_ids {
        sqlx::query("INSERT INTO article_tags (article_id, tag_id) VALUES ($1, $2)")
            .bind(article_id)
            .bind(tag_id)
            .execute(&mut **tx)
            .await?;
    }
    sqlx::query("DELETE FROM article_assets WHERE article_id=$1 AND role='content'")
        .bind(article_id)
        .execute(&mut **tx)
        .await?;
    for (index, asset_id) in content_asset_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO article_assets (article_id, asset_id, role, sort_order) VALUES ($1, $2, 'content', $3)",
        )
        .bind(article_id)
        .bind(asset_id)
        .bind(index as i32)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

fn unique_ids(ids: &[i64]) -> Result<Vec<i64>, ArticleError> {
    if ids.is_empty() || ids.len() > MAX_BATCH || ids.iter().any(|id| *id <= 0) {
        return Err(ArticleError::validation(
            "article_ids 必须为 1..100 个正整数",
        ));
    }
    let mut seen = HashSet::with_capacity(ids.len());
    Ok(ids.iter().copied().filter(|id| seen.insert(*id)).collect())
}

#[derive(Debug, Deserialize)]
struct CategoryRequest {
    name: Option<String>,
    slug: Option<String>,
    color: Option<String>,
    sort_order: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct TagRequest {
    name: String,
}

async fn admin_categories(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ArticleError> {
    require_admin(&state, &headers)?;
    let items = sqlx::query_as::<_, ValueCategory>(
        "SELECT c.id, c.name, c.slug, c.color, c.sort_order,
                COUNT(a.id) FILTER (WHERE a.status='published') AS published_count,
                COUNT(a.id) FILTER (WHERE a.status <> 'published') AS draft_count
         FROM categories c LEFT JOIN articles a ON a.category_id=c.id
         GROUP BY c.id ORDER BY c.sort_order, c.id",
    )
    .fetch_all(state.pool())
    .await?;
    Ok(Json(serde_json::json!({ "items": items })))
}

#[derive(Debug, Serialize, FromRow)]
struct ValueCategory {
    id: i64,
    name: String,
    slug: String,
    color: String,
    sort_order: i32,
    published_count: i64,
    draft_count: i64,
}

async fn admin_create_category(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CategoryRequest>,
) -> Result<(StatusCode, Json<Value>), ArticleError> {
    require_admin(&state, &headers)?;
    let name = required_text(request.name, "分类名称")?;
    let slug = normalize_slug(request.slug.as_deref().unwrap_or(&name))?;
    let color = normalize_color(request.color.as_deref().unwrap_or(""))?;
    let item = sqlx::query_as::<_, ValueCategory>(
        "INSERT INTO categories(name,slug,color,sort_order) VALUES($1,$2,$3,$4)
         RETURNING id,name,slug,color,sort_order,0::BIGINT AS published_count,0::BIGINT AS draft_count",
    )
    .bind(name)
    .bind(slug)
    .bind(color)
    .bind(request.sort_order.unwrap_or(0))
    .fetch_one(state.pool())
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(item).unwrap()),
    ))
}

async fn admin_update_category(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(request): Json<CategoryRequest>,
) -> Result<Json<Value>, ArticleError> {
    require_admin(&state, &headers)?;
    if request.name.is_none()
        && request.slug.is_none()
        && request.color.is_none()
        && request.sort_order.is_none()
    {
        return Err(ArticleError::validation("至少提交一个分类字段"));
    }
    let current = sqlx::query_as::<_, (String, String, String, i32)>(
        "SELECT name,slug,color,sort_order FROM categories WHERE id=$1",
    )
    .bind(id)
    .fetch_optional(state.pool())
    .await?
    .ok_or_else(|| ArticleError::NotFound("分类不存在".to_owned()))?;
    let item = sqlx::query_as::<_, ValueCategory>(
        "UPDATE categories SET name=$1,slug=$2,color=$3,sort_order=$4 WHERE id=$5
         RETURNING id,name,slug,color,sort_order,
           (SELECT COUNT(*) FROM articles WHERE category_id=categories.id AND status='published') AS published_count,
           (SELECT COUNT(*) FROM articles WHERE category_id=categories.id AND status <> 'published') AS draft_count",
    )
    .bind(request.name.map(|v| required_text(Some(v), "分类名称")).transpose()?.unwrap_or(current.0))
    .bind(request.slug.map(|v| normalize_slug(&v)).transpose()?.unwrap_or(current.1))
    .bind(request.color.map(|v| normalize_color(&v)).transpose()?.unwrap_or(current.2))
    .bind(request.sort_order.unwrap_or(current.3))
    .bind(id)
    .fetch_one(state.pool())
    .await?;
    Ok(Json(serde_json::to_value(item).unwrap()))
}

async fn admin_delete_category(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<StatusCode, ArticleError> {
    require_admin(&state, &headers)?;
    let used = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM articles WHERE category_id=$1")
        .bind(id)
        .fetch_one(state.pool())
        .await?;
    if used > 0 {
        return Err(ArticleError::Conflict("分类仍被文章引用".to_owned()));
    }
    let result = sqlx::query("DELETE FROM categories WHERE id=$1")
        .bind(id)
        .execute(state.pool())
        .await?;
    if result.rows_affected() == 0 {
        return Err(ArticleError::NotFound("分类不存在".to_owned()));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn admin_tags(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ArticleError> {
    require_admin(&state, &headers)?;
    let items = sqlx::query_as::<_, ValueTag>(
        "SELECT t.id,t.name,
         COUNT(a.id) FILTER (WHERE a.status='published') AS published_count,
         COUNT(a.id) FILTER (WHERE a.status <> 'published') AS draft_count
         FROM tags t LEFT JOIN article_tags at ON at.tag_id=t.id
         LEFT JOIN articles a ON a.id=at.article_id
         GROUP BY t.id ORDER BY (COUNT(a.id)) DESC,t.name",
    )
    .fetch_all(state.pool())
    .await?;
    Ok(Json(serde_json::json!({ "items": items })))
}

#[derive(Debug, Serialize, FromRow)]
struct ValueTag {
    id: i64,
    name: String,
    published_count: i64,
    draft_count: i64,
}

async fn admin_create_tag(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<TagRequest>,
) -> Result<(StatusCode, Json<Value>), ArticleError> {
    require_admin(&state, &headers)?;
    let name = required_text(Some(request.name), "标签名称")?;
    let item = sqlx::query_as::<_, ValueTag>(
        "INSERT INTO tags(name) VALUES($1) RETURNING id,name,0::BIGINT AS published_count,0::BIGINT AS draft_count",
    ).bind(name).fetch_one(state.pool()).await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(item).unwrap()),
    ))
}

async fn admin_update_tag(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(request): Json<TagRequest>,
) -> Result<Json<Value>, ArticleError> {
    require_admin(&state, &headers)?;
    let name = required_text(Some(request.name), "标签名称")?;
    let item = sqlx::query_as::<_, ValueTag>(
        "UPDATE tags SET name=$1 WHERE id=$2 RETURNING id,name,
         (SELECT COUNT(*) FROM article_tags at JOIN articles a ON a.id=at.article_id WHERE at.tag_id=tags.id AND a.status='published') AS published_count,
         (SELECT COUNT(*) FROM article_tags at JOIN articles a ON a.id=at.article_id WHERE at.tag_id=tags.id AND a.status <> 'published') AS draft_count",
    ).bind(name).bind(id).fetch_optional(state.pool()).await?
        .ok_or_else(|| ArticleError::NotFound("标签不存在".to_owned()))?;
    Ok(Json(serde_json::to_value(item).unwrap()))
}

async fn admin_delete_tag(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<StatusCode, ArticleError> {
    require_admin(&state, &headers)?;
    let result = sqlx::query("DELETE FROM tags WHERE id=$1")
        .bind(id)
        .execute(state.pool())
        .await?;
    if result.rows_affected() == 0 {
        return Err(ArticleError::NotFound("标签不存在".to_owned()));
    }
    Ok(StatusCode::NO_CONTENT)
}

fn required_text(value: Option<String>, label: &str) -> Result<String, ArticleError> {
    let value = value.unwrap_or_default().trim().to_owned();
    if value.is_empty() {
        return Err(ArticleError::validation(format!("{label}不能为空")));
    }
    Ok(value)
}

fn normalize_slug(value: &str) -> Result<String, ArticleError> {
    let normalized = value.trim().to_ascii_lowercase().replace(' ', "-");
    if normalized.is_empty()
        || normalized.len() > 80
        || !normalized
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(ArticleError::validation(
            "slug 只能包含字母、数字、短横线或下划线",
        ));
    }
    Ok(normalized)
}

fn normalize_color(value: &str) -> Result<String, ArticleError> {
    let value = value.trim();
    if value.is_empty()
        || (value.len() == 7
            && value.starts_with('#')
            && value[1..].chars().all(|ch| ch.is_ascii_hexdigit()))
    {
        Ok(value.to_owned())
    } else {
        Err(ArticleError::validation("颜色必须为空或 #RRGGBB"))
    }
}

use chrono::Datelike;

#[cfg(test)]
mod tests {
    use super::{normalize_summary, normalize_title, unique_ids};

    #[test]
    fn batch_ids_are_deduplicated_without_losing_request_order() {
        assert_eq!(unique_ids(&[7, 2, 7, 4, 2]).expect("valid ids"), [7, 2, 4]);
    }

    #[test]
    fn batch_ids_reject_empty_non_positive_and_oversized_requests() {
        assert!(unique_ids(&[]).is_err());
        assert!(unique_ids(&[1, 0]).is_err());
        assert!(unique_ids(&(1..=101).collect::<Vec<_>>()).is_err());
    }

    #[test]
    fn explicit_article_text_is_rejected_instead_of_silently_truncated() {
        assert!(normalize_title(Some(&"标".repeat(201))).is_err());
        assert!(normalize_summary(&"摘".repeat(281), "正文").is_err());
    }

    #[test]
    fn generated_summary_is_bounded_without_rejecting_a_long_first_paragraph() {
        let summary = normalize_summary("", &"正".repeat(400)).expect("generated summary");
        assert_eq!(summary.chars().count(), 280);
    }
}
