use loco_laravel::app::App;
use loco_rs::testing::prelude::*;
use serial_test::serial;

/// Exercises the home page route: since S9 the blog home (`modules::blog::web`)
/// serves `/` with the published-post feed — the starter landing page moved
/// aside (only `/hello` remains in `controllers::home`).
#[tokio::test]
#[serial]
async fn can_get_home_view() {
    request::<App, _, _>(|request, _ctx| async move {
        let res = request.get("/").await;

        assert_eq!(res.status_code(), 200);
        assert!(res.text().contains("Latest posts"));
    })
    .await;
}

/// Exercises the Tera view route end-to-end over HTTP: template lookup,
/// the i18n `t()` function, and the `ViewEngine` extractor wiring in
/// `app.rs` / `initializers::view_engine`.
#[tokio::test]
#[serial]
async fn can_get_hello_view() {
    request::<App, _, _>(|request, _ctx| async move {
        let res = request.get("/hello").await;

        assert_eq!(res.status_code(), 200);
        assert!(res.text().contains("Hello World"));
    })
    .await;
}
