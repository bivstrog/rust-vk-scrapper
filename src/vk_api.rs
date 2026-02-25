use crate::models::VkPostStats;
use crate::utils::{get_vk_api_domain, get_vk_api_version, get_vk_token};
use reqwest;
use rocket::response::status;
use serde_json::Value;

pub async fn call_vk(post_id: &str) -> Result<VkPostStats, status::BadRequest<String>> {
    let token = get_vk_token().map_err(|e| status::BadRequest(e))?;
    let domain = get_vk_api_domain().map_err(|e| status::BadRequest(e))?;
    let version = get_vk_api_version();

    let url = format!(
        "{}?access_token={}&v={}&posts={}",
        domain, token, version, post_id
    );

    let response = reqwest::get(&url)
        .await
        .map_err(|e| status::BadRequest(format!("Request failed: {}", e)))?;

    let data = response
        .text()
        .await
        .map_err(|e| status::BadRequest(format!("Failed to read response: {}", e)))?;

    // Parse JSON response
    let json_data: Value = serde_json::from_str(&data)
        .map_err(|e| status::BadRequest(format!("Failed to parse JSON: {}", e)))?;

    // Extract the required fields from the first post in the response array
    let post = &json_data["response"][0];

    let comments_count = post["comments"]["count"].as_u64().unwrap_or(0);
    let likes_count = post["likes"]["count"].as_u64().unwrap_or(0);
    let views_count = post["views"]["count"].as_u64().unwrap_or(0);
    let reposts_count = post["reposts"]["count"].as_u64().unwrap_or(0);

    Ok(VkPostStats {
        comments_count,
        likes_count,
        views_count,
        reposts_count,
    })
}
