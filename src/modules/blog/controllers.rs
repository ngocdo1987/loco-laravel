use loco_rs::prelude::*;

use axum::extract::Query;
use sea_orm::{Condition, DbErr, PaginatorTrait, QueryOrder, QuerySelect, QueryTrait};
use serde::{Deserialize, Serialize};

use super::models::{
    comment, post,
    responses::{CommentResponse, PostResponse},
};
use crate::models::_entities::{categories, comments, post_tags, posts, tags, users};

#[derive(Default, Deserialize)]
pub(super) struct ListQuery {
    pub(super) page: Option<String>,
    pub(super) limit: Option<String>,
    pub(super) author_id: Option<String>,
    pub(super) q: Option<String>,
    pub(super) category: Option<String>,
    pub(super) tag: Option<String>,
    pub(super) status: Option<String>,
}

#[derive(Serialize)]
pub(super) struct Paginated<T> {
    pub(super) data: Vec<T>,
    pub(super) page: u64,
    pub(super) limit: u64,
    pub(super) total: u64,
}

// Goravel's QueryInt uses the default for absent/empty values and zero
// when a nonempty value cannot be converted to an integer.
fn query_integer(value: Option<&str>, default: i64) -> i64 {
    value
        .filter(|value| !value.is_empty())
        .map_or(default, |value| value.parse().unwrap_or(0))
}

impl ListQuery {
    pub(super) fn pagination(
        &self,
    ) -> std::result::Result<(u64, u64), (&'static str, &'static str)> {
        let page = query_integer(self.page.as_deref(), 1);
        let limit = query_integer(self.limit.as_deref(), 20);
        if page < 1
            || !(1..=100).contains(&limit)
            || (page.unsigned_abs() - 1)
                .checked_mul(limit.unsigned_abs())
                .is_none()
        {
            return Err((
                "pagination",
                "page must be positive and limit must be between 1 and 100",
            ));
        }
        if query_integer(self.author_id.as_deref(), 0) < 0 {
            return Err(("author_id", "must be a positive identifier"));
        }
        Ok((page.unsigned_abs(), limit.unsigned_abs()))
    }
}

pub(super) fn validation(field: &str, message: &str) -> Response {
    (
        axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        Json(serde_json::json!({
            "message": "Validation failed",
            "errors": {field: {"validation": format!("{field} {message}")}}
        })),
    )
        .into_response()
}

pub(super) async fn published(db: &DatabaseConnection, slug: &str) -> Result<Option<posts::Model>> {
    Ok(posts::Entity::find()
        .filter(posts::Column::Slug.eq(slug))
        .filter(posts::Column::Status.eq(post::STATUS_PUBLISHED))
        .one(db)
        .await?)
}

pub(super) fn missing() -> Result<Response> {
    format::render()
        .status(axum::http::StatusCode::NOT_FOUND)
        .json(serde_json::json!({"message": "Resource not found"}))
}

/// Published-post feed shared by the public API (`/api/posts`) and the web
/// pages: q/category/tag/author filters + pagination, sanitized responses.
/// Returns the page rows and the total count.
pub(super) async fn feed(
    db: &DatabaseConnection,
    query: &ListQuery,
    page: u64,
    limit: u64,
) -> Result<(Vec<PostResponse>, u64)> {
    let mut select = posts::Entity::find().filter(posts::Column::Status.eq(post::STATUS_PUBLISHED));
    if let Some(term) = query.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let term = format!("%{term}%");
        select = select.filter(
            Condition::any()
                .add(posts::Column::Title.like(&term))
                .add(posts::Column::Excerpt.like(&term))
                .add(posts::Column::ContentHtml.like(&term)),
        );
    }
    if let Some(slug) = query
        .category
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        select = select.filter(
            posts::Column::CategoryId.in_subquery(
                categories::Entity::find()
                    .select_only()
                    .column(categories::Column::Id)
                    .filter(categories::Column::Slug.eq(slug))
                    .into_query(),
            ),
        );
    }
    if let Some(slug) = query
        .tag
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        select = select.filter(
            posts::Column::Id.in_subquery(
                post_tags::Entity::find()
                    .select_only()
                    .column(post_tags::Column::PostId)
                    .filter(
                        post_tags::Column::TagId.in_subquery(
                            tags::Entity::find()
                                .select_only()
                                .column(tags::Column::Id)
                                .filter(tags::Column::Slug.eq(slug))
                                .into_query(),
                        ),
                    )
                    .into_query(),
            ),
        );
    }
    let author_id = query_integer(query.author_id.as_deref(), 0);
    if author_id != 0 {
        select = select.filter(posts::Column::AuthorId.eq(author_id));
    }
    let pager = select
        .order_by_desc(posts::Column::PublishedAt)
        .order_by_desc(posts::Column::Id)
        .paginate(db, limit);
    let total = pager.num_items().await?;
    let mut data = Vec::new();
    for row in pager.fetch_page(page - 1).await? {
        data.push(public_post(post::Model::load_relations(db, row).await?));
    }
    Ok((data, total))
}

/// A post owned by `author_id` (goravel `findOwnedPost`; `None` = not the
/// caller's or missing — shared by the comment API and the web dashboard).
pub(super) async fn owned_post(
    db: &DatabaseConnection,
    author_id: i64,
    id: i64,
) -> std::result::Result<Option<posts::Model>, DbErr> {
    posts::Entity::find_by_id(id)
        .filter(posts::Column::AuthorId.eq(author_id))
        .one(db)
        .await
}

/// Approved comments for a post (shared by `/api/posts/{slug}/comments` and
/// the web post page). Returns the page rows and the total count.
pub(super) async fn approved_comments(
    db: &DatabaseConnection,
    post_id: i64,
    page: u64,
    limit: u64,
) -> Result<(Vec<CommentResponse>, u64)> {
    let pager = comments::Entity::find()
        .filter(comments::Column::PostId.eq(post_id))
        .filter(comments::Column::Status.eq(comment::STATUS_APPROVED))
        .order_by_asc(comments::Column::CreatedAt)
        .order_by_asc(comments::Column::Id)
        .paginate(db, limit);
    let total = pager.num_items().await?;
    let mut data = Vec::new();
    for row in pager.fetch_page(page - 1).await? {
        let author = users::Entity::find_by_id(row.author_id)
            .one(db)
            .await?
            .ok_or(ModelError::EntityNotFound)?;
        data.push(CommentResponse::new(&row, &author, false));
    }
    Ok((data, total))
}

/// Moderation queue rows for one post, oldest first (goravel
/// `OrderBy("created_at")`) — shared by the API queue
/// (`/api/me/posts/{id}/comments`) and the web moderation page. Returns the
/// page rows (with status) and the total count.
pub(super) async fn moderation_comments(
    db: &DatabaseConnection,
    post_id: i64,
    status: Option<&str>,
    page: u64,
    limit: u64,
) -> Result<(Vec<CommentResponse>, u64)> {
    let mut select = comments::Entity::find().filter(comments::Column::PostId.eq(post_id));
    if let Some(status) = status.filter(|status| !status.is_empty()) {
        select = select.filter(comments::Column::Status.eq(status));
    }
    let pager = select
        .order_by_asc(comments::Column::CreatedAt)
        .order_by_asc(comments::Column::Id)
        .paginate(db, limit);
    let total = pager.num_items().await?;
    let mut data = Vec::new();
    for row in pager.fetch_page(page - 1).await? {
        let author = users::Entity::find_by_id(row.author_id)
            .one(db)
            .await?
            .ok_or(ModelError::EntityNotFound)?;
        data.push(CommentResponse::new(&row, &author, true));
    }
    Ok((data, total))
}

pub(super) async fn index(
    State(ctx): State<AppContext>,
    Query(query): Query<ListQuery>,
) -> Result<Response> {
    let (page, limit) = match query.pagination() {
        Ok(p) => p,
        Err((field, message)) => return Ok(validation(field, message)),
    };
    let (data, total) = feed(&ctx.db, &query, page, limit).await?;
    format::json(Paginated {
        data,
        page,
        limit,
        total,
    })
}

pub(super) fn public_post(loaded: post::PostWithRelations) -> PostResponse {
    let mut response = PostResponse::from(loaded);
    // S4 is read-only: also protect clients from existing/imported HTML.
    // The author API also sanitizes new HTML before persistence.
    response.content_html = ammonia::clean(&response.content_html);
    response
}

pub(super) async fn show(
    State(ctx): State<AppContext>,
    Path(slug): Path<String>,
) -> Result<Response> {
    let Some(row) = published(&ctx.db, &slug).await? else {
        return missing();
    };
    format::json(public_post(
        post::Model::load_relations(&ctx.db, row).await?,
    ))
}

pub(super) async fn comments(
    State(ctx): State<AppContext>,
    Path(slug): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Response> {
    let (page, limit) = match query.pagination() {
        Ok(p) => p,
        Err((field, message)) => return Ok(validation(field, message)),
    };
    let Some(row) = published(&ctx.db, &slug).await? else {
        return missing();
    };
    let (data, total) = approved_comments(&ctx.db, row.id, page, limit).await?;
    format::json(Paginated {
        data,
        page,
        limit,
        total,
    })
}

pub(super) async fn categories(State(ctx): State<AppContext>) -> Result<Response> {
    let data = categories::Entity::find()
        .order_by_asc(categories::Column::Name)
        .all(&ctx.db)
        .await?;
    format::json(serde_json::json!({"data": data}))
}

pub(super) async fn tags(State(ctx): State<AppContext>) -> Result<Response> {
    let data = tags::Entity::find()
        .order_by_asc(tags::Column::Name)
        .all(&ctx.db)
        .await?;
    format::json(serde_json::json!({"data": data}))
}
