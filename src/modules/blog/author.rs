use loco_rs::prelude::*;

use axum::http::StatusCode;
use sea_orm::{PaginatorTrait, QueryOrder, QuerySelect, SqlErr};
use serde_json::Value;

use super::{
    controllers::{missing, public_post, validation, ListQuery, Paginated},
    models::post,
    requests::{slugify, InputError, PostInput},
};
use crate::models::_entities::{categories, post_tags, posts, tags, users};

type Author = auth::JWTWithUser<users::Model>;
type JsonBody = std::result::Result<Json<Value>, Error>;
pub(super) type WriteResult<T> = std::result::Result<T, WriteError>;

#[derive(Debug)]
pub(super) enum WriteError {
    Missing,
    SlugConflict,
    Relations,
    Database(DbErr),
}

impl From<DbErr> for WriteError {
    fn from(error: DbErr) -> Self {
        match error.sql_err() {
            Some(SqlErr::UniqueConstraintViolation(_)) => Self::SlugConflict,
            Some(SqlErr::ForeignKeyConstraintViolation(_)) => Self::Relations,
            _ => Self::Database(error),
        }
    }
}

impl WriteError {
    pub(super) fn response(self) -> Result<Response> {
        match self {
            Self::Missing => missing(),
            Self::SlugConflict => format::render()
                .status(StatusCode::CONFLICT)
                .json(data!({"message": "slug already exists"})),
            Self::Relations => Ok(validation(
                "relationships",
                "category or tag does not exist",
            )),
            Self::Database(error) => {
                tracing::error!(%error, "blog write failed");
                format::render()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .json(data!({"message": "Unable to process blog request"}))
            }
        }
    }
}

fn parse(body: JsonBody, create: bool) -> std::result::Result<PostInput, InputError> {
    let Json(value) = body.map_err(|_| InputError::Payload)?;
    PostInput::parse(value, create)
}

pub(super) async fn owned<C: ConnectionTrait>(
    db: &C,
    author_id: i64,
    id: i64,
    lock: bool,
) -> WriteResult<posts::Model> {
    let mut query = posts::Entity::find_by_id(id).filter(posts::Column::AuthorId.eq(author_id));
    if lock {
        query = query.lock_exclusive();
    }
    query.one(db).await?.ok_or(WriteError::Missing)
}

async fn validate_relations<C: ConnectionTrait>(db: &C, input: &PostInput) -> WriteResult<()> {
    if let Some(id) = input.category_id {
        if categories::Entity::find_by_id(id).one(db).await?.is_none() {
            return Err(WriteError::Relations);
        }
    }
    let ids = input.tag_ids.as_deref().unwrap_or_default();
    if !ids.is_empty()
        && tags::Entity::find()
            .filter(tags::Column::Id.is_in(ids.iter().copied()))
            .count(db)
            .await?
            != ids.len() as u64
    {
        return Err(WriteError::Relations);
    }
    Ok(())
}

async fn resolve_slug<C: ConnectionTrait>(
    db: &C,
    input: &PostInput,
    title: &str,
    except: i64,
    generated: bool,
) -> WriteResult<String> {
    let base = slugify(if input.slug.is_empty() {
        title
    } else {
        &input.slug
    });
    let mut candidate = base.clone();
    let mut suffix = 2_u64;
    loop {
        if posts::Entity::find()
            .filter(posts::Column::Slug.eq(&candidate))
            .filter(posts::Column::Id.ne(except))
            .count(db)
            .await?
            == 0
        {
            return Ok(candidate);
        }
        if !generated {
            return Err(WriteError::SlugConflict);
        }
        let ending = format!("-{suffix}");
        candidate = format!("{}{}", &base[..base.len().min(255 - ending.len())], ending);
        suffix += 1;
    }
}

async fn write_tags<C: ConnectionTrait>(db: &C, id: i64, input: &PostInput) -> WriteResult<()> {
    let ids = input.tag_ids.as_deref().unwrap_or_default();
    if !ids.is_empty() {
        post_tags::Entity::insert_many(ids.iter().map(|tag_id| post_tags::ActiveModel {
            post_id: Set(id),
            tag_id: Set(*tag_id),
        }))
        .exec(db)
        .await?;
    }
    Ok(())
}

pub(super) async fn create_post(
    db: &DatabaseConnection,
    author_id: i64,
    input: &PostInput,
) -> WriteResult<posts::Model> {
    let tx = db.begin().await?;
    validate_relations(&tx, input).await?;
    let slug = resolve_slug(&tx, input, &input.title, 0, input.slug.is_empty()).await?;
    let row = posts::ActiveModel {
        author_id: Set(author_id),
        category_id: Set(input.category_id),
        slug: Set(slug),
        title: Set(input.title.clone()),
        excerpt: Set(Some(input.excerpt.clone())),
        cover_image_url: Set(Some(input.cover_image_url.clone())),
        content_html: Set(ammonia::clean(&input.content_html)),
        status: Set(input.status.clone()),
        published_at: Set(
            (input.status == post::STATUS_PUBLISHED).then(|| chrono::Utc::now().into())
        ),
        ..Default::default()
    }
    .insert(&tx)
    .await?;
    write_tags(&tx, row.id, input).await?;
    tx.commit().await?;
    Ok(row)
}

pub(super) async fn update_post(
    db: &DatabaseConnection,
    author_id: i64,
    id: i64,
    input: &PostInput,
) -> WriteResult<posts::Model> {
    let tx = db.begin().await?;
    let row = owned(&tx, author_id, id, true).await?;
    if input.fields.contains("category_id") || input.fields.contains("tag_ids") {
        validate_relations(&tx, input).await?;
    }
    let mut active = row.clone().into_active_model();
    let title = if input.fields.contains("title") {
        &input.title
    } else {
        &row.title
    };
    if input.fields.contains("title") {
        active.title = Set(input.title.clone());
    }
    if input.fields.contains("slug") {
        active.slug = Set(resolve_slug(&tx, input, title, id, false).await?);
    }
    if input.fields.contains("excerpt") {
        active.excerpt = Set(Some(input.excerpt.clone()));
    }
    if input.fields.contains("cover_image_url") {
        active.cover_image_url = Set(Some(input.cover_image_url.clone()));
    }
    if input.fields.contains("content_html") {
        active.content_html = Set(ammonia::clean(&input.content_html));
    }
    if input.fields.contains("category_id") {
        active.category_id = Set(input.category_id);
    }
    if input.fields.contains("status") && input.status != row.status {
        active.status = Set(input.status.clone());
        active.published_at =
            Set((input.status == post::STATUS_PUBLISHED).then(|| chrono::Utc::now().into()));
    }
    let row = active.update(&tx).await?;
    if input.fields.contains("tag_ids") {
        post_tags::Entity::delete_many()
            .filter(post_tags::Column::PostId.eq(id))
            .exec(&tx)
            .await?;
        write_tags(&tx, id, input).await?;
    }
    tx.commit().await?;
    Ok(row)
}

/// Sets the cover image on an owned post (used by the upload slice's
/// `/api/me/posts/{id}/cover` endpoint, mirroring goravel's
/// `UpdatePost` with `Fields: {"cover_image_url"}`).
pub(super) async fn update_cover(
    db: &DatabaseConnection,
    author_id: i64,
    id: i64,
    url: &str,
) -> WriteResult<posts::Model> {
    update_post(
        db,
        author_id,
        id,
        &PostInput {
            cover_image_url: url.to_owned(),
            fields: std::collections::HashSet::from(["cover_image_url".to_owned()]),
            ..Default::default()
        },
    )
    .await
}

#[debug_handler]
pub(super) async fn store(
    author: Author,
    State(ctx): State<AppContext>,
    body: JsonBody,
) -> Result<Response> {
    let input = match parse(body, true) {
        Ok(input) => input,
        Err(error) => return Ok(error.response()),
    };
    let row = match create_post(&ctx.db, author.user.id, &input).await {
        Ok(row) => row,
        Err(error) => return error.response(),
    };
    let response = public_post(post::Model::load_relations(&ctx.db, row).await?);
    format::render()
        .status(StatusCode::CREATED)
        .json(data!({"message": "Post created successfully", "post": response}))
}

pub(super) async fn index(
    author: Author,
    State(ctx): State<AppContext>,
    Query(query): Query<ListQuery>,
) -> Result<Response> {
    let (page, limit) = match query.pagination() {
        Ok(p) => p,
        Err((field, message)) => return Ok(validation(field, message)),
    };
    let status = query.status.as_deref().unwrap_or_default().trim();
    if !matches!(status, "" | "draft" | "published") {
        return Ok(validation("status", "must be draft or published"));
    }
    let mut select = posts::Entity::find().filter(posts::Column::AuthorId.eq(author.user.id));
    if !status.is_empty() {
        select = select.filter(posts::Column::Status.eq(status));
    }
    let pager = select
        .order_by_desc(posts::Column::CreatedAt)
        .order_by_desc(posts::Column::Id)
        .paginate(&ctx.db, limit);
    let total = pager.num_items().await?;
    let mut data = Vec::new();
    for row in pager.fetch_page(page - 1).await? {
        data.push(public_post(
            post::Model::load_relations(&ctx.db, row).await?,
        ));
    }
    format::json(Paginated {
        data,
        page,
        limit,
        total,
    })
}

pub(super) async fn show(
    author: Author,
    State(ctx): State<AppContext>,
    Path(id): Path<i64>,
) -> Result<Response> {
    let row = match owned(&ctx.db, author.user.id, id, false).await {
        Ok(row) => row,
        Err(error) => return error.response(),
    };
    format::json(public_post(
        post::Model::load_relations(&ctx.db, row).await?,
    ))
}

#[debug_handler]
pub(super) async fn update(
    author: Author,
    State(ctx): State<AppContext>,
    Path(id): Path<i64>,
    body: JsonBody,
) -> Result<Response> {
    let input = match parse(body, false) {
        Ok(input) => input,
        Err(error) => return Ok(error.response()),
    };
    let row = match update_post(&ctx.db, author.user.id, id, &input).await {
        Ok(row) => row,
        Err(error) => return error.response(),
    };
    let response = public_post(post::Model::load_relations(&ctx.db, row).await?);
    format::json(data!({"message": "Post updated successfully", "post": response}))
}

pub(super) async fn destroy(
    author: Author,
    State(ctx): State<AppContext>,
    Path(id): Path<i64>,
) -> Result<Response> {
    let deleted = posts::Entity::delete_many()
        .filter(posts::Column::Id.eq(id))
        .filter(posts::Column::AuthorId.eq(author.user.id))
        .exec(&ctx.db)
        .await?;
    if deleted.rows_affected == 0 {
        return missing();
    }
    format::render().status(StatusCode::NO_CONTENT).empty()
}
