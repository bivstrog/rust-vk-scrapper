use sqlx::Row;

// Include all necessary modules for testing
#[allow(dead_code)]
#[path = "../src/db_commands.rs"]
mod db_commands;
#[allow(dead_code)]
#[path = "../src/models.rs"]
mod models;
#[allow(dead_code)]
#[path = "../src/tasks.rs"]
mod tasks;
#[allow(dead_code)]
#[path = "../src/utils.rs"]
mod utils;

// Mock VK API module
mod vk_api {
    use crate::models::VkPostStats;
    use rocket::response::status;
    use std::sync::atomic::Ordering;

    static CALL_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    pub async fn call_vk(_post_id: &str) -> Result<VkPostStats, status::BadRequest<String>> {
        let count = CALL_COUNTER.fetch_add(1, Ordering::SeqCst);

        // Simulate different responses based on call count
        let base = count + 1;
        Ok(VkPostStats {
            comments_count: (base * 2) as u64,
            likes_count: (base * 3) as u64,
            views_count: (base * 4) as u64,
            reposts_count: base as u64,
        })
    }

    pub fn reset_counter() {
        CALL_COUNTER.store(0, Ordering::SeqCst);
    }
}

mod test_utils;
use test_utils::setup_test_db;

use tasks::{init_all_tasks, poll_post_stats};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_poll_post_stats_calls_vk_and_saves_to_db() {
    vk_api::reset_counter();

    let pool = setup_test_db().await;

    // Create a post that needs polling (expires in 10 minutes to be safe)
    let post_id = sqlx::query(
        r#"
        INSERT INTO POST (vk_id, dt_parse_begin, dt_parse_end)
        VALUES ($1, CURRENT_TIMESTAMP - INTERVAL '60 seconds', CURRENT_TIMESTAMP + INTERVAL '600 seconds')
        RETURNING id
        "#
    )
    .bind("-123_456")
    .fetch_one(&pool)
    .await
    .expect("Failed to create post")
    .get::<i32, _>("id");

    // Create a mock scheduler
    let scheduler = tokio_cron_scheduler::JobScheduler::new()
        .await
        .expect("Failed to create scheduler");

    let job_id = uuid::Uuid::new_v4();

    // Call poll_post_stats
    let result = poll_post_stats(&job_id, &scheduler, &pool, post_id).await;
    assert!(result.is_ok(), "poll_post_stats should succeed");

    // Verify that POST_INFO was created
    let post_info_count = sqlx::query("SELECT COUNT(*) as count FROM POST_INFO WHERE post_id = $1")
        .bind(post_id)
        .fetch_one(&pool)
        .await
        .expect("Failed to query POST_INFO")
        .get::<i64, _>("count");

    assert_eq!(
        post_info_count, 1,
        "Should have created one POST_INFO entry"
    );

    // Verify the data was saved correctly (first call: likes=3, comments=2, reposts=1, views=4)
    let post_info = sqlx::query(
        "SELECT likes_count, comments_count, reposts_count, views_count FROM POST_INFO WHERE post_id = $1"
    )
    .bind(post_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to fetch POST_INFO");

    assert_eq!(post_info.get::<i32, _>("likes_count"), 3);
    assert_eq!(post_info.get::<i32, _>("comments_count"), 2);
    assert_eq!(post_info.get::<i32, _>("reposts_count"), 1);
    assert_eq!(post_info.get::<i32, _>("views_count"), 4);

    println!("✓ poll_post_stats successfully called VK API and saved data to DB");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_poll_post_stats_stops_when_ready_to_finish() {
    vk_api::reset_counter();

    let pool = setup_test_db().await;

    // Create a post that has already expired
    let post_id = sqlx::query(
        r#"
        INSERT INTO POST (vk_id, dt_parse_begin, dt_parse_end)
        VALUES ($1, CURRENT_TIMESTAMP - INTERVAL '600 seconds', CURRENT_TIMESTAMP - INTERVAL '300 seconds')
        RETURNING id
        "#
    )
    .bind("-123_789")
    .fetch_one(&pool)
    .await
    .expect("Failed to create post")
    .get::<i32, _>("id");

    // Create a mock scheduler
    let scheduler = tokio_cron_scheduler::JobScheduler::new()
        .await
        .expect("Failed to create scheduler");

    let job_id = uuid::Uuid::new_v4();

    // Call poll_post_stats - it should detect the post is expired and not poll
    let result = poll_post_stats(&job_id, &scheduler, &pool, post_id).await;
    assert!(
        result.is_ok(),
        "poll_post_stats should succeed even when stopping"
    );

    // Verify that NO POST_INFO was created (because post is expired)
    let post_info_count = sqlx::query("SELECT COUNT(*) as count FROM POST_INFO WHERE post_id = $1")
        .bind(post_id)
        .fetch_one(&pool)
        .await
        .expect("Failed to query POST_INFO")
        .get::<i64, _>("count");

    assert_eq!(
        post_info_count, 0,
        "Should NOT have created POST_INFO for expired post"
    );

    println!("✓ poll_post_stats correctly stops polling for expired posts");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_init_all_tasks_starts_tasks_for_active_posts() {
    vk_api::reset_counter();

    let pool = setup_test_db().await;

    // Create multiple posts with different states

    // 1. Active post without recent polling - SHOULD start task
    let active_post_id = sqlx::query(
        r#"
        INSERT INTO POST (vk_id, dt_parse_begin, dt_parse_end)
        VALUES ($1, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP + INTERVAL '300 seconds')
        RETURNING id
        "#,
    )
    .bind("-111_111")
    .fetch_one(&pool)
    .await
    .expect("Failed to create active post")
    .get::<i32, _>("id");

    // 2. Active post WITH recent polling - SHOULD NOT start task
    let active_with_recent_id = sqlx::query(
        r#"
        INSERT INTO POST (vk_id, dt_parse_begin, dt_parse_end)
        VALUES ($1, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP + INTERVAL '300 seconds')
        RETURNING id
        "#,
    )
    .bind("-222_222")
    .fetch_one(&pool)
    .await
    .expect("Failed to create active post with recent polling")
    .get::<i32, _>("id");

    // Add recent POST_INFO for the second post
    sqlx::query(
        r#"
        INSERT INTO POST_INFO (post_id, likes_count, comments_count, reposts_count, views_count, info_time)
        VALUES ($1, 10, 5, 2, 20, CURRENT_TIMESTAMP)
        "#
    )
    .bind(active_with_recent_id)
    .execute(&pool)
    .await
    .expect("Failed to create recent POST_INFO");

    // 3. Expired post - SHOULD NOT start task
    let expired_post_id = sqlx::query(
        r#"
        INSERT INTO POST (vk_id, dt_parse_begin, dt_parse_end)
        VALUES ($1, CURRENT_TIMESTAMP - INTERVAL '600 seconds', CURRENT_TIMESTAMP - INTERVAL '300 seconds')
        RETURNING id
        "#
    )
    .bind("-333_333")
    .fetch_one(&pool)
    .await
    .expect("Failed to create expired post")
    .get::<i32, _>("id");

    // Create scheduler
    let scheduler = tokio_cron_scheduler::JobScheduler::new()
        .await
        .expect("Failed to create scheduler");

    // Call init_all_tasks
    let result = init_all_tasks(&pool, &scheduler).await;
    assert!(result.is_ok(), "init_all_tasks should succeed");

    // We can't directly count jobs in the scheduler, but we can verify the function
    // completed successfully and printed the expected message
    println!("✓ init_all_tasks completed successfully");
    println!(
        "  - Should have started task for post {} (active without recent polling)",
        active_post_id
    );
    println!(
        "  - Should NOT have started task for post {} (active with recent polling)",
        active_with_recent_id
    );
    println!(
        "  - Should NOT have started task for post {} (expired)",
        expired_post_id
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_init_all_tasks_with_no_posts() {
    let pool = setup_test_db().await;

    // Create scheduler
    let scheduler = tokio_cron_scheduler::JobScheduler::new()
        .await
        .expect("Failed to create scheduler");

    // Call init_all_tasks with empty database
    let result = init_all_tasks(&pool, &scheduler).await;
    assert!(
        result.is_ok(),
        "init_all_tasks should succeed with no posts"
    );

    println!("✓ init_all_tasks handles empty database correctly");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_poll_post_stats_multiple_calls_accumulate_data() {
    vk_api::reset_counter();

    let pool = setup_test_db().await;

    // Create a post (expires in 10 minutes to be safe)
    let post_id = sqlx::query(
        r#"
        INSERT INTO POST (vk_id, dt_parse_begin, dt_parse_end)
        VALUES ($1, CURRENT_TIMESTAMP - INTERVAL '60 seconds', CURRENT_TIMESTAMP + INTERVAL '600 seconds')
        RETURNING id
        "#
    )
    .bind("-444_444")
    .fetch_one(&pool)
    .await
    .expect("Failed to create post")
    .get::<i32, _>("id");

    let scheduler = tokio_cron_scheduler::JobScheduler::new()
        .await
        .expect("Failed to create scheduler");

    let job_id = uuid::Uuid::new_v4();

    // Call poll_post_stats multiple times
    for i in 1..=3 {
        let result = poll_post_stats(&job_id, &scheduler, &pool, post_id).await;
        assert!(result.is_ok(), "poll_post_stats call {} should succeed", i);

        // Small delay between calls
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    // Verify that 3 POST_INFO entries were created
    let post_info_count = sqlx::query("SELECT COUNT(*) as count FROM POST_INFO WHERE post_id = $1")
        .bind(post_id)
        .fetch_one(&pool)
        .await
        .expect("Failed to query POST_INFO")
        .get::<i64, _>("count");

    assert_eq!(
        post_info_count, 3,
        "Should have created 3 POST_INFO entries"
    );

    // Verify the data is increasing (mock returns incrementing values)
    let post_infos = sqlx::query(
        "SELECT likes_count, comments_count, reposts_count, views_count FROM POST_INFO WHERE post_id = $1 ORDER BY info_time ASC"
    )
    .bind(post_id)
    .fetch_all(&pool)
    .await
    .expect("Failed to fetch POST_INFO");

    // First call: likes=3, comments=2, reposts=1, views=4
    assert_eq!(post_infos[0].get::<i32, _>("likes_count"), 3);
    assert_eq!(post_infos[0].get::<i32, _>("comments_count"), 2);

    // Second call: likes=6, comments=4, reposts=2, views=8
    assert_eq!(post_infos[1].get::<i32, _>("likes_count"), 6);
    assert_eq!(post_infos[1].get::<i32, _>("comments_count"), 4);

    // Third call: likes=9, comments=6, reposts=3, views=12
    assert_eq!(post_infos[2].get::<i32, _>("likes_count"), 9);
    assert_eq!(post_infos[2].get::<i32, _>("comments_count"), 6);

    println!("✓ poll_post_stats correctly accumulates data over multiple calls");
}
