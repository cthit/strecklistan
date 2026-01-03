use crate::database::DatabasePool;
use crate::models::inventory::{
    InventoryBundle as InventoryBundleRel, InventoryBundleItem, InventoryCSVExportItem,
    NewInventoryBundle as NewInventoryBundleRel, NewInventoryBundleItem,
};
use crate::util::CsvResponse;
use crate::util::ser::{Ser, SerAccept};
use crate::util::status_json::StatusJson as SJ;
use chrono::Utc;
use diesel::prelude::*;
use itertools::Itertools;
use rocket::form::Form;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{State, delete, get, post, put};
use std::collections::HashMap;
use strecklistan_api::inventory::InventoryBundle as InventoryBundleObj;
use strecklistan_api::inventory::{
    InventoryBundleId, InventoryItemId, InventoryItemStock, InventoryItemTag,
    NewInventoryBundle as NewInventoryBundleObj, NewInventoryItem,
};
use strecklistan_api::transaction::TransactionId;
use tokio::io::AsyncReadExt;

#[get("/inventory/items")]
pub fn get_items(
    db_pool: &State<DatabasePool>,
    accept: SerAccept,
) -> Result<Ser<HashMap<InventoryItemId, InventoryItemStock>>, SJ> {
    let mut connection = db_pool.inner().get()?;

    use crate::schema::views::inventory_stock::dsl::inventory_stock;
    Ok(accept.ser(
        inventory_stock
            .load(&mut connection)?
            .into_iter()
            .map(|item: InventoryItemStock| (item.id, item))
            .collect(),
    ))
}

#[post("/inventory/item", data = "<item>")]
pub fn post_item(
    db_pool: &State<DatabasePool>,
    accept: SerAccept,
    item: Json<NewInventoryItem>,
) -> Result<Ser<InventoryItemId>, SJ> {
    let NewInventoryItem {
        name,
        price,
        image_url,
    } = item.into_inner();
    let mut connection = db_pool.inner().get()?;
    use crate::schema::tables::inventory::dsl;
    let id = diesel::insert_into(dsl::inventory)
        .values((
            dsl::name.eq(name),
            dsl::price.eq(price),
            dsl::image_url.eq(image_url),
        ))
        .returning(dsl::id)
        .get_result(&mut connection)?;
    Ok(accept.ser(id))
}

#[put("/inventory/item/<id>", data = "<item>")]
pub fn put_item(
    db_pool: &State<DatabasePool>,
    id: InventoryItemId,
    item: Json<NewInventoryItem>,
) -> Result<SJ, SJ> {
    let NewInventoryItem {
        name,
        price,
        image_url,
    } = item.into_inner();
    let mut connection = db_pool.inner().get()?;
    use crate::schema::tables::inventory::dsl;
    diesel::update(dsl::inventory)
        .filter(dsl::id.eq(id))
        .set((
            dsl::name.eq(name),
            dsl::price.eq(price),
            dsl::image_url.eq(image_url),
        ))
        .execute(&mut connection)?;

    Ok(Status::Ok.into())
}

#[delete("/inventory/item/<id>")]
pub fn delete_item(db_pool: &State<DatabasePool>, id: InventoryItemId) -> Result<SJ, SJ> {
    let mut connection = db_pool.inner().get()?;
    connection.transaction::<_, SJ, _>(|connection| {
        // check if an existing transaction is referencing this item
        let can_delete = {
            use crate::schema::tables::transaction_items::dsl;
            dsl::transaction_items
                .filter(dsl::item_id.eq(id))
                .select(dsl::id)
                .get_result::<TransactionId>(connection)
                .optional()?
                .is_none()
        };

        use crate::schema::tables::inventory::dsl;

        if can_delete {
            // if no transaction references this item, we can delete it
            diesel::delete(dsl::inventory.filter(dsl::id.eq(id))).execute(connection)?;
        } else {
            // otherwise just mark it as deleted
            diesel::update(dsl::inventory)
                .filter(dsl::id.eq(id))
                .set(dsl::deleted_at.eq(Utc::now()))
                .execute(connection)?;
        }

        Ok(Status::Ok.into())
    })
}

#[get("/inventory/tags")]
pub fn get_tags(
    db_pool: &State<DatabasePool>,
    accept: SerAccept,
) -> Result<Ser<Vec<InventoryItemTag>>, SJ> {
    let mut connection = db_pool.inner().get()?;

    use crate::schema::tables::inventory_tags::dsl::inventory_tags;
    Ok(accept.ser(inventory_tags.load(&mut connection)?))
}

#[get("/inventory/bundles")]
pub fn get_bundles(
    db_pool: &State<DatabasePool>,
    accept: SerAccept,
) -> Result<Ser<HashMap<InventoryBundleId, InventoryBundleObj>>, SJ> {
    let mut connection = db_pool.inner().get()?;

    use crate::schema::tables::inventory_bundle_items::dsl::{bundle_id, inventory_bundle_items};
    use crate::schema::tables::inventory_bundles::dsl::{id, inventory_bundles};

    let joined: Vec<(InventoryBundleRel, Option<InventoryBundleItem>)> = inventory_bundles
        .left_join(inventory_bundle_items.on(bundle_id.eq(id)))
        .load(&mut connection)?;

    let bundles = joined
        .into_iter()
        .chunk_by(|(bundle, _)| bundle.id)
        .into_iter()
        .map(|(_, mut elements)| {
            let (bundle, item) = elements.next().unwrap();
            InventoryBundleObj {
                id: bundle.id,
                name: bundle.name,
                price: bundle.price.into(),
                image_url: bundle.image_url,
                item_ids: std::iter::once(item)
                    .chain(elements.map(|(_, item)| item))
                    .flatten() // Remove None:s
                    .map(|item| item.item_id)
                    .collect(),
            }
        })
        .map(|bundle| (bundle.id, bundle))
        .collect();

    Ok(accept.ser(bundles))
}

#[post("/inventory/bundle", data = "<bundle>")]
pub fn post_bundle(
    db_pool: &State<DatabasePool>,
    accept: SerAccept,
    bundle: Json<NewInventoryBundleObj>,
) -> Result<Ser<i32>, SJ> {
    let bundle = bundle.into_inner();
    let mut connection = db_pool.inner().get()?;
    connection.transaction::<_, SJ, _>(|connection| {
        let bundle_id = {
            use crate::schema::tables::inventory_bundles::dsl::{id, inventory_bundles};

            let new_bundle = NewInventoryBundleRel {
                name: bundle.name,
                price: bundle.price.into(),
                image_url: bundle.image_url,
            };

            diesel::insert_into(inventory_bundles)
                .values(new_bundle)
                .returning(id)
                .get_result(connection)?
        };

        {
            use crate::schema::tables::inventory_bundle_items::dsl::inventory_bundle_items;

            let new_items: Vec<_> = bundle
                .item_ids
                .into_iter()
                .map(|item_id| NewInventoryBundleItem { bundle_id, item_id })
                .collect();

            diesel::insert_into(inventory_bundle_items)
                .values(&new_items)
                .execute(connection)?;
        }

        Ok(accept.ser(bundle_id))
    })
}

#[put("/inventory/bundle/<bundle_id>", data = "<bundle>")]
pub fn put_bundle(
    db_pool: &State<DatabasePool>,
    bundle_id: InventoryBundleId,
    bundle: Json<NewInventoryBundleObj>,
) -> Result<SJ, SJ> {
    let mut connection = db_pool.inner().get()?;
    connection.transaction::<_, SJ, _>(|connection| {
        use crate::schema::tables::inventory_bundles::dsl::{id, inventory_bundles};

        let bundle = bundle.into_inner();
        let new_bundle = NewInventoryBundleRel {
            name: bundle.name,
            price: bundle.price.into(),
            image_url: bundle.image_url,
        };

        diesel::update(inventory_bundles)
            .set(&new_bundle)
            .filter(id.eq(bundle_id))
            .execute(connection)?;

        // TODO: handle changed items

        Ok(Status::Ok.into())
    })
}

#[delete("/inventory/bundle/<id>")]
pub fn delete_inventory_bundle(
    db_pool: &State<DatabasePool>,
    id: InventoryBundleId,
) -> Result<SJ, SJ> {
    let mut connection = db_pool.inner().get()?;
    connection.transaction::<_, SJ, _>(|connection| {
        {
            use crate::schema::tables::inventory_bundle_items::dsl::{
                bundle_id, inventory_bundle_items,
            };

            diesel::delete(inventory_bundle_items.filter(bundle_id.eq(id))).execute(connection)?;
        }

        {
            use crate::schema::tables::inventory_bundles::dsl;
            let deleted_id: i32 = diesel::delete(dsl::inventory_bundles.filter(dsl::id.eq(id)))
                .returning(dsl::id)
                .get_result(connection)?;
            assert_eq!(deleted_id, id);
        }

        Ok(Status::Ok.into())
    })
}

#[get("/inventory/csv")]
pub fn generate_csv<'r>(db_pool: &'_ State<DatabasePool>) -> Result<CsvResponse<'_>, SJ> {
    let mut connection = db_pool.inner().get()?;

    use crate::schema::views::inventory_stock::dsl::{inventory_stock, name, price, stock};
    let items: Vec<InventoryCSVExportItem> = inventory_stock
        .select((name, price, stock))
        .load(&mut connection)?;

    let mut wtr = csv::Writer::from_writer(vec![]);
    for item in items {
        wtr.serialize(item).map_err(|e| {
            SJ::new(
                Status::InternalServerError,
                format!("Failed to serialize CSV: {}", e),
            )
        })?;
    }
    let data = wtr.into_inner().map_err(|e| {
        SJ::new(
            Status::InternalServerError,
            format!("Failed to finalize CSV: {}", e),
        )
    })?;

    Ok(CsvResponse {
        data,
        filename: "inventory.csv",
    })
}

#[derive(rocket::FromForm)]
struct CsvUpload<'r> {
    file: rocket::fs::TempFile<'r>,
}

#[put("/inventory/csv/update", data = "<form>")]
pub async fn update_inventory_from_csv(
    db_pool: &State<DatabasePool>,
    config: &State<crate::Opt>,
    mut form: Form<CsvUpload<'_>>,
) -> Result<SJ, SJ> {
    let asset_account = config.csv_import_asset_account.ok_or_else(|| {
        SJ::new(
            Status::InternalServerError,
            "CSV_IMPORT_ASSET_ACCOUNT is not configured".to_string(),
        )
    })?;

    let expense_account = config.csv_import_expense_account.ok_or_else(|| {
        SJ::new(
            Status::InternalServerError,
            "CSV_IMPORT_EXPENSE_ACCOUNT is not configured".to_string(),
        )
    })?;

    let file = &mut form.file;
    if !file.content_type().map_or(false, |ct| ct.is_csv()) {
        return Err(SJ::new(
            Status::BadRequest,
            "Invalid file: must be a CSV".to_string(),
        ));
    }

    let mut csv_data = file.open().await.map_err(|_| {
        SJ::new(
            Status::InternalServerError,
            "Failed to read file".to_string(),
        )
    })?;

    let mut csv_string = String::new();
    csv_data
        .read_to_string(&mut csv_string)
        .await
        .map_err(|_| {
            SJ::new(
                Status::InternalServerError,
                "Failed to parse CSV content".to_string(),
            )
        })?;

    let mut rdr = csv::Reader::from_reader(csv_string.as_bytes());
    let headers = rdr
        .headers()
        .map_err(|_| SJ::new(Status::BadRequest, "Invalid CSV format".to_string()))?;

    // I can't figure out how to compare csv::StringRecord with something that is not allocated...
    // so we allocate a vec here. Consider optimizing later if performance is an issue.
    if headers != vec!["name", "price", "stock"] {
        return Err(SJ::new(
            Status::BadRequest,
            "Invalid CSV headers: expected 'name,price,stock'".to_string(),
        ));
    }

    let items: Vec<InventoryCSVExportItem> = rdr
        .deserialize()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| SJ::new(Status::BadRequest, format!("Invalid CSV data: {}", e)))?;

    let mut connection = db_pool.inner().get()?;

    connection.transaction::<_, SJ, _>(|connection| {
        use crate::schema::tables::inventory::dsl as inv_dsl;
        use crate::schema::views::inventory_stock::dsl as stock_dsl;

        // Collect all items that need stock adjustments
        let mut stock_adjustments: Vec<(InventoryItemId, String, Option<i32>, i32)> = Vec::new();

        for csv_item in items {
            let existing: Option<InventoryItemStock> = stock_dsl::inventory_stock
                .filter(stock_dsl::name.eq(&csv_item.name))
                .filter(stock_dsl::deleted_at.is_null())
                .first(connection)
                .optional()?;

            let (item_id, current_stock) = if let Some(existing_item) = existing {
                if csv_item.price.is_some() && csv_item.price != existing_item.price {
                    diesel::update(inv_dsl::inventory)
                        .filter(inv_dsl::id.eq(existing_item.id))
                        .set(inv_dsl::price.eq(csv_item.price))
                        .execute(connection)?;
                }
                (existing_item.id, existing_item.stock)
            } else {
                let new_id: InventoryItemId = diesel::insert_into(inv_dsl::inventory)
                    .values((
                        inv_dsl::name.eq(&csv_item.name),
                        inv_dsl::price.eq(csv_item.price),
                    ))
                    .returning(inv_dsl::id)
                    .get_result(connection)?;
                (new_id, 0)
            };

            let stock_change = csv_item.stock - current_stock;

            if stock_change != 0 {
                stock_adjustments.push((item_id, csv_item.name, csv_item.price, stock_change));
            }
        }

        // Create a single transaction for all stock adjustments if there are any
        if !stock_adjustments.is_empty() {
            // Split adjustments into positive and negative changes
            let (positive_adjustments, negative_adjustments): (Vec<_>, Vec<_>) =
                stock_adjustments.into_iter()
                    .partition(|(_, _, _, change)| *change > 0);

            // Helper function to create a transaction with bundles
            let create_transaction = |connection: &mut PgConnection,
                                     adjustments: Vec<(InventoryItemId, String, Option<i32>, i32)>,
                                     debited: i32,
                                     credited: i32,
                                     description: &str| -> Result<(), SJ> {
                if adjustments.is_empty() {
                    return Ok(());
                }

                let total_amount: i32 = adjustments.iter()
                    .filter_map(|(_, _, price, stock_change)| {
                        price.map(|p| p * stock_change.abs())
                    })
                    .sum();

                let transaction_id = {
                    use crate::schema::tables::transactions::dsl as trans_dsl;
                    diesel::insert_into(trans_dsl::transactions)
                        .values((
                            trans_dsl::description.eq(Some(description.to_string())),
                            trans_dsl::debited_account.eq(debited),
                            trans_dsl::credited_account.eq(credited),
                            trans_dsl::amount.eq(total_amount),
                        ))
                        .returning(trans_dsl::id)
                        .get_result::<TransactionId>(connection)?
                };

                // Create bundles for each item adjustment
                for (item_id, name, price, stock_change) in adjustments {
                    let bundle_id = {
                        use crate::schema::tables::transaction_bundles::dsl as bundle_dsl;
                        diesel::insert_into(bundle_dsl::transaction_bundles)
                            .values((
                                bundle_dsl::transaction_id.eq(transaction_id),
                                bundle_dsl::description.eq(Some(name)),
                                bundle_dsl::price.eq(price),
                                bundle_dsl::change.eq(stock_change),
                            ))
                            .returning(bundle_dsl::id)
                            .get_result::<i32>(connection)?
                    };

                    {
                        use crate::schema::tables::transaction_items::dsl as item_dsl;
                        diesel::insert_into(item_dsl::transaction_items)
                            .values((
                                item_dsl::bundle_id.eq(bundle_id),
                                item_dsl::item_id.eq(item_id),
                            ))
                            .execute(connection)?;
                    }
                }

                Ok(())
            };

            // Create transaction for positive changes
            create_transaction(
                connection,
                positive_adjustments,
                expense_account,
                asset_account,
                &config.csv_import_transaction_description
            )?;

            // Create transaction for negative changes
            // Use separate description if provided, otherwise use the same as increases
            let decrease_description = config.csv_import_transaction_description_decrease
                .as_ref()
                .unwrap_or(&config.csv_import_transaction_description);

            create_transaction(
                connection,
                negative_adjustments,
                asset_account,
                expense_account,
                decrease_description
            )?;
        }

        // Refresh the materialized view to update stock counts
        diesel::sql_query("REFRESH MATERIALIZED VIEW inventory_stock").execute(connection)?;

        Ok(Status::Ok.into())
    })
}
