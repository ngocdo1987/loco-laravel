//! Browser-facing dashboard pages — port of the `RegisterWebRoutes`
//! dashboard group in `goravel_base/app/modules/blog` (`/dashboard/**`:
//! posts CRUD, taxonomy management, comment moderation). The public comment
//! submit handler that S9 deferred lives in `web.rs`.
//!
//! Behavioral contract:
//!
//! - every route requires the `auth_token` cookie JWT (the frozen auth
//!   module's web login); anonymous visitors get goravel's 302 to `/login`;
//! - goravel carries feedback in session flash — loco has no session, so
//!   flash rides two cookies (`blog_flash`/`blog_error`, see `web.rs`) that
//!   are set on the redirect response and expired on the next render;
//! - forms validate with the same rules as the API: the multipart post form
//!   is folded into the API's `PostInput` validator and re-renders the form
//!   (200) with the first field error instead of a 422 JSON body, matching
//!   goravel's `renderPostForm` flow;
//! - create/update reuse the author API's transactional writers, taxonomy
//!   the taxonomy API's `save`/`remove`, moderation the comment API's query
//!   helpers — the dashboard is a thin shell over the same behavior, never a
//!   second implementation;
//! - CSRF: goravel embeds a session `_token` in every form; loco has no
//!   session layer and the auth module is frozen, so forms omit it
//!   (documented deviation, consistent with S9's comment form).

use loco_rs::{controller::views::engines::TeraView, controller::views::ViewEngine, prelude::*};

use axum::body::Bytes;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use sea_orm::{PaginatorTrait, QueryOrder};
use serde::Deserialize;
use serde_json::Value;

use super::{
    author,
    controllers::{self, ListQuery},
    models::post,
    requests::{InputError, PostInput, TaxonomyInput},
    taxonomy,
    uploads::store_image,
    web::{current_web_user, error_page, login_redirect, redirect_flash, render_flash},
};
use crate::models::_entities::{categories, comments, posts, tags, users};

/// The values of one rendered post form (goravel `postFormData`); shared
/// with the author API's multipart create/update bodies (goravel binds the
/// same `PostWriteRequest` from `form` + `json` tags alike).
#[derive(Default)]
pub(super) struct PostForm {
    title: String,
    slug: String,
    excerpt: String,
    cover_image_url: String,
    content_html: String,
    status: String,
    category_id: String,
    tag_ids: Vec<String>,
    pub(super) cover: Option<Bytes>,
    /// The field names actually submitted (goravel's `ctx.Request().All()`
    /// keys — the API's partial-update semantics).
    fields: std::collections::HashSet<String>,
}

/// A urlencoded `{ name }` form (taxonomy create/update).
#[derive(Default, Deserialize)]
pub(super) struct TaxonomyForm {
    name: Option<String>,
}

/// A urlencoded `{ status }` form (comment moderation).
#[derive(Default, Deserialize)]
pub(super) struct ModerationForm {
    status: Option<String>,
}

/// The moderation page's query string.
#[derive(Default, Deserialize)]
pub(super) struct ModerationQuery {
    post_id: Option<String>,
    status: Option<String>,
    page: Option<String>,
}

// Goravel's QueryInt: default for absent/empty, zero for unparsable.
fn query_integer(value: Option<&str>, default: i64) -> i64 {
    value
        .filter(|value| !value.is_empty())
        .map_or(default, |value| value.parse().unwrap_or(0))
}

// Goravel's RouteInt: zero for unparsable segments (fails lookups the same
// way a 404 would).
fn identifier(value: &str) -> i64 {
    value.parse().unwrap_or(0)
}

/// The own-posts feed (goravel `ListOwn`): the author's posts newest-first
/// with an optional status filter. Returns the page rows and the total.
async fn own_posts(
    db: &DatabaseConnection,
    author_id: i64,
    status: Option<&str>,
    page: u64,
    limit: u64,
) -> Result<(Vec<Value>, u64)> {
    let mut select = posts::Entity::find().filter(posts::Column::AuthorId.eq(author_id));
    if let Some(status) = status.filter(|status| !status.is_empty()) {
        select = select.filter(posts::Column::Status.eq(status));
    }
    let pager = select
        .order_by_desc(posts::Column::CreatedAt)
        .order_by_desc(posts::Column::Id)
        .paginate(db, limit);
    let total = pager.num_items().await?;
    let mut data = Vec::new();
    for row in pager.fetch_page(page - 1).await? {
        data.push(serde_json::to_value(controllers::public_post(
            post::Model::load_relations(db, row).await?,
        ))?);
    }
    Ok((data, total))
}

/// Collects the post form's multipart fields (goravel `viewPostInput` reads
/// the same inputs; `cover_image` only counts when a file was chosen).
pub(super) async fn post_form(multipart: &mut Multipart) -> PostForm {
    let mut form = PostForm::default();
    while let Ok(Some(part)) = multipart.next_field().await {
        let Some(name) = part.name().map(str::to_owned) else {
            continue;
        };
        match name.as_str() {
            "tag_ids" => {
                if let Ok(value) = part.text().await {
                    form.tag_ids.push(value);
                }
                form.fields.insert("tag_ids".to_owned());
            }
            "cover_image" => {
                let chosen = part.file_name().is_some_and(|name| !name.is_empty());
                if let Ok(bytes) = part.bytes().await {
                    if chosen && !bytes.is_empty() {
                        form.cover = Some(bytes);
                    }
                }
            }
            _ => {
                if let Ok(value) = part.text().await {
                    match name.as_str() {
                        "title" => form.title = value,
                        "slug" => form.slug = value,
                        "excerpt" => form.excerpt = value,
                        "cover_image_url" => form.cover_image_url = value,
                        "content_html" => form.content_html = value,
                        "status" => form.status = value,
                        "category_id" => form.category_id = value,
                        _ => continue,
                    }
                    form.fields.insert(name);
                }
            }
        }
    }
    form
}

/// Builds the API's partial-update payload value from a submitted form:
/// only the submitted keys, identifiers validated (goravel
/// `ValidateUpdatePost`'s `Fields` come from the submitted keys —
/// `ctx.Request().All()` — plus the `cover_image` file marker).
pub(super) fn submitted_value(form: &PostForm) -> std::result::Result<Value, InputError> {
    let mut object = serde_json::Map::new();
    let submitted = |key: &str| form.fields.contains(key);
    if submitted("title") {
        object.insert("title".to_owned(), serde_json::json!(form.title));
    }
    if submitted("slug") {
        object.insert("slug".to_owned(), serde_json::json!(form.slug));
    }
    if submitted("excerpt") {
        object.insert("excerpt".to_owned(), serde_json::json!(form.excerpt));
    }
    if submitted("cover_image_url") {
        object.insert(
            "cover_image_url".to_owned(),
            serde_json::json!(form.cover_image_url),
        );
    }
    if submitted("content_html") {
        object.insert(
            "content_html".to_owned(),
            serde_json::json!(form.content_html),
        );
    }
    if submitted("status") {
        object.insert("status".to_owned(), serde_json::json!(form.status));
    }
    if submitted("category_id") {
        let id = match form.category_id.trim() {
            "" => None,
            raw => match raw.parse::<i64>() {
                Ok(id) if id != 0 => Some(id),
                _ => {
                    return Err(InputError::Field(
                        "category_id",
                        "must be a valid category identifier",
                    ))
                }
            },
        };
        object.insert("category_id".to_owned(), serde_json::json!(id));
    }
    if submitted("tag_ids") {
        let mut tag_ids = Vec::new();
        for raw in &form.tag_ids {
            let raw = raw.trim();
            if raw.is_empty() {
                continue;
            }
            match raw.parse::<i64>() {
                Ok(id) if id != 0 => tag_ids.push(id),
                _ => {
                    return Err(InputError::Field(
                        "tag_ids",
                        "must contain unique valid tag identifiers",
                    ))
                }
            }
        }
        object.insert("tag_ids".to_owned(), serde_json::json!(tag_ids));
    }
    Ok(Value::Object(object))
}

/// Validates the form into the API's `PostInput` (goravel `viewPostInput` +
/// `validateViewCreatePost`/`validateViewUpdatePost`): identifiers are
/// checked here, the remaining rules (lengths, status, URL, tag limits) by
/// `PostInput::parse`. Updates always submit every field, like goravel's
/// controller-side field map.
fn form_input(form: &PostForm, create: bool) -> std::result::Result<PostInput, InputError> {
    let category_id = match form.category_id.trim() {
        "" => None,
        raw => match raw.parse::<i64>() {
            Ok(id) if id != 0 => Some(id),
            _ => {
                return Err(InputError::Field(
                    "category_id",
                    "must be a valid category identifier",
                ))
            }
        },
    };
    let mut tag_ids = Vec::new();
    for raw in &form.tag_ids {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        match raw.parse::<i64>() {
            Ok(id) if id != 0 => tag_ids.push(id),
            _ => {
                return Err(InputError::Field(
                    "tag_ids",
                    "must contain unique valid tag identifiers",
                ))
            }
        }
    }
    PostInput::parse(
        serde_json::json!({
            "title": form.title,
            "slug": form.slug,
            "excerpt": form.excerpt,
            "cover_image_url": form.cover_image_url,
            "content_html": form.content_html,
            "status": form.status,
            "category_id": category_id,
            "tag_ids": tag_ids,
        }),
        create,
    )
}

/// goravel `firstErrors`/`serviceErrors`: the field → message map rendered
/// back into the form.
fn error_map(field: &str, message: &str) -> Value {
    serde_json::json!({field: message})
}

fn input_errors(error: &InputError) -> Value {
    match error {
        InputError::Field(field, message) => error_map(field, &format!("{field} {message}")),
        InputError::Payload => error_map("form", "Invalid form payload"),
    }
}

fn write_errors(error: &author::WriteError) -> Value {
    match error {
        author::WriteError::Missing => error_map("form", "blog resource not found"),
        author::WriteError::SlugConflict => error_map("slug", "slug already exists"),
        author::WriteError::Relations => {
            error_map("relationships", "category or tag does not exist")
        }
        author::WriteError::Database(error) => {
            tracing::error!(%error, "blog write failed");
            error_map("form", "Unable to process blog request")
        }
    }
}

/// goravel's flashed service error for a failed taxonomy write (constraint
/// text when classified, otherwise the generic message after logging).
fn taxonomy_failure(error: &DbErr, action: &str) -> String {
    taxonomy::error_message(error).map_or_else(
        || {
            tracing::error!(%error, action);
            "Unable to process blog request".to_owned()
        },
        str::to_owned,
    )
}

/// Renders the shared post form (goravel `renderPostForm`). `checked` marks
/// the tag checkboxes; goravel's validation re-render leaves them unchecked
/// (`formFromRequest`), so callers pass `&[]` there for parity.
#[allow(clippy::too_many_arguments)]
async fn render_post_form(
    v: &TeraView,
    ctx: &AppContext,
    jar: &cookie::CookieJar,
    user: &users::Model,
    title: &str,
    action: &str,
    form: &PostForm,
    checked: &[i64],
    errors: Value,
    editing: bool,
) -> Result<Response> {
    let categories = categories::Entity::find()
        .order_by_asc(categories::Column::Name)
        .all(&ctx.db)
        .await?;
    let tags = tags::Entity::find()
        .order_by_asc(tags::Column::Name)
        .all(&ctx.db)
        .await?;
    let form_tags: Vec<Value> = tags
        .iter()
        .map(|tag| {
            serde_json::json!({"id": tag.id, "name": tag.name, "checked": checked.contains(&tag.id)})
        })
        .collect();
    let status = if form.status.is_empty() {
        "draft"
    } else {
        form.status.as_str()
    };
    render_flash(
        v,
        "blog/dashboard_post_form.html",
        data!({
            "title": title,
            "current_user": {"name": user.name},
            "action": action,
            "form": {
                "title": form.title,
                "slug": form.slug,
                "excerpt": form.excerpt,
                "cover_image_url": form.cover_image_url,
                "content_html": form.content_html,
                "status": status,
                "category_id": form.category_id.parse::<i64>().unwrap_or(0),
            },
            "form_tags": form_tags,
            "categories": categories,
            "errors": errors,
            "editing": editing,
        }),
        jar,
    )
    .await
}

/// Creates (or updates `update_id`) a post from the form as `author_id`,
/// uploading an optional cover first (goravel `CreatePost`/`UpdatePost`).
/// `Ok(id)` is the saved post id; `Err(errors)` re-renders the form.
async fn save_post(
    ctx: &AppContext,
    headers: &HeaderMap,
    author_id: i64,
    form: &PostForm,
    update_id: Option<i64>,
) -> std::result::Result<i64, Value> {
    let mut input = form_input(form, update_id.is_none()).map_err(|error| input_errors(&error))?;
    if let Some(bytes) = &form.cover {
        match store_image(ctx, headers, "uploads/post-covers", "cover_image", bytes).await {
            Ok((url, _)) => input.cover_image_url = url,
            Err(_) => {
                return Err(error_map(
                    "cover_image",
                    "Cover image must be a jpeg, png, gif, or webp file up to 5 MB.",
                ))
            }
        }
    }
    let result = match update_id {
        None => author::create_post(&ctx.db, author_id, &input).await,
        Some(id) => author::update_post(&ctx.db, author_id, id, &input).await,
    };
    match result {
        Ok(row) => Ok(row.id),
        Err(error) => Err(write_errors(&error)),
    }
}

/// `GET /dashboard/posts` — the author's posts with a status filter (goravel
/// `Dashboard`/`DashboardPosts`; `/dashboard` itself stays the frozen auth
/// module's post-login landing page — deliberate loco-side deviation).
#[debug_handler]
pub(super) async fn posts(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: cookie::CookieJar,
    Query(query): Query<ListQuery>,
) -> Result<Response> {
    let Some(user) = current_web_user(&ctx, &jar).await else {
        return Ok(login_redirect());
    };
    let Ok((page, limit)) = query.pagination() else {
        return Ok(error_page(
            StatusCode::BAD_REQUEST,
            "Invalid dashboard query.",
        ));
    };
    let status = query.status.as_deref().map(str::trim).unwrap_or_default();
    let (rows, _total) = own_posts(&ctx.db, user.id, Some(status), page, limit).await?;
    render_flash(
        &v,
        "blog/dashboard_posts.html",
        data!({
            "title": "Dashboard posts",
            "current_user": {"name": user.name},
            "posts": rows,
            "status": status,
        }),
        &jar,
    )
    .await
}

/// `GET /dashboard/posts/new` — an empty post form (status defaults to
/// draft, goravel `NewPost`).
#[debug_handler]
pub(super) async fn new_post(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: cookie::CookieJar,
) -> Result<Response> {
    let Some(user) = current_web_user(&ctx, &jar).await else {
        return Ok(login_redirect());
    };
    render_post_form(
        &v,
        &ctx,
        &jar,
        &user,
        "New post",
        "/dashboard/posts",
        &PostForm::default(),
        &[],
        serde_json::json!({}),
        false,
    )
    .await
}

/// `POST /dashboard/posts` — create from the form; validation and service
/// errors re-render the form, success flashes and redirects to the edit
/// page (goravel `CreatePost`).
#[debug_handler]
pub(super) async fn create_post(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    headers: HeaderMap,
    jar: cookie::CookieJar,
    mut multipart: Multipart,
) -> Result<Response> {
    let Some(user) = current_web_user(&ctx, &jar).await else {
        return Ok(login_redirect());
    };
    let form = post_form(&mut multipart).await;
    match save_post(&ctx, &headers, user.id, &form, None).await {
        Ok(id) => Ok(redirect_flash(
            &format!("/dashboard/posts/{id}/edit"),
            Some("Post created successfully."),
            None,
        )),
        Err(errors) => {
            render_post_form(
                &v,
                &ctx,
                &jar,
                &user,
                "New post",
                "/dashboard/posts",
                &form,
                &[],
                errors,
                false,
            )
            .await
        }
    }
}

/// `GET /dashboard/posts/{id}/edit` — the form loaded from an owned post;
/// anyone else's post renders the 404 error page (goravel `EditPost`).
#[debug_handler]
pub(super) async fn edit_post(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: cookie::CookieJar,
    Path(id): Path<String>,
) -> Result<Response> {
    let Some(user) = current_web_user(&ctx, &jar).await else {
        return Ok(login_redirect());
    };
    let Ok(row) = author::owned(&ctx.db, user.id, identifier(&id), false).await else {
        return Ok(error_page(StatusCode::NOT_FOUND, "Resource not found"));
    };
    let response = controllers::public_post(post::Model::load_relations(&ctx.db, row).await?);
    let form = PostForm {
        title: response.title.clone(),
        slug: response.slug.clone(),
        excerpt: response.excerpt.clone().unwrap_or_default(),
        cover_image_url: response.cover_image_url.clone().unwrap_or_default(),
        content_html: response.content_html.clone(),
        status: response.status.clone(),
        category_id: response
            .category
            .as_ref()
            .map(|category| category.id.to_string())
            .unwrap_or_default(),
        tag_ids: response.tags.iter().map(|tag| tag.id.to_string()).collect(),
        cover: None,
        fields: std::collections::HashSet::default(),
    };
    let checked: Vec<i64> = response.tags.iter().map(|tag| tag.id).collect();
    render_post_form(
        &v,
        &ctx,
        &jar,
        &user,
        "Edit post",
        &format!("/dashboard/posts/{}", response.id),
        &form,
        &checked,
        serde_json::json!({}),
        true,
    )
    .await
}

/// `POST /dashboard/posts/{id}` — update from the form; validation and
/// service errors re-render the form, success flashes and redirects to the
/// edit page (goravel `UpdatePost`).
#[debug_handler]
pub(super) async fn update_post(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    headers: HeaderMap,
    jar: cookie::CookieJar,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> Result<Response> {
    let Some(user) = current_web_user(&ctx, &jar).await else {
        return Ok(login_redirect());
    };
    let form = post_form(&mut multipart).await;
    match save_post(&ctx, &headers, user.id, &form, Some(identifier(&id))).await {
        Ok(id) => Ok(redirect_flash(
            &format!("/dashboard/posts/{id}/edit"),
            Some("Post updated successfully."),
            None,
        )),
        Err(errors) => {
            render_post_form(
                &v,
                &ctx,
                &jar,
                &user,
                "Edit post",
                &format!("/dashboard/posts/{id}"),
                &form,
                &[],
                errors,
                true,
            )
            .await
        }
    }
}

/// `POST /dashboard/posts/{id}/delete` — delete an owned post, then flash
/// and redirect to the list (goravel `DeletePost`).
#[debug_handler]
pub(super) async fn delete_post(
    State(ctx): State<AppContext>,
    jar: cookie::CookieJar,
    Path(id): Path<String>,
) -> Result<Response> {
    let Some(user) = current_web_user(&ctx, &jar).await else {
        return Ok(login_redirect());
    };
    let (flash, error) = match posts::Entity::delete_many()
        .filter(posts::Column::Id.eq(identifier(&id)))
        .filter(posts::Column::AuthorId.eq(user.id))
        .exec(&ctx.db)
        .await
    {
        Ok(deleted) if deleted.rows_affected > 0 => (Some("Post deleted successfully."), None),
        Ok(_) => (None, Some("Unable to delete post.")),
        Err(error) => {
            tracing::error!(%error, "dashboard post delete failed");
            (None, Some("Unable to delete post."))
        }
    };
    Ok(redirect_flash("/dashboard/posts", flash, error))
}

/// `GET /dashboard/categories` and `GET /dashboard/tags` — the taxonomy
/// management page (goravel `DashboardCategories`/`DashboardTags`).
async fn render_taxonomy(
    v: &TeraView,
    ctx: &AppContext,
    jar: &cookie::CookieJar,
    user: &users::Model,
    kind: taxonomy::Kind,
) -> Result<Response> {
    let (title, action, items): (&str, &str, Vec<Value>) = match kind {
        taxonomy::Kind::Category => {
            let rows = categories::Entity::find()
                .order_by_asc(categories::Column::Name)
                .all(&ctx.db)
                .await?;
            (
                "Categories",
                "/dashboard/categories",
                rows.into_iter()
                    .map(|row| serde_json::to_value(row).unwrap_or_default())
                    .collect(),
            )
        }
        taxonomy::Kind::Tag => {
            let rows = tags::Entity::find()
                .order_by_asc(tags::Column::Name)
                .all(&ctx.db)
                .await?;
            (
                "Tags",
                "/dashboard/tags",
                rows.into_iter()
                    .map(|row| serde_json::to_value(row).unwrap_or_default())
                    .collect(),
            )
        }
    };
    render_flash(
        v,
        "blog/dashboard_taxonomy.html",
        data!({
            "title": title,
            "current_user": {"name": user.name},
            "items": items,
            "is_categories": matches!(kind, taxonomy::Kind::Category),
            "action": action,
        }),
        jar,
    )
    .await
}

#[debug_handler]
pub(super) async fn categories(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: cookie::CookieJar,
) -> Result<Response> {
    let Some(user) = current_web_user(&ctx, &jar).await else {
        return Ok(login_redirect());
    };
    render_taxonomy(&v, &ctx, &jar, &user, taxonomy::Kind::Category).await
}

#[debug_handler]
pub(super) async fn tags(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: cookie::CookieJar,
) -> Result<Response> {
    let Some(user) = current_web_user(&ctx, &jar).await else {
        return Ok(login_redirect());
    };
    render_taxonomy(&v, &ctx, &jar, &user, taxonomy::Kind::Tag).await
}

/// `POST /dashboard/{categories,tags}` and `POST /dashboard/{categories,tags}/{id}`
/// — create or rename (goravel `saveTaxonomy`; flashes goravel's messages).
async fn save_taxonomy(
    ctx: &AppContext,
    kind: taxonomy::Kind,
    id: Option<i64>,
    form: TaxonomyForm,
) -> Response {
    let path = match kind {
        taxonomy::Kind::Category => "/dashboard/categories",
        taxonomy::Kind::Tag => "/dashboard/tags",
    };
    let name = form.name.unwrap_or_default();
    let (flash, error) = match TaxonomyInput::parse(serde_json::json!({"name": name})) {
        Err(_) => (
            None,
            Some("Name is required and must be 100 characters or less.".to_owned()),
        ),
        Ok(input) => match taxonomy::save(&ctx.db, kind, id, input.name).await {
            Ok(Some(_)) => (Some("Taxonomy saved.".to_owned()), None),
            Ok(None) => (None, Some("blog resource not found".to_owned())),
            Err(error) => (
                None,
                Some(taxonomy_failure(&error, "dashboard taxonomy save failed")),
            ),
        },
    };
    redirect_flash(path, flash.as_deref(), error.as_deref())
}

#[debug_handler]
pub(super) async fn create_category(
    State(ctx): State<AppContext>,
    jar: cookie::CookieJar,
    form: axum::Form<TaxonomyForm>,
) -> Result<Response> {
    if current_web_user(&ctx, &jar).await.is_none() {
        return Ok(login_redirect());
    }
    Ok(save_taxonomy(&ctx, taxonomy::Kind::Category, None, form.0).await)
}

#[debug_handler]
pub(super) async fn update_category(
    State(ctx): State<AppContext>,
    jar: cookie::CookieJar,
    Path(id): Path<String>,
    form: axum::Form<TaxonomyForm>,
) -> Result<Response> {
    if current_web_user(&ctx, &jar).await.is_none() {
        return Ok(login_redirect());
    }
    Ok(save_taxonomy(
        &ctx,
        taxonomy::Kind::Category,
        Some(identifier(&id)),
        form.0,
    )
    .await)
}

#[debug_handler]
pub(super) async fn delete_category(
    State(ctx): State<AppContext>,
    jar: cookie::CookieJar,
    Path(id): Path<String>,
) -> Result<Response> {
    delete_taxonomy(&ctx, &jar, taxonomy::Kind::Category, identifier(&id)).await
}

#[debug_handler]
pub(super) async fn create_tag(
    State(ctx): State<AppContext>,
    jar: cookie::CookieJar,
    form: axum::Form<TaxonomyForm>,
) -> Result<Response> {
    if current_web_user(&ctx, &jar).await.is_none() {
        return Ok(login_redirect());
    }
    Ok(save_taxonomy(&ctx, taxonomy::Kind::Tag, None, form.0).await)
}

#[debug_handler]
pub(super) async fn update_tag(
    State(ctx): State<AppContext>,
    jar: cookie::CookieJar,
    Path(id): Path<String>,
    form: axum::Form<TaxonomyForm>,
) -> Result<Response> {
    if current_web_user(&ctx, &jar).await.is_none() {
        return Ok(login_redirect());
    }
    Ok(save_taxonomy(&ctx, taxonomy::Kind::Tag, Some(identifier(&id)), form.0).await)
}

#[debug_handler]
pub(super) async fn delete_tag(
    State(ctx): State<AppContext>,
    jar: cookie::CookieJar,
    Path(id): Path<String>,
) -> Result<Response> {
    delete_taxonomy(&ctx, &jar, taxonomy::Kind::Tag, identifier(&id)).await
}

/// `POST /dashboard/{categories,tags}/{id}/delete` — goravel
/// `deleteTaxonomy`; constraint failures flash their message.
async fn delete_taxonomy(
    ctx: &AppContext,
    jar: &cookie::CookieJar,
    kind: taxonomy::Kind,
    id: i64,
) -> Result<Response> {
    if current_web_user(ctx, jar).await.is_none() {
        return Ok(login_redirect());
    }
    let path = match kind {
        taxonomy::Kind::Category => "/dashboard/categories",
        taxonomy::Kind::Tag => "/dashboard/tags",
    };
    let (flash, error) = match taxonomy::remove(&ctx.db, kind, id).await {
        Ok(true) => (Some("Taxonomy deleted.".to_owned()), None),
        Ok(false) => (None, Some("blog resource not found".to_owned())),
        Err(error) => (
            None,
            Some(taxonomy_failure(&error, "dashboard taxonomy delete failed")),
        ),
    };
    Ok(redirect_flash(path, flash.as_deref(), error.as_deref()))
}

/// The selected post's moderation rows: `None` = the post is not the
/// caller's (goravel `Moderation`'s 404 branch); no selection = empty rows.
async fn moderation_rows(
    db: &DatabaseConnection,
    author_id: i64,
    selected: Option<i64>,
    status: &str,
    page: u64,
) -> Result<Option<Vec<super::models::responses::CommentResponse>>> {
    let Some(post_id) = selected else {
        return Ok(Some(Vec::new()));
    };
    if controllers::owned_post(db, author_id, post_id)
        .await?
        .is_none()
    {
        return Ok(None);
    }
    let (rows, _total) =
        controllers::moderation_comments(db, post_id, Some(status), page, 50).await?;
    Ok(Some(rows))
}

/// `GET /dashboard/comments` — the moderation queue: the author's posts as
/// a selector, the selected post's comments with approve/reject/delete
/// actions (goravel `Moderation`).
#[debug_handler]
pub(super) async fn moderation(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: cookie::CookieJar,
    Query(query): Query<ModerationQuery>,
) -> Result<Response> {
    let Some(user) = current_web_user(&ctx, &jar).await else {
        return Ok(login_redirect());
    };
    let (rows, _total) = own_posts(&ctx.db, user.id, None, 1, 100).await?;
    let requested = query_integer(query.post_id.as_deref(), 0);
    let selected = if requested > 0 {
        Some(requested)
    } else {
        rows.first().and_then(|post| post["id"].as_i64())
    };
    let status = query.status.as_deref().map(str::trim).unwrap_or_default();
    let page = query_integer(query.page.as_deref(), 1)
        .max(1)
        .cast_unsigned();
    let Some(comment_rows) = moderation_rows(&ctx.db, user.id, selected, status, page).await?
    else {
        return Ok(error_page(StatusCode::NOT_FOUND, "Resource not found"));
    };
    render_flash(
        &v,
        "blog/dashboard_comments.html",
        data!({
            "title": "Moderate comments",
            "current_user": {"name": user.name},
            "posts": rows,
            "selected_post_id": selected.unwrap_or(0),
            "status": status,
            "comments": comment_rows,
        }),
        &jar,
    )
    .await
}

/// `POST /dashboard/posts/{id}/comments/{comment_id}` — approve or reject a
/// comment on an owned post (goravel `ModerateComment`).
#[debug_handler]
pub(super) async fn moderate_comment(
    State(ctx): State<AppContext>,
    jar: cookie::CookieJar,
    Path((id, comment_id)): Path<(String, String)>,
    form: axum::Form<ModerationForm>,
) -> Result<Response> {
    let Some(user) = current_web_user(&ctx, &jar).await else {
        return Ok(login_redirect());
    };
    let post_id = identifier(&id);
    let back = format!("/dashboard/comments?post_id={post_id}");
    let status = form.0.status.unwrap_or_default();
    if !matches!(status.as_str(), "approved" | "rejected") {
        return Ok(redirect_flash(
            &back,
            None,
            Some("Moderation status must be approved or rejected."),
        ));
    }
    let (flash, error) =
        match moderate(&ctx.db, user.id, post_id, identifier(&comment_id), &status).await {
            Ok(true) => (Some("Comment moderation updated."), None),
            Ok(false) => (None, Some("Unable to update comment.")),
            Err(error) => {
                tracing::error!(%error, "dashboard moderation failed");
                (None, Some("Unable to update comment."))
            }
        };
    Ok(redirect_flash(&back, flash, error))
}

/// Sets a comment's status on an owned post: `Ok(true)` = updated, `Ok(false)`
/// = post not owned or comment not on it, `Err` = database failure.
async fn moderate(
    db: &DatabaseConnection,
    author_id: i64,
    post_id: i64,
    comment_id: i64,
    status: &str,
) -> std::result::Result<bool, DbErr> {
    if controllers::owned_post(db, author_id, post_id)
        .await?
        .is_none()
    {
        return Ok(false);
    }
    let Some(row) = comments::Entity::find_by_id(comment_id)
        .filter(comments::Column::PostId.eq(post_id))
        .one(db)
        .await?
    else {
        return Ok(false);
    };
    let mut active = row.into_active_model();
    active.status = Set(status.to_owned());
    active.update(db).await?;
    Ok(true)
}

/// `POST /dashboard/posts/{id}/comments/{comment_id}/delete` — remove a
/// comment from an owned post (goravel `DeleteModeratedComment`).
#[debug_handler]
pub(super) async fn delete_comment(
    State(ctx): State<AppContext>,
    jar: cookie::CookieJar,
    Path((id, comment_id)): Path<(String, String)>,
) -> Result<Response> {
    let Some(user) = current_web_user(&ctx, &jar).await else {
        return Ok(login_redirect());
    };
    let post_id = identifier(&id);
    let back = format!("/dashboard/comments?post_id={post_id}");
    let (flash, error) =
        match delete_moderated(&ctx.db, user.id, post_id, identifier(&comment_id)).await {
            Ok(true) => (Some("Comment deleted."), None),
            Ok(false) => (None, Some("Unable to delete comment.")),
            Err(error) => {
                tracing::error!(%error, "dashboard comment delete failed");
                (None, Some("Unable to delete comment."))
            }
        };
    Ok(redirect_flash(&back, flash, error))
}

/// Deletes a comment from an owned post: `Ok(true)` = removed, `Ok(false)` =
/// post not owned or comment not on it, `Err` = database failure.
async fn delete_moderated(
    db: &DatabaseConnection,
    author_id: i64,
    post_id: i64,
    comment_id: i64,
) -> std::result::Result<bool, DbErr> {
    if controllers::owned_post(db, author_id, post_id)
        .await?
        .is_none()
    {
        return Ok(false);
    }
    let deleted = comments::Entity::delete_many()
        .filter(comments::Column::Id.eq(comment_id))
        .filter(comments::Column::PostId.eq(post_id))
        .exec(db)
        .await?;
    Ok(deleted.rows_affected > 0)
}
