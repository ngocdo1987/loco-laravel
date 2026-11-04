use loco_laravel::{
    app::App,
    models::_entities::{categories, comments, post_tags, posts, tags},
    modules::{auth::models::users, blog::seeders},
};
use loco_rs::prelude::*;
use sea_orm::PaginatorTrait;
use serde_json::{json, Value};
use serial_test::serial;

async fn login(request: &loco_rs::TestServer, email: &str) -> String {
    let response = request
        .post("/api/auth/login")
        .json(&json!({"email": email, "password": "Password123!"}))
        .await;
    response.assert_status_ok();
    let body: Value = response.json();
    body["token"].as_str().unwrap().to_owned()
}

async fn seed(ctx: &AppContext) {
    seeders::seed(ctx, std::path::Path::new("src/fixtures"))
        .await
        .unwrap();
}

#[tokio::test]
#[serial]
async fn author_routes_require_a_valid_existing_user() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        request
            .post("/api/posts")
            .json(&json!({}))
            .await
            .assert_status_unauthorized();
        request
            .get("/api/me/posts")
            .await
            .assert_status_unauthorized();
        request
            .get("/api/me/posts/1")
            .await
            .assert_status_unauthorized();
        request
            .patch("/api/me/posts/1")
            .json(&json!({}))
            .await
            .assert_status_unauthorized();
        request
            .delete("/api/me/posts/1")
            .await
            .assert_status_unauthorized();
        request
            .get("/api/me/posts")
            .authorization_bearer("invalid-token")
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
            .get("/api/me/posts")
            .authorization_bearer(&token)
            .await
            .assert_status_unauthorized();
        request
            .post("/api/posts")
            .authorization_bearer(&token)
            .json(&json!({"title":"No user", "content_html":"body"}))
            .await
            .assert_status_unauthorized();
    })
    .await;
}

#[tokio::test]
#[serial]
async fn author_can_create_sanitized_draft_publish_patch_and_delete_with_cascades() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let token = login(&request, "author@example.com").await;
        let user = users::Model::find_by_email(&ctx.db, "author@example.com").await.unwrap();
        let category = categories::Entity::find().one(&ctx.db).await.unwrap().unwrap();
        let tag = tags::Entity::find().one(&ctx.db).await.unwrap().unwrap();
        let response = request.post("/api/posts").authorization_bearer(&token).json(&json!({
            "title":" Sanitized Draft ", "content_html":"<p>Hello</p><script>alert(1)</script>",
            "excerpt":" News ", "cover_image_url":" https://example.com/cover.jpg ",
            "category_id":category.id,"tag_ids":[tag.id], "author_id":999999,"id":999999
        })).await;
        assert_eq!(response.status_code(), 201, "{}", response.text());
        let body: Value = response.json();
        assert_eq!(body["message"], "Post created successfully");
        let post = &body["post"];
        let id = post["id"].as_i64().unwrap();
        let path = format!("/api/me/posts/{id}");
        assert_ne!(id,999999);
        assert_eq!(post["author"]["id"], user.id);
        assert_eq!(post["author"].as_object().unwrap().len(),2);
        assert_eq!(post["title"], "Sanitized Draft");
        assert_eq!(post["slug"], "sanitized-draft");
        assert_eq!(post["excerpt"], "News");
        assert_eq!(post["cover_image_url"], "https://example.com/cover.jpg");
        assert_eq!(post["content_html"], "<p>Hello</p>");
        assert_eq!(post["status"], "draft");
        assert!(post["published_at"].is_null());
        assert_eq!(post["category"]["id"],category.id);
        assert_eq!(post["tags"][0]["id"],tag.id);
        let stored = posts::Entity::find_by_id(id).one(&ctx.db).await.unwrap().unwrap();
        assert_eq!(stored.content_html,"<p>Hello</p>");
        request.get("/api/posts/sanitized-draft").await.assert_status_not_found();
        request.get(&path).authorization_bearer(&token).await.assert_status_ok();
        let response=request.patch(&path).authorization_bearer(&token).json(&json!({"status":"published"})).await;
        response.assert_status_ok();
        let body:Value=response.json();
        assert_eq!(body["message"],"Post updated successfully");
        let published_at=body["post"]["published_at"].clone();
        assert!(published_at.is_string());
        request.get("/api/posts/sanitized-draft").await.assert_status_ok();
        let response=request.patch(&path).authorization_bearer(&token).json(&json!({"title":"New title", "status":"published", "content_html":"<p>Updated</p><img src=x onerror=alert(1)>","author_id":999999})).await;
        response.assert_status_ok();
        let body:Value=response.json();
        assert_eq!(body["post"]["slug"],"sanitized-draft");
        assert_eq!(body["post"]["published_at"],published_at);
        assert_eq!(body["post"]["author"]["id"],user.id);
        let stored=posts::Entity::find_by_id(id).one(&ctx.db).await.unwrap().unwrap();
        assert!(!stored.content_html.contains("onerror"));
        let response=request.patch(&path).authorization_bearer(&token).json(&json!({"status":"draft","slug":" Custom Slug ","category_id":null,"tag_ids":[],"excerpt":null,"cover_image_url":null})).await;
        response.assert_status_ok();
        let body:Value=response.json();
        assert_eq!(body["post"]["slug"],"custom-slug");
        assert_eq!(body["post"]["excerpt"],"");
        assert_eq!(body["post"]["cover_image_url"],"");
        assert!(body["post"]["category"].is_null());
        assert_eq!(body["post"]["tags"],json!([]));
        assert!(body["post"]["published_at"].is_null());
        request.get("/api/posts/custom-slug").await.assert_status_not_found();
        request.patch(&path).authorization_bearer(&token).json(&json!({"tag_ids":[tag.id]})).await.assert_status_ok();
        comments::ActiveModel{post_id:Set(id),author_id:Set(user.id),content:Set("comment".into()),..Default::default()}.insert(&ctx.db).await.unwrap();
        let response=request.delete(&path).authorization_bearer(&token).await;
        assert_eq!(response.status_code(),204);
        assert!(response.text().is_empty());
        assert!(posts::Entity::find_by_id(id).one(&ctx.db).await.unwrap().is_none());
        assert_eq!(post_tags::Entity::find().filter(post_tags::Column::PostId.eq(id)).count(&ctx.db).await.unwrap(),0);
        assert_eq!(comments::Entity::find().filter(comments::Column::PostId.eq(id)).count(&ctx.db).await.unwrap(),0);
        request.get(&path).authorization_bearer(&token).await.assert_status_not_found();
        request.delete(&path).authorization_bearer(&token).await.assert_status_not_found();
    }).await;
}

#[tokio::test]
#[serial]
async fn own_posts_enforce_ownership_status_and_pagination() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let token = login(&request, "author@example.com").await;
        let other = login(&request, "admin@example.com").await;
        let draft = posts::Entity::find()
            .filter(posts::Column::Slug.eq("draft-api-testing-checklist"))
            .one(&ctx.db)
            .await
            .unwrap()
            .unwrap();
        let path = format!("/api/me/posts/{}", draft.id);
        request
            .get(&path)
            .authorization_bearer(&other)
            .await
            .assert_status_not_found();
        request
            .patch(&path)
            .authorization_bearer(&other)
            .json(&json!({"title":"Stolen"}))
            .await
            .assert_status_not_found();
        request
            .delete(&path)
            .authorization_bearer(&other)
            .await
            .assert_status_not_found();
        let original = posts::Entity::find_by_id(draft.id)
            .one(&ctx.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(original.title, draft.title);
        for id in ["-1", "9223372036854775807"] {
            let path = format!("/api/me/posts/{id}");
            request
                .get(&path)
                .authorization_bearer(&token)
                .await
                .assert_status_not_found();
            request
                .patch(&path)
                .authorization_bearer(&token)
                .json(&json!({"title":"Missing"}))
                .await
                .assert_status_not_found();
            request
                .delete(&path)
                .authorization_bearer(&token)
                .await
                .assert_status_not_found();
        }
        let response = request
            .get("/api/me/posts?author_id=99999&q=missing&category=missing&tag=missing")
            .authorization_bearer(&token)
            .await;
        response.assert_status_ok();
        let body: Value = response.json();
        assert_eq!(body["total"], 2);
        assert_eq!(body["page"], 1);
        assert_eq!(body["limit"], 20);
        let author_id = users::Model::find_by_email(&ctx.db, "author@example.com")
            .await
            .unwrap()
            .id;
        for post in body["data"].as_array().unwrap() {
            assert_eq!(post["author"]["id"], author_id);
        }
        let response = request
            .get("/api/me/posts?status=%20draft%20&limit=1")
            .authorization_bearer(&token)
            .await;
        response.assert_status_ok();
        let body: Value = response.json();
        assert_eq!(body["total"], 1);
        assert_eq!(body["data"][0]["id"], draft.id);
        let response = request
            .get("/api/me/posts?status=draft&page=2&limit=1")
            .authorization_bearer(&token)
            .await;
        let body: Value = response.json();
        assert_eq!(body["total"], 1);
        assert_eq!(body["data"], json!([]));
        for query in [
            "status=pending",
            "page=0",
            "limit=101",
            "author_id=-1",
            "page=9223372036854775807&limit=100",
        ] {
            let response = request
                .get(&format!("/api/me/posts?{query}"))
                .authorization_bearer(&token)
                .await;
            assert_eq!(response.status_code(), 422, "{query}");
        }
    })
    .await;
}

#[tokio::test]
#[serial]
async fn slug_generation_explicit_conflicts_and_patch_fields_match_contract() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let token = login(&request, "author@example.com").await;
        let mut ids = Vec::new();
        for slug in ["same-title", "same-title-2", "same-title-3"] {
            let response = request
                .post("/api/posts")
                .authorization_bearer(&token)
                .json(&json!({"title":"Same Title","content_html":"body","status":"published"}))
                .await;
            assert_eq!(response.status_code(), 201, "{}", response.text());
            let body: Value = response.json();
            assert_eq!(body["post"]["slug"], slug);
            assert!(body["post"]["published_at"].is_string());
            ids.push(body["post"]["id"].as_i64().unwrap());
        }
        let response = request
            .post("/api/posts")
            .authorization_bearer(&token)
            .json(&json!({"title":"Explicit","slug":" SAME TITLE ","content_html":"body"}))
            .await;
        assert_eq!(response.status_code(), 409);
        let body: Value = response.json();
        assert_eq!(body["message"], "slug already exists");
        let path = format!("/api/me/posts/{}", ids[0]);
        request
            .patch(&path)
            .authorization_bearer(&token)
            .json(&json!({"slug":"same-title"}))
            .await
            .assert_status_ok();
        let response = request
            .patch(&path)
            .authorization_bearer(&token)
            .json(&json!({"slug":"same-title-2","title":"Should roll back"}))
            .await;
        assert_eq!(response.status_code(), 409);
        let stored = posts::Entity::find_by_id(ids[0])
            .one(&ctx.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.title, "Same Title");
        let response = request
            .patch(&path)
            .authorization_bearer(&token)
            .json(&json!({"slug":"","title":"Replacement Title"}))
            .await;
        response.assert_status_ok();
        let body: Value = response.json();
        assert_eq!(body["post"]["slug"], "replacement-title");
        let response = request
            .post("/api/posts")
            .authorization_bearer(&token)
            .json(&json!({"title":"こんにちは","content_html":"body"}))
            .await;
        assert_eq!(response.status_code(), 201, "{}", response.text());
        let body: Value = response.json();
        assert_eq!(body["post"]["slug"], "item");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn invalid_payloads_and_relations_leave_posts_and_tags_unchanged() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let token = login(&request, "author@example.com").await;
        let row = posts::Entity::find()
            .filter(posts::Column::Slug.eq("getting-started-with-goravel"))
            .one(&ctx.db)
            .await
            .unwrap()
            .unwrap();
        let path = format!("/api/me/posts/{}", row.id);
        let tag = tags::Entity::find().one(&ctx.db).await.unwrap().unwrap();
        for patch in [
            json!({"title":""}),
            json!({"content_html":null}),
            json!({"status":""}),
            json!({"status":"pending"}),
            json!({"excerpt":"x".repeat(501)}),
            json!({"slug":"x".repeat(256)}),
            json!({"cover_image_url":"javascript:alert(1)"}),
            json!({"cover_image_url":"https:///"}),
            json!({"tag_ids":[0]}),
            json!({"tag_ids":[-1]}),
            json!({"tag_ids":[tag.id,tag.id]}),
            json!({"tag_ids":(1..=21).collect::<Vec<i64>>()}),
            json!({}),
        ] {
            let response = request
                .patch(&path)
                .authorization_bearer(&token)
                .json(&patch)
                .await;
            assert_eq!(response.status_code(), 422, "{patch}");
            let body: Value = response.json();
            assert_eq!(body["message"], "Validation failed");
        }
        for payload in [
            json!({}),
            json!({"title":"","content_html":"body"}),
            json!({"title":"x".repeat(256),"content_html":"body"}),
            json!({"title":"Valid","content_html":"x".repeat(100001)}),
        ] {
            let response = request
                .post("/api/posts")
                .authorization_bearer(&token)
                .json(&payload)
                .await;
            assert_eq!(response.status_code(), 422);
        }
        for payload in [
            json!([]),
            json!(null),
            json!({"title":123,"content_html":"body"}),
            json!({"title":"Title","content_html":"body","tag_ids":"invalid"}),
        ] {
            let response = request
                .post("/api/posts")
                .authorization_bearer(&token)
                .json(&payload)
                .await;
            assert_eq!(response.status_code(), 400);
        }
        request
            .post("/api/posts")
            .authorization_bearer(&token)
            .content_type("application/json")
            .text("{")
            .await
            .assert_status_bad_request();
        let before_tags = post_tags::Entity::find()
            .filter(post_tags::Column::PostId.eq(row.id))
            .count(&ctx.db)
            .await
            .unwrap();
        for relation in [
            json!({"category_id":9223372036854775807_i64}),
            json!({"tag_ids":[9223372036854775807_i64]}),
        ] {
            let mut patch = relation.clone();
            patch["title"] = json!("Must not persist");
            let response = request
                .patch(&path)
                .authorization_bearer(&token)
                .json(&patch)
                .await;
            assert_eq!(response.status_code(), 422);
            let body: Value = response.json();
            assert!(body["errors"]["relationships"].is_object());
            let mut payload = relation;
            payload["title"] = json!("Must not insert");
            payload["content_html"] = json!("body");
            let response = request
                .post("/api/posts")
                .authorization_bearer(&token)
                .json(&payload)
                .await;
            assert_eq!(response.status_code(), 422);
        }
        assert_eq!(posts::Entity::find().count(&ctx.db).await.unwrap(), 3);
        assert_eq!(
            posts::Entity::find_by_id(row.id)
                .one(&ctx.db)
                .await
                .unwrap()
                .unwrap()
                .title,
            row.title
        );
        assert_eq!(
            post_tags::Entity::find()
                .filter(post_tags::Column::PostId.eq(row.id))
                .count(&ctx.db)
                .await
                .unwrap(),
            before_tags
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn content_limit_accepts_100000_characters_on_create_and_update() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let token = login(&request, "author@example.com").await;
        let content = "x".repeat(100_000);
        let response = request
            .post("/api/posts")
            .authorization_bearer(&token)
            .json(&json!({"title":"Content limit", "content_html":content}))
            .await;
        assert_eq!(
            response.status_code(),
            201,
            "Goravel permits 100000 characters: {}",
            response.text()
        );
        let body: Value = response.json();
        let id = body["post"]["id"].as_i64().unwrap();
        assert_eq!(body["post"]["content_html"], content);
        let content = "ế".repeat(100_000);
        let response = request
            .patch(&format!("/api/me/posts/{id}"))
            .authorization_bearer(&token)
            .json(&json!({"content_html":content}))
            .await;
        assert_eq!(
            response.status_code(),
            200,
            "the limit counts characters, not UTF-8 bytes: {}",
            response.text()
        );
        let stored = posts::Entity::find_by_id(id)
            .one(&ctx.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.content_html, content);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn content_migration_preserves_rows_and_guards_lossy_mysql_rollback() {
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{DatabaseBackend, Statement};

    let boot = boot_test::<App>().await.unwrap();
    let ctx = &boot.app_context;
    seed(ctx).await;
    let original = posts::Entity::find().one(&ctx.db).await.unwrap().unwrap();
    let content = "ế".repeat(100_000);
    let mut large = original.clone().into_active_model();
    large.content_html = Set(content.clone());
    large.update(&ctx.db).await.unwrap();

    let result = Migrator::down(&ctx.db, Some(1)).await;
    if ctx.db.get_database_backend() == DatabaseBackend::MySql {
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Cannot restore TEXT"));
    } else {
        // The new migration is a no-op on backends whose TEXT is already unbounded.
        result.unwrap();
    }
    let preserved = posts::Entity::find_by_id(original.id)
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(preserved.content_html, content);
    assert_eq!(posts::Entity::find().count(&ctx.db).await.unwrap(), 3);
    let mut small = preserved.into_active_model();
    small.content_html = Set(original.content_html.clone());
    small.update(&ctx.db).await.unwrap();
    if ctx.db.get_database_backend() == DatabaseBackend::MySql {
        Migrator::down(&ctx.db, Some(1)).await.unwrap();
        let column = ctx.db.query_one_raw(Statement::from_string(DatabaseBackend::MySql,
            "SELECT DATA_TYPE FROM information_schema.COLUMNS WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME='posts' AND COLUMN_NAME='content_html'"
        )).await.unwrap().unwrap();
        assert_eq!(column.try_get::<String>("", "DATA_TYPE").unwrap(), "text");
    }
    Migrator::up(&ctx.db, Some(1)).await.unwrap();
    let preserved = posts::Entity::find_by_id(original.id)
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(preserved.content_html, original.content_html);
    if ctx.db.get_database_backend() == DatabaseBackend::MySql {
        let column = ctx.db.query_one_raw(Statement::from_string(DatabaseBackend::MySql,
            "SELECT DATA_TYPE FROM information_schema.COLUMNS WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME='posts' AND COLUMN_NAME='content_html'"
        )).await.unwrap().unwrap();
        assert_eq!(
            column.try_get::<String>("", "DATA_TYPE").unwrap(),
            "longtext"
        );
    }
}
