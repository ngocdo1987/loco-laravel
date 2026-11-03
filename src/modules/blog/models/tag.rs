use loco_rs::prelude::*;

pub use crate::models::_entities::tags::{self, ActiveModel, Entity, Model};

impl Model {
    /// Finds a tag by its (unique) slug.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::EntityNotFound`] when no tag has the slug, or a
    /// DB error.
    pub async fn find_by_slug(db: &DatabaseConnection, slug: &str) -> ModelResult<Self> {
        let tag = Entity::find()
            .filter(
                model::query::condition()
                    .eq(tags::Column::Slug, slug)
                    .build(),
            )
            .one(db)
            .await?;
        tag.ok_or_else(|| ModelError::EntityNotFound)
    }
}
