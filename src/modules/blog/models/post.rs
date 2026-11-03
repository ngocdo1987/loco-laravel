use loco_rs::prelude::*;

use crate::models::_entities::{categories, tags, users};

pub use crate::models::_entities::posts::{self, ActiveModel, Entity, Model};

/// Post status values (goravel `PostStatusDraft` / `PostStatusPublished`).
pub const STATUS_DRAFT: &str = "draft";
pub const STATUS_PUBLISHED: &str = "published";

/// A post with its author, category and tags loaded — the shape the public
/// read API (S4) returns. Mirrors goravel's preloaded `Post.Author`,
/// `Post.Category`, `Post.Tags`.
pub struct PostWithRelations {
    pub post: Model,
    pub author: users::Model,
    pub category: Option<categories::Model>,
    pub tags: Vec<tags::Model>,
}

impl Model {
    /// True when the post is a draft (never shown by public reads).
    #[must_use]
    pub fn is_draft(&self) -> bool {
        self.status == STATUS_DRAFT
    }

    /// True when the post is published.
    #[must_use]
    pub fn is_published(&self) -> bool {
        self.status == STATUS_PUBLISHED
    }

    /// Finds a post by its (unique) slug, any status included — callers
    /// decide whether drafts are allowed.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::EntityNotFound`] when no post has the slug, or a
    /// DB error.
    pub async fn find_by_slug(db: &DatabaseConnection, slug: &str) -> ModelResult<Self> {
        let post = Entity::find()
            .filter(
                model::query::condition()
                    .eq(posts::Column::Slug, slug)
                    .build(),
            )
            .one(db)
            .await?;
        post.ok_or_else(|| ModelError::EntityNotFound)
    }

    /// Finds a post by slug with author, category and tags loaded.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::EntityNotFound`] when the post (or its author,
    /// which the FK guarantees exists) is missing, or a DB error.
    pub async fn find_by_slug_with_relations(
        db: &DatabaseConnection,
        slug: &str,
    ) -> ModelResult<PostWithRelations> {
        let post = Self::find_by_slug(db, slug).await?;
        Self::load_relations(db, post).await
    }

    /// Loads author, category and tags for an already-fetched post.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::EntityNotFound`] when the author is missing, or
    /// a DB error.
    pub async fn load_relations(
        db: &DatabaseConnection,
        post: Self,
    ) -> ModelResult<PostWithRelations> {
        let author = users::Entity::find_by_id(post.author_id)
            .one(db)
            .await?
            .ok_or_else(|| ModelError::EntityNotFound)?;
        let category = match post.category_id {
            Some(category_id) => categories::Entity::find_by_id(category_id).one(db).await?,
            None => None,
        };
        let tags = post.find_related(tags::Entity).all(db).await?;
        Ok(PostWithRelations {
            post,
            author,
            category,
            tags,
        })
    }
}
