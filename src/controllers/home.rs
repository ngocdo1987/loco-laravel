use loco_rs::{
    app::Hooks,
    controller::views::{engines::TeraView, ViewEngine},
    prelude::*,
};

use crate::app::App;

/// Renders `assets/views/home/index.html`: the landing page listing the
/// app's demo route and available auth API endpoints.
#[debug_handler]
async fn index(ViewEngine(v): ViewEngine<TeraView>) -> Result<Response> {
    format::render().view(
        &v,
        "home/index.html",
        serde_json::json!({
            "app_name": App::app_name(),
            "app_version": App::app_version(),
        }),
    )
}

/// Renders `assets/views/home/hello.html` through the Tera view engine
/// registered in `initializers::view_engine` (Loco's equivalent of a Laravel
/// `Route::get(...) => view('home.hello')`), including the i18n `t()`
/// function the template calls.
#[debug_handler]
async fn hello(ViewEngine(v): ViewEngine<TeraView>) -> Result<Response> {
    format::render().view(&v, "home/hello.html", serde_json::json!({}))
}

pub fn routes() -> Routes {
    Routes::new().add("/", get(index)).add("/hello", get(hello))
}
