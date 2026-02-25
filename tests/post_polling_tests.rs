#[macro_use] extern crate rocket;

use rocket::local::blocking::Client;
use rocket::http::{Status, ContentType};
use serde_json::json;
use std::sync::Arc;
use rstest::rstest;
use sqlx::Row;

// Include all necessary modules for testing
#[allow(dead_code)]
#[path = "../src/db_commands.rs"]
mod db_commands;
#[allow(dead_code)]
#[path = "../src/models.rs"]
mod models;
#[allow(dead_code)]
#[path = "../src/utils.rs"]
mod utils;
#[allow(dead_code)]
#[path = "../src/tasks.rs"]
mod tasks;

// Mock VK API module using models::VkPostStats
mod vk_api {
    use rocket::response::status;
    use crate::models::VkPostStats;
    use std::sync::atomic::Ordering;
    
    // Counter for tracking calls and generating different responses
    static CALL_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    
    pub async fn call_vk(post_id: &str) -> Result<VkPostStats, status::BadRequest<String>> {
        let count = CALL_COUNTER.fetch_add(1, Ordering::SeqCst);
        
        // Simulate different responses based on post_id
        if post_id.contains("999_999") {
            // Empty stats - post not found
            Ok(VkPostStats {
                comments_count: 0,
                likes_count: 0,
                views_count: 0,
                reposts_count: 0,
            })
        } else if count == 0 {
            // Initial call: views=1, others=0
            Ok(VkPostStats {
                comments_count: 0,
                likes_count: 0,
                views_count: 1,
                reposts_count: 0,
            })
        } else {
            // Subsequent calls: views > likes > comments > reposts
            let base = count + 1;
            Ok(VkPostStats {
                comments_count: (base * 2) as u64,
                likes_count: (base * 3) as u64,
                views_count: (base * 4) as u64,
                reposts_count: base as u64,
            })
        }
    }
    
    pub fn reset_counter() {
        CALL_COUNTER.store(0, Ordering::SeqCst);
    }
}

// Include endpoints with our mocked vk_api
#[path = "../src/endpoints.rs"]
mod endpoints;

use endpoints::{post_polling, get_polling};

mod test_utils;
use test_utils::setup_test_db;

fn create_test_rocket(pool: sqlx::PgPool) -> rocket::Rocket<rocket::Build> {
    // Create JobScheduler but DON'T start it - jobs won't execute
    let scheduler = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async {
            tokio_cron_scheduler::JobScheduler::new()
                .await
                .expect("Failed to create scheduler")
        });
    
    let scheduler = Arc::new(scheduler);
    
    rocket::build()
        .manage(Arc::new(pool))
        .manage(scheduler)
        .mount("/", rocket::routes![
            post_polling,
            get_polling
        ])
}

#[rstest]
#[case::invalid_domain("https://invalid.com/post123", false, Status::BadRequest, Some("Invalid VK link format"))]
#[case::missing_wall_prefix("https://vk.com/post123", false, Status::BadRequest, Some("Invalid VK link format"))]
#[case::empty_link("", false, Status::BadRequest, None)]
#[case::malformed_json_body("{invalid json}", false, Status::BadRequest, None)]
fn test_post_polling_error_cases(
    #[case] vk_link: &str,
    #[case] prolong: bool,
    #[case] expected_status: Status,
    #[case] expected_body_contains: Option<&str>,
) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let pool = rt.block_on(setup_test_db());
    let rocket = create_test_rocket(pool);
    let client = Client::tracked(rocket).expect("valid rocket instance");
    
    let body_str = if vk_link == "{invalid json}" {
        vk_link.to_string()
    } else {
        json!({
            "vk_link": vk_link,
            "prolong": prolong
        }).to_string()
    };
    
    let response = client
        .post("/polling")
        .header(ContentType::JSON)
        .body(body_str)
        .dispatch();
    
    assert_eq!(response.status(), expected_status);
    
    if let Some(expected_text) = expected_body_contains {
        let body = response.into_string().unwrap();
        assert!(body.contains(expected_text), "Expected body to contain '{}', but got: {}", expected_text, body);
    }
}

#[test]
fn test_post_polling_missing_fields() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let pool = rt.block_on(setup_test_db());
    let rocket = create_test_rocket(pool);
    let client = Client::tracked(rocket).expect("valid rocket instance");
    
    let response = client
        .post("/polling")
        .header(ContentType::JSON)
        .body(json!({
            "vk_link": "https://vk.com/wall-1_1"
            // missing "prolong" field
        }).to_string())
        .dispatch();
    
    assert_eq!(response.status(), Status::UnprocessableEntity);
}

#[test]
fn test_post_polling_post_not_found_in_vk() {
    vk_api::reset_counter();
    
    let rt = tokio::runtime::Runtime::new().unwrap();
    let pool = rt.block_on(setup_test_db());
    let rocket = create_test_rocket(pool);
    let client = Client::tracked(rocket).expect("valid rocket instance");
    
    let response = client
        .post("/polling")
        .header(ContentType::JSON)
        .body(json!({
            "vk_link": "https://vk.com/wall-999_999",
            "prolong": false
        }).to_string())
        .dispatch();
    
    // Should return BadRequest because post stats are empty (all zeros)
    assert_eq!(response.status(), Status::BadRequest);
    let body = response.into_string().unwrap();
    assert!(body.contains("Post not found in VK"));
}

#[rstest]
#[case::without_prolong(false)]
#[case::with_prolong(true)]
fn test_post_polling_success_with_mock(#[case] prolong: bool) {
    vk_api::reset_counter();
    
    let rt = tokio::runtime::Runtime::new().unwrap();
    let pool = rt.block_on(setup_test_db());
    let rocket = create_test_rocket(pool);
    let client = Client::tracked(rocket).expect("valid rocket instance");
    
    // First call
    let response1 = client
        .post("/polling")
        .header(ContentType::JSON)
        .body(json!({
            "vk_link": "https://vk.com/wall-1_1",
            "prolong": prolong
        }).to_string())
        .dispatch();
    
    // Verify successful response
    assert_eq!(response1.status(), Status::Ok);
    
    let body1: serde_json::Value = serde_json::from_str(&response1.into_string().unwrap()).unwrap();
    assert!(body1["scrapper_id"].is_number());
    assert_eq!(body1["vk_id"], "-1_1");
    assert!(body1["dt_parse_begin"].is_string());
    assert!(body1["dt_parse_end"].is_string());
    
    let scrapper_id = body1["scrapper_id"].as_i64().unwrap();
    let dt_parse_end_1 = body1["dt_parse_end"].as_str().unwrap().to_string();
    
    // Add a small delay to ensure timestamp changes
    // TODO in prod fixing using millisconds
    std::thread::sleep(std::time::Duration::from_secs(1));
    
    // Second call with the same vk_link
    let response2 = client
        .post("/polling")
        .header(ContentType::JSON)
        .body(json!({
            "vk_link": "https://vk.com/wall-1_1",
            "prolong": prolong
        }).to_string())
        .dispatch();
    
    assert_eq!(response2.status(), Status::Ok);
    
    let body2: serde_json::Value = serde_json::from_str(&response2.into_string().unwrap()).unwrap();
    
    // scrapper_id should be the same (same post)
    assert_eq!(body2["scrapper_id"].as_i64().unwrap(), scrapper_id);
    assert_eq!(body2["vk_id"], "-1_1");
    
    let dt_parse_end_2 = body2["dt_parse_end"].as_str().unwrap().to_string();
    
    if prolong {
        // With prolong=true, dt_parse_end should be updated (extended)
        assert_ne!(dt_parse_end_1, dt_parse_end_2,
            "With prolong=true, dt_parse_end should change on second call");
        
        // Parse dates to verify second end time is later
        let end1 = chrono::NaiveDateTime::parse_from_str(&dt_parse_end_1, "%Y-%m-%dT%H:%M:%S").unwrap();
        let end2 = chrono::NaiveDateTime::parse_from_str(&dt_parse_end_2, "%Y-%m-%dT%H:%M:%S").unwrap();
        assert!(end2 > end1, "Second dt_parse_end should be later than first when prolong=true");
    } else {
        // With prolong=false, dt_parse_end should remain the same
        assert_eq!(dt_parse_end_1, dt_parse_end_2,
            "With prolong=false, dt_parse_end should not change on second call");
    }
}

#[test]
fn test_async_task_is_scheduled() {
    vk_api::reset_counter();
    
    let rt = tokio::runtime::Runtime::new().unwrap();
    let pool = rt.block_on(setup_test_db());
    
    // Create scheduler and start it
    let scheduler = rt.block_on(async {
        let sched = tokio_cron_scheduler::JobScheduler::new()
            .await
            .expect("Failed to create scheduler");
        sched.start().await.expect("Failed to start scheduler");
        sched
    });
    
    let scheduler = Arc::new(scheduler);
    
    let rocket = rocket::build()
        .manage(Arc::new(pool.clone()))
        .manage(scheduler.clone())
        .mount("/", rocket::routes![
            post_polling,
            get_polling
        ]);
    
    let client = Client::tracked(rocket).expect("valid rocket instance");
    
    // Make POST request to create a polling task
    let response = client
        .post("/polling")
        .header(ContentType::JSON)
        .body(json!({
            "vk_link": "https://vk.com/wall-1_1",
            "prolong": false
        }).to_string())
        .dispatch();
    
    assert_eq!(response.status(), Status::Ok, "POST /polling should succeed");
    
    let body: serde_json::Value = serde_json::from_str(&response.into_string().unwrap()).unwrap();
    let scrapper_id = body["scrapper_id"].as_i64().unwrap();
    
    println!("✓ POST /polling succeeded, created post with scrapper_id: {}", scrapper_id);
    
    // Wait for the scheduled task to execute at least once
    // The pooling_delta is 5 seconds by default in test, so we wait a bit longer
    println!("Waiting for async task to execute (this may take up to 3 seconds)...");
    std::thread::sleep(std::time::Duration::from_secs(3));
    
    // Check if post_info was created by the async task
    let post_info_count = rt.block_on(async {
        let result = sqlx::query("SELECT COUNT(*) as count FROM POST_INFO WHERE post_id = $1")
            .bind(scrapper_id as i32)
            .fetch_one(&pool)
            .await
            .expect("Failed to query POST_INFO");
        
        result.get::<i64, _>("count")
    });
    
    assert!(
        post_info_count > 0,
        "Expected at least one POST_INFO entry to be created by the async task, but found {}",
        post_info_count
    );
    
    println!("✓ Async task successfully executed and created {} POST_INFO entries", post_info_count);
}

#[test]
fn test_async_task_not_scheduled_when_recent_polling_exists() {
    vk_api::reset_counter();
    
    let rt = tokio::runtime::Runtime::new().unwrap();
    let pool = rt.block_on(setup_test_db());
    
    let scheduler = rt.block_on(async {
        let sched = tokio_cron_scheduler::JobScheduler::new()
            .await
            .expect("Failed to create scheduler");
        sched.start().await.expect("Failed to start scheduler");
        sched
    });
    
    let scheduler = Arc::new(scheduler);
    
    let rocket = rocket::build()
        .manage(Arc::new(pool.clone()))
        .manage(scheduler.clone())
        .mount("/", rocket::routes![
            post_polling,
            get_polling
        ]);
    
    let client = Client::tracked(rocket).expect("valid rocket instance");
    
    // First request - should schedule a task
    let response1 = client
        .post("/polling")
        .header(ContentType::JSON)
        .body(json!({
            "vk_link": "https://vk.com/wall-2_2",
            "prolong": false
        }).to_string())
        .dispatch();
    
    assert_eq!(response1.status(), Status::Ok);
    let body1: serde_json::Value = serde_json::from_str(&response1.into_string().unwrap()).unwrap();
    let scrapper_id = body1["scrapper_id"].as_i64().unwrap() as i32;
    
    println!("Waiting for first task execution (this may take up to 3 seconds)...");
    std::thread::sleep(std::time::Duration::from_secs(3));
    
    let post_info_count_after_first = rt.block_on(async {
        let result = sqlx::query("SELECT COUNT(*) as count FROM POST_INFO WHERE post_id = $1")
            .bind(scrapper_id)
            .fetch_one(&pool)
            .await
            .expect("Failed to query POST_INFO");
        
        result.get::<i64, _>("count")
    });
    
    println!("After first request: {} POST_INFO entries", post_info_count_after_first);
    
    // Second request immediately after - should NOT schedule another task
    // because recent polling exists (within 2*pooling_delta)
    let response2 = client
        .post("/polling")
        .header(ContentType::JSON)
        .body(json!({
            "vk_link": "https://vk.com/wall-2_2",
            "prolong": false
        }).to_string())
        .dispatch();
    
    assert_eq!(response2.status(), Status::Ok);
    
    // Wait a bit to see if a second task would execute (it shouldn't)
    std::thread::sleep(std::time::Duration::from_secs(3));
    
    let post_info_count_after_second = rt.block_on(async {
        let result = sqlx::query("SELECT COUNT(*) as count FROM POST_INFO WHERE post_id = $1")
            .bind(scrapper_id)
            .fetch_one(&pool)
            .await
            .expect("Failed to query POST_INFO");
        
        result.get::<i64, _>("count")
    });
    
    println!("After second request: {} POST_INFO entries", post_info_count_after_second);
    
    // The count should be the same or only slightly increased (from the original task)
    // but not doubled, proving no new task was scheduled
    assert!(
        post_info_count_after_second <= post_info_count_after_first + 1,
        "Expected POST_INFO count to not significantly increase when recent polling exists"
    );
    
    println!("✓ Async task correctly NOT scheduled when recent polling exists");
}
