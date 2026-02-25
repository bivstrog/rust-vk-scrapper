#[macro_use] extern crate rocket;

mod db_commands;
mod vk_api;
mod tasks;
mod utils;

use tokio_cron_scheduler::{Job, JobScheduler};
use db_commands::{has_recent_post_info, get_or_create_post_with_prolong, get_post_with_data};
use tasks::{poll_post_stats, init_all_tasks};
use utils::{get_db_pool, get_pooling_delta_seconds, is_post_stats_empty};
use dotenv::dotenv;
use rocket::response::status;
use rocket::State;
use rocket::serde::json::Json;
use serde::{Deserialize, Serialize};
use vk_api::call_vk;
use sqlx::postgres::PgPool;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(crate = "rocket::serde")]
struct PollingRequest {
    vk_link: String,
    prolong: bool,
}

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
struct PollingResponse {
    scrapper_id: i32,
    vk_id: String,
    dt_parse_begin: String,
    dt_parse_end: String,
}

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
struct PostInfoDataResponse {
    comments_count: i32,
    likes_count: i32,
    views_count: i32,
    reposts_count: i32,
    info_time: String,
}

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
struct GetPollingResponse {
    scrapper_id: i32,
    vk_id: String,
    dt_parse_begin: String,
    dt_parse_end: String,
    dt_current: String,
    data: Vec<PostInfoDataResponse>,
}



#[post("/polling", data = "<request>")]
async fn post_polling(
    request: Json<PollingRequest>,
    pool: &State<Arc<PgPool>>,
    scheduler: &State<Arc<JobScheduler>>
) -> Result<Json<PollingResponse>, status::BadRequest<String>> {
    // Extract vk_id from vk_link (everything after https://vk.com/wall)
    let vk_id = request.vk_link
        .strip_prefix("https://vk.com/wall")
        .ok_or_else(|| status::BadRequest("Invalid VK link format. Expected: https://vk.com/wall...".to_string()))?
        .to_string();
    
    // Validate post exists in VK by calling API
    let stats = call_vk(&vk_id).await
        .map_err(|e| status::BadRequest(format!("VK API error: {:?}", e)))?;
    
    // Check if post stats are empty - post not found
    if is_post_stats_empty(&stats) {
        return Err(status::BadRequest("Post not found in VK".to_string()));
    }
    
    // Get or create post in database with prolong option
    let post_details = get_or_create_post_with_prolong(pool, &vk_id, request.prolong).await
        .map_err(|e| status::BadRequest(format!("Failed to get or create post: {}", e)))?;
    
    // Get pooling delta from utils
    let pooling_delta = get_pooling_delta_seconds();
    
    // Check if there's a recent post_info entry (within 2*pooling_delta)
    let has_recent = has_recent_post_info(pool, post_details.id, pooling_delta as i64).await
        .map_err(|e| status::BadRequest(format!("Failed to check recent post info: {}", e)))?;
    
    if !has_recent {
        // Create cron job for polling only if there's no recent polling
        let pool_inner = pool.inner().clone();
        let db_post_id = post_details.id;
        let job = Job::new_async(format!("*/{} * * * * *", pooling_delta).as_str(), move |job_id, locked_scheduler| {
            let pool = pool_inner.clone();
            let db_post_id = db_post_id;
            Box::pin(async move {
                if let Err(e) = poll_post_stats(&job_id, &locked_scheduler, &pool, db_post_id).await {
                    eprintln!("Error polling post stats: {}", e);
                }
            })
        }).map_err(|e| status::BadRequest(format!("Failed to create job: {}", e)))?;
        
        // Add job to the scheduler
        scheduler.add(job).await
            .map_err(|e| status::BadRequest(format!("Failed to add job: {}", e)))?;
    }
    
    // Return response
    Ok(Json(PollingResponse {
        scrapper_id: post_details.id,
        vk_id: post_details.vk_id,
        dt_parse_begin: post_details.dt_parse_begin.format("%Y-%m-%dT%H:%M:%S").to_string(),
        dt_parse_end: post_details.dt_parse_end.format("%Y-%m-%dT%H:%M:%S").to_string(),
    }))
}

#[get("/polling?<scrapper_id>")]
async fn get_polling(
    scrapper_id: i32,
    pool: &State<Arc<PgPool>>,
) -> Result<Json<GetPollingResponse>, status::NotFound<String>> {
    // Get post with data
    let post_with_data = get_post_with_data(pool, scrapper_id).await
        .map_err(|e| status::NotFound(format!("Database error: {}", e)))?
        .ok_or_else(|| status::NotFound(format!("Post with scrapper_id {} not found", scrapper_id)))?;
    
    // Get current timestamp
    let dt_current = chrono::Local::now().naive_local();
    
    // Convert data to response format
    let data: Vec<PostInfoDataResponse> = post_with_data.data.into_iter().map(|d| PostInfoDataResponse {
        comments_count: d.comments_count,
        likes_count: d.likes_count,
        views_count: d.views_count,
        reposts_count: d.reposts_count,
        info_time: d.info_time.format("%Y-%m-%dT%H:%M:%S").to_string(),
    }).collect();
    
    Ok(Json(GetPollingResponse {
        scrapper_id: post_with_data.id,
        vk_id: post_with_data.vk_id,
        dt_parse_begin: post_with_data.dt_parse_begin.format("%Y-%m-%dT%H:%M:%S").to_string(),
        dt_parse_end: post_with_data.dt_parse_end.format("%Y-%m-%dT%H:%M:%S").to_string(),
        dt_current: dt_current.format("%Y-%m-%dT%H:%M:%S").to_string(),
        data,
    }))
}

#[launch]
async fn rocket() -> _ {
    dotenv().ok();

    // Run database migrations
    let pool = get_db_pool().await
        .expect("Failed to create database pool");
    
    if let Err(e) = sqlx::migrate!().run(&pool).await {
        eprintln!("Failed to run database migrations: {}", e);
        panic!("Database migration failed");
    }

    // Create and start the scheduler
    let scheduler = JobScheduler::new().await
        .expect("Failed to create scheduler");
    
    scheduler.start().await
        .expect("Failed to start scheduler");
    
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