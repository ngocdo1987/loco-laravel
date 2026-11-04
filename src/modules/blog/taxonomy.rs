use loco_rs::prelude::*;

use axum::http::StatusCode;
use sea_orm::SqlErr;
use serde_json::Value;

use super::{
    controllers::missing,
    requests::{slugify, InputError, TaxonomyInput},
};
use crate::models::_entities::{categories, tags, users};

type User = auth::JWTWithUser<users::Model>;
type JsonBody = std::result::Result<Json<Value>, Error>;

#[derive(Clone, Copy)]
enum Kind {
    Category,
    Tag,
}

fn database_error(error: &DbErr) -> Result<Response> {
    if matches!(error, DbErr::RecordNotFound(_)) {
        return missing();
    }
    let message = match error.sql_err() {
        Some(SqlErr::UniqueConstraintViolation(_)) => "slug already exists",
        Some(SqlErr::ForeignKeyConstraintViolation(_)) => "taxonomy is assigned to a post",
        _ => {
            tracing::error!(%error, "taxonomy write failed");
            return format::render()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .json(data!({"message": "Unable to process blog request"}));
        }
    };
    format::render()
        .status(StatusCode::CONFLICT)
        .json(data!({"message": message}))
}

async fn save(
    db: &DatabaseConnection,
    kind: Kind,
    id: Option<i64>,
    name: String,
) -> std::result::Result<Option<Value>, DbErr> {
    let slug = slugify(&name);
    let result = match kind {
        Kind::Category => {
            let mut active = if let Some(id) = id {
                let Some(row) = categories::Entity::find_by_id(id).one(db).await? else {
                    return Ok(None);
                };
                row.into_active_model()
            } else {
                <categories::ActiveModel as Default>::default()
            };
            active.name = Set(name);
            active.slug = Set(slug);
            let row = if id.is_some() {
                active.update(db).await?
            } else {
                active.insert(db).await?
            };
            data!(row)
        }
        Kind::Tag => {
            let mut active = if let Some(id) = id {
                let Some(row) = tags::Entity::find_by_id(id).one(db).await? else {
                    return Ok(None);
                };
                row.into_active_model()
            } else {
                <tags::ActiveModel as Default>::default()
            };
            active.name = Set(name);
            active.slug = Set(slug);
            let row = if id.is_some() {
                active.update(db).await?
            } else {
                active.insert(db).await?
            };
            data!(row)
        }
    };
    Ok(Some(result))
}

async fn write(ctx: &AppContext, kind: Kind, id: Option<i64>, body: JsonBody) -> Result<Response> {
    let input = match body
        .map_err(|_| InputError::Payload)
        .and_then(|Json(value)| TaxonomyInput::parse(value))
    {
        Ok(input) => input,
        Err(error) => return Ok(error.response()),
    };
    match save(&ctx.db, kind, id, input.name).await {
        Ok(Some(row)) => format::render()
            .status(if id.is_some() {
                StatusCode::OK
            } else {
                StatusCode::CREATED
            })
            .json(row),
        Ok(None) => missing(),
        Err(error) => database_error(&error),
    }
}

async fn destroy(ctx: &AppContext, kind: Kind, id: i64) -> Result<Response> {
    // Let the existing RESTRICT foreign keys enforce this atomically, including
    // races with post/tag assignment. No check-then-delete window is needed.
    let result = match kind {
        Kind::Category => categories::Entity::delete_by_id(id).exec(&ctx.db).await,
        Kind::Tag => tags::Entity::delete_by_id(id).exec(&ctx.db).await,
    };
    match result {
        Ok(result) if result.rows_affected == 0 => missing(),
        Ok(_) => format::render().status(StatusCode::NO_CONTENT).empty(),
        Err(error) => database_error(&error),
    }
}

pub(super) async fn create_category(
    _user: User,
    State(ctx): State<AppContext>,
    body: JsonBody,
) -> Result<Response> {
    write(&ctx, Kind::Category, None, body).await
}

pub(super) async fn update_category(
    _user: User,
    State(ctx): State<AppContext>,
    Path(id): Path<String>,
    body: JsonBody,
) -> Result<Response> {
    write(&ctx, Kind::Category, Some(id.parse().unwrap_or(0)), body).await
}

pub(super) async fn delete_category(
    _user: User,
    State(ctx): State<AppContext>,
    Path(id): Path<String>,
) -> Result<Response> {
    destroy(&ctx, Kind::Category, id.parse().unwrap_or(0)).await
}

pub(super) async fn create_tag(
    _user: User,
    State(ctx): State<AppContext>,
    body: JsonBody,
) -> Result<Response> {
    write(&ctx, Kind::Tag, None, body).await
}

pub(super) async fn update_tag(
    _user: User,
    State(ctx): State<AppContext>,
    Path(id): Path<String>,
    body: JsonBody,
) -> Result<Response> {
    write(&ctx, Kind::Tag, Some(id.parse().unwrap_or(0)), body).await
}

pub(super) async fn delete_tag(
    _user: User,
    State(ctx): State<AppContext>,
    Path(id): Path<String>,
) -> Result<Response> {
    destroy(&ctx, Kind::Tag, id.parse().unwrap_or(0)).await
}
