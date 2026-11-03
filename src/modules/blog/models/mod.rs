//! Blog models — port of `goravel_base/app/modules/blog/models/blog.go`.
//!
//! `ActiveModelBehavior` for the blog entities lives in the shared
//! `src/models/{posts,categories,tags,comments,post_tags}.rs` wrappers
//! (loco 1.1 codegen contract, landed in S1) and only touches
//! `updated_at`; module models here add business logic on top and must NOT
//! re-implement `ActiveModelBehavior` (one impl per `ActiveModel` type).
//!
//! Cross-module note (STATE.md watch list): goravel's blog module depends
//! on auth for `User`; here the shared `crate::models::_entities::users`
//! is used directly, keeping modules decoupled.

pub mod category;
pub mod comment;
pub mod post;
pub mod responses;
pub mod tag;
