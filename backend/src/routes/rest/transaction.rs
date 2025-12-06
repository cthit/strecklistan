use crate::database::DatabasePool;
use crate::database::transaction::query_transaction;
use crate::models::transaction::{object, relational};
use crate::util::ser::{Ser, SerAccept};
use crate::util::status_json::StatusJson as SJ;
use diesel::prelude::*;
use itertools::Itertools;
use rocket::serde::json::Json;
use rocket::{State, delete, get, post};
use std::collections::HashMap;

/// POST `/transaction`
///
/// Create a new transaction
#[post("/transaction", data = "<transaction>")]
pub fn post_transaction(
    db_pool: &State<DatabasePool>,
    accept: SerAccept,
    transaction: Json<object::NewTransaction>,
) -> Result<Ser<i32>, SJ> {
    let mut connection = db_pool.inner().get()?;

    let object::NewTransaction {
        description,
        bundles,
        debited_account,
        credited_account,
        amount,
    } = transaction.into_inner();

    let transaction = relational::NewTransaction {
        description,
        time: None,
        debited_account,
        credited_account,
        amount: amount.into(),
    };

    connection.transaction::<_, SJ, _>(|connection| {
        let transaction_id = {
            use crate::schema::tables::transactions::dsl::*;
            diesel::insert_into(transactions)
                .values(transaction)
                .returning(id)
                .get_result(connection)?
        };

        for bundle in bundles.into_iter() {
            let new_bundle = relational::NewTransactionBundle {
                transaction_id,
                description: bundle.description,
                price: bundle.price.map(|p| p.into()),
                change: bundle.change,
            };

            let bundle_id = {
                use crate::schema::tables::transaction_bundles::dsl::*;
                diesel::insert_into(transaction_bundles)
                    .values(&new_bundle)
                    .returning(id)
                    .get_result(connection)?
            };

            let item_ids: Vec<_> = bundle
                .item_ids
                .into_iter()
                .flat_map(|(item_id, count)| std::iter::repeat(item_id).take(count as usize))
                .map(|item_id| relational::NewTransactionItem { bundle_id, item_id })
                .collect();

            {
                use crate::schema::tables::transaction_items::dsl::*;
                diesel::insert_into(transaction_items)
                    .values(&item_ids)
                    .execute(connection)?;
            }
        }

        Ok(accept.ser(transaction_id))
    })
}

/// DELETE `/transaction/<transaction_id>`
#[delete("/transaction/<transaction_id>")]
pub fn delete_transaction(
    db_pool: &State<DatabasePool>,
    accept: SerAccept,
    transaction_id: i32,
) -> Result<Ser<i32>, SJ> {
    let mut connection = db_pool.inner().get()?;

    use crate::schema::tables::transactions::dsl::{deleted_at, id, transactions};
    let deleted_id = diesel::update(transactions)
        .set(deleted_at.eq(Some(chrono::Utc::now().naive_utc())))
        .filter(id.eq(transaction_id))
        .returning(id)
        .get_result(&mut connection)?;

    Ok(accept.ser(deleted_id))
}

/// GET `/transactions`
///
/// Returns a list of all transactions
#[get("/transactions")]
pub fn get_transactions(
    db_pool: &State<DatabasePool>,
    accept: SerAccept,
) -> Result<Ser<Vec<object::Transaction>>, SJ> {
    let mut connection = db_pool.inner().get()?;

    let joined = query_transaction(&mut connection, Default::default())?;

    let transactions: Vec<object::Transaction> = joined
        .into_iter()
        .group_by(|(tr, _, _)| tr.id)
        .into_iter()
        .map(|(_, mut xs)| {
            let (t0, b0, i0) = xs.next().unwrap();

            object::Transaction {
                id: t0.id,
                description: t0.description,
                time: t0.time,
                debited_account: t0.debited_account,
                credited_account: t0.credited_account,
                amount: t0.amount.into(),
                bundles: std::iter::once(b0.map(|b0| (b0, i0)))
                    .chain(xs.map(|(_, bx, ix)| bx.map(|bx| (bx, ix))))
                    .flatten()
                    .group_by(|(bx, _)| bx.id)
                    .into_iter()
                    .map(|(_, mut xs)| {
                        let (bundle, i0) = xs.next().unwrap();
                        let mut item_ids = HashMap::new();
                        std::iter::once(i0)
                            .chain(xs.map(|(_, ix)| ix))
                            .flatten()
                            .for_each(|i| *item_ids.entry(i.item_id).or_default() += 1);

                        object::TransactionBundle {
                            description: bundle.description,
                            price: bundle.price.map(|p| p.into()),
                            change: bundle.change,
                            item_ids,
                        }
                    })
                    .collect(),
            }
        })
        .collect();

    Ok(accept.ser(transactions))
}
