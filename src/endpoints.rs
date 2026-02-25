use rocket::State;
use rocket::response::status;
use rocket::serde::json::Json;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use tokio_cron_scheduler::{Job, JobScheduler};

use crate::db_commands::{
    get_or_create_post_with_prolong, get_post_with_data, has_recent_post_info,
};
use crate::models::{GetPollingResponse, PollingRequest, PollingResponse, PostInfoDataResponse};
use crate::tasks::poll_post_stats;
use crate::utils::{get_pooling_delta_seconds, is_post_stats_empty};
use crate::vk_api::call_vk;

#[post("/polling", data = "<request>")]
pub async fn post_polling(
    request: Json<PollingRequest>,
    pool: &State<Arc<PgPool>>,
    scheduler: &State<Arc<JobScheduler>>,
) -> Result<Json<PollingResponse>, status::BadRequest<String>> {
    // Extract vk_id from vk_link (everything after https://vk.com/wall)
    let vk_id = request
        .vk_link
        .strip_prefix("https://vk.com/wall")
        .ok_or_else(|| {
            status::BadRequest(
                "Invalid VK link format. Expected: https://vk.com/wall...".to_string(),
            )
        })?
        .to_string();

    // Validate post exists in VK by calling API
    let stats = call_vk(&vk_id)
        .await
        .map_err(|e| status::BadRequest(format!("VK API error: {:?}", e)))?;

    // Check if post stats are empty - post not found
    if is_post_stats_empty(&stats) {
        return Err(status::BadRequest("Post not found in VK".to_string()));
    }

    // Get or create post in database with prolong option
    let post_details = get_or_create_post_with_prolong(pool, &vk_id, request.prolong)
        .await
        .map_err(|e| status::BadRequest(format!("Failed to get or create post: {}", e)))?;

    // Get pooling delta from utils
    let pooling_delta = get_pooling_delta_seconds();

    // Check if there's a recent post_info entry (within 2*pooling_delta)
    let has_recent = has_recent_post_info(pool, post_details.id, pooling_delta as i64)
        .await
        .map_err(|e| status::BadRequest(format!("Failed to check recent post info: {}", e)))?;

    if !has_recent {
        // Create cron job for polling only if there's no recent polling
        let pool_inner = pool.inner().clone();
        let db_post_id = post_details.id;
        let job = Job::new_async(
            format!("*/{} * * * * *", pooling_delta).as_str(),
            move |job_id, locked_scheduler| {
                let pool = pool_inner.clone();
                let db_post_id = db_post_id;
                Box::pin(async move {
                    if let Err(e) =
                        poll_post_stats(&job_id, &locked_scheduler, &pool, db_post_id).await
                    {
                        eprintln!("Error polling post stats: {}", e);
                    }
                })
            },
        )
        .map_err(|e| status::BadRequest(format!("Failed to create job: {}", e)))?;

        // Add job to the scheduler
        scheduler
            .add(job)
            .await
            .map_err(|e| status::BadRequest(format!("Failed to add job: {}", e)))?;
    }

    // Return response
    Ok(Json(PollingResponse {
        scrapper_id: post_details.id,
        vk_id: post_details.vk_id,
        dt_parse_begin: post_details
            .dt_parse_begin
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string(),
        dt_parse_end: post_details
            .dt_parse_end
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string(),
    }))
}

#[get("/polling?<scrapper_id>")]
pub async fn get_polling(
    scrapper_id: i32,
    pool: &State<Arc<PgPool>>,
) -> Result<Json<GetPollingResponse>, status::NotFound<String>> {
    // Get post with data
    let post_with_data = get_post_with_data(pool, scrapper_id)
        .await
        .map_err(|e| status::NotFound(format!("Database error: {}", e)))?
        .ok_or_else(|| {
            status::NotFound(format!("Post with scrapper_id {} not found", scrapper_id))
        })?;

    // Get current timestamp
    let dt_current = chrono::Local::now().naive_local();

    // Convert data to response format
    let data: Vec<PostInfoDataResponse> = post_with_data
        .data
        .into_iter()
        .map(|d| PostInfoDataResponse {
            comments_count: d.comments_count,
            likes_count: d.likes_count,
            views_count: d.views_count,
            reposts_count: d.reposts_count,
            info_time: d.info_time.format("%Y-%m-%dT%H:%M:%S").to_string(),
        })
        .collect();

    Ok(Json(GetPollingResponse {
        scrapper_id: post_with_data.id,
        vk_id: post_with_data.vk_id,
        dt_parse_begin: post_with_data
            .dt_parse_begin
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string(),
        dt_parse_end: post_with_data
            .dt_parse_end
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string(),
        dt_current: dt_current.format("%Y-%m-%dT%H:%M:%S").to_string(),
        data,
    }))
}
