/// Object oriented data models - for consumers of the API.
pub use strecklistan_api::transaction as object;

/// Relational data models - as represented by the database.
pub mod relational {
    use crate::schema::tables::{transaction_bundles, transaction_items, transactions};
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};

    #[derive(Insertable, Serialize, Deserialize, Debug, PartialEq)]
    #[diesel(table_name = transactions)]
    pub struct NewTransaction {
        pub description: Option<String>,
        pub time: Option<DateTime<Utc>>,
        pub debited_account: i32,
        pub credited_account: i32,
        pub amount: i32,
    }

    #[derive(Queryable, Serialize, Deserialize, Debug, PartialEq)]
    pub struct Transaction {
        pub id: i32,
        pub description: Option<String>,
        pub time: DateTime<Utc>,
        pub debited_account: i32,
        pub credited_account: i32,
        pub amount: i32,
        pub deleted_at: Option<DateTime<Utc>>,
    }

    #[derive(Insertable, Serialize, Deserialize, Debug, PartialEq)]
    #[diesel(table_name = transaction_bundles)]
    pub struct NewTransactionBundle {
        pub transaction_id: i32,
        pub description: Option<String>,
        pub price: Option<i32>,
        pub change: i32,
    }

    #[derive(Queryable, Serialize, Deserialize, Debug, PartialEq)]
    pub struct TransactionBundle {
        pub id: i32,
        pub transaction_id: i32,
        pub description: Option<String>,
        pub price: Option<i32>,
        pub change: i32,
    }

    #[derive(Insertable, Serialize, Deserialize, Debug, PartialEq)]
    #[diesel(table_name = transaction_items)]
    pub struct NewTransactionItem {
        pub bundle_id: i32,
        pub item_id: i32,
    }

    #[derive(Queryable, Serialize, Deserialize, Debug, PartialEq)]
    pub struct TransactionItem {
        pub id: i32,
        pub bundle_id: i32,
        pub item_id: i32,
    }
}
