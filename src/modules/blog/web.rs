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
//! - `POST /posts/{slug}/comments` (the S10 half of the comment form)
//!   mirrors goravel `SubmitComment`: anonymous → 302 `/login`, invalid
//!   content flashes goravel's fixed message, success queues a pending
//!   comment and flashes "Comment submitted for moderation.";
//! - feedback rides the two flash cookies defined here (`blog_flash`/
//!   `blog_error`, loco's sessionless stand-in for goravel's session flash):
//!   `redirect_flash` sets them on a redirect, `render_flash` shows and
//!   expires them on the next render (dashboard pages share this plumbing);
//! - `GET /storage/{*path}` serves the public storage disk — goravel wires
//!   this at app level (`Route.Static("storage", ...)`); here it lives on
//!   the blog module's web surface because that is where the URLs come
//!   from (S8's deferred static route).

use loco_rs::{
    auth::jwt, controller::views::engines::TeraView, controller::views::ViewEngine, prelude::*,
};

use axum::extract::{Form, Query};
use axum::http::{header, StatusCode};
use sea_orm::QueryOrder;

use super::{
    controllers::{self, ListQuery},
    models::{comment, post},
    requests::CommentInput,
};
use crate::models::_entities::{categories, comments, tags, users};

/// Same cookie the frozen auth module's web pages set on login.
const AUTH_COOKIE: &str = "auth_token";

/// The feed page size unless the request carries an explicit `limit`
/// (goravel: `if query.Limit == 20 { query.Limit = defaultViewLimit }`).
const DEFAULT_VIEW_LIMIT: u64 = 9;

/// Flash cookie names (goravel's session keys `flash`/`error`, carried as
/// cookies because loco has no session).
const FLASH_COOKIE: &str = "blog_flash";
const ERROR_COOKIE: &str = "blog_error";

fn cookie_header(name: &str, message: Option<&str>) -> String {
    message.map_or_else(
        || {
            // An already-expired cookie: both `Expires` (epoch) and
            // `Max-Age=0` so every client drops it.
            format!(
                "{name}=; Path=/; HttpOnly; SameSite=Lax; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Max-Age=0"
            )
        },
        |message| {
            cookie::Cookie::build((name.to_owned(), message.to_owned()))
                .path("/")
                .http_only(true)
                .same_site(cookie::SameSite::Lax)
                .build()
                .to_string()
        },
    )
}

/// A 302 redirect carrying flash feedback in cookies (goravel
/// `redirect` + `flash`/`flashError`).
pub(super) fn redirect_flash(location: &str, flash: Option<&str>, error: Option<&str>) -> Response {
    let mut response = (StatusCode::FOUND, [(header::LOCATION, location)]).into_response();
    let headers = response.headers_mut();
    if let Some(message) = flash {
        if let Ok(value) =
            header::HeaderValue::from_str(&cookie_header(FLASH_COOKIE, Some(message)))
        {
            headers.append(header::SET_COOKIE, value);
        }
    }
    if let Some(message) = error {
        if let Ok(value) =
            header::HeaderValue::from_str(&cookie_header(ERROR_COOKIE, Some(message)))
        {
            headers.append(header::SET_COOKIE, value);
        }
    }
    response
}

/// goravel's `ViewAuth` middleware: anonymous visitors are bounced to the
/// auth module's login page.
pub(super) fn login_redirect() -> Response {
    (StatusCode::FOUND, [(header::LOCATION, "/login")]).into_response()
}

/// Renders a view with the flash cookies' content spliced in as
/// `flash`/`error` alerts, expiring both cookies (goravel `pullFlash`/
/// `pullError` — show once).
pub(super) async fn render_flash(
    v: &TeraView,
    template: &str,
    mut data: serde_json::Value,
    jar: &cookie::CookieJar,
) -> Result<Response> {
    let flash = jar
        .get(FLASH_COOKIE)
        .map(|cookie| cookie.value().to_owned());
    let error = jar
        .get(ERROR_COOKIE)
        .map(|cookie| cookie.value().to_owned());
    if let Some(object) = data.as_object_mut() {
        object.insert("flash".to_owned(), serde_json::json!(flash));
        object.insert("error".to_owned(), serde_json::json!(error));
    }
    let mut response = format::render().view(v, template, data)?;
    let headers = response.headers_mut();
    for (name, value) in [
        (FLASH_COOKIE, flash.is_some()),
        (ERROR_COOKIE, error.is_some()),
    ] {
        if value {
            if let Ok(value) = header::HeaderValue::from_str(&cookie_header(name, None)) {
                headers.append(header::SET_COOKIE, value);
            }
        }
    }
    Ok(response)
}

/// Mirrors the auth module's private `current_web_user`: validate the
/// `auth_token` cookie JWT and load the user; `None` means anonymous.
pub(super) async fn current_web_user(
    ctx: &AppContext,
    jar: &cookie::CookieJar,
) -> Option<users::Model> {
    let token = jar.get(AUTH_COOKIE)?.value().to_string();
    let jwt_secret = ctx.config.get_jwt_config().ok()?;
    let claims = jwt::JWT::new(&jwt_secret.secret).validate(&token).ok()?;
    crate::modules::auth::models::users::Model::find_by_pid(&ctx.db, &claims.claims.pid)
        .await
        .ok()
}

/// goravel's `renderError`: a minimal HTML page with the given status.
pub(super) fn error_page(status: StatusCode, message: &str) -> Response {
    (
        status,
        [("content-type", "text/html; charset=utf-8")],
        format!("<!doctype html><title>Error</title><h1>{message}</h1>"),
    )
        .into_response()
}

#[allow(clippy::too_many_arguments)]
async fn render_feed(
    v: &TeraView,
    ctx: &AppContext,
    jar: &cookie::CookieJar,
    template: &str,
    query: &ListQuery,
    title: String,
    description: &str,
    invalid: &'static str,
) -> Result<Response> {
    let Ok((page, limit)) = query.pagination() else {
        return Ok(error_page(StatusCode::BAD_REQUEST, invalid));
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
        "Invalid feed query.",
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
        "Invalid category query.",
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
        "Invalid tag query.",
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
    render_flash(
        &v,
        "blog/post.html",
        data!({
            "title": post.title,
            "post": post,
            "comments": comments,
            "authenticated": authenticated,
            "current_user": user,
        }),
        &jar,
    )
    .await
}

/// A urlencoded `{ content }` comment form.
#[derive(Default, serde::Deserialize)]
pub(super) struct CommentForm {
    content: Option<String>,
}
/// `POST /posts/{slug}/comments` — submit a comment for moderation from the
/// post page's form (goravel `SubmitComment`, the S9-deferred half).
#[debug_handler]
pub(super) async fn submit_comment(
    State(ctx): State<AppContext>,
    jar: cookie::CookieJar,
    Path(slug): Path<String>,
    form: Form<CommentForm>,
) -> Result<Response> {
    let back = format!("/posts/{slug}");
    let Some(user) = current_web_user(&ctx, &jar).await else {
        return Ok(login_redirect());
    };
    let content = form.0.content.unwrap_or_default();
    let Ok(input) = CommentInput::parse(serde_json::json!({"content": content})) else {
        return Ok(redirect_flash(
            &back,
            None,
            Some("Comment content is required and must be 2000 characters or less."),
        ));
    };
    let Some(post) = controllers::published(&ctx.db, &slug).await? else {
        return Ok(redirect_flash(
            &back,
            None,
            Some("Unable to submit comment."),
        ));
    };
    match (comments::ActiveModel {
        post_id: Set(post.id),
        author_id: Set(user.id),
        content: Set(input.content),
        status: Set(comment::STATUS_PENDING.to_owned()),
        ..Default::default()
    })
    .insert(&ctx.db)
    .await
    {
        Ok(_) => Ok(redirect_flash(
            &back,
            Some("Comment submitted for moderation."),
            None,
        )),
        Err(error) => {
            tracing::error!(%error, "web comment submit failed");
            Ok(redirect_flash(
                &back,
                None,
                Some("Unable to submit comment."),
            ))
        }
    }
}

/// `GET /storage/{*path}` — serve the public storage disk (uploaded covers
/// and images) with a per-extension content type; anything outside the disk
/// is a 404, like goravel's static route.
#[debug_handler]
pub(super) async fn storage_asset(
    State(ctx): State<AppContext>,
    Path(path): Path<String>,
) -> Result<Response> {
    if path.split('/').any(|segment| segment == "..") {
        return Ok((StatusCode::NOT_FOUND, "Not found").into_response());
    }
    let data: Vec<u8> = match ctx.storage.download(std::path::Path::new(&path)).await {
        Ok(data) => data,
        Err(_) => return Ok((StatusCode::NOT_FOUND, "Not found").into_response()),
    };
    let mime = match path.rsplit('.').next() {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    };
    Ok(([(header::CONTENT_TYPE, mime)], data).into_response())
}
