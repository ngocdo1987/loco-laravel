pub mod auth;

use loco_rs::controller::{AppRoutes, Routes};

/// A self-registering feature module: owns its routes (and, by convention,
/// its controllers/models/requests/views/mailer/tasks under
/// `src/modules/<name>/`).
pub trait Module {
    /// Bookkeeping/logging only today.
    fn name(&self) -> &'static str;
    /// One entry per distinct route group the module owns (e.g. a
    /// prefixed JSON API plus root-level web pages), since a single
    /// `Routes` can only carry one `.prefix(...)`.
    fn routes(&self) -> Vec<Routes>;
}

fn registry() -> Vec<Box<dyn Module>> {
    vec![Box::new(auth::AuthModule)]
}

/// Fold every registered module's route groups into `AppRoutes`, replacing a
/// hand-maintained `.add_route(...)` chain in `app.rs` for feature modules.
#[must_use]
pub fn register_routes(mut routes: AppRoutes) -> AppRoutes {
    for module in registry() {
        for group in module.routes() {
            routes = routes.add_route(group);
        }
    }
    routes
}
