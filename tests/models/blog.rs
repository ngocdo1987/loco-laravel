use loco_laravel::app::App;
use loco_laravel::models::_entities::{post_tags, users};
use loco_laravel::modules::blog::models::{category, comment, post, responses, tag};
use loco_rs::prelude::*;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serial_test::serial;

/// Seed helpers keeping tests independent of the S3 fixtures; unique slugs
/// per test keep serial runs from colliding.
mod seed {
    use loco_laravel::models::_entities::users;
    use loco_laravel::modules::auth::models::users::ActiveModel as UserActiveModel;
    use loco_laravel::modules::blog::models::{category, post, tag};
    use sea_orm::{ActiveModelTrait, ActiveValue, DatabaseConnection};

    pub async fn user(db: &DatabaseConnection, email: &str, name: &str) -> users::Model {
        UserActiveModel {
            email: ActiveValue::Set(email.to_string()),
            name: ActiveValue::Set(name.to_string()),
            password: ActiveValue::Set("not-a-login-password".to_string()),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("seed user")
    }

    pub async fn category(db: &DatabaseConnection, name: &str, slug: &str) -> category::Model {
        category::ActiveModel {
            name: ActiveValue::Set(name.to_string()),
            slug: ActiveValue::Set(slug.to_string()),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("seed category")
    }

    pub async fn tag(db: &DatabaseConnection, name: &str, slug: &str) -> tag::Model {
        tag::ActiveModel {
            name: ActiveValue::Set(name.to_string()),
            slug: ActiveValue::Set(slug.to_string()),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("seed tag")
    }

    pub async fn post(
        db: &DatabaseConnection,
        author_id: i64,
        category_id: Option<i64>,
        slug: &str,
        status: Option<&str>,
    ) -> post::Model {
        post::ActiveModel {
            author_id: ActiveValue::Set(author_id),
            category_id: ActiveValue::Set(category_id),
            slug: ActiveValue::Set(slug.to_string()),
            title: ActiveValue::Set(format!("title for {slug}")),
            content_html: ActiveValue::Set("<p>hi</p>".to_string()),
            status: ActiveValue::Set(status.unwrap_or(post::STATUS_DRAFT).to_string()),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("seed post")
    }
}

async fn link_tag(db: &DatabaseConnection, post_id: i64, tag_id: i64) {
    post_tags::ActiveModel {
        post_id: sea_orm::ActiveValue::Set(post_id),
        tag_id: sea_orm::ActiveValue::Set(tag_id),
    }
    .insert(db)
    .await
    .expect("seed post_tag");
}

#[tokio::test]
#[serial]
async fn find_by_slug_returns_category_tag_and_post() {
    let boot = boot_test::<App>().await.expect("boot test app");
    let db = &boot.app_context.db;

    let author = seed::user(db, "author-2@t.dev", "Author Two").await;
    let category = seed::category(db, "Rust", "rust-2").await;
    let tag = seed::tag(db, "Intro", "intro-2").await;
    let post = seed::post(db, author.id, None, "post-slug-2", None).await;
    link_tag(db, post.id, tag.id).await;

    let found_category = category::Model::find_by_slug(db, "rust-2")
        .await
        .expect("category by slug");
    assert_eq!(found_category.id, category.id);

    let found_tag = tag::Model::find_by_slug(db, "intro-2")
        .await
        .expect("tag by slug");
    assert_eq!(found_tag.id, tag.id);

    let found_post = post::Model::find_by_slug(db, "post-slug-2")
        .await
        .expect("post by slug");
    assert_eq!(found_post.id, post.id);
    // schema default: drafts until explicitly published
    assert!(found_post.is_draft());
    assert!(!found_post.is_published());

    let missing = post::Model::find_by_slug(db, "does-not-exist").await;
    assert!(matches!(missing, Err(ModelError::EntityNotFound)));
}

#[tokio::test]
#[serial]
async fn post_with_relations_builds_public_response() {
    let boot = boot_test::<App>().await.expect("boot test app");
    let db = &boot.app_context.db;

    let author = seed::user(db, "author-3@t.dev", "Author Three").await;
    let category = seed::category(db, "Go", "go-3").await;
    let tag = seed::tag(db, "Deep", "deep-3").await;
    let post = seed::post(
        db,
        author.id,
        Some(category.id),
        "post-slug-3",
        Some(post::STATUS_PUBLISHED),
    )
    .await;
    link_tag(db, post.id, tag.id).await;

    let loaded = post::Model::find_by_slug_with_relations(db, "post-slug-3")
        .await
        .expect("load with relations");
    assert_eq!(loaded.category.as_ref().map(|c| c.id), Some(category.id));
    let response = responses::PostResponse::from(loaded);
    let json = serde_json::to_value(&response).expect("serialize response");

    assert_eq!(json["status"], post::STATUS_PUBLISHED);
    assert_eq!(json["author"]["name"], "Author Three");
    assert_eq!(json["category"]["slug"], "go-3");
    assert_eq!(json["tags"].as_array().map(std::vec::Vec::len), Some(1));
    assert_eq!(json["tags"][0]["slug"], "deep-3");
    // goravel JSON tags are snake_case — entity/DTO fields serialize the same
    assert!(json.get("cover_image_url").is_some());
    assert!(json.get("content_html").is_some());
}

#[tokio::test]
#[serial]
async fn comment_status_helpers_and_response_omit_status() {
    let boot = boot_test::<App>().await.expect("boot test app");
    let db = &boot.app_context.db;

    let author = seed::user(db, "commenter-4@t.dev", "Commenter").await;
    let post = seed::post(db, author.id, None, "post-slug-4", None).await;

    let comment = comment::ActiveModel {
        post_id: sea_orm::ActiveValue::Set(post.id),
        author_id: sea_orm::ActiveValue::Set(author.id),
        content: sea_orm::ActiveValue::Set("nice post".to_string()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("seed comment");

    // schema default is pending
    assert!(comment.is_pending());
    assert!(!comment.is_approved());
    assert!(!comment.is_rejected());

    let loaded_author = users::Entity::find()
        .filter(users::Column::Id.eq(author.id))
        .one(db)
        .await
        .expect("load author")
        .expect("author exists");

    let public_view = responses::CommentResponse::new(&comment, &loaded_author, false);
    let json = serde_json::to_value(&public_view).expect("serialize");
    assert!(json.get("status").is_none(), "public view omits status");
    assert_eq!(json["content"], "nice post");
    assert_eq!(json["author"]["name"], "Commenter");

    let moderation_view = responses::CommentResponse::new(&comment, &loaded_author, true);
    let json = serde_json::to_value(&moderation_view).expect("serialize");
    assert_eq!(json["status"], comment::STATUS_PENDING);
}
