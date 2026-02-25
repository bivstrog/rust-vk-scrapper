use sqlx::{PgPool, Row};
use sqlx::postgres::PgPoolOptions;
use chrono::NaiveDateTime;

// Test database setup
pub async fn setup_test_db() -> sqlx::PgPool {
    dotenv::dotenv().ok();
    
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");
    
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to connect to test database");
    
    // Run migrations
    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("Failed to run migrations");
    
    // Clean up existing data
    sqlx::query("TRUNCATE TABLE POST_INFO, POST RESTART IDENTITY CASCADE")
        .execute(&pool)
        .await
        .expect("Failed to clean test database");
    
    pool
}

// Database utility functions
#[allow(dead_code)]
pub async fn insert_post(
    pool: &PgPool,
    vk_id: &str,
    dt_parse_begin: NaiveDateTime,
    dt_parse_end: NaiveDateTime,
) -> Result<i32, sqlx::Error> {
    let result = sqlx::query(
        r#"
        INSERT INTO POST (vk_id, dt_parse_begin, dt_parse_end)
        VALUES ($1, $2, $3)
        RETURNING id
        "#
    )
    .bind(vk_id)
    .bind(dt_parse_begin)
    .bind(dt_parse_end)
    .fetch_one(pool)
    .await?;
    
    Ok(result.get("id"))
}

#[allow(dead_code)]
pub async fn insert_post_info(
    pool: &PgPool,
    post_id: i32,
    likes_count: i32,
    comments_count: i32,
    reposts_count: i32,
    views_count: i32,
    info_time: NaiveDateTime,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO POST_INFO (post_id, likes_count, comments_count, reposts_count, views_count, info_time)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#
    )
    .bind(post_id)
    .bind(likes_count)
    .bind(comments_count)
    .bind(reposts_count)
    .bind(views_count)
    .bind(info_time)
    .execute(pool)
    .await?;
    
    Ok(())
}

#[allow(dead_code)]
pub async fn get_post_by_id(pool: &PgPool, post_id: i32) -> Result<Option<(i32, String, NaiveDateTime, NaiveDateTime)>, sqlx::Error> {
    let result = sqlx::query(
        r#"
        SELECT id, vk_id, dt_parse_begin, dt_parse_end
        FROM POST
        WHERE id = $1
        "#
    )
    .bind(post_id)
    .fetch_optional(pool)
    .await?;
    
    Ok(result.map(|row| (
        row.get("id"),
        row.get("vk_id"),
        row.get("dt_parse_begin"),
        row.get("dt_parse_end"),
    )))
}

#[allow(dead_code)]
pub async fn get_post_info_by_post_id(
    pool: &PgPool,
    post_id: i32,
) -> Result<Vec<(i32, i32, i32, i32, NaiveDateTime)>, sqlx::Error> {
    let results = sqlx::query(
        r#"
        SELECT likes_count, comments_count, reposts_count, views_count, info_time
        FROM POST_INFO
        WHERE post_id = $1
        ORDER BY info_time ASC
        "#
    )
    .bind(post_id)
    .fetch_all(pool)
    .await?;
    
    Ok(results.iter().map(|row| (
        row.get("likes_count"),
        row.get("comments_count"),
        row.get("reposts_count"),
        row.get("views_count"),
        row.get("info_time"),
    )).collect())
}
