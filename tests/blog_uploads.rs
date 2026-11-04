use loco_laravel::{
    app::App,
    models::_entities::posts,
    modules::{auth::models::users, blog::seeders},
};
use loco_rs::prelude::*;
use serde_json::Value;
use serial_test::serial;

const BOUNDARY: &str = "----locolaraveltestboundary";

fn png() -> Vec<u8> {
    let mut bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(b"fake-png-payload");
    bytes
}

fn jpeg() -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xE0];
    bytes.extend_from_slice(b"fake-jpeg-payload");
    bytes
}

fn gif() -> Vec<u8> {
    let mut bytes = b"GIF89a".to_vec();
    bytes.extend_from_slice(b"fake-gif-payload");
    bytes
}

fn webp() -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&[0x00, 0x00, 0x00, 0x20]);
    bytes.extend_from_slice(b"WEBPfake-webp-payload");
    bytes
}

fn multipart(field: &str, filename: &str, mime: &str, content: &[u8]) -> (String, Vec<u8>) {
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{BOUNDARY}\r\ncontent-disposition: form-data; name=\"{field}\"; \
             filename=\"{filename}\"\r\ncontent-type: {mime}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(content);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={BOUNDARY}"), body)
}

/// POST `path` with an authenticated multipart upload; returns
/// (status, body text, parsed JSON).
async fn send(
    request: &loco_rs::TestServer,
    path: &str,
    token: &str,
    field: &str,
    content: &[u8],
) -> (axum::http::StatusCode, String, Value) {
    let (content_type, body) = multipart(field, "upload.bin", "application/octet-stream", content);
    let response = request
        .post(path)
        .content_type(&content_type)
        .authorization_bearer(token)
        .bytes(body.into())
        .await;
    let status = response.status_code();
    let text = response.text();
    let json = serde_json::from_str(&text).unwrap_or(Value::Null);
    (status, text, json)
}

fn cleanup(path: &str) {
    std::fs::remove_file(format!("storage/app/public/{path}")).ok();
}

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

async fn post_by_slug(ctx: &AppContext, slug: &str) -> posts::Model {
    posts::Entity::find()
        .filter(posts::Column::Slug.eq(slug))
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
#[serial]
async fn upload_routes_require_a_valid_existing_user() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let post = post_by_slug(&ctx, "getting-started-with-goravel").await;
        for path in [
            "/api/uploads/images",
            "/api/uploads/post-covers",
            &format!("/api/me/posts/{}/cover", post.id),
        ] {
            let (content_type, body) = multipart("image", "a.png", "image/png", &png());
            request
                .post(path)
                .content_type(&content_type)
                .bytes(body.into())
                .await
                .assert_status_unauthorized();
            request
                .post(path)
                .authorization_bearer("invalid-token")
                .bytes(png().into())
                .await
                .assert_status_unauthorized();
        }
        let token = login(&request, "reader@example.com").await;
        let user = users::Model::find_by_email(&ctx.db, "reader@example.com")
            .await
            .unwrap();
        users::Entity::delete_by_id(user.id)
            .exec(&ctx.db)
            .await
            .unwrap();
        let (status, _, _) = send(&request, "/api/uploads/images", &token, "image", &png()).await;
        assert_eq!(status, axum::http::StatusCode::UNAUTHORIZED);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn image_uploads_validate_and_store() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let reader = login(&request, "reader@example.com").await;

        // A non-multipart body reads as "no file uploaded" (goravel parity).
        let response = request
            .post("/api/uploads/images")
            .authorization_bearer(&reader)
            .json(&serde_json::json!({"image": "x"}))
            .await;
        assert_eq!(response.status_code(), 422, "{}", response.text());

        // Missing named field.
        let (content_type, body) = multipart("other", "a.txt", "text/plain", b"nope");
        let response = request
            .post("/api/uploads/images")
            .content_type(&content_type)
            .authorization_bearer(&reader)
            .bytes(body.into())
            .await;
        assert_eq!(response.status_code(), 422, "{}", response.text());
        let parsed: Value = response.json();
        assert_eq!(parsed["message"], "Validation failed");
        assert_eq!(parsed["errors"]["image"]["validation"], "image is required");

        // Not an image.
        let (status, text, parsed) = send(
            &request,
            "/api/uploads/images",
            &reader,
            "image",
            b"plain text bytes",
        )
        .await;
        assert_eq!(
            status,
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "{text}"
        );
        assert_eq!(
            parsed["errors"]["image"]["validation"],
            "image must be a jpeg, png, gif, or webp image"
        );

        // Over 5 MB (valid png magic, oversized payload).
        let mut huge = png();
        huge.extend(std::iter::repeat_n(0, 5 * 1024 * 1024));
        let (status, text, parsed) =
            send(&request, "/api/uploads/images", &reader, "image", &huge).await;
        assert_eq!(
            status,
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "{text}"
        );
        assert_eq!(
            parsed["errors"]["image"]["validation"],
            "image must not exceed 5 MB"
        );

        // Every allowed type stores and reports url + path.
        for (label, content) in [
            ("png", png()),
            ("jpg", jpeg()),
            ("gif", gif()),
            ("webp", webp()),
        ] {
            let (status, text, parsed) =
                send(&request, "/api/uploads/images", &reader, "image", &content).await;
            assert_eq!(status, axum::http::StatusCode::CREATED, "{text} ({label})");
            let url = parsed["url"].as_str().unwrap();
            let path = parsed["path"].as_str().unwrap();
            assert!(url.starts_with("http://"), "{url}");
            assert!(url.contains("/storage/uploads/images/"), "{url}");
            assert!(path.starts_with("uploads/images/"), "{path}");
            assert!(path.ends_with(&format!(".{label}")), "{path}");
            let stored = std::fs::read(format!("storage/app/public/{path}")).unwrap();
            assert_eq!(stored, content);
            cleanup(path);
        }
    })
    .await;
}

#[tokio::test]
#[serial]
async fn post_cover_uploads_and_own_post_cover() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let author = login(&request, "author@example.com").await;
        let admin = login(&request, "admin@example.com").await;
        let post = post_by_slug(&ctx, "getting-started-with-goravel").await;

        // Standalone cover upload uses the `image` field.
        let (status, text, parsed) = send(
            &request,
            "/api/uploads/post-covers",
            &author,
            "image",
            &png(),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::CREATED, "{text}");
        assert!(parsed["path"]
            .as_str()
            .unwrap()
            .starts_with("uploads/post-covers/"));
        cleanup(parsed["path"].as_str().unwrap());

        // ...while the own-post endpoint requires `cover_image`.
        let (status, text, parsed) = send(
            &request,
            &format!("/api/me/posts/{}/cover", post.id),
            &author,
            "image",
            &png(),
        )
        .await;
        assert_eq!(
            status,
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "{text}"
        );
        assert_eq!(
            parsed["errors"]["cover_image"]["validation"],
            "cover_image is required"
        );

        // Owner flow: upload + persist on the owned post.
        let (status, text, parsed) = send(
            &request,
            &format!("/api/me/posts/{}/cover", post.id),
            &author,
            "cover_image",
            &png(),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::CREATED, "{text}");
        assert_eq!(parsed["message"], "Post cover updated successfully");
        let url = parsed["url"].as_str().unwrap();
        let path = parsed["path"].as_str().unwrap();
        assert!(url.contains("/storage/uploads/post-covers/"), "{url}");
        assert_eq!(parsed["post"]["id"], post.id);
        assert_eq!(parsed["post"]["cover_image_url"], url);
        let row = posts::Entity::find_by_id(post.id)
            .one(&ctx.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.cover_image_url.as_deref(), Some(url));
        cleanup(path);

        // Non-owner and unknown posts 404 (goravel findOwnedPost parity).
        let (status, text, _) = send(
            &request,
            &format!("/api/me/posts/{}/cover", post.id),
            &admin,
            "cover_image",
            &png(),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::NOT_FOUND, "{text}");
        let (status, text, _) = send(
            &request,
            "/api/me/posts/999999/cover",
            &author,
            "cover_image",
            &png(),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::NOT_FOUND, "{text}");

        // Remove the orphaned uploads the 404 flows left behind (the files
        // are stored before the ownership check, like goravel).
        if let Ok(entries) = std::fs::read_dir("storage/app/public/uploads/post-covers") {
            for entry in entries.flatten() {
                std::fs::remove_file(entry.path()).ok();
            }
        }
    })
    .await;
}
