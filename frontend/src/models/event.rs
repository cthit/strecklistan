use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)] // currently not used
pub struct Event {
    pub id: i32,
    pub title: String,
    pub background: String,
    pub location: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub price: i32,
    pub published: bool,
    pub signups: i64,
}
