//! Re-export shim for the users entity.
//!
//! The `ActiveModelBehavior` implementation for users lives in
//! `src/modules/auth/models/users.rs` (module-first convention — the auth
//! module owns user behavior such as validation and pid generation).
//!
//! This file exists so that `cargo loco db entities` (loco 1.1 writes a
//! `src/models/<table>.rs` wrapper only when the file does not exist) keeps
//! leaving users to the auth module instead of writing a conflicting
//! `ActiveModelBehavior` impl here.
pub use super::_entities::users::{ActiveModel, Entity, Model};
pub type Users = Entity;
