mod author;
mod controllers;
pub mod models;
mod requests;
pub mod seeders;
mod taxonomy;

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
            .add("/posts", post(author::store))
            .add("/me/posts", get(author::index))
            .add("/me/posts/{id}", get(author::show))
            .add("/me/posts/{id}", patch(author::update))
            .add("/me/posts/{id}", delete(author::destroy))
            .add("/posts/{slug}", get(controllers::show))
            .add("/posts/{slug}/comments", get(controllers::comments))
            .add("/categories", get(controllers::categories))
            .add("/categories", post(taxonomy::create_category))
            .add("/categories/{id}", patch(taxonomy::update_category))
            .add("/categories/{id}", delete(taxonomy::delete_category))
            .add("/tags", get(controllers::tags))
            .add("/tags", post(taxonomy::create_tag))
            .add("/tags/{id}", patch(taxonomy::update_tag))
            .add("/tags/{id}", delete(taxonomy::delete_tag))]
    }
}
