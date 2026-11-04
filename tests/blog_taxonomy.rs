use loco_laravel::{
    app::App,
    models::_entities::{categories, post_tags, posts, tags},
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
        .json(&json!({"email":email,"password":"Password123!"}))
        .await;
    response.assert_status_ok();
    let body: Value = response.json();
    body["token"].as_str().unwrap().to_owned()
}

#[tokio::test]
#[serial]
async fn taxonomy_requires_a_valid_existing_user_for_every_write() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        for kind in ["categories", "tags"] {
            let path = format!("/api/{kind}");
            let item = format!("{path}/1");
            request.get(&path).await.assert_status_ok();
            request
                .post(&path)
                .json(&json!({}))
                .await
                .assert_status_unauthorized();
            request
                .patch(&item)
                .json(&json!({}))
                .await
                .assert_status_unauthorized();
            request.delete(&item).await.assert_status_unauthorized();
            request
                .post(&path)
                .authorization_bearer("invalid")
                .json(&json!({"name":"Invalid token"}))
                .await
                .assert_status_unauthorized();
            request
                .patch(&item)
                .authorization_bearer("invalid")
                .json(&json!({"name":"Invalid token"}))
                .await
                .assert_status_unauthorized();
            request
                .delete(&item)
                .authorization_bearer("invalid")
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
        for kind in ["categories", "tags"] {
            let path = format!("/api/{kind}");
            let item = format!("{path}/1");
            request
                .post(&path)
                .authorization_bearer(&token)
                .json(&json!({"name":"Deleted user"}))
                .await
                .assert_status_unauthorized();
            request
                .patch(&item)
                .authorization_bearer(&token)
                .json(&json!({"name":"Deleted user"}))
                .await
                .assert_status_unauthorized();
            request
                .delete(&item)
                .authorization_bearer(&token)
                .await
                .assert_status_unauthorized();
        }
        assert_eq!(categories::Entity::find().count(&ctx.db).await.unwrap(), 5);
        assert_eq!(tags::Entity::find().count(&ctx.db).await.unwrap(), 6);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn taxonomy_crud_returns_entities_and_is_shared_between_authenticated_users() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let author = login(&request, "author@example.com").await;
        let reader = login(&request, "reader@example.com").await;
        for kind in ["categories", "tags"] {
            let path = format!("/api/{kind}");
            let response = request.post(&path).authorization_bearer(&author)
                .json(&json!({"name":"  New Taxonomy!  ", "id":999999,"slug":"ignored","created_at":"2000-01-01T00:00:00Z"})).await;
            assert_eq!(response.status_code(),201,"{}",response.text());
            let row: Value = response.json();
            let id = row["id"].as_i64().unwrap();
            assert_ne!(id,999999);
            assert_eq!(row["name"],"New Taxonomy!");
            assert_eq!(row["slug"],"new-taxonomy");
            assert_eq!(row.as_object().unwrap().len(),5);
            assert!(chrono::DateTime::parse_from_rfc3339(row["created_at"].as_str().unwrap()).is_ok());
            assert!(chrono::DateTime::parse_from_rfc3339(row["updated_at"].as_str().unwrap()).is_ok());
            let item = format!("{path}/{id}");
            let response = request.patch(&item).authorization_bearer(&reader)
                .json(&json!({"name":" Renamed Taxonomy ","id":999999,"slug":"ignored"})).await;
            response.assert_status_ok();
            let changed: Value = response.json();
            assert_eq!(changed["id"],id);
            assert_eq!(changed["name"],"Renamed Taxonomy");
            assert_eq!(changed["slug"],"renamed-taxonomy");
            assert_eq!(changed["created_at"],row["created_at"]);
            let response = request.get(&path).await;
            response.assert_status_ok();
            let list: Value = response.json();
            assert!(list["data"].as_array().unwrap().contains(&changed));
            let response = request.delete(&item).authorization_bearer(&reader).await;
            assert_eq!(response.status_code(),204);
            assert!(response.text().is_empty());
            let response = request.get(&path).await;
            let list: Value = response.json();
            assert!(!list["data"].as_array().unwrap().iter().any(|row| row["id"]==id));
            request.delete(&item).authorization_bearer(&author).await.assert_status_not_found();
        }
    }).await;
}

#[tokio::test]
#[serial]
async fn taxonomy_slug_conflicts_preserve_existing_rows_and_ids() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let token = login(&request, "author@example.com").await;
        for kind in ["categories", "tags"] {
            let path = format!("/api/{kind}");
            let response = request
                .post(&path)
                .authorization_bearer(&token)
                .json(&json!({"name":"Same Name"}))
                .await;
            assert_eq!(response.status_code(), 201);
            let first: Value = response.json();
            let first_path = format!("{path}/{}", first["id"].as_i64().unwrap());
            let response = request
                .post(&path)
                .authorization_bearer(&token)
                .json(&json!({"name":"  SAME---NAME!!! "}))
                .await;
            assert_eq!(response.status_code(), 409);
            let conflict: Value = response.json();
            assert_eq!(conflict["message"], "slug already exists");
            request
                .patch(&first_path)
                .authorization_bearer(&token)
                .json(&json!({"name":"Same Name"}))
                .await
                .assert_status_ok();
            let response = request
                .post(&path)
                .authorization_bearer(&token)
                .json(&json!({"name":"Other Name"}))
                .await;
            assert_eq!(response.status_code(), 201);
            let other: Value = response.json();
            let other_path = format!("{path}/{}", other["id"].as_i64().unwrap());
            let response = request
                .patch(&other_path)
                .authorization_bearer(&token)
                .json(&json!({"name":"Same Name"}))
                .await;
            assert_eq!(response.status_code(), 409);
            let response = request.get(&path).await;
            let list: Value = response.json();
            assert!(
                list["data"].as_array().unwrap().contains(&other),
                "failed rename must not change timestamps, name or slug"
            );
        }
    })
    .await;
}

#[tokio::test]
#[serial]
async fn taxonomy_validation_handles_unicode_boundaries_and_bad_payloads() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let token = login(&request, "author@example.com").await;
        for kind in ["categories", "tags"] {
            let path = format!("/api/{kind}");
            let response = request
                .post(&path)
                .authorization_bearer(&token)
                .json(&json!({"name":"Validation Target"}))
                .await;
            assert_eq!(response.status_code(), 201);
            let original: Value = response.json();
            let item = format!("{path}/{}", original["id"].as_i64().unwrap());
            for payload in [
                json!({}),
                json!({"name":null}),
                json!({"name":""}),
                json!({"name":" \t\n\u{2003}"}),
                json!({"name":"x".repeat(101)}),
                json!({"name":"ế".repeat(101)}),
            ] {
                for response in [
                    request
                        .post(&path)
                        .authorization_bearer(&token)
                        .json(&payload)
                        .await,
                    request
                        .patch(&item)
                        .authorization_bearer(&token)
                        .json(&payload)
                        .await,
                ] {
                    assert_eq!(
                        response.status_code(),
                        422,
                        "{payload}: {}",
                        response.text()
                    );
                    let body: Value = response.json();
                    assert_eq!(body["message"], "Validation failed");
                    assert!(body["errors"]["name"]["validation"].is_string());
                }
            }
            for payload in [
                json!([]),
                json!(null),
                json!({"name":123}),
                json!({"name":{}}),
            ] {
                request
                    .post(&path)
                    .authorization_bearer(&token)
                    .json(&payload)
                    .await
                    .assert_status_bad_request();
                request
                    .patch(&item)
                    .authorization_bearer(&token)
                    .json(&payload)
                    .await
                    .assert_status_bad_request();
            }
            request
                .post(&path)
                .authorization_bearer(&token)
                .content_type("application/json")
                .text("{")
                .await
                .assert_status_bad_request();
            request
                .patch(&item)
                .authorization_bearer(&token)
                .content_type("application/json")
                .text("{")
                .await
                .assert_status_bad_request();
            let response = request.get(&path).await;
            let list: Value = response.json();
            assert!(list["data"].as_array().unwrap().contains(&original));
            for (name, slug) in [
                ("a".repeat(100), "a".repeat(100)),
                ("İ".repeat(100), "i".repeat(100)),
                ("ế".repeat(100), "item".to_owned()),
            ] {
                let response = request
                    .post(&path)
                    .authorization_bearer(&token)
                    .json(&json!({"name":name}))
                    .await;
                assert_eq!(response.status_code(), 201, "{}", response.text());
                let body: Value = response.json();
                assert_eq!(body["name"], name);
                assert_eq!(body["slug"], slug);
            }
            let response = request
                .post(&path)
                .authorization_bearer(&token)
                .json(&json!({"name":"你好"}))
                .await;
            assert_eq!(
                response.status_code(),
                409,
                "fallback item slug must also be unique"
            );
        }
    })
    .await;
}

#[tokio::test]
#[serial]
async fn taxonomy_missing_ids_return_not_found_and_linked_taxonomy_cannot_be_deleted() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let token = login(&request, "author@example.com").await;
        for kind in ["categories", "tags"] {
            for id in [
                "0",
                "-1",
                "missing",
                "9223372036854775807",
                "9223372036854775808",
            ] {
                let item = format!("/api/{kind}/{id}");
                let response = request
                    .patch(&item)
                    .authorization_bearer(&token)
                    .json(&json!({"name":"Missing"}))
                    .await;
                response.assert_status_not_found();
                let body: Value = response.json();
                assert_eq!(body["message"], "Resource not found");
                let response = request.delete(&item).authorization_bearer(&token).await;
                response.assert_status_not_found();
            }
            let response = request
                .patch(&format!("/api/{kind}/0"))
                .authorization_bearer(&token)
                .json(&json!({}))
                .await;
            assert_eq!(
                response.status_code(),
                422,
                "validation precedes the missing-record lookup"
            );
        }
        let category = categories::Entity::find()
            .filter(categories::Column::Slug.eq("engineering"))
            .one(&ctx.db)
            .await
            .unwrap()
            .unwrap();
        let tag = tags::Entity::find()
            .filter(tags::Column::Slug.eq("go"))
            .one(&ctx.db)
            .await
            .unwrap()
            .unwrap();
        for (kind, id) in [("categories", category.id), ("tags", tag.id)] {
            let item = format!("/api/{kind}/{id}");
            let response = request.delete(&item).authorization_bearer(&token).await;
            assert_eq!(response.status_code(), 409);
            let body: Value = response.json();
            assert_eq!(body["message"], "taxonomy is assigned to a post");
            request
                .patch(&item)
                .authorization_bearer(&token)
                .json(&json!({"name":format!("Renamed {kind}")}))
                .await
                .assert_status_ok();
        }
        let response = request
            .get("/api/posts?category=renamed-categories&tag=renamed-tags")
            .await;
        response.assert_status_ok();
        let body: Value = response.json();
        assert_eq!(body["total"], 1);
        let first_id = body["data"][0]["id"].as_i64().unwrap();
        assert_eq!(body["data"][0]["category"]["id"], category.id);
        assert!(body["data"][0]["tags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == tag.id));
        request
            .patch(&format!("/api/me/posts/{first_id}"))
            .authorization_bearer(&token)
            .json(&json!({"category_id":null,"tag_ids":[]}))
            .await
            .assert_status_ok();
        request
            .delete(&format!("/api/categories/{}", category.id))
            .authorization_bearer(&token)
            .await
            .assert_status(axum::http::StatusCode::NO_CONTENT);
        let response = request
            .delete(&format!("/api/tags/{}", tag.id))
            .authorization_bearer(&token)
            .await;
        assert_eq!(
            response.status_code(),
            409,
            "a draft's tag assignment must also block deletion"
        );
        let draft = posts::Entity::find()
            .filter(posts::Column::Slug.eq("draft-api-testing-checklist"))
            .one(&ctx.db)
            .await
            .unwrap()
            .unwrap();
        request
            .patch(&format!("/api/me/posts/{}", draft.id))
            .authorization_bearer(&token)
            .json(&json!({"tag_ids":[]}))
            .await
            .assert_status_ok();
        request
            .delete(&format!("/api/tags/{}", tag.id))
            .authorization_bearer(&token)
            .await
            .assert_status(axum::http::StatusCode::NO_CONTENT);
        assert_eq!(posts::Entity::find().count(&ctx.db).await.unwrap(), 3);
        assert_eq!(
            post_tags::Entity::find()
                .filter(post_tags::Column::TagId.eq(tag.id))
                .count(&ctx.db)
                .await
                .unwrap(),
            0
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn concurrent_taxonomy_creation_returns_one_created_and_one_conflict() {
    request::<App, _, _>(|request, ctx| async move {
        seed(&ctx).await;
        let first_token = login(&request, "author@example.com").await;
        let second_token = login(&request, "reader@example.com").await;
        for kind in ["categories", "tags"] {
            let path = format!("/api/{kind}");
            let payload = json!({"name":"Concurrent Name"});
            let first = request
                .post(&path)
                .authorization_bearer(&first_token)
                .json(&payload);
            let second = request
                .post(&path)
                .authorization_bearer(&second_token)
                .json(&payload);
            let (first, second) = tokio::join!(async { first.await }, async { second.await });
            let mut statuses = [first.status_code().as_u16(), second.status_code().as_u16()];
            statuses.sort_unstable();
            assert_eq!(statuses, [201, 409]);
            let response = request.get(&path).await;
            let list: Value = response.json();
            assert_eq!(
                list["data"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|row| row["slug"] == "concurrent-name")
                    .count(),
                1
            );
        }
    })
    .await;
}
