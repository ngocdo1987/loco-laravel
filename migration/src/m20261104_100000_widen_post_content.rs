use sea_orm_migration::{prelude::*, sea_orm::DatabaseBackend};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // PostgreSQL and SQLite TEXT already support the API's 100,000-character limit.
        if manager.get_database_backend() == DatabaseBackend::MySql {
            manager
                .get_connection()
                .execute_unprepared(
                    "ALTER TABLE `posts` MODIFY COLUMN `content_html` LONGTEXT NOT NULL",
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == DatabaseBackend::MySql {
            let db = manager.get_connection();
            let oversized = db
                .query_one_raw(sea_orm::Statement::from_string(
                    DatabaseBackend::MySql,
                    "SELECT 1 FROM posts WHERE OCTET_LENGTH(content_html) > 65535 LIMIT 1",
                ))
                .await?;
            if oversized.is_some() {
                return Err(DbErr::Custom(
                    "Cannot restore TEXT: posts contain HTML exceeding 65535 bytes".into(),
                ));
            }
            db.execute_unprepared("ALTER TABLE `posts` MODIFY COLUMN `content_html` TEXT NOT NULL")
                .await?;
        }
        Ok(())
    }
}
