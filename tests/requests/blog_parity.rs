//! S11 — port of `goravel_base/tests/blog_test.go`
//! (`TestBlogTestSuite.TestPublishingModerationAndTaxonomyFlow`): the parity
//! contract for the blog API slices (S4–S8). One end-to-end flow with its own
//! users, asserting published-only reads, sanitization, pending comments and
//! moderation, taxonomy conflicts, and the cover upload paths — including the
//! multipart `POST /api/posts` body goravel's write endpoints accept
//! (`PostWriteRequest` binds `form` + `json` tags alike).
//!
//! Deviations from the Go original (established, deliberate):
//! - login goes through loco's `/api/auth/login` (token) instead of
//!   goravel's `/api/login` (access_token) — the frozen auth module's shape;
//! - users are created directly in the DB (same hash as Goravel's
//!   `UserSeeder`) because loco's register endpoint sends verification mail.

use loco_laravel::{
    app::App,
    models::_entities::{categories, posts, tags, users},
};
use loco_rs::prelude::*;
use sea_orm::ColumnTrait;
use sea_orm::EntityTrait;
use sea_orm::QueryFilter;
use serial_test::serial;

const AUTHOR_EMAIL: &str = "blog-author@example.com";
const READER_EMAIL: &str = "blog-reader@example.com";
const PASSWORD: &str = "correct-password";
const CATEGORY_SLUG: &str = "blog-test-engineering";
const TAG_SLUG: &str = "blog-test-go";
const BOUNDARY: &str = "----locolaraveltestboundary";

/// goravel's 1×1 PNG fixture (`writePNGPart`).
const PNG: [u8; 68] = [
    0x89, 0x50, 0x4e, 0x47, 0xd, 0xa, 0x1a, 0xa, 0x0, 0x0, 0x0, 0xd, 0x49, 0x48, 0x44, 0x52, 0x0,
    0x0, 0x0, 0x1, 0x0, 0x0, 0x0, 0x1, 0x8, 0x4, 0x0, 0x0, 0x0, 0xb5, 0x1c, 0xc, 0x2, 0x0, 0x0,
    0x0, 0xb, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xfc, 0xff, 0x1f, 0x0, 0x3, 0x3, 0x2, 0x0,
    0xef, 0xbf, 0xa7, 0xdb, 0x0, 0x0, 0x0, 0x0, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// A multipart body from text fields plus optional PNG file parts
/// (`imageUploadBody`/`postCreateBodyWithCover`).
fn multipart_body(fields: &[(&str, &str)], files: &[&str]) -> (String, Vec<u8>) {
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\ncontent-disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
            )
            .as_bytes(),
        );
    }
    for field in files {
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\ncontent-disposition: form-data; name=\"{field}\"; \
                 filename=\"fixture.png\"\r\ncontent-type: image/png\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(&PNG);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={BOUNDARY}"), body)
}

/// goravel's `cleanupBlogFixtures`: remove this suite's users and their
/// data (posts cascade comments and post_tags; the taxonomy goes once no
/// post references it).
async fn cleanup(ctx: &AppContext) {
    let owned = users::Entity::find()
        .filter(users::Column::Email.is_in([AUTHOR_EMAIL, READER_EMAIL]))
        .all(&ctx.db)
        .await
        .unwrap();
    for user in &owned {
        posts::Entity::delete_many()
            .filter(posts::Column::AuthorId.eq(user.id))
            .exec(&ctx.db)
            .await
            .unwrap();
    }
    categories::Entity::delete_many()
        .filter(categories::Column::Slug.eq(CATEGORY_SLUG))
        .exec(&ctx.db)
        .await
        .unwrap();
    tags::Entity::delete_many()
        .filter(tags::Column::Slug.eq(TAG_SLUG))
        .exec(&ctx.db)
        .await
        .unwrap();
    users::Entity::delete_many()
        .filter(users::Column::Email.is_in([AUTHOR_EMAIL, READER_EMAIL]))
        .exec(&ctx.db)
        .await
        .unwrap();
}

/// goravel's `SetupTest`: fresh users with the suite password.
async fn setup(ctx: &AppContext) {
    cleanup(ctx).await;
    let password = loco_rs::hash::hash_password(PASSWORD).unwrap();
    for (name, email) in [("Blog Author", AUTHOR_EMAIL), ("Blog Reader", READER_EMAIL)] {
        users::ActiveModel {
            name: Set(name.to_owned()),
            email: Set(email.to_owned()),
            password: Set(password.clone()),
            ..Default::default()
        }
        .insert(&ctx.db)
        .await
        .unwrap();
    }
}

async fn login(request: &loco_rs::TestServer, email: &str) -> String {
    let response = request
        .post("/api/auth/login")
        .json(&serde_json::json!({"email": email, "password": PASSWORD}))
        .await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    body["token"].as_str().unwrap().to_owned()
}

fn remove_stored(path: &str) {
    std::fs::remove_file(format!("storage/app/public/{path}")).ok();
}

/// The full goravel blog parity flow.
#[tokio::test]
#[serial]
async fn publishing_moderation_and_taxonomy_flow() {
    request::<App, _, _>(|request, ctx| async move {
        setup(&ctx).await;

        // Anonymous writes are unauthorized.
        let response = request
            .post("/api/posts")
            .json(&serde_json::json!({"title": "No Token", "content_html": "<p>body</p>"}))
            .await;
        response.assert_status(axum::http::StatusCode::UNAUTHORIZED);

        let author_token = login(&request, AUTHOR_EMAIL).await;
        let reader_token = login(&request, READER_EMAIL).await;

        // Generic image upload lands under /storage/uploads/images.
        let (content_type, body) = multipart_body(&[], &["image"]);
        let response = request
            .post("/api/uploads/images")
            .authorization_bearer(&author_token)
            .content_type(&content_type)
            .bytes(body.into())
            .await;
        response.assert_status(axum::http::StatusCode::CREATED);
        let text = response.text();
        assert!(text.contains("/storage/uploads/images"), "{text}");
        let upload: serde_json::Value = serde_json::from_str(&text).unwrap();
        remove_stored(upload["path"].as_str().unwrap());

        // Standalone cover upload — its URL feeds the JSON create below.
        let (content_type, body) = multipart_body(&[], &["image"]);
        let response = request
            .post("/api/uploads/post-covers")
            .authorization_bearer(&author_token)
            .content_type(&content_type)
            .bytes(body.into())
            .await;
        response.assert_status(axum::http::StatusCode::CREATED);
        let text = response.text();
        assert!(text.contains("/storage/uploads/post-covers"), "{text}");
        let upload: serde_json::Value = serde_json::from_str(&text).unwrap();
        let cover_url = upload["url"].as_str().unwrap().to_owned();
        remove_stored(upload["path"].as_str().unwrap());

        // Multipart create with an inline cover file (goravel's write
        // endpoints accept form bodies with `cover_image`).
        let (content_type, body) = multipart_body(
            &[
                ("title", "Multipart Cover Draft"),
                ("content_html", "<p>Cover body</p>"),
            ],
            &["cover_image"],
        );
        let response = request
            .post("/api/posts")
            .authorization_bearer(&author_token)
            .content_type(&content_type)
            .bytes(body.into())
            .await;
        response.assert_status(axum::http::StatusCode::CREATED);
        let text = response.text();
        assert!(text.contains("/storage/uploads/post-covers"), "{text}");
        let create: serde_json::Value = serde_json::from_str(&text).unwrap();
        let direct_cover_url = create["post"]["cover_image_url"].as_str().unwrap();
        remove_stored(direct_cover_url.split("/storage/").nth(1).unwrap());

        // Taxonomy creation trims and slugifies padded names.
        let response = request
            .post("/api/categories")
            .authorization_bearer(&author_token)
            .json(&serde_json::json!({"name": " Blog Test Engineering "}))
            .await;
        response.assert_status(axum::http::StatusCode::CREATED);
        let category: serde_json::Value = response.json();
        let category_id = category["id"].as_i64().unwrap();

        let response = request
            .post("/api/tags")
            .authorization_bearer(&author_token)
            .json(&serde_json::json!({"name": "Blog Test Go"}))
            .await;
        response.assert_status(axum::http::StatusCode::CREATED);
        let tag: serde_json::Value = response.json();
        let tag_id = tag["id"].as_i64().unwrap();

        // Create with script content: stored sanitized, never echoed.
        let response = request
            .post("/api/posts")
            .authorization_bearer(&author_token)
            .json(&serde_json::json!({
                "title": " Sanitized Draft ",
                "excerpt": "News",
                "cover_image_url": cover_url,
                "content_html": "<p>Hello</p><script>alert(1)</script>",
                "category_id": category_id,
                "tag_ids": [tag_id],
            }))
            .await;
        response.assert_status(axum::http::StatusCode::CREATED);
        let text = response.text();
        assert!(!text.contains("<script>"), "{text}");
        assert!(!text.contains("alert(1)"), "{text}");
        let create: serde_json::Value = serde_json::from_str(&text).unwrap();
        let post_id = create["post"]["id"].as_i64().unwrap();

        // Drafts are invisible on the public read.
        let response = request.get("/api/posts/sanitized-draft").await;
        response.assert_status(axum::http::StatusCode::NOT_FOUND);

        // Publish through the partial-update endpoint.
        let response = request
            .patch(&format!("/api/me/posts/{post_id}"))
            .authorization_bearer(&author_token)
            .json(&serde_json::json!({"status": "published"}))
            .await;
        response.assert_status_ok();
        assert!(response.text().contains("published"));

        // The filtered feed finds the published, sanitized post.
        let response = request
            .get(&format!(
                "/api/posts?q=Sanitized&category={CATEGORY_SLUG}&tag={TAG_SLUG}"
            ))
            .await;
        response.assert_status_ok();
        let text = response.text();
        assert!(text.contains("sanitized-draft"), "{text}");
        assert!(!text.contains("<script>"), "{text}");

        // Readers may comment; new comments start pending.
        let response = request
            .post("/api/posts/sanitized-draft/comments")
            .authorization_bearer(&reader_token)
            .json(&serde_json::json!({"content": " Needs approval "}))
            .await;
        response.assert_status(axum::http::StatusCode::CREATED);
        let text = response.text();
        assert!(text.contains("pending"), "{text}");
        let comment: serde_json::Value = serde_json::from_str(&text).unwrap();
        let comment_id = comment["comment"]["id"].as_i64().unwrap();

        // Pending comments are not public.
        let response = request.get("/api/posts/sanitized-draft/comments").await;
        response.assert_status_ok();
        assert!(!response.text().contains("Needs approval"));

        // The owner approves; now it is public.
        let response = request
            .patch(&format!("/api/me/posts/{post_id}/comments/{comment_id}"))
            .authorization_bearer(&author_token)
            .json(&serde_json::json!({"status": "approved"}))
            .await;
        response.assert_status_ok();
        assert!(response.text().contains("approved"));

        let response = request.get("/api/posts/sanitized-draft/comments").await;
        response.assert_status_ok();
        assert!(response.text().contains("Needs approval"));

        // Taxonomy assigned to a post cannot be deleted (409).
        let response = request
            .delete(&format!("/api/categories/{category_id}"))
            .authorization_bearer(&author_token)
            .await;
        response.assert_status(axum::http::StatusCode::CONFLICT);

        cleanup(&ctx).await;
    })
    .await;
}

/// goravel's `Update` also accepts multipart with only a cover file (a
/// single-field partial update); the contract beyond the Go suite's flow.
#[tokio::test]
#[serial]
async fn multipart_cover_only_patch_updates_the_post() {
    request::<App, _, _>(|request, ctx| async move {
        setup(&ctx).await;
        let author_token = login(&request, AUTHOR_EMAIL).await;

        let response = request
            .post("/api/posts")
            .authorization_bearer(&author_token)
            .json(&serde_json::json!({
                "title": "Cover Patch Target",
                "content_html": "<p>body</p>",
            }))
            .await;
        response.assert_status(axum::http::StatusCode::CREATED);
        let create: serde_json::Value = response.json();
        let post_id = create["post"]["id"].as_i64().unwrap();

        let (content_type, body) = multipart_body(&[], &["cover_image"]);
        let response = request
            .patch(&format!("/api/me/posts/{post_id}"))
            .authorization_bearer(&author_token)
            .content_type(&content_type)
            .bytes(body.into())
            .await;
        response.assert_status_ok();
        let text = response.text();
        assert!(text.contains("Post updated successfully"), "{text}");
        assert!(text.contains("/storage/uploads/post-covers"), "{text}");
        let update: serde_json::Value = serde_json::from_str(&text).unwrap();
        remove_stored(
            update["post"]["cover_image_url"]
                .as_str()
                .unwrap()
                .split("/storage/")
                .nth(1)
                .unwrap(),
        );

        cleanup(&ctx).await;
    })
    .await;
}
