use crate::strings;
use gloo_net::http::Request;
use seed::app::cmds::timeout;
use seed::prelude::*;
use strecklistan_api::{
    izettle::IZettlePayment,
    transaction::{NewTransaction, TransactionId},
};

const POLL_TIMEOUT_MS: u32 = 1000;

/// Helper component for handling iZettle payments
#[derive(Clone)]
pub struct IZettlePay {
    pending: Option<i32>,
}

#[derive(Clone, Debug)]
pub enum IZettlePayMsg {
    /// Poll for payment completion
    PollPendingPayment(i32),

    /// There was an error processing the payment
    Error(IZettlePayErr),

    /// The payment was completed and the transaction committed
    PaymentCompleted { transaction_id: TransactionId },

    /// The payment was intentionally cancelled
    PaymentCancelled,
}

#[derive(Clone, Debug)]
pub enum IZettlePayErr {
    /// No transaction existed with the given ID
    NoTransaction { reference: i32 },

    /// The payment failed for some reason
    PaymentFailed { reference: i32, reason: String },

    /// A network request has failed
    NetworkError { reason: String },
}

impl IZettlePay {
    pub fn new() -> Self {
        IZettlePay { pending: None }
    }

    pub fn pay(&mut self, transaction: NewTransaction, mut orders: impl Orders<IZettlePayMsg>) {
        if self.pending.is_some() {
            return;
        }

        orders.perform_cmd(async move {
            let result = async {
                Request::post("/api/izettle/client/transaction")
                    .json(&transaction)?
                    .send()
                    .await?
                    .json()
                    .await
            }
            .await;
            match result {
                Ok(reference) => Some(IZettlePayMsg::PollPendingPayment(reference)),
                Err(e) => {
                    gloo_console::error!(format!("Failed to post transaction {e}"));
                    Some(IZettlePayMsg::Error(IZettlePayErr::NetworkError {
                        reason: strings::POSTING_TRANSACTION_FAILED.to_string(),
                    }))
                }
            }
        });
    }

    pub fn pending(&self) -> Option<i32> {
        self.pending
    }

    pub fn update(&mut self, msg: IZettlePayMsg, mut orders: impl Orders<IZettlePayMsg>) {
        match msg {
            IZettlePayMsg::PaymentCancelled | IZettlePayMsg::PaymentCompleted { .. } => {
                self.pending = None
            }
            IZettlePayMsg::Error(error) => {
                self.pending = None;
                match error {
                    IZettlePayErr::PaymentFailed { reference, reason } => {
                        gloo_console::error!("iZettle payment {} failed: {}", reference, reason);
                    }
                    IZettlePayErr::NoTransaction { reference } => {
                        gloo_console::error!("iZettle payment {} does not exist", reference);
                    }
                    IZettlePayErr::NetworkError { .. } => {}
                }
            }
            IZettlePayMsg::PollPendingPayment(reference) => {
                self.pending = Some(reference);

                orders.perform_cmd(async move {
                    let result = async {
                        Request::get(&format!("/api/izettle/client/poll/{}", reference))
                            .send()
                            .await?
                            .json()
                            .await
                    }
                    .await;
                    match result {
                        Ok(IZettlePayment::Pending) => {
                            timeout(POLL_TIMEOUT_MS, || ()).await;
                            Some(IZettlePayMsg::PollPendingPayment(reference))
                        }
                        Ok(IZettlePayment::Paid { transaction_id }) => {
                            Some(IZettlePayMsg::PaymentCompleted { transaction_id })
                        }
                        Ok(IZettlePayment::Cancelled) => Some(IZettlePayMsg::PaymentCancelled),
                        Ok(IZettlePayment::NoTransaction) => {
                            Some(IZettlePayMsg::Error(IZettlePayErr::NoTransaction {
                                reference,
                            }))
                        }
                        Ok(IZettlePayment::Failed { reason }) => {
                            Some(IZettlePayMsg::Error(IZettlePayErr::PaymentFailed {
                                reference,
                                reason,
                            }))
                        }
                        Err(e) => {
                            gloo_console::error!(format!("Failed to poll for payment: {e}"));
                            Some(IZettlePayMsg::Error(IZettlePayErr::NetworkError {
                                reason: strings::POLLING_TRANSACTION_FAILED.to_string(),
                            }))
                        }
                    }
                });
            }
        }
    }
}
