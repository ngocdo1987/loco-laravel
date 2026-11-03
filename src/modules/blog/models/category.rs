use loco_rs::prelude::*;

pub use crate::models::_entities::categories::{self, ActiveModel, Entity, Model};

impl Model {
    /// Finds a category by its (unique) slug.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::EntityNotFound`] when no category has the slug,
    /// or a DB error.
    pub async fn find_by_slug(db: &DatabaseConnection, slug: &str) -> ModelResult<Self> {
        let category = Entity::find()
            .filter(
                model::query::condition()
                    .eq(categories::Column::Slug, slug)
                    .build(),
            )
            .one(db)
            .await?;
        category.ok_or_else(|| ModelError::EntityNotFound)
    }
}
