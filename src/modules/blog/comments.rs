//! Comment endpoints — port of the comment API in
//! `goravel_base/app/modules/blog` (submit, delete-own, moderation queue,
//! moderate, delete-moderated). Behavioral contract:
//!
//! - new comments always start `pending` and are only visible through the
//!   moderation queue until approved;
//! - submit targets a **published** post by slug; everything else addresses
//!   the post owner by id (404 for anyone else, matching `findOwnedPost`);
//! - moderation status changes are restricted to `approved`/`rejected`.

use loco_rs::prelude::*;

use axum::http::StatusCode;
use sea_orm::{PaginatorTrait, QueryOrder};
use serde_json::Value;

use super::{
    controllers::{missing, published, validation, ListQuery, Paginated},
    models::{comment, responses::CommentResponse},
    requests::{CommentInput, InputError, ModerationInput},
};
use crate::models::_entities::{comments, posts, users};

type User = auth::JWTWithUser<users::Model>;
type JsonBody = std::result::Result<Json<Value>, Error>;

fn failed(error: &DbErr) -> Result<Response> {
    tracing::error!(%error, "comment write failed");
    format::render()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .json(data!({"message": "Unable to process blog request"}))
}

// Goravel's RouteInt yields 0 for non-numeric path segments, which then
// simply fails the lookup with a 404 — parse to the same effect.
fn identifier(value: &str) -> i64 {
    value.parse().unwrap_or(0)
}

fn comment_body(body: JsonBody) -> std::result::Result<CommentInput, InputError> {
    match body {
        Ok(Json(value)) => CommentInput::parse(value),
        Err(_) => Err(InputError::Payload),
    }
}

fn moderation_body(body: JsonBody) -> std::result::Result<ModerationInput, InputError> {
    match body {
        Ok(Json(value)) => ModerationInput::parse(value),
        Err(_) => Err(InputError::Payload),
    }
}

async fn owned_post(
    db: &DatabaseConnection,
    author_id: i64,
    id: i64,
) -> Result<Option<posts::Model>> {
    Ok(posts::Entity::find_by_id(id)
        .filter(posts::Column::AuthorId.eq(author_id))
        .one(db)
        .await?)
}

/// `POST /api/posts/{slug}/comments` — submit a comment for moderation.
#[debug_handler]
pub(super) async fn submit(
    user: User,
    State(ctx): State<AppContext>,
    Path(slug): Path<String>,
    body: JsonBody,
) -> Result<Response> {
    let input = match comment_body(body) {
        Ok(input) => input,
        Err(error) => return Ok(error.response()),
    };
    let Some(post) = published(&ctx.db, &slug).await? else {
        return missing();
    };
    let row = match (comments::ActiveModel {
        post_id: Set(post.id),
        author_id: Set(user.user.id),
        content: Set(input.content),
        status: Set(comment::STATUS_PENDING.to_owned()),
        ..Default::default()
    })
    .insert(&ctx.db)
    .await
    {
        Ok(row) => row,
        Err(error) => return failed(&error),
    };
    format::render().status(StatusCode::CREATED).json(data!({
        "message": "Comment submitted for moderation",
        "comment": CommentResponse::new(&row, &user.user, true)
    }))
}

/// `DELETE /api/comments/{id}` — authors may delete their own comment.
pub(super) async fn delete_own(
    user: User,
    State(ctx): State<AppContext>,
    Path(id): Path<String>,
) -> Result<Response> {
    let deleted = match comments::Entity::delete_many()
        .filter(comments::Column::Id.eq(identifier(&id)))
        .filter(comments::Column::AuthorId.eq(user.user.id))
        .exec(&ctx.db)
        .await
    {
        Ok(deleted) => deleted,
        Err(error) => return failed(&error),
    };
    if deleted.rows_affected == 0 {
        return missing();
    }
    format::render().status(StatusCode::NO_CONTENT).empty()
}

/// `GET /api/me/posts/{id}/comments` — the post owner's moderation queue.
pub(super) async fn queue(
    user: User,
    State(ctx): State<AppContext>,
    Path(id): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Response> {
    let (page, limit) = match query.pagination() {
        Ok(p) => p,
        Err((field, message)) => return Ok(validation(field, message)),
    };
    let status = query.status.as_deref().unwrap_or_default().trim();
    if !matches!(
        status,
        "" | comment::STATUS_PENDING | comment::STATUS_APPROVED | comment::STATUS_REJECTED
    ) {
        return Ok(validation(
            "status",
            "must be pending, approved, or rejected",
        ));
    }
    let post_id = identifier(&id);
    if owned_post(&ctx.db, user.user.id, post_id).await?.is_none() {
        return missing();
    }
    let mut select = comments::Entity::find().filter(comments::Column::PostId.eq(post_id));
    if !status.is_empty() {
        select = select.filter(comments::Column::Status.eq(status));
    }
    let pager = select
        .order_by_asc(comments::Column::CreatedAt)
        .order_by_asc(comments::Column::Id)
        .paginate(&ctx.db, limit);
    let total = pager.num_items().await?;
    let mut data = Vec::new();
    for row in pager.fetch_page(page - 1).await? {
        let author = users::Entity::find_by_id(row.author_id)
            .one(&ctx.db)
            .await?
            .ok_or(ModelError::EntityNotFound)?;
        data.push(CommentResponse::new(&row, &author, true));
    }
    format::json(Paginated {
        data,
        page,
        limit,
        total,
    })
}

/// `PATCH /api/me/posts/{id}/comments/{comment_id}` — approve or reject.
#[debug_handler]
pub(super) async fn moderate(
    user: User,
    State(ctx): State<AppContext>,
    Path((id, comment_id)): Path<(String, String)>,
    body: JsonBody,
) -> Result<Response> {
    let input = match moderation_body(body) {
        Ok(input) => input,
        Err(error) => return Ok(error.response()),
    };
    let post_id = identifier(&id);
    if owned_post(&ctx.db, user.user.id, post_id).await?.is_none() {
        return missing();
    }
    let Some(row) = comments::Entity::find_by_id(identifier(&comment_id))
        .filter(comments::Column::PostId.eq(post_id))
        .one(&ctx.db)
        .await?
    else {
        return missing();
    };
    let mut active = row.into_active_model();
    active.status = Set(input.status);
    let row = match active.update(&ctx.db).await {
        Ok(row) => row,
        Err(error) => return failed(&error),
    };
    let author = users::Entity::find_by_id(row.author_id)
        .one(&ctx.db)
        .await?
        .ok_or(ModelError::EntityNotFound)?;
    format::render().json(data!({
        "message": "Comment moderation updated",
        "comment": CommentResponse::new(&row, &author, true)
    }))
}

/// `DELETE /api/me/posts/{id}/comments/{comment_id}` — the post owner
/// removes a comment from their post.
pub(super) async fn delete_moderated(
    user: User,
    State(ctx): State<AppContext>,
    Path((id, comment_id)): Path<(String, String)>,
) -> Result<Response> {
    let post_id = identifier(&id);
    if owned_post(&ctx.db, user.user.id, post_id).await?.is_none() {
        return missing();
    }
    let deleted = match comments::Entity::delete_many()
        .filter(comments::Column::Id.eq(identifier(&comment_id)))
        .filter(comments::Column::PostId.eq(post_id))
        .exec(&ctx.db)
        .await
    {
        Ok(deleted) => deleted,
        Err(error) => return failed(&error),
    };
    if deleted.rows_affected == 0 {
        return missing();
    }
    format::render().status(StatusCode::NO_CONTENT).empty()
}
