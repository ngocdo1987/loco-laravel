use loco_laravel::app::App;
use loco_rs::testing::prelude::*;
use serial_test::serial;

/// Exercises the home page route: it must exist (no more falling through to
/// the router-level 404 fallback) and list the demo/API links.
#[tokio::test]
#[serial]
async fn can_get_home_view() {
    request::<App, _, _>(|request, _ctx| async move {
        let res = request.get("/").await;

        assert_eq!(res.status_code(), 200);
        assert!(res.text().contains("/hello"));
        assert!(res.text().contains("/api/auth/register"));
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
