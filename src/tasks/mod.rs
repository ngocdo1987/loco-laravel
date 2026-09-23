//! Generator landing zone — intentionally empty.
//!
//! `cargo loco generate task <name>` always writes to
//! `src/tasks/<name>.rs` and injects a `pub mod` line here; it errors out if
//! this file doesn't exist. After generating, move the new file's content
//! into the right `src/modules/<name>/tasks.rs`, then remove the `pub mod`
//! line this generator added here and delete the leftover
//! `src/tasks/<name>.rs`. See `docs/modules.md`.
