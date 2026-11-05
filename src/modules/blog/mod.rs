mod author;
mod comments;
mod controllers;
mod dashboard;
pub mod models;
mod requests;
pub mod seeders;
mod taxonomy;
mod uploads;
mod web;

pub struct BlogModule;

impl super::Module for BlogModule {
    fn name(&self) -> &'static str {
        "blog"
    }

    fn routes(&self) -> Vec<loco_rs::controller::Routes> {
        use loco_rs::prelude::*;
        vec![
            Routes::new()
                .prefix("/api")
                .add("/posts", get(controllers::index))
                .add("/posts", post(author::store))
                .add("/me/posts", get(author::index))
                .add("/me/posts/{id}", get(author::show))
                .add("/me/posts/{id}", patch(author::update))
                .add("/me/posts/{id}", delete(author::destroy))
                .add("/me/posts/{id}/cover", post(uploads::cover))
                .add("/uploads/images", post(uploads::image))
                .add("/uploads/post-covers", post(uploads::post_cover))
                .add("/posts/{slug}", get(controllers::show))
                .add("/posts/{slug}/comments", get(controllers::comments))
                .add("/posts/{slug}/comments", post(comments::submit))
                .add("/comments/{id}", delete(comments::delete_own))
                .add("/me/posts/{id}/comments", get(comments::queue))
                .add(
                    "/me/posts/{id}/comments/{comment_id}",
                    patch(comments::moderate),
                )
                .add(
                    "/me/posts/{id}/comments/{comment_id}",
                    delete(comments::delete_moderated),
                )
                .add("/categories", get(controllers::categories))
                .add("/categories", post(taxonomy::create_category))
                .add("/categories/{id}", patch(taxonomy::update_category))
                .add("/categories/{id}", delete(taxonomy::delete_category))
                .add("/tags", get(controllers::tags))
                .add("/tags", post(taxonomy::create_tag))
                .add("/tags/{id}", patch(taxonomy::update_tag))
                .add("/tags/{id}", delete(taxonomy::delete_tag)),
            Routes::new()
                .add("/", get(web::home))
                .add("/posts/{slug}", get(web::show_post))
                .add("/posts/{slug}/comments", post(web::submit_comment))
                .add("/categories/{slug}", get(web::category))
                .add("/tags/{slug}", get(web::tag))
                .add("/storage/{*path}", get(web::storage_asset)),
            // goravel `RegisterWebRoutes`' auth-guarded dashboard group.
            // (`GET /dashboard` itself stays the frozen auth module's
            // post-login landing page — see the module docs.)
            Routes::new()
                .prefix("/dashboard")
                .add("/posts", get(dashboard::posts))
                .add("/posts/new", get(dashboard::new_post))
                .add("/posts", post(dashboard::create_post))
                .add("/posts/{id}/edit", get(dashboard::edit_post))
                .add("/posts/{id}", post(dashboard::update_post))
                .add("/posts/{id}/delete", post(dashboard::delete_post))
                .add("/categories", get(dashboard::categories))
                .add("/categories", post(dashboard::create_category))
                .add("/categories/{id}", post(dashboard::update_category))
                .add("/categories/{id}/delete", post(dashboard::delete_category))
                .add("/tags", get(dashboard::tags))
                .add("/tags", post(dashboard::create_tag))
                .add("/tags/{id}", post(dashboard::update_tag))
                .add("/tags/{id}/delete", post(dashboard::delete_tag))
                .add("/comments", get(dashboard::moderation))
                .add(
                    "/posts/{id}/comments/{comment_id}",
                    post(dashboard::moderate_comment),
                )
                .add(
                    "/posts/{id}/comments/{comment_id}/delete",
                    post(dashboard::delete_comment),
                ),
        ]
    }
}
