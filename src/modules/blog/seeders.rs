use loco_rs::prelude::*;

use crate::models::_entities::{categories, comments, post_tags, posts, tags, users};
use sea_orm::{ActiveValue::NotSet, DatabaseTransaction, TryIntoModel};
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct Fixtures {
    users: Vec<UserSeed>,
    categories: Vec<TaxonomySeed>,
    tags: Vec<TaxonomySeed>,
    posts: Vec<PostSeed>,
    comments: Vec<CommentSeed>,
}

#[derive(Deserialize)]
struct UserSeed {
    name: String,
    email: String,
}

#[derive(Deserialize)]
struct TaxonomySeed {
    name: String,
    slug: String,
}

#[derive(Deserialize)]
struct PostSeed {
    author_email: String,
    category_slug: String,
    tag_slugs: Vec<String>,
    slug: String,
    title: String,
    excerpt: String,
    #[serde(default)]
    cover_image_url: String,
    content_html: String,
    status: String,
}

#[derive(Deserialize)]
struct CommentSeed {
    post_slug: String,
    author_email: String,
    content: String,
    status: String,
}

/// Loads the Goravel blog demo fixtures, updating existing records by their
/// natural keys. Demo users use `Password123!`, as in Goravel's `UserSeeder`.
///
/// # Errors
/// Returns file, fixture parsing, missing dependency or database errors. All
/// changes are committed together so a failed seed leaves no partial blog.
pub async fn seed(ctx: &AppContext, base: &Path) -> Result<()> {
    let data = tokio::fs::read(base.join("blog.json")).await?;
    let fixtures: Fixtures = serde_json::from_slice(&data)?;
    let password = loco_rs::hash::hash_password("Password123!")?;
    let txn = ctx.db.begin().await?;
    let now: chrono::DateTime<chrono::FixedOffset> = chrono::Utc::now().into();

    for user in fixtures.users {
        let email = normalize(&user.email);
        let existing = users::Entity::find()
            .filter(users::Column::Email.eq(&email))
            .one(&txn)
            .await?;
        let mut model = existing.map_or_else(Default::default, IntoActiveModel::into_active_model);
        model.email = Set(email);
        model.name = Set(user.name.trim().to_string());
        model.password = Set(password.clone());
        model.save(&txn).await?;
    }

    seed_taxonomy(&txn, fixtures.categories, fixtures.tags, now).await?;
    for item in fixtures.posts {
        seed_post(&txn, item, now).await?;
    }
    for item in fixtures.comments {
        let post = posts::Entity::find()
            .filter(posts::Column::Slug.eq(normalize(&item.post_slug)))
            .one(&txn)
            .await?
            .ok_or_else(|| Error::string(&format!("seed post {:?} not found", item.post_slug)))?;
        let author_id = user_id(&txn, &item.author_email).await?;
        let content = item.content.trim().to_string();
        let existing = comments::Entity::find()
            .filter(comments::Column::PostId.eq(post.id))
            .filter(comments::Column::AuthorId.eq(author_id))
            .filter(comments::Column::Content.eq(&content))
            .one(&txn)
            .await?;
        let mut model = existing.map_or_else(Default::default, IntoActiveModel::into_active_model);
        model.post_id = Set(post.id);
        model.author_id = Set(author_id);
        model.content = Set(content);
        model.status = Set(item.status.trim().to_string());
        if model.id == NotSet {
            model.created_at = Set(now);
        }
        model.updated_at = Set(now);
        model.save(&txn).await?;
    }
    txn.commit().await?;
    Ok(())
}

fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

async fn user_id(txn: &DatabaseTransaction, email: &str) -> Result<i64> {
    let user = users::Entity::find()
        .filter(users::Column::Email.eq(normalize(email)))
        .one(txn)
        .await?
        .ok_or_else(|| Error::string(&format!("seed user {email:?} not found")))?;
    Ok(user.id)
}

async fn seed_post(
    txn: &DatabaseTransaction,
    item: PostSeed,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> Result<()> {
    let author_id = user_id(txn, &item.author_email).await?;
    let category = categories::Entity::find()
        .filter(categories::Column::Slug.eq(normalize(&item.category_slug)))
        .one(txn)
        .await?
        .ok_or_else(|| {
            Error::string(&format!("seed category {:?} not found", item.category_slug))
        })?;
    let slug = normalize(&item.slug);
    let existing = posts::Entity::find()
        .filter(posts::Column::Slug.eq(&slug))
        .one(txn)
        .await?;
    let mut model = existing.map_or_else(Default::default, IntoActiveModel::into_active_model);
    model.author_id = Set(author_id);
    model.category_id = Set(Some(category.id));
    model.slug = Set(slug);
    model.title = Set(item.title.trim().to_string());
    model.excerpt = Set(Some(item.excerpt));
    model.cover_image_url = Set(Some(item.cover_image_url));
    let status = item.status.trim();
    model.published_at = Set((status == super::models::post::STATUS_PUBLISHED).then_some(now));
    model.status = Set(status.to_string());
    model.content_html = Set(item.content_html);
    if model.id == NotSet {
        model.created_at = Set(now);
    }
    model.updated_at = Set(now);
    let post = model.save(txn).await?.try_into_model()?;

    post_tags::Entity::delete_many()
        .filter(post_tags::Column::PostId.eq(post.id))
        .exec(txn)
        .await?;
    for slug in item.tag_slugs {
        let tag = tags::Entity::find()
            .filter(tags::Column::Slug.eq(normalize(&slug)))
            .one(txn)
            .await?
            .ok_or_else(|| Error::string(&format!("seed tag {slug:?} not found")))?;
        post_tags::ActiveModel {
            post_id: Set(post.id),
            tag_id: Set(tag.id),
        }
        .insert(txn)
        .await?;
    }
    Ok(())
}

async fn seed_taxonomy(
    txn: &DatabaseTransaction,
    category_seeds: Vec<TaxonomySeed>,
    tag_seeds: Vec<TaxonomySeed>,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> Result<()> {
    for item in category_seeds {
        let slug = normalize(&item.slug);
        let existing = categories::Entity::find()
            .filter(categories::Column::Slug.eq(&slug))
            .one(txn)
            .await?;
        let mut model = existing.map_or_else(Default::default, IntoActiveModel::into_active_model);
        model.name = Set(item.name.trim().to_string());
        model.slug = Set(slug);
        if model.id == NotSet {
            model.created_at = Set(now);
        }
        model.updated_at = Set(now);
        model.save(txn).await?;
    }
    for item in tag_seeds {
        let slug = normalize(&item.slug);
        let existing = tags::Entity::find()
            .filter(tags::Column::Slug.eq(&slug))
            .one(txn)
            .await?;
        let mut model = existing.map_or_else(Default::default, IntoActiveModel::into_active_model);
        model.name = Set(item.name.trim().to_string());
        model.slug = Set(slug);
        if model.id == NotSet {
            model.created_at = Set(now);
        }
        model.updated_at = Set(now);
        model.save(txn).await?;
    }
    Ok(())
}
