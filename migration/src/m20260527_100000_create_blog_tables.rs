use loco_rs::schema::*;
use sea_orm_migration::prelude::*;

/// Port of `goravel_base` `20260527100000_create_blog_tables`
/// (read-only reference:
/// `/home/ubuntu/projects/GOLANG/goravel_base/database/migrations/`).
///
/// Type mapping (per STATE.md watch list):
/// - `table.ID()` / `UnsignedBigInteger` -> `big_pk_auto` / `big_integer`
///   (goravel uses `MySQL` uint IDs; loco convention is i64)
/// - `TimestampsTz` / `TimestampTz` -> `timestamp with time zone`
/// - `LongText` -> `text` (closest cross-dialect sea-query type)
/// - `status` stays `varchar(20) + default` exactly like goravel (string
///   statuses, not PG enums) so sqlite/mysql/postgres behave identically
///
/// All FKs are declared inline in `CREATE TABLE` (not `ALTER`ed in afterwards)
/// so they are enforced on every backend, including the sqlite scratch/test
/// DBs. `posts.category_id` and `post_tags.tag_id` keep goravel's
/// `RESTRICT ON DELETE`; every other FK is `CASCADE`, as in goravel.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        create_categories(m).await?;
        create_tags(m).await?;
        create_posts(m).await?;
        create_post_tags(m).await?;
        create_comments(m).await?;
        create_indexes(m).await?;
        Ok(())
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        // reverse dependency order, same as the goravel Down()
        drop_table(m, "comments").await?;
        drop_table(m, "post_tags").await?;
        drop_table(m, "posts").await?;
        drop_table(m, "tags").await?;
        drop_table(m, "categories").await?;
        Ok(())
    }
}

async fn create_categories(m: &SchemaManager<'_>) -> Result<(), DbErr> {
    // categories: id, name(100), slug(120) unique, timestamps
    create_table(
        m,
        "categories",
        &[
            ("id", ColType::PkAuto),
            ("name", ColType::StringLen(100)),
            ("slug", ColType::StringLenUniq(120)),
        ],
        &[],
    )
    .await
}

async fn create_tags(m: &SchemaManager<'_>) -> Result<(), DbErr> {
    // tags: same shape as categories
    create_table(
        m,
        "tags",
        &[
            ("id", ColType::PkAuto),
            ("name", ColType::StringLen(100)),
            ("slug", ColType::StringLenUniq(120)),
        ],
        &[],
    )
    .await
}

async fn create_posts(m: &SchemaManager<'_>) -> Result<(), DbErr> {
    // posts: built by hand instead of the `refs` helper because the
    // helper hardcodes CASCADE, while goravel RESTRICTs category deletes.
    let posts = Table::create()
        .table(Alias::new("posts"))
        .if_not_exists()
        .col(big_pk_auto(Alias::new("id")))
        .col(
            ColumnDef::new(Alias::new("author_id"))
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(Alias::new("category_id"))
                .big_integer()
                .null(),
        )
        .col(
            ColumnDef::new(Alias::new("slug"))
                .string_len(255)
                .not_null()
                .unique_key(),
        )
        .col(
            ColumnDef::new(Alias::new("title"))
                .string_len(255)
                .not_null(),
        )
        .col(ColumnDef::new(Alias::new("excerpt")).string_len(500).null())
        .col(
            ColumnDef::new(Alias::new("cover_image_url"))
                .string_len(2048)
                .null(),
        )
        .col(ColumnDef::new(Alias::new("content_html")).text().not_null())
        .col(
            ColumnDef::new(Alias::new("status"))
                .string_len(20)
                .not_null()
                .default("draft".to_string()),
        )
        .col(
            ColumnDef::new(Alias::new("published_at"))
                .timestamp_with_time_zone()
                .null(),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk-posts-author_id-to-users")
                .from(Alias::new("posts"), Alias::new("author_id"))
                .to(Alias::new("users"), Alias::new("id"))
                .on_delete(ForeignKeyAction::Cascade)
                .on_update(ForeignKeyAction::Cascade),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk-posts-category_id-to-categories")
                .from(Alias::new("posts"), Alias::new("category_id"))
                .to(Alias::new("categories"), Alias::new("id"))
                .on_delete(ForeignKeyAction::Restrict)
                .on_update(ForeignKeyAction::Cascade),
        )
        .to_owned();
    m.create_table(timestamps_tz(posts)).await
}

async fn create_post_tags(m: &SchemaManager<'_>) -> Result<(), DbErr> {
    // post_tags (m2m): composite PK (post_id, tag_id) — equivalent to
    // goravel's UNIQUE(post_id, tag_id) and friendlier to SeaORM entities.
    let mut pk = Index::create();
    pk.name("idx-post_tags-refs-pk")
        .table(Alias::new("post_tags"))
        .col(Alias::new("post_id"))
        .col(Alias::new("tag_id"));
    let post_tags = Table::create()
        .table(Alias::new("post_tags"))
        .if_not_exists()
        .col(
            ColumnDef::new(Alias::new("post_id"))
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(Alias::new("tag_id"))
                .big_integer()
                .not_null(),
        )
        .primary_key(&mut pk)
        .foreign_key(
            ForeignKey::create()
                .name("fk-post_tags-post_id-to-posts")
                .from(Alias::new("post_tags"), Alias::new("post_id"))
                .to(Alias::new("posts"), Alias::new("id"))
                .on_delete(ForeignKeyAction::Cascade)
                .on_update(ForeignKeyAction::Cascade),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk-post_tags-tag_id-to-tags")
                .from(Alias::new("post_tags"), Alias::new("tag_id"))
                .to(Alias::new("tags"), Alias::new("id"))
                .on_delete(ForeignKeyAction::Restrict)
                .on_update(ForeignKeyAction::Cascade),
        )
        .to_owned();
    m.create_table(post_tags).await
}

async fn create_comments(m: &SchemaManager<'_>) -> Result<(), DbErr> {
    // comments: post_id/author_id FKs via the helper (both CASCADE,
    // matching goravel; the helper declares them inline in CREATE TABLE)
    create_table(
        m,
        "comments",
        &[
            ("id", ColType::PkAuto),
            ("post_id", ColType::BigInteger),
            ("author_id", ColType::BigInteger),
            ("content", ColType::Text),
            (
                "status",
                ColType::StringLenWithDefault(20, "pending".to_string()),
            ),
        ],
        &[("posts", "post_id"), ("users", "author_id")],
    )
    .await
}

async fn create_indexes(m: &SchemaManager<'_>) -> Result<(), DbErr> {
    // secondary indexes, mirroring the goravel migration
    m.exec_stmt(
        Index::create()
            .name("idx-posts-status-published_at")
            .table(Alias::new("posts"))
            .col(Alias::new("status"))
            .col(Alias::new("published_at"))
            .to_owned(),
    )
    .await?;
    m.exec_stmt(
        Index::create()
            .name("idx-posts-author_id")
            .table(Alias::new("posts"))
            .col(Alias::new("author_id"))
            .to_owned(),
    )
    .await?;
    m.exec_stmt(
        Index::create()
            .name("idx-posts-category_id")
            .table(Alias::new("posts"))
            .col(Alias::new("category_id"))
            .to_owned(),
    )
    .await?;
    m.exec_stmt(
        Index::create()
            .name("idx-post_tags-tag_id")
            .table(Alias::new("post_tags"))
            .col(Alias::new("tag_id"))
            .to_owned(),
    )
    .await?;
    m.exec_stmt(
        Index::create()
            .name("idx-comments-post_id-status-created_at")
            .table(Alias::new("comments"))
            .col(Alias::new("post_id"))
            .col(Alias::new("status"))
            .col(Alias::new("created_at"))
            .to_owned(),
    )
    .await?;
    Ok(())
}
