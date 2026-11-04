use loco_laravel::{
    app::App,
    models::_entities::{comments, posts},
    modules::blog::{models::post, seeders},
};
use loco_rs::prelude::*;
use serde_json::Value;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn public_posts_filter_paginate_and_load_relations() {
    request::<App, _, _>(|request, ctx| async move {
        seeders::seed(&ctx, std::path::Path::new("src/fixtures"))
            .await
            .unwrap();
        let first = post::Model::find_by_slug(&ctx.db, "getting-started-with-goravel")
            .await
            .unwrap();
        let mut dated = first.clone().into_active_model();
        dated.published_at = Set(Some(
            chrono::DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z").unwrap(),
        ));
        dated.update(&ctx.db).await.unwrap();
        let response = request.get("/api/posts").await;
        response.assert_status_ok();
        let body: Value = response.json();
        assert_eq!(body["total"], 2);
        assert_eq!(body["page"], 1);
        assert_eq!(body["limit"], 20);
        assert_eq!(body["data"][0]["slug"], "nuxt-frontend-for-goravel-api");
        for item in body["data"].as_array().unwrap() {
            assert_eq!(item["status"], "published");
            assert_eq!(item["author"].as_object().unwrap().len(), 2);
            assert!(item["category"].is_object());
            assert!(!item["tags"].as_array().unwrap().is_empty());
        }
        let filtered = request
            .get(&format!(
                "/api/posts?q=%20Goravel%20&category=%20engineering%20&tag=api&author_id={}",
                first.author_id
            ))
            .await;
        filtered.assert_status_ok();
        let filtered: Value = filtered.json();
        assert_eq!(filtered["total"], 1);
        assert_eq!(filtered["data"][0]["id"], first.id);
        for url in [
            "/api/posts?category=missing",
            "/api/posts?tag=missing",
            "/api/posts?q=nonexistent",
            "/api/posts?category=engineering&tag=nuxt",
            "/api/posts?author_id=9223372036854775807",
        ] {
            let response = request.get(url).await;
            response.assert_status_ok();
            let body: Value = response.json();
            assert_eq!(body["total"], 0, "{url}");
            assert_eq!(body["data"], serde_json::json!([]));
        }
        for term in ["small%20REST", "Laravel-style"] {
            let response = request.get(&format!("/api/posts?q={term}")).await;
            response.assert_status_ok();
            let body: Value = response.json();
            assert_eq!(body["total"], 1);
            assert_eq!(body["data"][0]["id"], first.id);
        }
        let response = request.get("/api/posts?page=2&limit=1").await;
        let body: Value = response.json();
        assert_eq!(body["page"], 2);
        assert_eq!(body["limit"], 1);
        assert_eq!(body["total"], 2);
        assert_eq!(body["data"][0]["id"], first.id);
        let response = request.get("/api/posts?page=3&limit=1").await;
        let body: Value = response.json();
        assert_eq!(body["total"], 2);
        assert_eq!(body["data"], serde_json::json!([]));
        // A matching draft must never leak through an OR search predicate or status query.
        let response = request.get("/api/posts?q=Checklist&status=draft").await;
        let body: Value = response.json();
        assert_eq!(body["total"], 0);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn public_show_and_comments_hide_drafts_and_moderation_data() {
    request::<App, _, _>(|request, ctx| async move {
        seeders::seed(&ctx, std::path::Path::new("src/fixtures"))
            .await
            .unwrap();
        for slug in ["missing", "draft-api-testing-checklist"] {
            for suffix in ["", "/comments"] {
                let response = request.get(&format!("/api/posts/{slug}{suffix}")).await;
                response.assert_status_not_found();
                let body: Value = response.json();
                assert_eq!(body["message"], "Resource not found");
            }
        }
        let response = request.get("/api/posts/getting-started-with-goravel").await;
        response.assert_status_ok();
        let body: Value = response.json();
        assert_eq!(body["title"], "Getting Started with Goravel");
        assert_eq!(body["author"]["name"], "Author User");
        assert_eq!(body["category"]["slug"], "engineering");
        assert_eq!(body["tags"].as_array().unwrap().len(), 3);
        for slug in [
            "getting-started-with-goravel",
            "nuxt-frontend-for-goravel-api",
        ] {
            let response = request.get(&format!("/api/posts/{slug}/comments")).await;
            response.assert_status_ok();
            let body: Value = response.json();
            assert_eq!(body["total"], 1);
            let comment = &body["data"][0];
            assert!(comment.get("status").is_none());
            assert_eq!(comment["author"].as_object().unwrap().len(), 2);
            assert_eq!(comment["author"]["name"], "Reader User");
            let response = request
                .get(&format!("/api/posts/{slug}/comments?page=2&limit=1"))
                .await;
            let body: Value = response.json();
            assert_eq!(body["total"], 1);
            assert_eq!(body["data"], serde_json::json!([]));
        }
        // Published posts with no category/tags/comments are valid.
        let row = post::Model::find_by_slug(&ctx.db, "draft-api-testing-checklist")
            .await
            .unwrap();
        let mut row = row.into_active_model();
        row.status = Set("published".into());
        row.category_id = Set(None);
        row.update(&ctx.db).await.unwrap();
        let response = request
            .get("/api/posts/draft-api-testing-checklist/comments")
            .await;
        let body: Value = response.json();
        assert_eq!(body["data"], serde_json::json!([]));
        let response = request.get("/api/posts/draft-api-testing-checklist").await;
        let body: Value = response.json();
        assert!(body["category"].is_null());
        assert_eq!(
            comments::Entity::find().all(&ctx.db).await.unwrap().len(),
            4
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn public_taxonomy_sorted_and_invalid_queries_rejected() {
    request::<App, _, _>(|request, ctx| async move {
        seeders::seed(&ctx, std::path::Path::new("src/fixtures"))
            .await
            .unwrap();
        for (url, count) in [("/api/categories", 5), ("/api/tags", 6)] {
            let response = request.get(url).await;
            response.assert_status_ok();
            let body: Value = response.json();
            let rows = body["data"].as_array().unwrap();
            assert_eq!(rows.len(), count);
            let names: Vec<_> = rows
                .iter()
                .map(|row| row["name"].as_str().unwrap())
                .collect();
            assert!(names.windows(2).all(|pair| pair[0] <= pair[1]));
        }
        for path in [
            "/api/posts",
            "/api/posts/getting-started-with-goravel/comments",
        ] {
            for query in [
                "page=invalid",
                "limit=invalid",
                "page=0",
                "page=-1",
                "limit=0",
                "limit=101",
                "author_id=-1",
                "page=9223372036854775807&limit=100",
            ] {
                let response = request.get(&format!("{path}?{query}")).await;
                assert_eq!(response.status_code(), 422, "{path}?{query}");
                let body: Value = response.json();
                assert_eq!(body["message"], "Validation failed");
                let field = if query.starts_with("author_id") {
                    "author_id"
                } else {
                    "pagination"
                };
                assert!(body["errors"][field]["validation"].is_string());
            }
        }
        let response = request
            .get("/api/posts?page=&limit=&author_id=invalid")
            .await;
        response.assert_status_ok();
        let body: Value = response.json();
        assert_eq!(body["page"], 1);
        assert_eq!(body["limit"], 20);
        assert_eq!(body["total"], 2);
        assert_eq!(posts::Entity::find().all(&ctx.db).await.unwrap().len(), 3);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn public_html_is_sanitized_in_feed_and_detail_without_mutating_storage() {
    request::<App, _, _>(|request, ctx| async move {
        seeders::seed(&ctx, std::path::Path::new("src/fixtures")).await.unwrap();
        let original = post::Model::find_by_slug(&ctx.db, "getting-started-with-goravel")
            .await.unwrap();
        let unsafe_html = r#"<p>Hello <strong>reader</strong></p><script>alert(1)</script><img src="https://example.com/a.png" onerror="alert(2)"><a href="javascript:alert(3)">bad link</a><a href="https://example.com">good link</a><iframe src="https://example.com"></iframe><svg onload="alert(4)"></svg>"#;
        let mut dirty = original.clone().into_active_model();
        dirty.content_html = Set(unsafe_html.into());
        dirty.update(&ctx.db).await.unwrap();
        let detail = request.get("/api/posts/getting-started-with-goravel").await;
        detail.assert_status_ok();
        let detail: Value = detail.json();
        let html = detail["content_html"].as_str().unwrap();
        assert!(html.contains("<p>Hello <strong>reader</strong></p>"));
        assert!(html.contains(r#"src="https://example.com/a.png""#));
        assert!(html.contains(r#"href="https://example.com""#));
        for unsafe_fragment in ["<script", "alert(", "onerror", "javascript:", "<iframe", "<svg", "onload"] {
            assert!(!html.contains(unsafe_fragment), "unsafe HTML fragment: {unsafe_fragment}");
        }
        let feed = request.get("/api/posts?category=engineering").await;
        feed.assert_status_ok();
        let feed: Value = feed.json();
        assert_eq!(feed["data"][0]["content_html"], detail["content_html"]);
        let stored = posts::Entity::find_by_id(original.id).one(&ctx.db).await.unwrap().unwrap();
        assert_eq!(stored.content_html, unsafe_html, "public reads must not rewrite stored content");
    }).await;
}
