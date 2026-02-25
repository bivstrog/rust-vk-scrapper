use crate::models::{PostDetails, PostInfoData, PostWithData};
use crate::utils::{get_pooling_delta_seconds, get_pooling_period_seconds};
use sqlx::Row;
use sqlx::postgres::PgPool;

pub async fn is_ready_to_finish(pool: &PgPool, post_id: i32) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"
        SELECT CURRENT_TIMESTAMP > dt_parse_end as is_ready_to_finish
        FROM POST
        WHERE id = $1
        "#,
    )
    .bind(post_id)
    .fetch_optional(pool)
    .await?;

    match result {
        Some(row) => Ok(row.get("is_ready_to_finish")),
        None => Ok(true),
    }
}

pub async fn save_post_info(
    pool: &PgPool,
    post_id: i32,
    likes_count: i32,
    comments_count: i32,
    reposts_count: i32,
    views_count: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO POST_INFO (post_id, likes_count, comments_count, reposts_count, views_count, info_time)
        VALUES ($1, $2, $3, $4, $5, CURRENT_TIMESTAMP)
        "#
    )
    .bind(post_id)
    .bind(likes_count)
    .bind(comments_count)
    .bind(reposts_count)
    .bind(views_count)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_vk_id_by_post_id(
    pool: &PgPool,
    post_id: i32,
) -> Result<Option<String>, sqlx::Error> {
    let result = sqlx::query(
        r#"
        SELECT vk_id FROM POST
        WHERE id = $1
        "#,
    )
    .bind(post_id)
    .fetch_optional(pool)
    .await?;

    Ok(result.map(|row| row.get("vk_id")))
}

pub async fn has_recent_post_info(
    pool: &PgPool,
    post_id: i32,
    seconds: i64,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM POST_INFO
            WHERE post_id = $1
            AND info_time > CURRENT_TIMESTAMP - (2 * $2 * INTERVAL '1 second')
        ) as has_recent
        "#,
    )
    .bind(post_id)
    .bind(seconds)
    .fetch_one(pool)
    .await?;

    Ok(result.get("has_recent"))
}

pub async fn get_posts_needing_polling(pool: &PgPool) -> Result<Vec<(i32, String)>, sqlx::Error> {
    let pooling_delta = get_pooling_delta_seconds();

    let results = sqlx::query(
        r#"
        SELECT DISTINCT p.id, p.vk_id
        FROM POST p
        WHERE p.dt_parse_end > CURRENT_TIMESTAMP
        AND NOT EXISTS (
            SELECT 1 FROM POST_INFO pi
            WHERE pi.post_id = p.id
            AND pi.info_time > CURRENT_TIMESTAMP - (2 * $1 * INTERVAL '1 second')
        )
        "#,
    )
    .bind(pooling_delta)
    .fetch_all(pool)
    .await?;

    Ok(results
        .iter()
        .map(|row| (row.get("id"), row.get("vk_id")))
        .collect())
}

pub async fn get_or_create_post_with_prolong(
    pool: &PgPool,
    vk_id: &str,
    prolong: bool,
) -> Result<PostDetails, sqlx::Error> {
    let pooling_period = get_pooling_period_seconds();

    // Start a transaction to prevent race conditions
    let mut tx = pool.begin().await?;

    // Try to find an existing post within the current time range with row lock
    let existing_post = sqlx::query(
        r#"
        SELECT id, vk_id, dt_parse_begin, dt_parse_end
        FROM POST
        WHERE vk_id = $1
        AND tsrange(dt_parse_begin, dt_parse_end) @> CURRENT_TIMESTAMP::timestamp
        LIMIT 1
        FOR UPDATE
        "#,
    )
    .bind(vk_id)
    .fetch_optional(&mut *tx)
    .await?;

    let post_details = if let Some(row) = existing_post {
        if prolong {
            // Prolong the existing post
            let updated = sqlx::query(
                r#"
                UPDATE POST
                SET dt_parse_end = CURRENT_TIMESTAMP + ($1 * INTERVAL '1 second')
                WHERE id = $2
                RETURNING id, vk_id, dt_parse_begin, dt_parse_end
                "#,
            )
            .bind(pooling_period)
            .bind(row.get::<i32, _>("id"))
            .fetch_one(&mut *tx)
            .await?;

            PostDetails {
                id: updated.get("id"),
                vk_id: updated.get("vk_id"),
                dt_parse_begin: updated.get("dt_parse_begin"),
                dt_parse_end: updated.get("dt_parse_end"),
            }
        } else {
            // Return existing post without prolonging
            PostDetails {
                id: row.get("id"),
                vk_id: row.get("vk_id"),
                dt_parse_begin: row.get("dt_parse_begin"),
                dt_parse_end: row.get("dt_parse_end"),
            }
        }
    } else {
        // No existing post found, create a new one
        let result = sqlx::query(
            r#"
            INSERT INTO POST (vk_id, dt_parse_begin, dt_parse_end)
            VALUES ($1, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP + ($2 * INTERVAL '1 second'))
            RETURNING id, vk_id, dt_parse_begin, dt_parse_end
            "#,
        )
        .bind(vk_id)
        .bind(pooling_period)
        .fetch_one(&mut *tx)
        .await?;

        PostDetails {
            id: result.get("id"),
            vk_id: result.get("vk_id"),
            dt_parse_begin: result.get("dt_parse_begin"),
            dt_parse_end: result.get("dt_parse_end"),
        }
    };

    // Commit the transaction
    tx.commit().await?;

    Ok(post_details)
}

pub async fn get_post_with_data(
    pool: &PgPool,
    scrapper_id: i32,
) -> Result<Option<PostWithData>, sqlx::Error> {
    // Get post details
    let post = sqlx::query(
        r#"
        SELECT id, vk_id, dt_parse_begin, dt_parse_end
        FROM POST
        WHERE id = $1
        "#,
    )
    .bind(scrapper_id)
    .fetch_optional(pool)
    .await?;

    let post = match post {
        Some(p) => p,
        None => return Ok(None),
    };

    // Get post info data sorted by info_time
    let data_rows = sqlx::query(
        r#"
        SELECT comments_count, likes_count, views_count, reposts_count, info_time
        FROM POST_INFO
        WHERE post_id = $1
        ORDER BY info_time ASC
        "#,
    )
    .bind(scrapper_id)
    .fetch_all(pool)
    .await?;

    let data: Vec<PostInfoData> = data_rows
        .iter()
        .map(|row| PostInfoData {
            comments_count: row.get("comments_count"),
            likes_count: row.get("likes_count"),
            views_count: row.get("views_count"),
            reposts_count: row.get("reposts_count"),
            info_time: row.get("info_time"),
        })
        .collect();

    Ok(Some(PostWithData {
        id: post.get("id"),
        vk_id: post.get("vk_id"),
        dt_parse_begin: post.get("dt_parse_begin"),
        dt_parse_end: post.get("dt_parse_end"),
        data,
    }))
}
