//! Raises the axum body limit for multipart uploads.
//!
//! axum's `Multipart` extractor defaults to 2 MB; the blog allows 5 MB
//! uploads (checked at the application level). Config lives in code —
//! `config/*.yaml` is on the migration denylist.

use loco_rs::prelude::*;

use axum::extract::DefaultBodyLimit;
use axum::Router;

const LIMIT: usize = 8 * 1024 * 1024;

pub struct BodyLimitInitializer;

#[async_trait]
impl Initializer for BodyLimitInitializer {
    fn name(&self) -> String {
        "body-limit".to_owned()
    }

    async fn after_routes(&self, router: Router, _ctx: &AppContext) -> Result<Router> {
        Ok(router.layer(DefaultBodyLimit::max(LIMIT)))
    }
}
