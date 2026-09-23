pub mod controllers;
pub mod mailer;
pub mod models;
pub mod requests;
pub mod routes;
pub mod tasks;
pub mod views;

pub struct AuthModule;

impl super::Module for AuthModule {
    fn name(&self) -> &'static str {
        "auth"
    }

    fn routes(&self) -> loco_rs::controller::Routes {
        routes::routes()
    }
}
