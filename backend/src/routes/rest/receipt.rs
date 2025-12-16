use crate::database::DatabasePool;
use crate::database::transaction::{TransactionFilter, objectify_transations, query_transaction};
use crate::models::izettle_transaction::IZettlePostTransaction;
use crate::util::status_json::StatusJson as SJ;
use diesel::prelude::*;
use rocket::http::Status;
use rocket::response::content::RawHtml;
use rocket::{State, get};
use rocket_dyn_templates::Template;
use serde::Serialize;
use std::collections::HashMap;
use strecklistan_api::inventory::{InventoryItem, InventoryItemId};
use strecklistan_api::transaction::TransactionId;

const RECEIPT_TEMPLATE_NAME: &str = "receipt";

#[derive(Debug, Serialize)]
struct ReceiptTemplateData {
    date: String,
    products: Vec<ReceiptItem>,
    total: f64,
    transaction_id: TransactionId,
    payment_meta: Vec<ReceiptMetaItem>,
}

#[derive(Debug, Serialize)]
struct ReceiptItem {
    pub product: String,
    pub count: i32,
    pub amount: f64,
}

#[derive(Debug, Serialize)]
struct ReceiptMetaItem {
    key: String,
    value: String,
}

#[get("/receipt/<transaction_id>")]
pub async fn get_receipt(
    db_pool: &State<DatabasePool>,
    transaction_id: TransactionId,
) -> Result<RawHtml<Template>, SJ> {
    let mut connection = db_pool.inner().get()?;

    // query all data associated with the transaction id
    let (transaction, izettle, inventory) = connection.transaction::<_, SJ, _>(|connection| {
        let transaction = query_transaction(
            connection,
            TransactionFilter {
                id: Some(transaction_id),
                ..Default::default()
            },
        )?;
        let transaction = objectify_transations(transaction)
            .into_iter()
            .next()
            .ok_or(Status::NotFound)?;

        let izettle: Option<IZettlePostTransaction> = {
            use crate::schema::tables::izettle_post_transaction::dsl;
            dsl::izettle_post_transaction
                .filter(dsl::transaction_id.eq(transaction_id))
                .first(connection)
                .optional()?
        };

        // the inventory is used for looking up names of TransactionItem:s
        let inventory: HashMap<InventoryItemId, InventoryItem> = {
            use crate::schema::tables::inventory::dsl;
            dsl::inventory
                .load(connection)?
                .into_iter()
                .map(|item: InventoryItem| (item.id, item))
                .collect()
        };

        Ok((transaction, izettle, inventory))
    })?;

    // generate receipt items from transaction bundles
    let receipt_items: Vec<ReceiptItem> = transaction
        .bundles
        .iter()
        .map(|bundle| {
            let get_name = |item_id| inventory[&item_id].name.as_str();
            let rendered = bundle.render(&get_name);
            ReceiptItem {
                product: rendered.name.to_string(),
                count: -rendered.change,
                amount: rendered.price.unwrap_or_default().as_f64(),
                // If we ever want to display complicated bundles in the future...
                //sub_items: rendered.items.map(...).collect()
            }
        })
        .collect();

    /// Helper for building a ReceiptMetaItem
    fn meta<K: ToString>(key: K, value: Option<String>) -> Option<ReceiptMetaItem> {
        value.map(|value| ReceiptMetaItem {
            key: key.to_string(),
            value,
        })
    }

    let mut payment_meta = vec![];

    // If we have card payment information, append it to the payment_meta
    if let Some(izettle) = izettle {
        payment_meta.append(&mut vec![
            izettle.card_type.and_then(|t| meta(t, izettle.masked_pan)),
            meta("Betalsätt", izettle.card_payment_entry_mode),
            meta("Bank", izettle.card_issuing_bank),
        ]);
    }

    let data = ReceiptTemplateData {
        date: transaction.time.format("%Y-%m-%d %H:%M:%S").to_string(),
        products: receipt_items,
        total: transaction.amount.as_f64(),
        transaction_id,
        payment_meta: payment_meta.into_iter().flatten().collect(),
    };

    Ok(RawHtml(Template::render(RECEIPT_TEMPLATE_NAME, &data)))
}
