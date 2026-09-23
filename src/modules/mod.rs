pub mod auth;

use loco_rs::controller::{AppRoutes, Routes};

/// A self-registering feature module: owns its routes (and, by convention,
/// its controllers/models/requests/views/mailer/tasks under
/// `src/modules/<name>/`).
pub trait Module {
    /// Bookkeeping/logging only today.
    fn name(&self) -> &'static str;
    fn routes(&self) -> Routes;
}

fn registry() -> Vec<Box<dyn Module>> {
    vec![Box::new(auth::AuthModule)]
}

/// Fold every registered module's routes into `AppRoutes`, replacing a
/// hand-maintained `.add_route(...)` chain in `app.rs` for feature modules.
#[must_use]
pub fn register_routes(mut routes: AppRoutes) -> AppRoutes {
    for module in registry() {
        routes = routes.add_route(module.routes());
    }
    routes
}
