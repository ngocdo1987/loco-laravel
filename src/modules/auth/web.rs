//! Browser-facing auth pages (login/register/dashboard), rendered with Tera.
//!
//! These reuse the exact same model/mailer logic as the JSON API in
//! `controllers.rs` — the only difference is that the issued JWT is carried
//! in an `auth_token` `HttpOnly` cookie instead of a JSON response body, so a
//! plain HTML form (not a JS client) can stay logged in across requests.
//! There is no separate session store: "is the visitor logged in" is
//! answered by validating that cookie's JWT, same as the API's `auth::JWT`
//! extractor does for the `Authorization` header.

use axum::response::Redirect;
use loco_rs::{auth::jwt, prelude::*};

use crate::models::_entities::users;

use super::mailer::AuthMailer;
use super::models::users::Model;
use super::requests::{LoginParams, RegisterParams};

const AUTH_COOKIE: &str = "auth_token";

/// Reads the `auth_token` cookie (if any), validates it as a JWT, and loads
/// the user it names. Returns `None` for a missing, malformed, or
/// no-longer-valid cookie — callers treat that exactly like "logged out"
/// rather than erroring, since an anonymous visitor is the normal case for
/// these pages.
async fn current_web_user(ctx: &AppContext, jar: &cookie::CookieJar) -> Option<Model> {
    let token = jar.get(AUTH_COOKIE)?.value().to_string();
    let jwt_secret = ctx.config.get_jwt_config().ok()?;
    let claims = jwt::JWT::new(&jwt_secret.secret).validate(&token).ok()?;
    Model::find_by_pid(&ctx.db, &claims.claims.pid).await.ok()
}

fn auth_cookie(token: String) -> cookie::Cookie<'static> {
    cookie::Cookie::build((AUTH_COOKIE, token))
        .http_only(true)
        .path("/")
        .same_site(cookie::SameSite::Lax)
        .build()
}

fn login_context(email: &str, error: Option<&str>) -> serde_json::Value {
    data!({ "email": email, "error": error })
}

fn register_context(name: &str, email: &str, error: Option<&str>) -> serde_json::Value {
    data!({ "name": name, "email": email, "error": error })
}

#[debug_handler]
pub(super) async fn login_form(
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<TeraView>,
    jar: cookie::CookieJar,
) -> Result<Response> {
    if current_web_user(&ctx, &jar).await.is_some() {
        return Ok(Redirect::to("/dashboard").into_response());
    }
    format::render().view(&v, "auth/login.html", login_context("", None))
}

#[debug_handler]
pub(super) async fn login_submit(
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<TeraView>,
    jar: cookie::CookieJar,
    Form(params): Form<LoginParams>,
) -> Result<Response> {
    if current_web_user(&ctx, &jar).await.is_some() {
        return Ok(Redirect::to("/dashboard").into_response());
    }

    let user = match users::Model::find_by_email(&ctx.db, &params.email).await {
        Ok(user) if user.verify_password(&params.password) => user,
        _ => {
            return format::render().view(
                &v,
                "auth/login.html",
                login_context(&params.email, Some("Invalid credentials")),
            );
        }
    };

    let jwt_secret = ctx.config.get_jwt_config()?;
    let Ok(token) = user.generate_jwt(&jwt_secret.secret, jwt_secret.expiration) else {
        return format::render().view(
            &v,
            "auth/login.html",
            login_context(&params.email, Some("Invalid credentials")),
        );
    };

    let jar = jar.add(auth_cookie(token));
    Ok((jar, Redirect::to("/dashboard")).into_response())
}

#[debug_handler]
pub(super) async fn register_form(
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<TeraView>,
    jar: cookie::CookieJar,
) -> Result<Response> {
    if current_web_user(&ctx, &jar).await.is_some() {
        return Ok(Redirect::to("/dashboard").into_response());
    }
    format::render().view(&v, "auth/register.html", register_context("", "", None))
}

#[debug_handler]
pub(super) async fn register_submit(
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<TeraView>,
    jar: cookie::CookieJar,
    Form(params): Form<RegisterParams>,
) -> Result<Response> {
    if current_web_user(&ctx, &jar).await.is_some() {
        return Ok(Redirect::to("/dashboard").into_response());
    }

    let user = match users::Model::create_with_password(&ctx.db, &params).await {
        Ok(user) => user,
        Err(ModelError::EntityAlreadyExists) => {
            return format::render().view(
                &v,
                "auth/register.html",
                register_context(
                    &params.name,
                    &params.email,
                    Some("Email already registered"),
                ),
            );
        }
        Err(_) => {
            return format::render().view(
                &v,
                "auth/register.html",
                register_context(
                    &params.name,
                    &params.email,
                    Some("Unable to register. Please check your details."),
                ),
            );
        }
    };

    let user = user
        .into_active_model()
        .set_email_verification_sent(&ctx.db)
        .await?;
    AuthMailer::send_welcome(&ctx, &user).await?;

    let jwt_secret = ctx.config.get_jwt_config()?;
    let token = user.generate_jwt(&jwt_secret.secret, jwt_secret.expiration)?;

    let jar = jar.add(auth_cookie(token));
    Ok((jar, Redirect::to("/dashboard")).into_response())
}

#[debug_handler]
pub(super) async fn logout(jar: cookie::CookieJar) -> Result<Response> {
    let jar = jar.remove(cookie::Cookie::from(AUTH_COOKIE));
    Ok((jar, Redirect::to("/login")).into_response())
}

#[debug_handler]
pub(super) async fn dashboard(
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<TeraView>,
    jar: cookie::CookieJar,
) -> Result<Response> {
    let Some(user) = current_web_user(&ctx, &jar).await else {
        return Ok(Redirect::to("/login").into_response());
    };

    format::render().view(
        &v,
        "auth/dashboard.html",
        data!({
            "name": user.name,
            "email": user.email,
            "pid": user.pid.to_string(),
        }),
    )
}
