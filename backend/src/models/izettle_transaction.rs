use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
pub use strecklistan_api::transaction as object;

use crate::schema::tables::{
    izettle_post_transaction, izettle_transaction, izettle_transaction_bundle,
    izettle_transaction_item,
};

#[derive(Queryable, Serialize, Deserialize, Debug, PartialEq)]
pub struct IZettleTransactionPartial {
    pub id: i32,
    pub amount: i32,
}

#[derive(Queryable, Serialize, Deserialize, Debug, PartialEq)]
pub struct IZettleTransaction {
    pub id: i32,
    pub description: Option<String>,
    pub time: DateTime<Utc>,
    pub debited_account: i32,
    pub credited_account: i32,
    pub amount: i32,
}

#[derive(Queryable, Serialize, Deserialize, Debug, Default, PartialEq)]
pub struct IZettlePostTransaction {
    pub izettle_transaction_id: i32,
    pub transaction_id: Option<i32>,
    pub status: String,
    pub error: Option<String>,
    pub card_type: Option<String>,
    pub card_payment_entry_mode: Option<String>,
    pub card_issuing_bank: Option<String>,
    pub masked_pan: Option<String>,
}

#[derive(Insertable, Serialize, Deserialize, Debug, PartialEq)]
#[diesel(table_name = izettle_transaction)]
pub struct NewIZettleTransaction {
    pub description: Option<String>,
    pub time: Option<DateTime<Utc>>,
    pub debited_account: i32,
    pub credited_account: i32,
    pub amount: i32,
}

#[derive(Insertable, Serialize, Deserialize, Debug, PartialEq)]
#[diesel(table_name = izettle_transaction_bundle)]
pub struct NewIZettleTransactionBundle {
    pub transaction_id: i32,
    pub description: Option<String>,
    pub price: Option<i32>,
    pub change: i32,
}

#[derive(Insertable, Serialize, Deserialize, Debug, PartialEq)]
#[diesel(table_name = izettle_transaction_item)]
pub struct NewIZettleTransactionItem {
    pub bundle_id: i32,
    pub item_id: i32,
}

#[derive(Insertable, Serialize, Deserialize, Debug, PartialEq)]
#[diesel(table_name = izettle_post_transaction)]
pub struct NewIZettlePostTransaction {
    pub izettle_transaction_id: i32,
    pub transaction_id: Option<i32>,
    pub card_type: Option<String>,
    pub card_payment_entry_mode: Option<String>,
    pub card_issuing_bank: Option<String>,
    pub masked_pan: Option<String>,
    pub status: String,
    pub error: Option<String>,
}

pub const TRANSACTION_IN_PROGRESS: &str = "in_progress";
pub const TRANSACTION_PAID: &str = "paid";
pub const TRANSACTION_CANCELLED: &str = "cancelled";
pub const TRANSACTION_FAILED: &str = "failed";
