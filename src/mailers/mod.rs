//! Generator landing zone — intentionally empty.
//!
//! `cargo loco generate mailer <name>` always writes to
//! `src/mailers/<name>.rs` (+ templates) and injects a `pub mod` line here;
//! it errors out if this file doesn't exist. After generating, move the new
//! file's content (and its template directory) into the right
//! `src/modules/<name>/mailer.rs` + `src/modules/<name>/mailer/`, updating
//! its `include_dir!` paths, then remove the `pub mod` line this generator
//! added here and delete the leftover `src/mailers/<name>.rs` (+
//! templates). See `docs/modules.md`.
