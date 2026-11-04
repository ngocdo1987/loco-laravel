//! Browser-facing public pages — port of `RegisterWebRoutes`' GET handlers
//! in `goravel_base/app/modules/blog` (`Home`, `ShowPost`, `Category`, `Tag`)
//! as Tera views under `assets/views/blog/`.
//!
//! Behavioral contract:
//!
//! - feeds reuse the API's published-only query semantics (q/category/tag
//!   filters, pagination) but default to a **9-item** view limit (goravel's
//!   `defaultViewLimit`, applied whenever the request did not pass an
//!   explicit `limit`);
//! - invalid pagination renders goravel's minimal HTML error page (400),
//!   unknown posts/drafts render the 404 error page;
//! - the post page lists approved comments only and shows the comment form
//!   for logged-in visitors — "logged in" is the `auth_token` cookie JWT
//!   issued by the (frozen) auth module's web pages, resolved exactly like
//!   its `current_web_user` helper;
//! - the comment submit handler (`POST /posts/{slug}/comments`) is dashboard
//!   scope and lands with S10; the form renders for parity but posts 404
//!   until then.

use loco_rs::{
    auth::jwt, controller::views::engines::TeraView, controller::views::ViewEngine, prelude::*,
};

use axum::extract::Query;
use axum::http::StatusCode;
use sea_orm::QueryOrder;

use super::{
    controllers::{self, ListQuery},
    models::post,
};
use crate::models::_entities::{categories, tags, users};

/// Same cookie the frozen auth module's web pages set on login.
const AUTH_COOKIE: &str = "auth_token";

/// The feed page size unless the request carries an explicit `limit`
/// (goravel: `if query.Limit == 20 { query.Limit = defaultViewLimit }`).
const DEFAULT_VIEW_LIMIT: u64 = 9;

/// Mirrors the auth module's private `current_web_user`: validate the
/// `auth_token` cookie JWT and load the user; `None` means anonymous.
async fn current_web_user(ctx: &AppContext, jar: &cookie::CookieJar) -> Option<users::Model> {
    let token = jar.get(AUTH_COOKIE)?.value().to_string();
    let jwt_secret = ctx.config.get_jwt_config().ok()?;
    let claims = jwt::JWT::new(&jwt_secret.secret).validate(&token).ok()?;
    crate::modules::auth::models::users::Model::find_by_pid(&ctx.db, &claims.claims.pid)
        .await
        .ok()
}

/// goravel's `renderError`: a minimal HTML page with the given status.
fn error_page(status: StatusCode, message: &str) -> Response {
    (
        status,
        [("content-type", "text/html; charset=utf-8")],
        format!("<!doctype html><title>Error</title><h1>{message}</h1>"),
    )
        .into_response()
}

async fn render_feed(
    v: &TeraView,
    ctx: &AppContext,
    jar: &cookie::CookieJar,
    template: &str,
    query: &ListQuery,
    title: String,
    description: &str,
) -> Result<Response> {
    let Ok((page, limit)) = query.pagination() else {
        return Ok(error_page(StatusCode::BAD_REQUEST, "Invalid feed query."));
    };
    // goravel's default-9 view limit kicks in for the implicit limit only.
    let limit = if limit == 20 {
        DEFAULT_VIEW_LIMIT
    } else {
        limit
    };
    let (posts, total) = controllers::feed(&ctx.db, query, page, limit).await?;
    let categories = categories::Entity::find()
        .order_by_asc(categories::Column::Name)
        .all(&ctx.db)
        .await?;
    let tags = tags::Entity::find()
        .order_by_asc(tags::Column::Name)
        .all(&ctx.db)
        .await?;
    let current_user = current_web_user(ctx, jar).await;
    let user = current_user
        .as_ref()
        .map(|user| serde_json::json!({"name": user.name}));
    format::render().view(
        v,
        template,
        data!({
            "title": title,
            "description": description,
            "posts": posts,
            "categories": categories,
            "tags": tags,
            "q": query.q.clone().unwrap_or_default(),
            "category": query.category.clone().unwrap_or_default(),
            "tag": query.tag.clone().unwrap_or_default(),
            "page": page,
            "limit": limit,
            "total": total,
            "has_prev": page > 1,
            "prev_page": page - 1,
            "has_next": page.saturating_mul(limit) < total,
            "next_page": page + 1,
            "current_user": user,
        }),
    )
}

/// `GET /` — blog home feed.
#[debug_handler]
pub(super) async fn home(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: cookie::CookieJar,
    Query(query): Query<ListQuery>,
) -> Result<Response> {
    render_feed(
        &v,
        &ctx,
        &jar,
        "blog/home.html",
        &query,
        "Latest posts".to_owned(),
        "Browse published posts from the Goravel blog.",
    )
    .await
}

/// `GET /categories/{slug}` — feed filtered to one category.
#[debug_handler]
pub(super) async fn category(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: cookie::CookieJar,
    Path(slug): Path<String>,
    Query(mut query): Query<ListQuery>,
) -> Result<Response> {
    query.category = Some(slug.clone());
    render_feed(
        &v,
        &ctx,
        &jar,
        "blog/feed.html",
        &query,
        format!("Category: {slug}"),
        "Published posts in this category.",
    )
    .await
}

/// `GET /tags/{slug}` — feed filtered to one tag.
#[debug_handler]
pub(super) async fn tag(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: cookie::CookieJar,
    Path(slug): Path<String>,
    Query(mut query): Query<ListQuery>,
) -> Result<Response> {
    query.tag = Some(slug.clone());
    render_feed(
        &v,
        &ctx,
        &jar,
        "blog/feed.html",
        &query,
        format!("Tag: {slug}"),
        "Published posts tagged with this topic.",
    )
    .await
}

/// `GET /posts/{slug}` — a published post with its approved comments.
#[debug_handler]
pub(super) async fn show_post(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: cookie::CookieJar,
    Path(slug): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Response> {
    let Some(row) = controllers::published(&ctx.db, &slug).await? else {
        return Ok(error_page(StatusCode::NOT_FOUND, "Resource not found"));
    };
    // goravel reads only `page` here (default 1, limit fixed at 20).
    let page = query
        .page
        .as_deref()
        .map(|value| str::parse::<u64>(value).unwrap_or(1))
        .filter(|value| *value >= 1)
        .unwrap_or(1);
    let post = controllers::public_post(post::Model::load_relations(&ctx.db, row).await?);
    let (comments, _total) = controllers::approved_comments(&ctx.db, post.id, page, 20).await?;
    let current_user = current_web_user(&ctx, &jar).await;
    let authenticated = current_user.is_some();
    let user = current_user
        .as_ref()
        .map(|user| serde_json::json!({"name": user.name}));
    format::render().view(
        &v,
        "blog/post.html",
        data!({
            "title": post.title,
            "post": post,
            "comments": comments,
            "authenticated": authenticated,
            "current_user": user,
        }),
    )
}
