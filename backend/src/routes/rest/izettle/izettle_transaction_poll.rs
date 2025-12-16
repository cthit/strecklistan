use crate::database::DatabasePool;
use crate::diesel::RunQueryDsl;
use crate::models::izettle_transaction::{
    IZettlePostTransaction, TRANSACTION_CANCELLED, TRANSACTION_FAILED, TRANSACTION_IN_PROGRESS,
    TRANSACTION_PAID,
};
use crate::util::StatusJson;
use crate::util::ser::{Ser, SerAccept};
use diesel::{ExpressionMethods, QueryDsl};
use log::error;
use rocket::http::Status;
use rocket::{State, get};
use serde::Serialize;
use strecklistan_api::izettle::IZettlePayment;

#[derive(Clone, Serialize)]
pub struct IZettleResult {
    pub transaction_accepted: bool,
}

#[get("/izettle/client/poll/<izettle_transaction_id>")]
pub async fn poll_for_izettle(
    db_pool: &State<DatabasePool>,
    accept: SerAccept,
    izettle_transaction_id: i32,
) -> Result<Ser<IZettlePayment>, StatusJson> {
    let mut connection = db_pool.inner().get()?;

    let post_izettle_transaction: Result<IZettlePostTransaction, diesel::result::Error> = {
        use crate::schema::tables::izettle_post_transaction::dsl::{
            izettle_post_transaction, izettle_transaction_id as iz_id,
        };

        izettle_post_transaction
            .filter(iz_id.eq(izettle_transaction_id))
            .first(&mut connection)
    };

    match post_izettle_transaction {
        Err(diesel::result::Error::NotFound) => Ok(accept.ser(IZettlePayment::NoTransaction)),
        Ok(IZettlePostTransaction { status, .. }) if status == TRANSACTION_IN_PROGRESS => {
            Ok(accept.ser(IZettlePayment::Pending))
        }
        Ok(IZettlePostTransaction {
            status,
            transaction_id,
            ..
        }) if status == TRANSACTION_PAID => {
            let transaction_id = transaction_id.ok_or_else(|| {
                error!(
                    "izettle_post_transaction {} marked as paid, not but transaction_id was None",
                    izettle_transaction_id
                );
                StatusJson::new(Status::InternalServerError, "Internal Server Error")
            })?;
            Ok(accept.ser(IZettlePayment::Paid { transaction_id }))
        }
        Ok(IZettlePostTransaction { status, .. }) if status == TRANSACTION_CANCELLED => {
            Ok(accept.ser(IZettlePayment::Cancelled))
        }
        Ok(IZettlePostTransaction { status, error, .. }) if status == TRANSACTION_FAILED => {
            Ok(accept.ser(IZettlePayment::Failed {
                reason: error.unwrap_or_else(|| "Unknown error".to_string()),
            }))
        }
        Err(err) => Err(err.into()),
        Ok(transaction) => Err(StatusJson {
            status: Status::new(500),
            description: format!(
                "Invalid status {}, perhaps add it to the match.",
                transaction.status
            ),
        }),
    }
}
