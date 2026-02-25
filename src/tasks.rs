use crate::db_commands::{
    get_posts_needing_polling, get_vk_id_by_post_id, is_ready_to_finish, save_post_info,
};
use crate::utils::get_pooling_delta_seconds;
use crate::vk_api::call_vk;
use sqlx::postgres::PgPool;
use tokio_cron_scheduler::{Job, JobScheduler};

pub async fn init_all_tasks(
    pool: &PgPool,
    scheduler: &JobScheduler,
) -> Result<(), Box<dyn std::error::Error>> {
    // Get pooling delta from utils
    let pooling_delta = get_pooling_delta_seconds();

    // Get all posts that need polling
    let posts = get_posts_needing_polling(pool).await?;

    println!("Initializing {} polling tasks on startup", posts.len());

    for (db_post_id, vk_id) in posts {
        // Create cron job for polling
        let pool_clone = pool.clone();
        let job = Job::new_async(
            format!("*/{} * * * * *", pooling_delta).as_str(),
            move |job_id, locked_scheduler| {
                let pool = pool_clone.clone();
                let db_post_id = db_post_id;
                Box::pin(async move {
                    if let Err(e) =
                        poll_post_stats(&job_id, &locked_scheduler, &pool, db_post_id).await
                    {
                        eprintln!("Error polling post stats: {}", e);
                    }
                })
            },
        )?;

        // Add job to the scheduler
        scheduler.add(job).await?;

        println!(
            "Started polling task for post {} (db_id: {})",
            vk_id, db_post_id
        );
    }

    Ok(())
}

pub async fn poll_post_stats(
    job_id: &uuid::Uuid,
    locked_scheduler: &JobScheduler,
    pool: &PgPool,
    db_post_id: i32,
) -> Result<(), Box<dyn std::error::Error>> {
    // Check if the task should finish
    if is_ready_to_finish(pool, db_post_id).await? {
        println!("Post {} is ready to finish, stopping polling", db_post_id);
        let _ = locked_scheduler.remove(job_id).await;
        return Ok(());
    }

    // Get vk_id from database
    let vk_id = get_vk_id_by_post_id(pool, db_post_id)
        .await?
        .ok_or("Post not found")?;

    // Call VK API
    let stats = call_vk(&vk_id)
        .await
        .map_err(|e| format!("VK API call failed: {:?}", e))?;

    // Save post info to database
    save_post_info(
        pool,
        db_post_id,
        stats.likes_count as i32,
        stats.comments_count as i32,
        stats.reposts_count as i32,
        stats.views_count as i32,
    )
    .await?;

    println!(
        "Successfully polled stats for post {}: likes={}, comments={}, reposts={}, views={}",
        db_post_id, stats.likes_count, stats.comments_count, stats.reposts_count, stats.views_count
    );

    Ok(())
}
