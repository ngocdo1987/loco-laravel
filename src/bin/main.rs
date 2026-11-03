use loco_laravel::app::App;
use loco_rs::cli;
use migration::Migrator;

#[tokio::main]
async fn main() -> loco_rs::Result<()> {
    Box::pin(cli::main::<App, Migrator>()).await
}
