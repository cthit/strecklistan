use chrono::{NaiveDate, NaiveTime};
use core::fmt::Debug;
use js_sys::encode_uri_component;
use mime::Mime;
use serde::Serialize;
use std::io::Write;
use strecklistan_api::{
    book_account::BookAccountId,
    currency::Currency,
    inventory::InventoryItemId,
    transaction::{Transaction, TransactionId},
};
use wasm_bindgen::JsCast;
use web_sys::{window, Element, HtmlElement};

pub fn csv_writer<W: Write>(writer: W) -> csv::Writer<W> {
    csv::WriterBuilder::new()
        .double_quote(true)
        .quote(b'"')
        .quote_style(csv::QuoteStyle::NonNumeric)
        .flexible(false)
        .has_headers(true)
        .delimiter(b',')
        .from_writer(writer)
}

#[derive(Copy, Clone, Debug)]
pub enum CSVStyleTransaction {
    PerItem,
    //PerTransaction,
}

pub fn make_csv_transaction_list(
    transactions: &[Transaction],
    style: CSVStyleTransaction,
) -> String {
    let mut data: Vec<u8> = vec![];
    let mut writer = csv_writer(&mut data);

    match style {
        CSVStyleTransaction::PerItem => {
            #[derive(Serialize)]
            struct Record<'a> {
                transaction_id: TransactionId,
                description: Option<&'a String>,
                date: NaiveDate,
                time: NaiveTime,
                debited_account: BookAccountId,
                credited_account: BookAccountId,
                amount: Currency,

                bundle_index: Option<usize>,
                bundle_description: Option<&'a String>,
                bundle_price: Option<Currency>,
                bundle_change: Option<i32>,

                item_id: Option<InventoryItemId>,
                item_amount: Option<u32>,
            }

            for transaction in transactions {
                let tr_record = Record {
                    transaction_id: transaction.id,
                    description: transaction.description.as_ref(),
                    date: transaction.time.naive_utc().date(),
                    time: transaction.time.time(),
                    debited_account: transaction.debited_account,
                    credited_account: transaction.credited_account,
                    amount: transaction.amount,

                    bundle_index: None,
                    bundle_description: None,
                    bundle_price: None,
                    bundle_change: None,

                    item_id: None,
                    item_amount: None,
                };

                if transaction.bundles.is_empty() {
                    writer.serialize(tr_record).unwrap();
                } else {
                    for (bundle_index, bundle) in transaction.bundles.iter().enumerate() {
                        let bundle_record = Record {
                            bundle_index: Some(bundle_index),
                            bundle_description: bundle.description.as_ref(),
                            bundle_price: bundle.price,
                            bundle_change: Some(bundle.change),

                            ..tr_record
                        };

                        if bundle.item_ids.is_empty() {
                            writer.serialize(bundle_record).unwrap();
                        } else {
                            for (item_id, item_amount) in bundle.item_ids.iter() {
                                let item_record = Record {
                                    item_id: Some(*item_id),
                                    item_amount: Some(*item_amount),

                                    ..bundle_record
                                };

                                writer.serialize(item_record).unwrap();
                            }
                        }
                    }
                }
            }
        }
    }

    drop(writer);
    String::from_utf8(data).unwrap()
}

/// Make the browser download the provided non-binary file
pub fn download_file(filename: &str, mime_type: Mime, text: &str) -> Result<(), ()> {
    fn log_error<T: Debug>(err: T) {
        gloo_console::error!(format!("{err:?}"))
    }

    let window = window().ok_or(())?;
    let document = window.document().ok_or(())?;
    let body = document.body().ok_or(())?;
    let element: Element = document.create_element("a").map_err(log_error)?;
    let element: HtmlElement = element.dyn_into().map_err(log_error)?;

    let text: String = encode_uri_component(text).into();
    element
        .set_attribute(
            "href",
            &format!("data:{};charset=utf-8,{}", mime_type, text),
        )
        .map_err(log_error)?;
    element
        .set_attribute("download", filename)
        .map_err(log_error)?;
    element.set_hidden(true);

    body.append_child(&element).map_err(log_error)?;

    element.click();

    body.remove_child(&element).map_err(log_error)?;

    Ok(())
}
