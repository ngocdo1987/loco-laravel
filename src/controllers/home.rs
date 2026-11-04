use loco_rs::{
    controller::views::{engines::TeraView, ViewEngine},
    prelude::*,
};

/// Renders `assets/views/home/hello.html` through the Tera view engine
/// registered in `initializers::view_engine` (Loco's equivalent of a Laravel
/// `Route::get(...) => view('home.hello')`), including the i18n `t()`
/// function the template calls.
///
/// The blog home page took over the `/` route in S9 (see
/// `modules::blog::web`); only `/hello` remains here.
#[debug_handler]
async fn hello(ViewEngine(v): ViewEngine<TeraView>) -> Result<Response> {
    format::render().view(&v, "home/hello.html", serde_json::json!({}))
}

pub fn routes() -> Routes {
    Routes::new().add("/hello", get(hello))
}
