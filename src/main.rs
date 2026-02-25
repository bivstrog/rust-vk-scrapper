#[macro_use]
extern crate rocket;

mod db_commands;
mod endpoints;
mod models;
mod tasks;
mod utils;
mod vk_api;

use dotenv::dotenv;
use endpoints::{get_polling, post_polling};
use std::sync::Arc;
use tasks::init_all_tasks;
use tokio_cron_scheduler::JobScheduler;
use utils::get_db_pool;

#[launch]
async fn rocket() -> rocket::Rocket<rocket::Build> {
    dotenv().ok();

    // Run database migrations
    let pool = get_db_pool().await.expect("Failed to create database pool");

    if let Err(e) = sqlx::migrate!().run(&pool).await {
        eprintln!("Failed to run database migrations: {}", e);
        panic!("Database migration failed");
    }

    // Create and start the scheduler
    let scheduler = JobScheduler::new()
        .await
        .expect("Failed to create scheduler");

    scheduler.start().await.expect("Failed to start scheduler");

    // Initialize all active polling tasks
    if let Err(e) = init_all_tasks(&pool, &scheduler).await {
        eprintln!("Failed to initialize polling tasks: {}", e);
    }

    let scheduler = Arc::new(scheduler);

    rocket::build()
        .manage(Arc::new(pool))
        .manage(scheduler)
        .mount("/", routes![post_polling, get_polling])
}
