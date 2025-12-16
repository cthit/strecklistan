use crate::app::Msg;
use crate::components::filter_menu::{FilterMenu, FilterMenuMsg};
use crate::generated::css_classes::C;
use crate::page::loading::Loading;
use crate::util::export::{CSVStyleTransaction, download_file, make_csv_transaction_list};
use crate::util::simple_ev;
use chrono::{FixedOffset, Local};
use gloo_net::http::Request;
use seed::prelude::*;
use seed::*;
use seed_fetcher::Resources;
use seed_fetcher::{NotAvailable, ResourceStore, event};
use std::collections::HashMap;
use strecklistan_api::{
    book_account::{BookAccount, BookAccountId, MasterAccounts},
    currency::Currency,
    inventory::{InventoryItemId, InventoryItemStock},
    transaction::{Transaction, TransactionId},
};

const VIEW_COUNT_CHUNK: usize = 50;

#[derive(Copy, Clone, Debug)]
pub enum ExportFormat {
    Json,
    Csv(CSVStyleTransaction),
}

#[derive(Clone, Debug)]
pub enum TransactionsMsg {
    DeleteTransaction(TransactionId),
    TransactionDeleted(TransactionId),
    SetShowDelete(bool),
    SetShowLeftPanel(bool),
    FilterMenuMsg(FilterMenuMsg),
    IncreaseViewLimit,
    ExportData(ExportFormat),
    ShowReceipt(TransactionId),

    ResFetched(event::Fetched),
    #[allow(dead_code)] // not currently used
    ResMarkDirty(event::MarkDirty),
}

#[derive(Clone)]
pub struct TransactionsPage {
    show_delete: bool,
    show_left_panel: bool,
    view_limit: usize,
    filter_menu: FilterMenu,
    timezone: FixedOffset,

    /// Only show transactions in this list
    filtered_transactions: Vec<usize>,

    /// The balance of all accounts based on the filtered transactions
    accounts_balance: HashMap<BookAccountId, Currency>,
}

#[derive(Resources)]
struct Res<'a> {
    #[url = "/api/transactions"]
    transactions: &'a Vec<Transaction>,

    #[url = "/api/inventory/items"]
    #[policy = "SilentRefetch"]
    inventory: &'a HashMap<InventoryItemId, InventoryItemStock>,

    #[url = "/api/book_accounts"]
    book_accounts: &'a HashMap<BookAccountId, BookAccount>,

    #[url = "/api/book_accounts/masters"]
    master_accounts: &'a MasterAccounts,
}

impl TransactionsPage {
    pub fn new(rs: &ResourceStore, orders: &mut impl Orders<TransactionsMsg>) -> Self {
        let mut page = TransactionsPage {
            show_delete: false,
            show_left_panel: false,
            timezone: *Local::now().offset(),
            view_limit: VIEW_COUNT_CHUNK,
            filter_menu: FilterMenu::new(vec!["datum", "klockslag", "summa", "debet", "kredit"]),
            filtered_transactions: vec![],
            accounts_balance: HashMap::new(),
        };

        orders.subscribe(TransactionsMsg::ResFetched);
        orders.subscribe(TransactionsMsg::ResMarkDirty);

        Res::acquire(rs, orders)
            .map(|res| page.filter_transactions(&res))
            .ok();
        page
    }

    /// Rebuild self.filtered_transactions
    fn filter_transactions(&mut self, res: &Res) {
        self.filtered_transactions = res
            .transactions
            .iter()
            .enumerate()
            .filter(|(_, tr)| {
                self.filter_menu.filter(&[
                    &tr.time.with_timezone(&self.timezone).format("%Y-%m-%d"), // datum
                    &tr.time.with_timezone(&self.timezone).format("%H:%M:%S"), // klockslag
                    &tr.amount,                                                // summa
                    &res.book_accounts.get(&tr.debited_account).unwrap().name, // debet
                    &res.book_accounts.get(&tr.credited_account).unwrap().name, // kredit
                ])
            })
            .map(|(i, _)| i)
            .collect();

        self.accounts_balance.clear();
        for tr in self
            .filtered_transactions
            .iter()
            .map(|&i| &res.transactions[i])
        {
            if let Some(acc) = res.book_accounts.get(&tr.debited_account) {
                *self.accounts_balance.entry(tr.debited_account).or_default() +=
                    acc.debit_diff(tr.amount);
            }
            if let Some(acc) = res.book_accounts.get(&tr.credited_account) {
                *self
                    .accounts_balance
                    .entry(tr.credited_account)
                    .or_default() += acc.credit_diff(tr.amount);
            }
        }
    }

    pub fn update(
        &mut self,
        msg: TransactionsMsg,
        rs: &ResourceStore,
        orders: &mut impl Orders<Msg>,
    ) -> Result<(), NotAvailable> {
        let res = Res::acquire(rs, orders)?;

        let mut orders_local = orders.proxy(Msg::Transactions);
        match msg {
            TransactionsMsg::ResFetched(event::Fetched(resource)) => {
                if Res::has_resource(resource) {
                    self.filter_transactions(&res);
                }
            }
            TransactionsMsg::ResMarkDirty(_) => {}
            TransactionsMsg::DeleteTransaction(id) => {
                self.show_delete = false;
                orders_local.perform_cmd(async move {
                    let result = async {
                        Request::delete(&format!("/api/transaction/{}", id))
                            .send()
                            .await?
                            .json()
                            .await
                    }
                    .await;
                    result
                        .map_err(|e| {
                            gloo_console::error!(format!("Failed to delete transaction {e:?}"));
                        })
                        .map(TransactionsMsg::TransactionDeleted)
                        .ok()
                });
            }

            TransactionsMsg::TransactionDeleted(id) => {
                gloo_console::log!(format!("Transaction {} deleted", id));
                rs.mark_as_dirty(Res::transactions_url(), orders);
                rs.mark_as_dirty(Res::book_accounts_url(), orders);
                rs.mark_as_dirty(Res::inventory_url(), orders);
            }

            TransactionsMsg::SetShowDelete(show_delete) => {
                self.show_delete = show_delete;
            }
            TransactionsMsg::SetShowLeftPanel(show_left_panel) => {
                self.show_left_panel = show_left_panel;
            }
            TransactionsMsg::FilterMenuMsg(msg) => {
                self.filter_menu
                    .update(msg, &mut orders_local.proxy(TransactionsMsg::FilterMenuMsg));
                self.view_limit = VIEW_COUNT_CHUNK; // reset view limit
                self.filter_transactions(&res);
            }
            TransactionsMsg::IncreaseViewLimit => {
                self.view_limit += VIEW_COUNT_CHUNK;
                self.filter_transactions(&res);
            }
            TransactionsMsg::ExportData(format) => {
                let transactions: Vec<_> = self
                    .filtered_transactions
                    .iter()
                    .map(|&index| res.transactions[index].clone())
                    .collect();
                match format {
                    ExportFormat::Json => {
                        let serialized = serde_json::to_string(&transactions).unwrap();
                        download_file("transactions.json", mime::APPLICATION_JSON, &serialized)
                            .ok();
                    }
                    ExportFormat::Csv(style) => {
                        let serialized = make_csv_transaction_list(&transactions, style);
                        download_file("transactions.csv", mime::TEXT_CSV, &serialized).ok();
                    }
                }
            }
            TransactionsMsg::ShowReceipt(id) => {
                window().open_with_url(&format!("/api/receipt/{id}")).ok();
            }
        }

        Ok(())
    }

    pub fn view(&self, rs: &ResourceStore) -> Node<Msg> {
        let res = match Res::acquire_now(rs) {
            Ok(res) => res,
            Err(_) => return Loading::view(),
        };

        let show_acc_entry = |name: &str, balance: Currency| {
            div![
                C![C.balance_entry],
                span![name],
                span![": "],
                span![C![C.flex_span_spacing]],
                span![format!("{}:-", balance)],
            ]
        };
        let show_acc = |id: &BookAccountId| {
            show_acc_entry(
                res.book_accounts
                    .get(id)
                    .map(|acc| acc.name.as_str())
                    .unwrap_or("[missing]"),
                self.accounts_balance
                    .get(id)
                    .copied()
                    .unwrap_or_else(|| 0.into()),
            )
        };

        let transaction_list: Vec<_> = self
            .filtered_transactions
            .iter()
            .take(self.view_limit)
            .map(|&i| &res.transactions[i])
            .map(|tr| view_transaction(self.timezone, &res, tr, self.show_delete))
            .collect();

        div![
            C![C.transactions_page],
            div![
                C![C.left_panel],
                if self.show_left_panel {
                    C![C.left_panel_showing]
                } else {
                    C![]
                },
                div![
                    C![C.left_panel_entry],
                    h2![C![C.left_panel_entry_header], "Balansräkning"],
                ],
                div![
                    C![C.balance_sheet, C.margin_hcenter],
                    show_acc(&res.master_accounts.bank_account_id),
                    show_acc(&res.master_accounts.cash_account_id),
                    show_acc(&res.master_accounts.sales_account_id),
                    show_acc(&res.master_accounts.purchases_account_id),
                    show_acc_entry(
                        "Tillgodo Totalt",
                        self.accounts_balance
                            .iter()
                            .filter_map(|(id, balance)| res
                                .book_accounts
                                .get(id)
                                .map(|acc| (acc, balance)))
                            .filter(|(acc, _)| acc.creditor.is_some())
                            .map(|(_, balance)| *balance)
                            .fold(0.into(), |a: Currency, b| a + b)
                    ),
                ],
                hr![C![C.left_panel_entry]],
                div![
                    C![C.left_panel_entry],
                    h2![C![C.left_panel_entry_header], "Filtrera (WIP)"],
                ],
                self.filter_menu
                    .view()
                    .map_msg(TransactionsMsg::FilterMenuMsg),
                div![
                    C![C.left_panel_entry],
                    h2![C![C.left_panel_entry_header], "Exportera Data"],
                    button![
                        C![C.wide_button],
                        "JSON",
                        simple_ev(Ev::Click, TransactionsMsg::ExportData(ExportFormat::Json)),
                    ],
                    button![
                        C![C.wide_button],
                        C![C.space_above],
                        "CSV (En rad per vara)",
                        simple_ev(
                            Ev::Click,
                            TransactionsMsg::ExportData(ExportFormat::Csv(
                                CSVStyleTransaction::PerItem
                            ))
                        ),
                    ],
                ],
                // TODO: implement this
                /*
                button![
                    C![C.wide_button, C.mt_2],
                    "CSV (En rad per transaktion)",
                    simple_ev(Ev::Click, TransactionsMsg::ExportData(
                            ExportFormat::CSV(CSVStyleTransaction::PerTransaction))),
                ],
                */
            ],
            button![
                C![C.left_panel_button],
                simple_ev(
                    Ev::Click,
                    TransactionsMsg::SetShowLeftPanel(!self.show_left_panel),
                ),
                "⚙"
            ],
            div![if self.show_left_panel {
                C![C.left_panel_sub_spacer]
            } else {
                C![C.left_panel_sub_spacer, C.left_panel_sub_spacer_hidden]
            },],
            div![
                C![C.transactions_list],
                div![
                    C![C.transactions_page_button_box],
                    button![
                        C![C.transactions_page_show_delete],
                        C![C.space_above],
                        "Radera transaktioner?",
                        simple_ev(Ev::Click, TransactionsMsg::SetShowDelete(!self.show_delete)),
                    ],
                ],
                transaction_list,
                if self.view_limit < self.filtered_transactions.len() {
                    button![
                        C![C.wide_button],
                        "Visa fler",
                        simple_ev(Ev::Click, TransactionsMsg::IncreaseViewLimit),
                    ]
                } else {
                    empty![]
                },
            ],
        ]
        .map_msg(Msg::Transactions)
    }
}

fn view_transaction(
    timezone: FixedOffset,
    res: &Res,
    transaction: &Transaction,
    show_delete: bool,
) -> Node<TransactionsMsg> {
    div![
        C![C.transaction_view],
        p![
            C![C.transaction_line],
            span![format!("#{} ", transaction.id)],
            span![transaction.description.as_deref().unwrap_or("Transaktion")],
            button![
                C![C.transaction_view_receipt_button],
                simple_ev(Ev::Click, TransactionsMsg::ShowReceipt(transaction.id)),
            ],
            IF![show_delete => {
                button![
                    C![C.transaction_view_delete_button],
                    simple_ev(
                        Ev::Click,
                        TransactionsMsg::DeleteTransaction(transaction.id)
                    ),
                    "✖",
                ]
            }],
        ],
        p![
            C![C.transaction_line],
            format!(
                "{}",
                transaction
                    .time
                    .with_timezone(&timezone)
                    .format("%Y-%m-%d %H:%M:%S %Z"),
            )
        ],
        p![
            C![C.transaction_line],
            span!["Debet: "],
            span![
                C![C.font_bold],
                res.book_accounts
                    .get(&transaction.debited_account)
                    .map(|acc| acc.name.as_str())
                    .unwrap_or("[MISSING]")
            ],
        ],
        p![
            C![C.transaction_line],
            span!["Kredit: "],
            span![
                C![C.font_bold],
                res.book_accounts
                    .get(&transaction.credited_account)
                    .map(|acc| acc.name.as_str())
                    .unwrap_or("[MISSING]")
            ],
        ],
        transaction
            .bundles
            .iter()
            .map(|bundle| {
                let mut items = bundle.item_ids.keys().map(|id| &res.inventory[id]);

                // TODO: Properly display more complicated bundles

                let (item_name, item_price) = match items.next() {
                    None => (None, 0),
                    Some(InventoryItemStock { name, price, .. }) => {
                        (Some(name.as_str()), price.unwrap_or(0))
                    }
                };

                let name = bundle
                    .description
                    .as_deref()
                    .or(item_name)
                    .unwrap_or("[NAMN SAKNAS]");
                let price = bundle.price.unwrap_or_else(|| item_price.into());
                p![
                    C![C.transaction_entry],
                    span![
                        C![C.transaction_entry_item_name],
                        format!("{}x {}", -bundle.change, name),
                    ],
                    span![C![C.transaction_entry_item_price], format!("{}:-", price),],
                ]
            })
            .collect::<Vec<_>>(),
        p![
            span!["Totalt: "],
            span![
                C![C.transaction_entry_item_price],
                format!("{}:-", transaction.amount),
            ],
        ],
    ]
}
