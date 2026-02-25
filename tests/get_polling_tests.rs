#[macro_use]
extern crate rocket;

use rocket::http::Status;
use rocket::local::blocking::Client;
use rstest::rstest;
use serde_json::Value;
use std::sync::Arc;

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

// Mock VK API module using models::VkPostStats
mod vk_api {
    use crate::models::VkPostStats;
    use rocket::response::status;

    pub async fn call_vk(_post_id: &str) -> Result<VkPostStats, status::BadRequest<String>> {
        // Simple mock - just return some data
        Ok(VkPostStats {
            comments_count: 0,
            likes_count: 0,
            views_count: 1,
            reposts_count: 0,
        })
    }
}

// Include endpoints with our mocked vk_api
#[allow(dead_code)]
#[path = "../src/endpoints.rs"]
mod endpoints;

use endpoints::get_polling;

mod test_utils;
use test_utils::{insert_post, insert_post_info, setup_test_db};

fn create_test_rocket(pool: sqlx::PgPool) -> rocket::Rocket<rocket::Build> {
    let scheduler = tokio::runtime::Runtime::new().unwrap().block_on(async {
        tokio_cron_scheduler::JobScheduler::new()
            .await
            .expect("Failed to create scheduler")
    });

    let scheduler = Arc::new(scheduler);

    rocket::build()
        .manage(Arc::new(pool))
        .manage(scheduler)
        .mount("/", rocket::routes![get_polling])
}

#[rstest]
#[case::not_found(99999, Status::NotFound, "not found")]
#[case::invalid_id(0, Status::NotFound, "not found")]
fn test_get_polling_error_cases(
    #[case] scrapper_id: i32,
    #[case] expected_status: Status,
    #[case] expected_body_contains: &str,
) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let pool = rt.block_on(setup_test_db());
    let rocket = create_test_rocket(pool);
    let client = Client::tracked(rocket).expect("valid rocket instance");

    let response = client
        .get(format!("/polling?scrapper_id={}", scrapper_id))
        .dispatch();

    assert_eq!(response.status(), expected_status);
    let body = response.into_string().unwrap();
    assert!(body.contains(expected_body_contains));
}

#[test]
fn test_get_polling_with_data() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let pool = rt.block_on(setup_test_db());

    // Insert test data
    let post_id = rt.block_on(async {
        let now = chrono::Local::now().naive_local();
        let end_time = now + chrono::Duration::hours(1);

        // Insert post
        let post_id = insert_post(&pool, "-123_456", now, end_time)
            .await
            .expect("Failed to insert post");

        // Insert post_info entries
        insert_post_info(&pool, post_id, 10, 5, 2, 100, now)
            .await
            .expect("Failed to insert post_info 1");

        insert_post_info(
            &pool,
            post_id,
            20,
            10,
            4,
            200,
            now + chrono::Duration::minutes(30),
        )
        .await
        .expect("Failed to insert post_info 2");

        insert_post_info(
            &pool,
            post_id,
            30,
            15,
            6,
            300,
            now + chrono::Duration::minutes(60),
        )
        .await
        .expect("Failed to insert post_info 3");

        post_id
    });

    let rocket = create_test_rocket(pool);
    let client = Client::tracked(rocket).expect("valid rocket instance");

    let response = client
        .get(format!("/polling?scrapper_id={}", post_id))
        .dispatch();

    assert_eq!(response.status(), Status::Ok);

    let body = response.into_string().unwrap();
    let json: Value = serde_json::from_str(&body).expect("Failed to parse JSON");

    // Verify response structure
    assert_eq!(json["scrapper_id"], post_id);
    assert_eq!(json["vk_id"], "-123_456");
    assert!(json["dt_parse_begin"].is_string());
    assert!(json["dt_parse_end"].is_string());
    assert!(json["dt_current"].is_string());

    // Verify data array
    let data = json["data"].as_array().expect("data should be an array");
    assert_eq!(data.len(), 3, "Should have 3 post_info entries");

    // Verify first entry
    assert_eq!(data[0]["likes_count"], 10);
    assert_eq!(data[0]["comments_count"], 5);
    assert_eq!(data[0]["reposts_count"], 2);
    assert_eq!(data[0]["views_count"], 100);

    // Verify second entry
    assert_eq!(data[1]["likes_count"], 20);
    assert_eq!(data[1]["comments_count"], 10);
    assert_eq!(data[1]["reposts_count"], 4);
    assert_eq!(data[1]["views_count"], 200);

    // Verify third entry
    assert_eq!(data[2]["likes_count"], 30);
    assert_eq!(data[2]["comments_count"], 15);
    assert_eq!(data[2]["reposts_count"], 6);
    assert_eq!(data[2]["views_count"], 300);
}
