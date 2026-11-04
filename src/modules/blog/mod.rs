mod controllers;
pub mod models;
pub mod seeders;

pub struct BlogModule;

impl super::Module for BlogModule {
    fn name(&self) -> &'static str {
        "blog"
    }

    fn routes(&self) -> Vec<loco_rs::controller::Routes> {
        use loco_rs::prelude::*;
        vec![Routes::new()
            .prefix("/api")
            .add("/posts", get(controllers::index))
            .add("/posts/{slug}", get(controllers::show))
            .add("/posts/{slug}/comments", get(controllers::comments))
            .add("/categories", get(controllers::categories))
            .add("/tags", get(controllers::tags))]
    }
}
