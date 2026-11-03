use chrono::SubsecRound;
use loco_laravel::{
    app::App,
    modules::auth::{models::users, requests::RegisterParams},
};
use loco_laravel::{
    models::_entities::{categories, comments, post_tags, posts, tags},
    modules::blog::seeders,
};
use loco_rs::{app::Hooks, prelude::*};
use sea_orm::PaginatorTrait;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn seed_preserves_fixtures_and_advances_generated_ids() {
    let boot = boot_test::<App>().await.unwrap();
    let ctx = &boot.app_context;
    seed::<App>(ctx).await.expect("seed fixtures");

    let first = users::Model::find_by_email(&ctx.db, "user1@example.com")
        .await
        .unwrap();
    let second = users::Model::find_by_email(&ctx.db, "user2@example.com")
        .await
        .unwrap();
    assert_eq!((first.id, second.id), (1, 2));
    assert!(first.verify_password("12341234"));
    let fixture_time = chrono::DateTime::parse_from_rfc3339("2023-11-12T12:34:56.789Z").unwrap();
    // The current MySQL schema stores timestamps at whole-second precision.
    let expected_time = if ctx.db.get_database_backend() == sea_orm::DatabaseBackend::MySql {
        fixture_time.round_subsecs(0)
    } else {
        fixture_time
    };
    assert_eq!(first.created_at, expected_time);

    let created = users::Model::create_with_password(
        &ctx.db,
        &RegisterParams {
            email: "after-fixtures@example.com".to_string(),
            name: "After fixtures".to_string(),
            password: "12341234".to_string(),
        },
    )
    .await
    .expect("create user after explicit fixture IDs");
    assert!(created.id > second.id);
    assert!(users::Entity::find_by_id(first.id)
        .one(&ctx.db)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
#[serial]
async fn seed_propagates_duplicate_rows_and_missing_files() {
    let boot = boot_test::<App>().await.unwrap();
    let ctx = &boot.app_context;
    seed::<App>(ctx).await.unwrap();
    assert!(seed::<App>(ctx).await.is_err(), "duplicate IDs must fail");

    let missing = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    assert!(
        App::seed(ctx, &missing).await.is_err(),
        "missing files must fail"
    );
}

#[tokio::test]
#[serial]
async fn blog_seed_matches_goravel_data_and_relations() {
    let boot = boot_test::<App>().await.unwrap();
    let ctx = &boot.app_context;
    seed::<App>(ctx).await.unwrap();
    assert_eq!(categories::Entity::find().count(&ctx.db).await.unwrap(), 5);
    assert_eq!(tags::Entity::find().count(&ctx.db).await.unwrap(), 6);
    assert_eq!(posts::Entity::find().count(&ctx.db).await.unwrap(), 3);
    assert_eq!(comments::Entity::find().count(&ctx.db).await.unwrap(), 4);
    assert_eq!(post_tags::Entity::find().count(&ctx.db).await.unwrap(), 8);

    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../src/fixtures/blog.json")).unwrap();
    for item in fixture["users"].as_array().unwrap() {
        let user = users::Model::find_by_email(&ctx.db, item["email"].as_str().unwrap())
            .await
            .unwrap();
        assert_eq!(user.name, item["name"].as_str().unwrap());
        assert!(user.verify_password("Password123!"));
    }
    for item in fixture["categories"].as_array().unwrap() {
        let row = categories::Entity::find()
            .filter(categories::Column::Slug.eq(item["slug"].as_str().unwrap()))
            .one(&ctx.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.name, item["name"].as_str().unwrap());
    }
    for item in fixture["tags"].as_array().unwrap() {
        let row = tags::Entity::find()
            .filter(tags::Column::Slug.eq(item["slug"].as_str().unwrap()))
            .one(&ctx.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.name, item["name"].as_str().unwrap());
    }
    for item in fixture["posts"].as_array().unwrap() {
        let row = loco_laravel::modules::blog::models::post::Model::find_by_slug_with_relations(
            &ctx.db,
            item["slug"].as_str().unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(row.author.email, item["author_email"].as_str().unwrap());
        assert_eq!(
            row.category.unwrap().slug,
            item["category_slug"].as_str().unwrap()
        );
        assert_eq!(row.post.title, item["title"].as_str().unwrap());
        assert_eq!(row.post.excerpt.as_deref(), item["excerpt"].as_str());
        assert_eq!(
            row.post.cover_image_url.as_deref(),
            item["cover_image_url"].as_str()
        );
        assert_eq!(
            row.post.content_html,
            item["content_html"].as_str().unwrap()
        );
        assert_eq!(row.post.status, item["status"].as_str().unwrap());
        assert_eq!(row.post.published_at.is_some(), row.post.is_published());
        let mut actual: Vec<_> = row.tags.into_iter().map(|tag| tag.slug).collect();
        let mut expected: Vec<_> = item["tag_slugs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_string())
            .collect();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected);
    }
    for item in fixture["comments"].as_array().unwrap() {
        let row = comments::Entity::find()
            .filter(comments::Column::Content.eq(item["content"].as_str().unwrap()))
            .one(&ctx.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.status, item["status"].as_str().unwrap());
        let author = users::Entity::find_by_id(row.author_id)
            .one(&ctx.db)
            .await
            .unwrap()
            .unwrap();
        let post = posts::Entity::find_by_id(row.post_id)
            .one(&ctx.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(author.email, item["author_email"].as_str().unwrap());
        assert_eq!(post.slug, item["post_slug"].as_str().unwrap());
    }
}

#[tokio::test]
#[serial]
async fn blog_seed_reruns_restore_data_and_tags_without_duplicates() {
    let boot = boot_test::<App>().await.unwrap();
    let ctx = &boot.app_context;
    let base = std::path::Path::new("src/fixtures");
    seeders::seed(ctx, base).await.unwrap();
    let post = loco_laravel::modules::blog::models::post::Model::find_by_slug(
        &ctx.db,
        "getting-started-with-goravel",
    )
    .await
    .unwrap();
    let original_id = post.id;
    let created_at = post.created_at;
    let mut edited = post.into_active_model();
    edited.title = Set("Edited title".into());
    edited.status = Set("draft".into());
    edited.published_at = Set(None);
    edited.update(&ctx.db).await.unwrap();
    post_tags::Entity::delete_many()
        .filter(post_tags::Column::PostId.eq(original_id))
        .exec(&ctx.db)
        .await
        .unwrap();
    let category = categories::Entity::find()
        .filter(categories::Column::Slug.eq("engineering"))
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap();
    let mut edited = category.into_active_model();
    edited.name = Set("Edited category".into());
    edited.update(&ctx.db).await.unwrap();
    let comment = comments::Entity::find()
        .filter(comments::Column::Status.eq("pending"))
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap();
    let comment_id = comment.id;
    let mut edited = comment.into_active_model();
    edited.status = Set("approved".into());
    edited.update(&ctx.db).await.unwrap();
    seeders::seed(ctx, base).await.unwrap();
    let restored = loco_laravel::modules::blog::models::post::Model::find_by_slug_with_relations(
        &ctx.db,
        "getting-started-with-goravel",
    )
    .await
    .unwrap();
    assert_eq!(restored.post.id, original_id);
    assert_eq!(restored.post.created_at, created_at);
    assert_eq!(restored.post.title, "Getting Started with Goravel");
    assert!(restored.post.is_published());
    assert!(restored.post.published_at.is_some());
    assert_eq!(restored.category.unwrap().name, "Engineering");
    assert_eq!(restored.tags.len(), 3);
    assert_eq!(
        comments::Entity::find_by_id(comment_id)
            .one(&ctx.db)
            .await
            .unwrap()
            .unwrap()
            .status,
        "pending"
    );
    assert_eq!(users::Entity::find().count(&ctx.db).await.unwrap(), 3);
    assert_eq!(categories::Entity::find().count(&ctx.db).await.unwrap(), 5);
    assert_eq!(tags::Entity::find().count(&ctx.db).await.unwrap(), 6);
    assert_eq!(posts::Entity::find().count(&ctx.db).await.unwrap(), 3);
    assert_eq!(comments::Entity::find().count(&ctx.db).await.unwrap(), 4);
    assert_eq!(post_tags::Entity::find().count(&ctx.db).await.unwrap(), 8);
}

#[tokio::test]
#[serial]
async fn blog_seed_missing_dependency_rolls_back_all_changes() {
    let boot = boot_test::<App>().await.unwrap();
    let ctx = &boot.app_context;
    let mut fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../src/fixtures/blog.json")).unwrap();
    fixture["posts"][0]["tag_slugs"][0] = serde_json::json!("missing-tag");
    let base = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir(&base).unwrap();
    std::fs::write(
        base.join("blog.json"),
        serde_json::to_vec(&fixture).unwrap(),
    )
    .unwrap();
    let result = seeders::seed(ctx, &base).await;
    std::fs::remove_dir_all(&base).unwrap();
    assert!(result.unwrap_err().to_string().contains("missing-tag"));
    assert_eq!(users::Entity::find().count(&ctx.db).await.unwrap(), 0);
    assert_eq!(categories::Entity::find().count(&ctx.db).await.unwrap(), 0);
    assert_eq!(tags::Entity::find().count(&ctx.db).await.unwrap(), 0);
    assert_eq!(posts::Entity::find().count(&ctx.db).await.unwrap(), 0);
    assert_eq!(comments::Entity::find().count(&ctx.db).await.unwrap(), 0);
    assert_eq!(post_tags::Entity::find().count(&ctx.db).await.unwrap(), 0);
}
