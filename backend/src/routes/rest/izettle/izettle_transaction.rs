use crate::database::DatabasePool;
use crate::models::izettle_transaction::{
    NewIZettlePostTransaction, NewIZettleTransaction, NewIZettleTransactionBundle,
    NewIZettleTransactionItem, TRANSACTION_IN_PROGRESS,
};
use crate::models::transaction::object;
use crate::routes::rest::izettle::IZettleNotifier;
use crate::util::ser::{Ser, SerAccept};
use crate::util::status_json::StatusJson as SJ;
use diesel::{Connection, RunQueryDsl};
use rocket::serde::json::Json;
use rocket::{State, post};

#[post("/izettle/client/transaction", data = "<transaction>")]
pub async fn begin_izettle_transaction(
    db_pool: &State<DatabasePool>,
    notifier: &State<IZettleNotifier>,
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

    let transaction = NewIZettleTransaction {
        description,
        time: None,
        debited_account,
        credited_account,
        amount: amount.into(),
    };

    connection.transaction::<_, SJ, _>(|connection| {
        let transactions_id = {
            use crate::schema::tables::izettle_transaction::dsl::*;
            diesel::insert_into(izettle_transaction)
                .values(transaction)
                .returning(id)
                .get_result(connection)?
        };

        for bundle in bundles.into_iter() {
            let new_bundle = NewIZettleTransactionBundle {
                transaction_id: transactions_id,
                description: bundle.description,
                price: bundle.price.map(|p| p.into()),
                change: bundle.change,
            };

            let bundle_id = {
                use crate::schema::tables::izettle_transaction_bundle::dsl::*;
                diesel::insert_into(izettle_transaction_bundle)
                    .values(&new_bundle)
                    .returning(id)
                    .get_result(connection)?
            };

            let item_ids: Vec<_> = bundle
                .item_ids
                .into_iter()
                .flat_map(|(item_id, count)| std::iter::repeat_n(item_id, count as usize))
                .map(|item_id| NewIZettleTransactionItem { bundle_id, item_id })
                .collect();

            {
                use crate::schema::tables::izettle_transaction_item::dsl::*;
                diesel::insert_into(izettle_transaction_item)
                    .values(&item_ids)
                    .execute(connection)?;
            }
        }

        {
            let post_tran: NewIZettlePostTransaction = NewIZettlePostTransaction {
                izettle_transaction_id: transactions_id,
                transaction_id: None,
                card_type: None,
                card_payment_entry_mode: None,
                card_issuing_bank: None,
                masked_pan: None,
                status: TRANSACTION_IN_PROGRESS.to_string(),
                error: None,
            };

            use crate::schema::tables::izettle_post_transaction::dsl::*;
            diesel::insert_into(izettle_post_transaction)
                .values(post_tran)
                .execute(connection)?;
        }

        notifier.notify();

        Ok(accept.ser(transactions_id))
    })
}
