#![allow(elided_lifetimes_in_paths)]
#![allow(clippy::wildcard_imports)]
pub use sea_orm_migration::prelude::*;
mod m20220101_000001_users;
mod m20260527_100000_create_blog_tables;
mod m20261104_100000_widen_post_content;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_users::Migration),
            Box::new(m20260527_100000_create_blog_tables::Migration),
            Box::new(m20261104_100000_widen_post_content::Migration),
            // inject-above (do not remove this comment)
        ]
    }
}
