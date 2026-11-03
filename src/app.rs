use async_trait::async_trait;
use loco_rs::{
    app::{AppContext, Hooks, Initializer},
    bgworker::{BackgroundWorker, Queue},
    boot::{create_app, BootResult, StartMode},
    config::Config,
    controller::AppRoutes,
    db::{self, truncate_table},
    environment::Environment,
    task::Tasks,
    Result,
};
use migration::Migrator;
use std::path::Path;

#[allow(unused_imports)]
use crate::{
    controllers, initializers, models::_entities::users, modules,
    workers::downloader::DownloadWorker,
};

pub struct App;
#[async_trait]
impl Hooks for App {
    fn app_name() -> &'static str {
        env!("CARGO_CRATE_NAME")
    }

    fn app_version() -> String {
        format!(
            "{} ({})",
            env!("CARGO_PKG_VERSION"),
            option_env!("BUILD_SHA")
                .or(option_env!("GITHUB_SHA"))
                .unwrap_or("dev")
        )
    }

    async fn boot(
        mode: StartMode,
        environment: &Environment,
        config: Config,
    ) -> Result<BootResult> {
        create_app::<Self, Migrator>(mode, environment, config).await
    }

    async fn initializers(_ctx: &AppContext) -> Result<Vec<Box<dyn Initializer>>> {
        Ok(vec![Box::new(
            initializers::view_engine::ViewEngineInitializer,
        )])
    }

    fn routes(_ctx: &AppContext) -> AppRoutes {
        let routes = AppRoutes::with_default_routes().add_route(controllers::home::routes());
        modules::register_routes(routes)
    }
    async fn connect_workers(ctx: &AppContext, queue: &Queue) -> Result<()> {
        queue.register(DownloadWorker::build(ctx)).await?;
        Ok(())
    }

    #[allow(unused_variables)]
    fn register_tasks(tasks: &mut Tasks) {
        // tasks-inject (do not remove)
        tasks.register(modules::auth::tasks::UserCreate);
    }
    async fn truncate(ctx: &AppContext) -> Result<()> {
        truncate_table(&ctx.db, users::Entity).await?;
        Ok(())
    }
    async fn seed(ctx: &AppContext, base: &Path) -> Result<()> {
        let result =
            db::seed::<users::ActiveModel>(&ctx.db, &base.join("users.yaml").display().to_string())
                .await;

        // Loco 1.1 inserts the fixtures before resetting the ID sequence, whose
        // has_id_column helper only supports Postgres/SQLite. MySQL already
        // advances AUTO_INCREMENT when explicit fixture IDs are inserted.
        match result {
            Err(loco_rs::Error::DB(sea_orm::DbErr::BackendNotSupported {
                db,
                ctx: "has_id_column",
            })) if ctx.db.get_database_backend() == sea_orm::DatabaseBackend::MySql
                && db == "MySql" =>
            {
                Ok(())
            }
            other => other,
        }?;
        modules::blog::seeders::seed(ctx, base).await
    }
}
