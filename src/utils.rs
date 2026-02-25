use sqlx::postgres::PgPool;
use sqlx::postgres::PgPoolOptions;
use crate::vk_api::VkPostStats;

pub fn get_pooling_period_seconds() -> i32 {
    std::env::var("POOLING_PERIOD_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300) // Default 5 minutes
}

pub fn get_pooling_delta_seconds() -> i32 {
    std::env::var("POOLING_DELTA_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30) // Default 30 seconds
}

pub async fn get_db_pool() -> Result<PgPool, sqlx::Error> {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set in .env file");
    
    PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
}

pub fn get_vk_token() -> Result<String, String> {
    std::env::var("VK_TOKEN").map_err(|e| e.to_string())
}

pub fn get_vk_api_domain() -> Result<String, String> {
    std::env::var("VK_API_DOMAIN").map_err(|e| e.to_string())
}

pub fn get_vk_api_version() -> String {
    std::env::var("VK_API_VERSION").unwrap_or_else(|_| "5.199".to_string())
}

pub fn is_post_stats_empty(stats: &VkPostStats) -> bool {
    stats.likes_count == 0 && stats.comments_count == 0 &&
    stats.reposts_count == 0 && stats.views_count == 0
}