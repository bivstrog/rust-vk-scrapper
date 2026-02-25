use serde::{Deserialize, Serialize};

// Request/Response structures for API endpoints
#[derive(Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct PollingRequest {
    pub vk_link: String,
    pub prolong: bool,
}

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
pub struct PollingResponse {
    pub scrapper_id: i32,
    pub vk_id: String,
    pub dt_parse_begin: String,
    pub dt_parse_end: String,
}

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
pub struct PostInfoDataResponse {
    pub comments_count: i32,
    pub likes_count: i32,
    pub views_count: i32,
    pub reposts_count: i32,
    pub info_time: String,
}

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
pub struct GetPollingResponse {
    pub scrapper_id: i32,
    pub vk_id: String,
    pub dt_parse_begin: String,
    pub dt_parse_end: String,
    pub dt_current: String,
    pub data: Vec<PostInfoDataResponse>,
}

// VK API structures
#[derive(Debug, Serialize, Deserialize)]
pub struct VkPostStats {
    pub comments_count: u64,
    pub likes_count: u64,
    pub views_count: u64,
    pub reposts_count: u64,
}

// Database structures
pub struct PostDetails {
    pub id: i32,
    pub vk_id: String,
    pub dt_parse_begin: chrono::NaiveDateTime,
    pub dt_parse_end: chrono::NaiveDateTime,
}

pub struct PostInfoData {
    pub comments_count: i32,
    pub likes_count: i32,
    pub views_count: i32,
    pub reposts_count: i32,
    pub info_time: chrono::NaiveDateTime,
}

pub struct PostWithData {
    pub id: i32,
    pub vk_id: String,
    pub dt_parse_begin: chrono::NaiveDateTime,
    pub dt_parse_end: chrono::NaiveDateTime,
    pub data: Vec<PostInfoData>,
}
