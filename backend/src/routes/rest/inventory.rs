use crate::database::DatabasePool;
use crate::models::inventory::{
    InventoryBundle as InventoryBundleRel, InventoryBundleItem,
    NewInventoryBundle as NewInventoryBundleRel, NewInventoryBundleItem,
};
use crate::util::ser::{Ser, SerAccept};
use crate::util::status_json::StatusJson as SJ;
use chrono::Utc;
use diesel::prelude::*;
use itertools::Itertools;
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
