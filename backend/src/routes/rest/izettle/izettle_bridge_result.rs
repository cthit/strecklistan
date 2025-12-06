use crate::database::DatabasePool;
use crate::diesel::RunQueryDsl;
use crate::models::izettle_transaction::{
    IZettlePostTransaction, IZettleTransaction, TRANSACTION_CANCELLED, TRANSACTION_FAILED,
    TRANSACTION_PAID,
};
use crate::models::transaction::relational;
use crate::models::transaction::relational::{
    NewTransaction, NewTransactionBundle, NewTransactionItem,
};
use crate::util::status_json::StatusJson as SJ;
use diesel::r2d2::{ConnectionManager, PooledConnection};
use diesel::{Connection, ExpressionMethods, JoinOnDsl, PgConnection, QueryDsl};
use itertools::Itertools;
use log::info;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{State, post};
use serde::{Deserialize, Serialize};
use std::iter;

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum PaymentResponse {
    TransactionPaid {
        /// For example, "MASTERCARD"
        card_type: Option<String>,

        /// For example, "CONTACTLESS_EMV"
        card_payment_entry_mode: Option<String>,

        /// For example, "SWEDBANK"
        card_issuing_bank: Option<String>,

        /// Masked primary account number, e.g. "************1234"
        masked_pan: Option<String>,
    },
    TransactionFailed {
        reason: String,
    },
    TransactionCancelled,
}

#[post(
    "/izettle/bridge/payment_response/<reference>",
    data = "<payment_response>"
)]
pub async fn complete_izettle_transaction(
    reference: i32,
    payment_response: Json<PaymentResponse>,
    db_pool: &State<DatabasePool>,
) -> Result<SJ, SJ> {
    let mut connection = db_pool.inner().get()?;

    connection.transaction::<_, SJ, _>(|connection| {
        let joined: Vec<(
            IZettleTransaction,
            Option<relational::TransactionBundle>,
            Option<relational::TransactionItem>,
        )> = {
            use crate::schema::tables::izettle_transaction::dsl::{
                id as transaction_id, izettle_transaction,
            };
            use crate::schema::tables::izettle_transaction_bundle::dsl::{
                id as bundle_id, izettle_transaction_bundle, transaction_id as bundle_trans_id,
            };
            use crate::schema::tables::izettle_transaction_item::dsl::{
                bundle_id as item_bundle_id, izettle_transaction_item,
            };
            izettle_transaction
                .left_join(izettle_transaction_bundle.on(bundle_trans_id.eq(transaction_id)))
                .left_join(izettle_transaction_item.on(item_bundle_id.eq(bundle_id)))
                .filter(transaction_id.eq(reference))
                .load(connection)?
        };

        let grouped = joined
            .into_iter()
            .group_by(|(transaction, _, _)| transaction.id);

        let (izettle_transaction_id, mut transaction_rows) = match grouped.into_iter().next() {
            Some(group) => group,
            None => {
                return Err(SJ::new(
                    Status::BadRequest,
                    format!("No pending transaction with reference {}", reference),
                ));
            }
        };

        {
            // Delete the transaction from izettle_transaction
            use crate::schema::tables::izettle_transaction::dsl::{
                id as iz_id, izettle_transaction,
            };
            diesel::delete(izettle_transaction)
                .filter(iz_id.eq(izettle_transaction_id))
                .execute(connection)?;
        }

        match payment_response.into_inner() {
            PaymentResponse::TransactionPaid {
                card_payment_entry_mode,
                card_type,
                card_issuing_bank,
                masked_pan,
            } => {
                // Get all the joined rows for the selected izettle transaction
                let (izettle_transaction, bundle0, item0) = transaction_rows.next().unwrap();

                // Insert transaction row from izettle_transaction to regular transaction table
                let new_transaction_id = {
                    let new_transaction: NewTransaction = NewTransaction {
                        description: izettle_transaction.description.clone(),
                        time: Some(izettle_transaction.time),
                        debited_account: izettle_transaction.debited_account,
                        credited_account: izettle_transaction.credited_account,
                        amount: izettle_transaction.amount,
                    };

                    use crate::schema::tables::transactions::dsl::*;
                    diesel::insert_into(transactions)
                        .values(new_transaction)
                        .returning(id)
                        .get_result(connection)?
                };

                // Iterate over all the joined rows for each *bundle* in the transaction
                let bundles = iter::once((bundle0, item0))
                    .chain(transaction_rows.map(|(_, bundle, item)| (bundle, item)))
                    .filter_map(|(bundle, item)| bundle.map(|bundle| (bundle, item)))
                    .group_by(|(bundle, _)| bundle.id);
                for (_bundle_id, mut bundle_rows) in bundles.into_iter() {
                    let (bundle, item0) = bundle_rows.next().unwrap();

                    // Insert bundle row from izettle_transaction_bundle to regular bundle table
                    let new_bundle_id: i32 = {
                        let new_bundle: NewTransactionBundle = NewTransactionBundle {
                            transaction_id: new_transaction_id,
                            description: bundle.description.clone(),
                            price: bundle.price,
                            change: bundle.change,
                        };

                        use crate::schema::tables::transaction_bundles::dsl::*;
                        diesel::insert_into(transaction_bundles)
                            .values(new_bundle)
                            .returning(id)
                            .get_result(connection)?
                    };

                    // Iterate over all the joined rows for each *item* in the bundle
                    let items = iter::once(item0)
                        .chain(bundle_rows.map(|(_, item)| item))
                        .flatten();
                    for item in items {
                        // Insert item row ...
                        let new_item: NewTransactionItem = NewTransactionItem {
                            bundle_id: new_bundle_id,
                            item_id: item.item_id,
                        };

                        use crate::schema::tables::transaction_items::dsl::*;
                        diesel::insert_into(transaction_items)
                            .values(new_item)
                            .execute(connection)?;
                    }
                }

                // Mark the transaction in izettle_transaction as paid
                update_izettle_post_transaction(
                    IZettlePostTransaction {
                        izettle_transaction_id,
                        transaction_id: Some(new_transaction_id),
                        status: TRANSACTION_PAID.to_string(),
                        error: None,
                        card_type,
                        card_payment_entry_mode,
                        card_issuing_bank,
                        masked_pan,
                    },
                    connection,
                )?;

                Ok(SJ::new(Status::Ok, "Transcation completed"))
            }
            PaymentResponse::TransactionFailed { reason } => {
                info!("IZettle failed due to: {}", reason);

                // Mark the transaction as failed
                update_izettle_post_transaction(
                    IZettlePostTransaction {
                        izettle_transaction_id,
                        status: TRANSACTION_FAILED.to_string(),
                        error: Some(reason),
                        ..Default::default()
                    },
                    connection,
                )?;

                Ok(SJ::new(Status::Ok, "Transcation cancelled with failure"))
            }
            PaymentResponse::TransactionCancelled => {
                // Mark the transaction as cancelled
                update_izettle_post_transaction(
                    IZettlePostTransaction {
                        izettle_transaction_id,
                        status: TRANSACTION_CANCELLED.to_string(),
                        ..Default::default()
                    },
                    connection,
                )?;

                Ok(SJ::new(Status::Ok, "Transaction cancelled"))
            }
        }
    })
}

fn update_izettle_post_transaction(
    transaction: IZettlePostTransaction,
    connection: &mut PooledConnection<ConnectionManager<PgConnection>>,
) -> Result<(), diesel::result::Error> {
    use crate::schema::tables::izettle_post_transaction::dsl::{
        card_issuing_bank as bank, card_payment_entry_mode as payment_mode, card_type as c_type,
        error as err, izettle_post_transaction, izettle_transaction_id as iz_tran_id,
        masked_pan as last_four, status as stat, transaction_id as tran_id,
    };
    let IZettlePostTransaction {
        izettle_transaction_id,
        status,
        transaction_id,
        error,
        card_type,
        card_payment_entry_mode,
        card_issuing_bank,
        masked_pan,
    } = transaction;

    diesel::update(izettle_post_transaction)
        .filter(iz_tran_id.eq(izettle_transaction_id))
        .set((
            tran_id.eq(transaction_id),
            stat.eq(status),
            err.eq(error),
            c_type.eq(card_type),
            payment_mode.eq(card_payment_entry_mode),
            bank.eq(card_issuing_bank),
            last_four.eq(masked_pan),
        ))
        .execute(connection)?;

    Ok(())
}
