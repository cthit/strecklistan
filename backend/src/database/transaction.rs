use crate::database::DatabaseConn;
use crate::models::transaction::{object, relational};
use diesel::prelude::*;
use diesel::result::Error;
use diesel::sql_types::Bool;
use itertools::Itertools;
use std::collections::HashMap;
use strecklistan_api::transaction::TransactionId;

pub type TransactionJoined = Vec<(
    relational::Transaction,
    Option<relational::TransactionBundle>,
    Option<relational::TransactionItem>,
)>;

#[derive(Default)]
#[non_exhaustive]
pub struct TransactionFilter {
    /// Whether to include rows marked as deleted
    pub deleted: bool,

    /// Only yield rows with this transaction id
    pub id: Option<TransactionId>,
}

pub fn query_transaction(
    connection: &mut DatabaseConn,
    filter: TransactionFilter,
) -> Result<TransactionJoined, Error> {
    use crate::schema::tables::transaction_bundles::dsl::{
        id as bundle_id, transaction_bundles, transaction_id as bundle_transaction_id,
    };
    use crate::schema::tables::transaction_items::dsl::{
        bundle_id as item_bundle_id, transaction_items,
    };
    use crate::schema::tables::transactions::dsl::{
        deleted_at, id as transaction_id, time, transactions,
    };

    transactions
        .filter(deleted_at.is_null().or::<_, Bool>(filter.deleted))
        .filter(
            transaction_id
                .eq(filter.id.unwrap_or(-1))
                .or::<_, Bool>(filter.id.is_none()),
        )
        .left_join(transaction_bundles.on(transaction_id.eq(bundle_transaction_id)))
        .left_join(transaction_items.on(bundle_id.eq(item_bundle_id)))
        .order_by(time.desc())
        .order_by(transaction_id.desc())
        .load(connection)
}

/// Convert the flat joined rows of a transaction in the database, to a hierarchical object.
pub fn objectify_transations(transactions: TransactionJoined) -> Vec<object::Transaction> {
    transactions
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
        .collect()
}
