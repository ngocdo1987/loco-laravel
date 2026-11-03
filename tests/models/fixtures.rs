use chrono::SubsecRound;
use loco_laravel::{
    app::App,
    modules::auth::{models::users, requests::RegisterParams},
};
use loco_rs::{app::Hooks, prelude::*};
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
