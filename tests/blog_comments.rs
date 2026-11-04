use loco_laravel::{
    app::App,
    models::_entities::{comments, posts},
    modules::{auth::models::users, blog::seeders},
};
use loco_rs::prelude::*;
use sea_orm::PaginatorTrait;
use serde_json::{json, Value};
use serial_test::serial;

async fn seed(ctx: &AppContext) {
    seeders::seed(ctx, std::path::Path::new("src/fixtures"))
        .await
        .unwrap();
}

async fn login(request: &loco_rs::TestServer, email: &str) -> String {
    let response = request
        .post("/api/auth/login")
        .json(&json!({"email": email, "password": "Password123!"}))
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
async fn comment_routes_require_a_valid_existing_user() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let post = post_by_slug(&ctx, "getting-started-with-goravel").await;
        request
            .post("/api/posts/getting-started-with-goravel/comments")
            .json(&json!({"content": "No auth"}))
            .await
            .assert_status_unauthorized();
        request
            .delete("/api/comments/1")
            .await
            .assert_status_unauthorized();
        request
            .get(&format!("/api/me/posts/{}/comments", post.id))
            .await
            .assert_status_unauthorized();
        request
            .patch(&format!("/api/me/posts/{}/comments/1", post.id))
            .json(&json!({"status": "approved"}))
            .await
            .assert_status_unauthorized();
        request
            .delete(&format!("/api/me/posts/{}/comments/1", post.id))
            .await
            .assert_status_unauthorized();
        let token = login(&request, "reader@example.com").await;
        let user = users::Model::find_by_email(&ctx.db, "reader@example.com")
            .await
            .unwrap();
        users::Entity::delete_by_id(user.id)
            .exec(&ctx.db)
            .await
            .unwrap();
        request
            .post("/api/posts/getting-started-with-goravel/comments")
            .authorization_bearer(&token)
            .json(&json!({"content": "Deleted user"}))
            .await
            .assert_status_unauthorized();
        request
            .delete("/api/comments/1")
            .authorization_bearer(&token)
            .await
            .assert_status_unauthorized();
        // The reader's two seeded comments cascade away with their account
        // (FK parity with goravel: comments.author_id ON DELETE CASCADE).
        assert_eq!(comments::Entity::find().count(&ctx.db).await.unwrap(), 2);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn submit_starts_pending_and_hides_until_approved() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let reader = login(&request, "reader@example.com").await;
        let author = login(&request, "author@example.com").await;
        let post = post_by_slug(&ctx, "getting-started-with-goravel").await;

        let create = request
            .post("/api/posts/getting-started-with-goravel/comments")
            .authorization_bearer(&reader)
            .json(&json!({"content": "  Fresh take on this  "}))
            .await;
        assert_eq!(create.status_code(), 201, "{}", create.text());
        let body: Value = create.json();
        assert_eq!(body["message"], "Comment submitted for moderation");
        assert_eq!(body["comment"]["status"], "pending");
        assert_eq!(body["comment"]["content"], "Fresh take on this");
        assert_eq!(body["comment"]["author"]["name"], "Reader User");
        let comment_id = body["comment"]["id"].as_i64().unwrap();

        // Pending comments are invisible on the public list.
        let response = request
            .get("/api/posts/getting-started-with-goravel/comments")
            .await;
        response.assert_status_ok();
        let public: Value = response.json();
        assert_eq!(public["total"], 1);
        assert!(!public["data"].to_string().contains("Fresh take"));

        // The queue exposes status, filtered or not.
        let response = request
            .get(&format!(
                "/api/me/posts/{}/comments?status=pending",
                post.id
            ))
            .authorization_bearer(&author)
            .await;
        response.assert_status_ok();
        let pending: Value = response.json();
        assert_eq!(pending["total"], 2, "{pending}");
        assert!(pending["data"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["status"] == "pending"));

        // Moderation to approved makes it public.
        let response = request
            .patch(&format!(
                "/api/me/posts/{}/comments/{}",
                post.id, comment_id
            ))
            .authorization_bearer(&author)
            .json(&json!({"status": "approved"}))
            .await;
        response.assert_status_ok();
        let body: Value = response.json();
        assert_eq!(body["message"], "Comment moderation updated");
        assert_eq!(body["comment"]["status"], "approved");
        let response = request
            .get("/api/posts/getting-started-with-goravel/comments")
            .await;
        response.assert_status_ok();
        let public: Value = response.json();
        assert_eq!(public["total"], 2);
        assert!(public["data"].to_string().contains("Fresh take"));

        // Rejecting hides it again.
        request
            .patch(&format!(
                "/api/me/posts/{}/comments/{}",
                post.id, comment_id
            ))
            .authorization_bearer(&author)
            .json(&json!({"status": "rejected"}))
            .await
            .assert_status_ok();
        let response = request
            .get("/api/posts/getting-started-with-goravel/comments")
            .await;
        response.assert_status_ok();
        let public: Value = response.json();
        assert_eq!(public["total"], 1);
        assert!(!public["data"].to_string().contains("Fresh take"));

        // Authors may withdraw their own comment whatever its status.
        let response = request
            .delete(&format!("/api/comments/{comment_id}"))
            .authorization_bearer(&reader)
            .await;
        assert_eq!(response.status_code(), 204);
        request
            .delete(&format!("/api/comments/{comment_id}"))
            .authorization_bearer(&reader)
            .await
            .assert_status_not_found();
        assert_eq!(comments::Entity::find().count(&ctx.db).await.unwrap(), 4);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn submit_validates_content_and_published_slug() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let reader = login(&request, "reader@example.com").await;
        for payload in [
            json!({}),
            json!({"content": null}),
            json!({"content": "   "}),
            json!({"content": "x".repeat(2001)}),
        ] {
            let response = request
                .post("/api/posts/getting-started-with-goravel/comments")
                .authorization_bearer(&reader)
                .json(&payload)
                .await;
            assert_eq!(response.status_code(), 422, "{}", response.text());
            let body: Value = response.json();
            assert_eq!(body["message"], "Validation failed");
            assert!(body["errors"]["content"]["validation"]
                .as_str()
                .unwrap()
                .starts_with("content"));
        }
        // A 2000-character comment is accepted.
        let response = request
            .post("/api/posts/getting-started-with-goravel/comments")
            .authorization_bearer(&reader)
            .json(&json!({"content": "x".repeat(2000)}))
            .await;
        assert_eq!(response.status_code(), 201, "{}", response.text());
        // Drafts and unknown slugs are invisible.
        request
            .post("/api/posts/draft-api-testing-checklist/comments")
            .authorization_bearer(&reader)
            .json(&json!({"content": "Draft"}))
            .await
            .assert_status_not_found();
        request
            .post("/api/posts/missing-post/comments")
            .authorization_bearer(&reader)
            .json(&json!({"content": "Missing"}))
            .await
            .assert_status_not_found();
    })
    .await;
}

#[tokio::test]
#[serial]
async fn moderation_is_restricted_to_the_post_owner() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let author = login(&request, "author@example.com").await;
        let reader = login(&request, "reader@example.com").await;
        let post = post_by_slug(&ctx, "getting-started-with-goravel").await;
        let other = post_by_slug(&ctx, "nuxt-frontend-for-goravel-api").await;
        let path = format!("/api/me/posts/{}/comments", post.id);

        // Non-owners see nothing, not even the queue.
        request
            .get(&format!("{path}?status=pending"))
            .authorization_bearer(&reader)
            .await
            .assert_status_not_found();
        request
            .get(&path)
            .authorization_bearer(&reader)
            .await
            .assert_status_not_found();
        request
            .patch(&format!("{path}/999999"))
            .authorization_bearer(&reader)
            .json(&json!({"status": "approved"}))
            .await
            .assert_status_not_found();
        request
            .delete(&format!("{path}/999999"))
            .authorization_bearer(&reader)
            .await
            .assert_status_not_found();

        // Unknown post ids 404 for the owner too.
        request
            .get("/api/me/posts/999999/comments")
            .authorization_bearer(&author)
            .await
            .assert_status_not_found();

        // Invalid queue filters are rejected before the ownership lookup.
        request
            .get(&format!("{path}?status=bogus"))
            .authorization_bearer(&reader)
            .await
            .assert_status_unprocessable_entity();
        request
            .get(&format!("{path}?limit=0"))
            .authorization_bearer(&reader)
            .await
            .assert_status_unprocessable_entity();

        // Moderation status must be approved or rejected.
        for status in ["", "pending", "bogus"] {
            let response = request
                .patch(&format!("{path}/1"))
                .authorization_bearer(&author)
                .json(&json!({"status": status}))
                .await;
            assert_eq!(response.status_code(), 422, "{}", response.text());
        }

        // A comment from another post cannot be moderated through this post.
        let foreign = comments::Entity::find()
            .filter(comments::Column::PostId.eq(other.id))
            .one(&ctx.db)
            .await
            .unwrap()
            .unwrap();
        request
            .patch(&format!("{path}/{}", foreign.id))
            .authorization_bearer(&author)
            .json(&json!({"status": "approved"}))
            .await
            .assert_status_not_found();
        request
            .delete(&format!("{path}/{}", foreign.id))
            .authorization_bearer(&author)
            .await
            .assert_status_not_found();

        // The owner may delete a moderated comment from their own post.
        let pending = comments::Entity::find()
            .filter(comments::Column::PostId.eq(post.id))
            .filter(comments::Column::Status.eq("pending"))
            .one(&ctx.db)
            .await
            .unwrap()
            .unwrap();
        let response = request
            .delete(&format!("{path}/{}", pending.id))
            .authorization_bearer(&author)
            .await;
        assert_eq!(response.status_code(), 204);
        request
            .delete(&format!("{path}/{}", pending.id))
            .authorization_bearer(&author)
            .await
            .assert_status_not_found();

        // delete-own only touches the author's own comments.
        let seeded = comments::Entity::find()
            .filter(comments::Column::PostId.eq(post.id))
            .filter(comments::Column::Status.eq("approved"))
            .one(&ctx.db)
            .await
            .unwrap()
            .unwrap();
        request
            .delete(&format!("/api/comments/{}", seeded.id))
            .authorization_bearer(&author)
            .await
            .assert_status_not_found();
        assert_eq!(comments::Entity::find().count(&ctx.db).await.unwrap(), 3);
    })
    .await;
}
