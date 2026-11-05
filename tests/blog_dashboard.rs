//! S10 — web dashboard + web comment submit + `/storage` statics.
//!
//! Behavioral reference: `goravel_base/app/modules/blog`'s
//! `RegisterWebRoutes` dashboard group and `SubmitComment` — cookie-session
//! guard (302 `/login`), flash feedback cookies, form re-renders (200) with
//! field errors, redirects to the edit page/list, and goravel's fixed
//! flash/error messages.

use loco_laravel::{app::App, modules::blog::seeders};
use loco_rs::prelude::*;
use serde_json::Value;
use serial_test::serial;

const BOUNDARY: &str = "----locolaraveltestboundary";

async fn seed(ctx: &AppContext) {
    seeders::seed(ctx, std::path::Path::new("src/fixtures"))
        .await
        .unwrap();
}

async fn login(request: &loco_rs::TestServer, email: &str) -> String {
    let response = request
        .post("/api/auth/login")
        .json(&serde_json::json!({"email": email, "password": "Password123!"}))
        .await;
    response.assert_status_ok();
    let body: Value = response.json();
    body["token"].as_str().unwrap().to_owned()
}

/// The `set-cookie` pairs of a response (each truncated at its first `;`).
fn cookie_pairs(headers: &axum::http::HeaderMap) -> Vec<String> {
    headers
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| value.split(';').next().map(str::to_owned))
        .collect()
}

fn cookie_value(pairs: &[String], name: &str) -> Option<String> {
    pairs
        .iter()
        .find(|pair| pair.starts_with(&format!("{name}=")))
        .map(|pair| pair[name.len() + 1..].to_owned())
}

/// Cookie header value joining the auth cookie with any flash cookies.
fn cookie_header(token: Option<&str>, pairs: &[String]) -> String {
    let mut parts: Vec<String> = pairs.to_vec();
    if let Some(token) = token {
        parts.push(format!("auth_token={token}"));
    }
    parts.join("; ")
}

async fn get(
    request: &loco_rs::TestServer,
    path: &str,
    token: Option<&str>,
    cookies: &[String],
) -> (u16, String, Vec<String>) {
    let response = request
        .get(path)
        .add_header("cookie", cookie_header(token, cookies))
        .await;
    (
        response.status_code().as_u16(),
        response.text(),
        cookie_pairs(response.headers()),
    )
}

/// A multipart post-form body from text fields plus an optional file part.
fn multipart_form(fields: &[(&str, &str)], file: Option<(&str, &str, &[u8])>) -> (String, Vec<u8>) {
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\ncontent-disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
            )
            .as_bytes(),
        );
    }
    if let Some((name, filename, content)) = file {
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\ncontent-disposition: form-data; name=\"{name}\"; \
                 filename=\"{filename}\"\r\ncontent-type: application/octet-stream\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(content);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={BOUNDARY}"), body)
}

/// A urlencoded form body.
fn urlencoded(fields: &[(&str, &str)]) -> (String, Vec<u8>) {
    let body = fields
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("&");
    (
        "application/x-www-form-urlencoded".to_owned(),
        body.into_bytes(),
    )
}

async fn post(
    request: &loco_rs::TestServer,
    path: &str,
    token: Option<&str>,
    cookies: &[String],
    content_type: &str,
    body: Vec<u8>,
) -> (u16, String, Vec<String>) {
    let response = request
        .post(path)
        .add_header("cookie", cookie_header(token, cookies))
        .content_type(content_type)
        .bytes(body.into())
        .await;
    (
        response.status_code().as_u16(),
        response.text(),
        cookie_pairs(response.headers()),
    )
}

async fn own_post_id(request: &loco_rs::TestServer, token: &str, slug: &str) -> i64 {
    let response = request
        .get("/api/me/posts?limit=100")
        .authorization_bearer(token)
        .await;
    response.assert_status_ok();
    let body: Value = response.json();
    body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|post| post["slug"] == slug)
        .map(|post| post["id"].as_i64().unwrap())
        .unwrap()
}

fn png() -> Vec<u8> {
    let mut bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(b"dashboard-cover-payload");
    bytes
}

#[tokio::test]
#[serial]
async fn dashboard_requires_login() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        for path in [
            "/dashboard/posts",
            "/dashboard/posts/new",
            "/dashboard/categories",
            "/dashboard/tags",
            "/dashboard/comments",
        ] {
            let (status, _, headers) = get(&request, path, None, &[]).await;
            assert_eq!(status, 302, "{path}");
            assert!(
                headers.is_empty() || !headers.iter().any(|c| c.starts_with("blog_")),
                "{path} flashed without a session"
            );
        }
        // Guarded form posts bounce anonymous callers too.
        let (content_type, body) = urlencoded(&[("name", "Sneaky")]);
        let (status, _, _) = post(
            &request,
            "/dashboard/categories",
            None,
            &[],
            &content_type,
            body,
        )
        .await;
        assert_eq!(status, 302);

        let (content_type, body) = urlencoded(&[("content", "hi")]);
        let (status, _, _) = post(
            &request,
            "/posts/getting-started-with-goravel/comments",
            None,
            &[],
            &content_type,
            body,
        )
        .await;
        assert_eq!(status, 302);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn dashboard_posts_list_and_status_filter() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let token = login(&request, "author@example.com").await;

        // Both seeded posts (published + draft) are listed for their author.
        let (status, text, _) = get(&request, "/dashboard/posts", Some(&token), &[]).await;
        assert_eq!(status, 200);
        assert!(text.contains("Dashboard posts"), "{text}");
        assert!(text.contains("Getting Started with Goravel"), "{text}");
        assert!(text.contains("Draft API Testing Checklist"), "{text}");

        // Status filter narrows to the drafts.
        let (status, text, _) =
            get(&request, "/dashboard/posts?status=draft", Some(&token), &[]).await;
        assert_eq!(status, 200);
        assert!(text.contains("Draft API Testing Checklist"), "{text}");
        assert!(!text.contains("Getting Started with Goravel"), "{text}");

        // Invalid pagination renders the minimal 400 error page.
        let (status, text, _) = get(&request, "/dashboard/posts?limit=0", Some(&token), &[]).await;
        assert_eq!(status, 400, "{text}");
        assert!(text.contains("Invalid dashboard query."), "{text}");

        // Readers with no posts see the empty state.
        let token = login(&request, "reader@example.com").await;
        let (status, text, _) = get(&request, "/dashboard/posts", Some(&token), &[]).await;
        assert_eq!(status, 200);
        assert!(text.contains("No posts yet."), "{text}");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn dashboard_create_post_with_cover_and_flash() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let token = login(&request, "author@example.com").await;

        // The empty form renders with the shared controls.
        let (status, text, _) = get(&request, "/dashboard/posts/new", Some(&token), &[]).await;
        assert_eq!(status, 200);
        assert!(text.contains("New post"), "{text}");
        assert!(text.contains("action=\"/dashboard/posts\""), "{text}");
        assert!(text.contains("name=\"tag_ids\""), "{text}");

        // Create with a cover upload: redirect to the edit page + flash.
        let (content_type, body) = multipart_form(
            &[
                ("title", "Dashboard Created Post"),
                ("slug", "dashboard-created-post"),
                ("excerpt", "Born in the dashboard form."),
                ("content_html", "<p>Hello from the dashboard.</p>"),
                ("status", "published"),
            ],
            Some(("cover_image", "cover.png", &png())),
        );
        let (status, _, cookies) = post(
            &request,
            "/dashboard/posts",
            Some(&token),
            &[],
            &content_type,
            body,
        )
        .await;
        assert_eq!(status, 302);
        assert_eq!(
            cookie_value(&cookies, "blog_flash").as_deref(),
            Some("Post created successfully.")
        );

        // The edit page renders the new post with the flash, then clears it.
        let id = own_post_id(&request, &token, "dashboard-created-post").await;
        let (status, text, cookies) = get(
            &request,
            &format!("/dashboard/posts/{id}/edit"),
            Some(&token),
            &cookies,
        )
        .await;
        assert_eq!(status, 200);
        assert!(text.contains("Edit post"), "{text}");
        assert!(text.contains("Post created successfully."), "{text}");
        assert!(text.contains("Dashboard Created Post"), "{text}");
        assert!(!cookies.is_empty(), "flash cookie must expire on render");

        let (_, text, _) = get(
            &request,
            &format!("/dashboard/posts/{id}/edit"),
            Some(&token),
            &[],
        )
        .await;
        assert!(!text.contains("Post created successfully."));

        // The post exists with the cover stored under /storage.
        let response = request
            .get("/api/me/posts?limit=100")
            .authorization_bearer(&token)
            .await;
        let posts: Value = response.json();
        let created = posts["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|post| post["slug"] == "dashboard-created-post")
            .unwrap();
        let cover = created["cover_image_url"].as_str().unwrap();
        assert!(cover.contains("/storage/uploads/post-covers/"), "{cover}");
        let path = cover.split("/storage/").nth(1).unwrap();
        let (status, _, headers) = get(&request, &format!("/storage/{path}"), None, &[]).await;
        assert_eq!(status, 200, "cover must be served");
        assert_eq!(
            headers
                .iter()
                .filter(|c| c.starts_with("content-type"))
                .count(),
            0,
            "content-type lives on the response headers, not cookies"
        );
        std::fs::remove_file(format!("storage/app/public/{path}")).ok();
    })
    .await;
}

#[tokio::test]
#[serial]
async fn dashboard_create_post_validation_and_conflicts() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let token = login(&request, "author@example.com").await;

        // Missing title re-renders the form (200) with the field error.
        let (content_type, body) = multipart_form(
            &[("content_html", "<p>no title</p>"), ("status", "draft")],
            None,
        );
        let (status, text, _) = post(
            &request,
            "/dashboard/posts",
            Some(&token),
            &[],
            &content_type,
            body,
        )
        .await;
        assert_eq!(status, 200, "validation re-renders, not 422");
        assert!(text.contains("New post"), "{text}");
        assert!(text.contains("title is required"), "{text}");

        // A duplicate slug surfaces the service error inline.
        let (content_type, body) = multipart_form(
            &[
                ("title", "Clashing Post"),
                ("slug", "getting-started-with-goravel"),
                ("content_html", "<p>clash</p>"),
                ("status", "draft"),
            ],
            None,
        );
        let (status, text, _) = post(
            &request,
            "/dashboard/posts",
            Some(&token),
            &[],
            &content_type,
            body,
        )
        .await;
        assert_eq!(status, 200);
        assert!(text.contains("slug already exists"), "{text}");

        // A non-numeric category id is a field error.
        let (content_type, body) = multipart_form(
            &[
                ("title", "Bad Category"),
                ("content_html", "<p>x</p>"),
                ("status", "draft"),
                ("category_id", "not-a-number"),
            ],
            None,
        );
        let (status, text, _) = post(
            &request,
            "/dashboard/posts",
            Some(&token),
            &[],
            &content_type,
            body,
        )
        .await;
        assert_eq!(status, 200);
        assert!(
            text.contains("category_id must be a valid category identifier"),
            "{text}"
        );

        // A bogus cover upload shows goravel's fixed message.
        let (content_type, body) = multipart_form(
            &[
                ("title", "Bad Cover"),
                ("content_html", "<p>x</p>"),
                ("status", "draft"),
            ],
            Some(("cover_image", "cover.txt", b"not-an-image")),
        );
        let (status, text, _) = post(
            &request,
            "/dashboard/posts",
            Some(&token),
            &[],
            &content_type,
            body,
        )
        .await;
        assert_eq!(status, 200);
        assert!(
            text.contains("Cover image must be a jpeg, png, gif, or webp file up to 5 MB."),
            "{text}"
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn dashboard_edit_update_delete_post() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let token = login(&request, "author@example.com").await;

        // Editing someone else's post renders the 404 error page.
        let foreign = own_post_id(
            &request,
            &login(&request, "admin@example.com").await,
            "nuxt-frontend-for-goravel-api",
        )
        .await;
        let (status, text, _) = get(
            &request,
            &format!("/dashboard/posts/{foreign}/edit"),
            Some(&token),
            &[],
        )
        .await;
        assert_eq!(status, 404, "{}", text);
        assert!(text.contains("Resource not found"), "{text}");

        // Update an owned post: title change sticks + success flash.
        let id = own_post_id(&request, &token, "getting-started-with-goravel").await;
        let (content_type, body) = multipart_form(
            &[
                ("title", "Getting Started with Goravel (v2)"),
                ("slug", "getting-started-with-goravel"),
                (
                    "excerpt",
                    "Build a small REST API with Goravel, migrations, validation, and services.",
                ),
                ("cover_image_url", ""),
                ("content_html", "<p>Updated body.</p>"),
                ("status", "published"),
            ],
            None,
        );
        let (status, _, cookies) = post(
            &request,
            &format!("/dashboard/posts/{id}"),
            Some(&token),
            &[],
            &content_type,
            body.clone(),
        )
        .await;
        assert_eq!(status, 302);
        assert_eq!(
            cookie_value(&cookies, "blog_flash").as_deref(),
            Some("Post updated successfully.")
        );
        let response = request
            .get("/api/me/posts?limit=100")
            .authorization_bearer(&token)
            .await;
        let posts: Value = response.json();
        assert!(posts["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|post| post["title"] == "Getting Started with Goravel (v2)"));

        // Updating an unknown id re-renders with goravel's form error.
        let (status, text, _) = post(
            &request,
            "/dashboard/posts/999999",
            Some(&token),
            &[],
            &content_type,
            body.clone(),
        )
        .await;
        assert_eq!(status, 200, "service error re-renders the form");
        assert!(text.contains("blog resource not found"), "{text}");

        // Delete: gone from the list, second delete flashes the failure.
        let (status, _, cookies) = post(
            &request,
            &format!("/dashboard/posts/{id}/delete"),
            Some(&token),
            &[],
            "application/x-www-form-urlencoded",
            Vec::new(),
        )
        .await;
        assert_eq!(status, 302);
        assert_eq!(
            cookie_value(&cookies, "blog_flash").as_deref(),
            Some("Post deleted successfully.")
        );
        let response = request
            .get("/api/me/posts?limit=100")
            .authorization_bearer(&token)
            .await;
        let posts: Value = response.json();
        assert!(!posts["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|post| post["slug"] == "getting-started-with-goravel"));
        let (_, _, cookies) = post(
            &request,
            &format!("/dashboard/posts/{id}/delete"),
            Some(&token),
            &[],
            "application/x-www-form-urlencoded",
            Vec::new(),
        )
        .await;
        assert_eq!(
            cookie_value(&cookies, "blog_error").as_deref(),
            Some("Unable to delete post.")
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn dashboard_taxonomy_flows() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let token = login(&request, "author@example.com").await;

        // The categories page lists the seeded taxonomy.
        let (status, text, _) = get(&request, "/dashboard/categories", Some(&token), &[]).await;
        assert_eq!(status, 200);
        assert!(text.contains("Categories"), "{text}");
        assert!(text.contains("Engineering"), "{text}");

        // Create: flash + the new row (with slugified name) appears.
        let (content_type, body) = urlencoded(&[("name", "Dashboard Category")]);
        let (status, _, cookies) = post(
            &request,
            "/dashboard/categories",
            Some(&token),
            &[],
            &content_type,
            body,
        )
        .await;
        assert_eq!(status, 302);
        assert_eq!(
            cookie_value(&cookies, "blog_flash").as_deref(),
            Some("Taxonomy saved.")
        );
        let (_, text, _) = get(&request, "/dashboard/categories", Some(&token), &cookies).await;
        assert!(text.contains("Dashboard Category"), "{text}");
        assert!(text.contains("dashboard-category"), "{text}");

        // Duplicate name flashes the slug conflict.
        let (content_type, body) = urlencoded(&[("name", "Engineering")]);
        let (_, _, cookies) = post(
            &request,
            "/dashboard/categories",
            Some(&token),
            &[],
            &content_type,
            body,
        )
        .await;
        assert_eq!(
            cookie_value(&cookies, "blog_error").as_deref(),
            Some("slug already exists")
        );

        // Empty name flashes goravel's fixed validation message.
        let (content_type, body) = urlencoded(&[("name", "")]);
        let (_, _, cookies) = post(
            &request,
            "/dashboard/categories",
            Some(&token),
            &[],
            &content_type,
            body,
        )
        .await;
        assert_eq!(
            cookie_value(&cookies, "blog_error").as_deref(),
            Some("Name is required and must be 100 characters or less.")
        );

        // Deleting a category assigned to a post flashes the RESTRICT error.
        let response = request.get("/api/categories").await;
        let categories: Value = response.json();
        let engineering = categories["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|category| category["slug"] == "engineering")
            .unwrap();
        let engineering_id = engineering["id"].as_i64().unwrap();
        let (_, _, cookies) = post(
            &request,
            &format!("/dashboard/categories/{engineering_id}/delete"),
            Some(&token),
            &[],
            "application/x-www-form-urlencoded",
            Vec::new(),
        )
        .await;
        assert_eq!(
            cookie_value(&cookies, "blog_error").as_deref(),
            Some("taxonomy is assigned to a post")
        );

        // Deleting the unused one succeeds; unknown ids flash not-found.
        let (content_type, body) = urlencoded(&[("name", "Lonely Tag")]);
        let (_, _, _) = post(
            &request,
            "/dashboard/tags",
            Some(&token),
            &[],
            &content_type,
            body,
        )
        .await;
        let response = request.get("/api/tags").await;
        let tags: Value = response.json();
        let lonely = tags["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tag| tag["slug"] == "lonely-tag")
            .unwrap();
        let lonely_id = lonely["id"].as_i64().unwrap();
        let (_, _, cookies) = post(
            &request,
            &format!("/dashboard/tags/{lonely_id}/delete"),
            Some(&token),
            &[],
            "application/x-www-form-urlencoded",
            Vec::new(),
        )
        .await;
        assert_eq!(
            cookie_value(&cookies, "blog_flash").as_deref(),
            Some("Taxonomy deleted.")
        );
        let (_, _, cookies) = post(
            &request,
            "/dashboard/tags/999999/delete",
            Some(&token),
            &[],
            "application/x-www-form-urlencoded",
            Vec::new(),
        )
        .await;
        assert_eq!(
            cookie_value(&cookies, "blog_error").as_deref(),
            Some("blog resource not found")
        );

        // Renaming through the row form.
        let (content_type, body) = urlencoded(&[("name", "Renamed Tag")]);
        let (_, _, cookies) = post(
            &request,
            &format!("/dashboard/tags/{lonely_id}"),
            Some(&token),
            &[],
            &content_type,
            body,
        )
        .await;
        assert_eq!(
            cookie_value(&cookies, "blog_error").as_deref(),
            Some("blog resource not found"),
            "renaming a deleted tag flashes not-found"
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn dashboard_moderation_flows() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let token = login(&request, "author@example.com").await;
        let post_id = own_post_id(&request, &token, "getting-started-with-goravel").await;

        // The queue for the selected post shows its comments (the default
        // selection is the newest own post — the draft — so query explicitly).
        let (status, text, _) = get(
            &request,
            &format!("/dashboard/comments?post_id={post_id}"),
            Some(&token),
            &[],
        )
        .await;
        assert_eq!(status, 200);
        assert!(text.contains("Moderate comments"), "{text}");
        assert!(
            text.contains("Could you add a follow-up about deployment"),
            "{text}"
        );

        // Someone else's post id renders the 404 error page.
        let foreign = own_post_id(
            &request,
            &login(&request, "admin@example.com").await,
            "nuxt-frontend-for-goravel-api",
        )
        .await;
        let (status, text, _) = get(
            &request,
            &format!("/dashboard/comments?post_id={foreign}"),
            Some(&token),
            &[],
        )
        .await;
        assert_eq!(status, 404, "{}", text);

        // The pending comment id comes from the API queue.
        let response = request
            .get(&format!("/api/me/posts/{post_id}/comments?status=pending"))
            .authorization_bearer(&token)
            .await;
        let queue: Value = response.json();
        let comment_id = queue["data"][0]["id"].as_i64().unwrap();

        // An invalid status flashes goravel's fixed message.
        let (content_type, body) = urlencoded(&[("status", "pending")]);
        let (_, _, cookies) = post(
            &request,
            &format!("/dashboard/posts/{post_id}/comments/{comment_id}"),
            Some(&token),
            &[],
            &content_type,
            body,
        )
        .await;
        assert_eq!(
            cookie_value(&cookies, "blog_error").as_deref(),
            Some("Moderation status must be approved or rejected.")
        );

        // Approve: the comment flips to approved and shows on the post page.
        let (content_type, body) = urlencoded(&[("status", "approved")]);
        let (status, _, cookies) = post(
            &request,
            &format!("/dashboard/posts/{post_id}/comments/{comment_id}"),
            Some(&token),
            &[],
            &content_type,
            body,
        )
        .await;
        assert_eq!(status, 302);
        assert_eq!(
            cookie_value(&cookies, "blog_flash").as_deref(),
            Some("Comment moderation updated.")
        );
        let (status, text, _) =
            get(&request, "/posts/getting-started-with-goravel", None, &[]).await;
        assert_eq!(status, 200);
        assert!(
            text.contains("Could you add a follow-up"),
            "approved comment must be public: {text}"
        );

        // Delete it from the moderated post.
        let (_, _, cookies) = post(
            &request,
            &format!("/dashboard/posts/{post_id}/comments/{comment_id}/delete"),
            Some(&token),
            &[],
            "application/x-www-form-urlencoded",
            Vec::new(),
        )
        .await;
        assert_eq!(
            cookie_value(&cookies, "blog_flash").as_deref(),
            Some("Comment deleted.")
        );
        let response = request
            .get(&format!("/api/me/posts/{post_id}/comments?status=approved"))
            .authorization_bearer(&token)
            .await;
        let queue: Value = response.json();
        assert!(!queue["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|comment| comment["id"].as_i64() == Some(comment_id)));
    })
    .await;
}

#[tokio::test]
#[serial]
async fn web_comment_submit_flow() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let reader = login(&request, "reader@example.com").await;

        // Empty content flashes goravel's fixed message on the post page.
        let (content_type, body) = urlencoded(&[("content", "")]);
        let (status, _, cookies) = post(
            &request,
            "/posts/getting-started-with-goravel/comments",
            Some(&reader),
            &[],
            &content_type,
            body,
        )
        .await;
        assert_eq!(status, 302);
        assert_eq!(
            cookie_value(&cookies, "blog_error").as_deref(),
            Some("Comment content is required and must be 2000 characters or less.")
        );
        let (status, text, _) = get(
            &request,
            "/posts/getting-started-with-goravel",
            Some(&reader),
            &cookies,
        )
        .await;
        assert_eq!(status, 200);
        assert!(
            text.contains("Comment content is required and must be 2000 characters or less."),
            "{text}"
        );

        // A valid submit queues a pending comment for the owner.
        let (content_type, body) = urlencoded(&[("content", "Web form comment for moderation.")]);
        let (status, _, cookies) = post(
            &request,
            "/posts/getting-started-with-goravel/comments",
            Some(&reader),
            &[],
            &content_type,
            body,
        )
        .await;
        assert_eq!(status, 302);
        assert_eq!(
            cookie_value(&cookies, "blog_flash").as_deref(),
            Some("Comment submitted for moderation.")
        );
        let author = login(&request, "author@example.com").await;
        let post_id = own_post_id(&request, &author, "getting-started-with-goravel").await;
        let response = request
            .get(&format!("/api/me/posts/{post_id}/comments?status=pending"))
            .authorization_bearer(&author)
            .await;
        let queue: Value = response.json();
        assert!(
            queue["data"]
                .as_array()
                .unwrap()
                .iter()
                .any(|comment| comment["content"] == "Web form comment for moderation."),
            "{queue}"
        );

        // Unknown slugs flash the generic failure.
        let (content_type, body) = urlencoded(&[("content", "anyone there?")]);
        let (_, _, cookies) = post(
            &request,
            "/posts/no-such-post/comments",
            Some(&reader),
            &[],
            &content_type,
            body,
        )
        .await;
        assert_eq!(
            cookie_value(&cookies, "blog_error").as_deref(),
            Some("Unable to submit comment.")
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn storage_serves_uploads_and_rejects_traversal() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let token = login(&request, "author@example.com").await;

        // Upload through the API, then fetch the same bytes from /storage.
        let mut bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(b"storage-static-payload");
        let mut body = Vec::new();
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\ncontent-disposition: form-data; name=\"image\"; \
                 filename=\"pixel.png\"\r\ncontent-type: image/png\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(&bytes);
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
        let response = request
            .post("/api/uploads/images")
            .authorization_bearer(&token)
            .content_type(&format!("multipart/form-data; boundary={BOUNDARY}"))
            .bytes(body.into())
            .await;
        response.assert_status(axum::http::StatusCode::CREATED);
        let upload: Value = response.json();
        let path = upload["path"].as_str().unwrap();

        let response = request.get(&format!("/storage/{path}")).await;
        response.assert_status_ok();
        assert!(response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("image/png")));
        std::fs::remove_file(format!("storage/app/public/{path}")).ok();

        // Missing files and traversal attempts are 404s.
        let response = request.get("/storage/uploads/images/nope.png").await;
        assert_eq!(response.status_code(), 404);
        let response = request.get("/storage/..%2FCargo.toml").await;
        assert_eq!(response.status_code(), 404);
    })
    .await;
}
