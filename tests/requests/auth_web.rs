use axum::http::header;
use loco_laravel::{
    app::App,
    modules::auth::requests::{LoginParams, RegisterParams},
};
use loco_rs::testing::prelude::*;
use serial_test::serial;

/// Exercises the Tera-rendered login/register/dashboard pages: the JWT is
/// carried in an `auth_token` cookie instead of a JSON body, but it's the
/// exact same auth logic as `/api/auth/*` (see `src/modules/auth/web.rs`).
#[tokio::test]
#[serial]
async fn can_get_login_and_register_forms() {
    request::<App, _, _>(|request, _ctx| async move {
        let login = request.get("/login").await;
        assert_eq!(login.status_code(), 200);
        assert!(login.text().contains("name=\"password\""));

        let register = request.get("/register").await;
        assert_eq!(register.status_code(), 200);
        assert!(register.text().contains("name=\"name\""));
    })
    .await;
}

#[tokio::test]
#[serial]
async fn dashboard_redirects_to_login_when_logged_out() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/dashboard").await;

        assert_eq!(response.status_code(), 303);
        assert_eq!(response.header(header::LOCATION), "/login");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn can_register_login_and_reach_dashboard() {
    request::<App, _, _>(|request, _ctx| async move {
        let register_response = request
            .post("/register")
            .form(&RegisterParams {
                email: "web-user@loco.com".to_string(),
                password: "12341234".to_string(),
                name: "Web User".to_string(),
            })
            .await;

        assert_eq!(register_response.status_code(), 303);
        assert_eq!(register_response.header(header::LOCATION), "/dashboard");

        let auth_cookie = register_response.cookie("auth_token");

        let dashboard = request
            .get("/dashboard")
            .add_cookie(auth_cookie.clone())
            .await;
        assert_eq!(dashboard.status_code(), 200);
        assert!(dashboard.text().contains("Web User"));
        assert!(dashboard.text().contains("web-user@loco.com"));

        // Guest pages redirect an already-logged-in visitor away.
        let login_while_in = request.get("/login").add_cookie(auth_cookie.clone()).await;
        assert_eq!(login_while_in.status_code(), 303);
        assert_eq!(login_while_in.header(header::LOCATION), "/dashboard");

        let duplicate = request
            .post("/register")
            .form(&RegisterParams {
                email: "web-user@loco.com".to_string(),
                password: "12341234".to_string(),
                name: "Someone Else".to_string(),
            })
            .await;
        assert_eq!(duplicate.status_code(), 200);
        assert!(duplicate.text().contains("Email already registered"));

        let wrong_password = request
            .post("/login")
            .form(&LoginParams {
                email: "web-user@loco.com".to_string(),
                password: "wrong-password".to_string(),
            })
            .await;
        assert_eq!(wrong_password.status_code(), 200);
        assert!(wrong_password.text().contains("Invalid credentials"));

        let logout = request.post("/logout").add_cookie(auth_cookie).await;
        assert_eq!(logout.status_code(), 303);
        assert_eq!(logout.header(header::LOCATION), "/login");

        let login_response = request
            .post("/login")
            .form(&LoginParams {
                email: "web-user@loco.com".to_string(),
                password: "12341234".to_string(),
            })
            .await;
        assert_eq!(login_response.status_code(), 303);
        assert_eq!(login_response.header(header::LOCATION), "/dashboard");

        let dashboard_again = request
            .get("/dashboard")
            .add_cookie(login_response.cookie("auth_token"))
            .await;
        assert_eq!(dashboard_again.status_code(), 200);
    })
    .await;
}
