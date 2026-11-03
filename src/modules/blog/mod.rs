pub mod models;
pub mod seeders;

pub struct BlogModule;

impl super::Module for BlogModule {
    fn name(&self) -> &'static str {
        "blog"
    }

    /// S2 is models-only; S4+ registers the public/author API route groups
    /// here as separate `Routes` entries (one per prefix).
    fn routes(&self) -> Vec<loco_rs::controller::Routes> {
        Vec::new()
    }
}
