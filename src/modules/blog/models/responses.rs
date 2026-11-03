//! Response DTOs — port of the response structs and constructors in
//! `goravel_base/app/modules/blog/models/blog.go`.
//!
//! Field names serialize `snake_case`, matching the goravel JSON tags
//! (`cover_image_url`, `content_html`, ...).

use sea_orm::entity::prelude::DateTimeWithTimeZone;
use serde::Serialize;

use super::post::PostWithRelations;
use crate::models::_entities::{categories, comments, tags, users};

/// The author slice exposed on post/comment responses (goravel
/// `AuthorResponse`: id + name only, never the whole user).
#[derive(Debug, Serialize)]
pub struct AuthorResponse {
    pub id: i64,
    pub name: String,
}

impl From<&users::Model> for AuthorResponse {
    fn from(author: &users::Model) -> Self {
        Self {
            id: author.id,
            name: author.name.clone(),
        }
    }
}

/// Public shape of a post (goravel `PostResponse`).
#[derive(Debug, Serialize)]
pub struct PostResponse {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub excerpt: Option<String>,
    pub cover_image_url: Option<String>,
    pub content_html: String,
    pub status: String,
    pub published_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub author: AuthorResponse,
    pub category: Option<categories::Model>,
    pub tags: Vec<tags::Model>,
}

impl From<PostWithRelations> for PostResponse {
    fn from(loaded: PostWithRelations) -> Self {
        let PostWithRelations {
            post,
            author,
            category,
            tags,
        } = loaded;
        Self {
            id: post.id,
            slug: post.slug,
            title: post.title,
            excerpt: post.excerpt,
            cover_image_url: post.cover_image_url,
            content_html: post.content_html,
            status: post.status,
            published_at: post.published_at,
            created_at: post.created_at,
            updated_at: post.updated_at,
            author: AuthorResponse::from(&author),
            category,
            tags,
        }
    }
}

/// Public shape of a comment (goravel `CommentResponse`).
///
/// `status` is omitted unless the caller is allowed to see it (moderation
/// views), mirroring `json:"status,omitempty"` + `NewCommentResponse(comment,
/// includeStatus)`.
#[derive(Debug, Serialize)]
pub struct CommentResponse {
    pub id: i64,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub author: AuthorResponse,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

impl CommentResponse {
    /// Builds a comment response; `include_status` controls whether the
    /// moderation status is exposed.
    #[must_use]
    pub fn new(comment: &comments::Model, author: &users::Model, include_status: bool) -> Self {
        Self {
            id: comment.id,
            content: comment.content.clone(),
            status: include_status.then(|| comment.status.clone()),
            author: AuthorResponse::from(author),
            created_at: comment.created_at,
            updated_at: comment.updated_at,
        }
    }
}
