use loco_rs::prelude::*;

use super::controllers::{
    current, forgot, login, magic_link, magic_link_verify, register, resend_verification_email,
    reset, verify,
};
use super::web;

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/api/auth")
        .add("/register", post(register))
        .add("/verify/{token}", get(verify))
        .add("/login", post(login))
        .add("/forgot", post(forgot))
        .add("/reset", post(reset))
        .add("/current", get(current))
        .add("/magic-link", post(magic_link))
        .add("/magic-link/{token}", get(magic_link_verify))
        .add("/resend-verification-mail", post(resend_verification_email))
}

/// Browser-facing pages (root-level, not under `/api`).
///
/// The Loco/Tera equivalent of a Laravel/Goravel "web" auth controller:
/// same JWT issued by [`routes()`] above, just carried in an `auth_token`
/// cookie instead of a JSON body.
pub fn web_routes() -> Routes {
    Routes::new()
        .add("/login", get(web::login_form))
        .add("/login", post(web::login_submit))
        .add("/register", get(web::register_form))
        .add("/register", post(web::register_submit))
        .add("/logout", post(web::logout))
        .add("/dashboard", get(web::dashboard))
}
