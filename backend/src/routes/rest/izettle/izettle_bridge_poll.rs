use crate::database::DatabasePool;
use crate::diesel::RunQueryDsl;
use crate::models::izettle_transaction::IZettleTransactionPartial;
use crate::routes::rest::izettle::IZettleNotifier;
use crate::schema::tables::izettle_transaction::dsl::izettle_transaction;
use crate::util::StatusJson;
use crate::util::ser::{Ser, SerAccept};
use diesel::result::Error;
use diesel::{ExpressionMethods, QueryDsl, QueryResult};
use rocket::{State, get};
use serde::Serialize;
use std::time::Duration;

#[derive(Serialize)]
#[serde(tag = "type")]
pub enum BridgePollResult {
    PendingPayment(IZettleTransactionPartial),
    NoPendingTransaction,
}

/// Check if a pending transaction exists
///
/// If timeout is set, hang the request until a transaction
/// arrives or the timeout (milliseconds) has passed
#[get("/izettle/bridge/poll?<timeout>")]
pub async fn poll_for_transaction(
    db_pool: &State<DatabasePool>,
    notifier: &State<IZettleNotifier>,
    timeout: Option<u64>,
    accept: SerAccept,
) -> Result<Ser<BridgePollResult>, StatusJson> {
    let mut connection = db_pool.inner().get()?;

    let notification = timeout.map(|millis| notifier.wait(Duration::from_millis(millis)));

    let mut query_transaction = move || -> QueryResult<IZettleTransactionPartial> {
        use crate::schema::tables::izettle_transaction::dsl::{amount, id, time};

        izettle_transaction
            .order_by(time.asc())
            .select((id, amount))
            .first(&mut connection)
    };

    let mut transaction_result = query_transaction();

    // if there was no pending transaction, query again if we were notified within the timeout
    if let Err(Error::NotFound) = &transaction_result
        && let Some(notification) = notification
        && notification.await
    {
        transaction_result = query_transaction();
    }

    if let Err(Error::NotFound) = transaction_result {
        Ok(accept.ser(BridgePollResult::NoPendingTransaction))
    } else {
        Ok(accept.ser(BridgePollResult::PendingPayment(transaction_result?)))
    }
}
