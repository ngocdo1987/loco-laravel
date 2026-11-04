use loco_laravel::{app::App, modules::blog::seeders};
use loco_rs::prelude::*;
use serde_json::Value;
use serial_test::serial;

async fn seed(ctx: &AppContext) {
    seeders::seed(ctx, std::path::Path::new("src/fixtures"))
        .await
        .unwrap();
}

async fn login_token(request: &loco_rs::TestServer, email: &str) -> String {
    let response = request
        .post("/api/auth/login")
        .json(&serde_json::json!({"email": email, "password": "Password123!"}))
        .await;
    response.assert_status_ok();
    let body: Value = response.json();
    body["token"].as_str().unwrap().to_owned()
}

#[tokio::test]
#[serial]
async fn home_lists_published_posts_and_sidebar() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let response = request.get("/").await;
        response.assert_status_ok();
        let text = response.text();
        assert!(text.contains("Latest posts"), "missing title");
        assert!(text.contains("Goravel Blog"), "missing brand");
        assert!(text.contains("Getting Started with Goravel"));
        assert!(text.contains("Nuxt Frontend for the Goravel API"));
        assert!(
            !text.contains("Draft API Testing Checklist"),
            "draft leaked"
        );
        assert!(text.contains("/categories/engineering"));
        assert!(text.contains("/tags/goravel"));
        // Anonymous navbar shows the login/register links.
        assert!(text.contains("/register"));
        assert!(!text.contains("Logout"));

        // Search narrows the feed ("frontend" only appears in the Nuxt post).
        let response = request.get("/?q=frontend").await;
        response.assert_status_ok();
        let text = response.text();
        assert!(text.contains("Nuxt Frontend for the Goravel API"), "{text}");
        assert!(!text.contains("Getting Started with Goravel"), "{text}");

        // 3 posts, 9-per-page view limit: no pagination links.
        let response = request.get("/").await;
        assert!(!response.text().contains("Next"));

        // Bad pagination renders the minimal 400 error page.
        let response = request.get("/?limit=0").await;
        assert_eq!(response.status_code(), 400, "{}", response.text());
        assert!(response.text().contains("Invalid feed query."));
    })
    .await;
}

#[tokio::test]
#[serial]
async fn category_and_tag_feeds_filter_posts() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let response = request.get("/categories/engineering").await;
        response.assert_status_ok();
        let text = response.text();
        assert!(text.contains("Category: engineering"), "{text}");
        assert!(text.contains("Published posts in this category."));
        // Engineering holds the getting-started post (and a draft that must
        // not leak); product holds the Nuxt post.
        assert!(text.contains("Getting Started with Goravel"));
        assert!(!text.contains("Nuxt Frontend for the Goravel API"));
        assert!(!text.contains("Draft API Testing Checklist"));

        let response = request.get("/categories/product").await;
        response.assert_status_ok();
        let text = response.text();
        assert!(text.contains("Nuxt Frontend for the Goravel API"), "{text}");
        assert!(!text.contains("Getting Started with Goravel"));

        // The draft-only category shows the empty state, not an error.
        let response = request.get("/categories/does-not-exist").await;
        response.assert_status_ok();
        assert!(response.text().contains("No published posts found."));

        // Tag feeds filter by the post_tags join.
        let response = request.get("/tags/goravel").await;
        response.assert_status_ok();
        let text = response.text();
        assert!(text.contains("Tag: goravel"), "{text}");
        assert!(text.contains("Getting Started with Goravel"));

        let response = request.get("/tags/unknown-tag").await;
        response.assert_status_ok();
        assert!(response.text().contains("No published posts found."));

        let response = request.get("/tags/goravel?limit=0").await;
        assert_eq!(response.status_code(), 400);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn post_page_renders_sanitized_content_and_approved_comments() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let response = request.get("/posts/getting-started-with-goravel").await;
        response.assert_status_ok();
        let text = response.text();
        assert!(text.contains("Getting Started with Goravel"));
        assert!(text.contains("Author User"));
        // The approved seeded comment renders; the pending one must not.
        assert!(text.contains("This helped me understand"), "{text}");
        assert!(!text.contains("Could you add a follow-up"));
        // Anonymous visitors see the login prompt instead of the form.
        assert!(text.contains("Login</a> to submit a comment."));
        assert!(!text.contains("Submit for moderation"));

        // Logged-in visitors (auth_token cookie) get the comment form.
        let token = login_token(&request, "reader@example.com").await;
        let response = request
            .get("/posts/getting-started-with-goravel")
            .add_header("cookie", format!("auth_token={token}"))
            .await;
        response.assert_status_ok();
        let text = response.text();
        assert!(text.contains("Submit for moderation"), "{text}");
        assert!(text.contains("Add a comment"));
        assert!(text.contains("Logout"));

        // Drafts and unknown slugs render the 404 error page.
        let response = request.get("/posts/draft-api-testing-checklist").await;
        assert_eq!(response.status_code(), 404, "{}", response.text());
        assert!(response.text().contains("Resource not found"));
        let response = request.get("/posts/missing-post").await;
        assert_eq!(response.status_code(), 404);
    })
    .await;
}
