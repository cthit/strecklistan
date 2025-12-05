use crate::components::parsed_input::{ParsedInput, ParsedInputMsg};
use crate::generated::css_classes::C;
use crate::strings;
use crate::util::simple_ev;
use gloo_net::http::Request;
use seed::prelude::*;
use seed::*;
use seed_fetcher::ResourceStore;
use seed_fetcher::Resources;
use std::collections::HashMap;
use std::convert::TryInto;
use strecklistan_api::book_account::{BookAccount, BookAccountType};
use strecklistan_api::{
    book_account::{BookAccountId, MasterAccounts},
    currency::{AbsCurrency, Currency},
    inventory::{
        InventoryBundle, InventoryBundleId, InventoryItemId, InventoryItemStock as InventoryItem,
    },
    transaction::{NewTransaction, TransactionBundle, TransactionId},
};

#[derive(Clone, Debug)]
pub enum CheckoutMsg {
    ConfirmPurchase,
    OverpayConfirmPurchase,
    PurchaseSent {
        transaction_id: TransactionId,
    },

    TotalInputMsg(ParsedInputMsg),
    AddItem {
        item_id: InventoryItemId,
        amount: i32,
    },
    AddBundle {
        bundle_id: InventoryBundleId,
        amount: i32,
    },
    SetBundleChange {
        bundle_index: usize,
        change: i32,
    },
    ClearCart,
}

#[derive(Clone)]
pub struct Checkout {
    transaction_total_input: ParsedInput<AbsCurrency>,
    transaction_bundles: Vec<TransactionBundle>,
    override_transaction_total: bool,
    pub debited_account: Option<BookAccountId>,
    pub confirm_button_message: Option<&'static str>,
    pub waiting_for_izettle: bool,
    pub overpay_confirm_enabled: bool,
}

#[derive(Resources)]
struct Res<'a> {
    #[url = "/api/book_accounts/masters"]
    master_accounts: &'a MasterAccounts,

    #[policy = "SilentRefetch"]
    #[url = "/api/inventory/items"]
    inventory: &'a HashMap<InventoryItemId, InventoryItem>,

    #[url = "/api/inventory/bundles"]
    bundles: &'a HashMap<InventoryBundleId, InventoryBundle>,

    #[url = "/api/book_accounts"]
    #[policy = "SilentRefetch"]
    book_accounts: &'a HashMap<BookAccountId, BookAccount>,
}

impl Checkout {
    pub fn new(rs: &ResourceStore, orders: &mut impl Orders<CheckoutMsg>) -> Self {
        Res::acquire(rs, orders).ok();
        Checkout {
            transaction_bundles: vec![],
            debited_account: None,
            transaction_total_input: ParsedInput::new_with_text("0")
                .with_error_message(strings::INVALID_MONEY_MESSAGE_SHORT)
                .with_input_kind("text"),
            override_transaction_total: false,
            waiting_for_izettle: false,
            confirm_button_message: None,
            overpay_confirm_enabled: false,
        }
    }

    pub fn update(
        &mut self,
        msg: CheckoutMsg,
        rs: &ResourceStore,
        orders: &mut impl Orders<CheckoutMsg>,
    ) {
        let res = match Res::acquire(rs, orders) {
            Ok(res) => res,
            Err(_) => return,
        };

        match msg {
            CheckoutMsg::OverpayConfirmPurchase => self.overpay_confirm_enabled = true,
            CheckoutMsg::ConfirmPurchase => {
                self.remove_cleared_items();
                if let Some(transaction) = self.build_transaction(rs) {
                    self.waiting_for_izettle = true;

                    orders.perform_cmd(async move {
                        let result = async {
                            Request::post("/api/transaction")
                                .json(&transaction)?
                                .send()
                                .await?
                                .json()
                                .await
                        }
                        .await;
                        match result {
                            Ok(transaction_id) => {
                                Some(CheckoutMsg::PurchaseSent { transaction_id })
                            }
                            Err(e) => {
                                // TODO: show notification
                                gloo_console::error!(format!("Failed to post transaction {e}"));
                                None
                            }
                        }
                    });
                }
                self.overpay_confirm_enabled = false;
            }
            CheckoutMsg::PurchaseSent { transaction_id } => {
                self.waiting_for_izettle = false;
                gloo_console::log!("Posted transaction ID: ", transaction_id);
                self.transaction_total_input.set_value(Default::default());
                self.transaction_bundles = vec![];
                self.debited_account = None;
                self.override_transaction_total = false;
            }
            CheckoutMsg::TotalInputMsg(msg) => {
                match &msg {
                    ParsedInputMsg::FocusOut => {
                        if self.transaction_total_input.parsed().is_none() {
                            self.override_transaction_total = false;
                            self.overpay_confirm_enabled = false
                        }
                    }
                    ParsedInputMsg::Input(_) => {
                        self.override_transaction_total = true;
                        self.overpay_confirm_enabled = false
                    }
                    _ => {}
                }
                self.transaction_total_input.update(msg);
            }
            CheckoutMsg::AddItem { item_id, amount } => {
                if !self.waiting_for_izettle {
                    let item = res
                        .inventory
                        .get(&item_id)
                        .unwrap_or_else(|| panic!("No inventory item with that id exists"))
                        .clone();

                    let mut item_ids = HashMap::new();
                    item_ids.insert(item.id, 1);

                    let bundle = TransactionBundle {
                        description: None,
                        price: Some(item.price.unwrap_or(0).into()),
                        change: -amount,
                        item_ids,
                    };

                    if let Some(b) = self.transaction_bundles.iter_mut().find(|b| {
                        b.item_ids == bundle.item_ids && b.description == bundle.description
                    }) {
                        b.change -= amount;
                    } else {
                        self.transaction_bundles.push(bundle);
                    }
                }
            }
            CheckoutMsg::AddBundle { bundle_id, amount } => {
                let bundle = res
                    .bundles
                    .get(&bundle_id)
                    .unwrap_or_else(|| panic!("No inventory bundle with that id exists"))
                    .clone();

                let mut item_ids = HashMap::new();
                for &id in bundle.item_ids.iter() {
                    *item_ids.entry(id).or_default() += 1;
                }

                let bundle = TransactionBundle {
                    description: Some(bundle.name.clone()),
                    price: Some(bundle.price),
                    change: -amount,
                    item_ids,
                };

                if let Some(b) = self
                    .transaction_bundles
                    .iter_mut()
                    .find(|b| b.item_ids == bundle.item_ids && b.description == bundle.description)
                {
                    b.change -= amount;
                } else {
                    gloo_console::log!(format!("Pushing bundle: {bundle:?}"));
                    self.transaction_bundles.push(bundle);
                }
            }
            CheckoutMsg::SetBundleChange {
                bundle_index,
                change,
            } => {
                self.transaction_bundles[bundle_index].change = change;
            }
            CheckoutMsg::ClearCart => {
                self.transaction_bundles.clear();
            }
        }

        self.recompute_new_transaction_total();
    }

    fn recompute_new_transaction_total(&mut self) {
        if !self.override_transaction_total {
            let amount: Currency = self
                .transaction_bundles
                .iter()
                .map(|bundle| -bundle.change * bundle.price.map(|p| p.into()).unwrap_or(0i32))
                .sum::<i32>()
                .into();
            self.transaction_total_input
                .set_value(amount.try_into().unwrap_or_default());
        }
    }

    pub fn build_transaction(&self, rs: &ResourceStore) -> Option<NewTransaction> {
        Res::acquire_now(rs)
            .ok()
            .zip(self.transaction_total_input.parsed().copied())
            .and_then(|(res, amount)| {
                Some(NewTransaction {
                    bundles: self.transaction_bundles.clone(),
                    amount: amount.into(),
                    description: Some(strings::TRANSACTION_SALE.into()),
                    credited_account: res.master_accounts.sales_account_id,
                    debited_account: self.debited_account?,
                })
            })
    }

    pub fn transaction_amount(&self) -> Currency {
        self.transaction_total_input
            .parsed()
            .copied()
            .unwrap_or_default()
            .into()
    }

    pub fn set_debited(&mut self, account_id: BookAccountId) {
        self.overpay_confirm_enabled = false;
        self.debited_account = Some(account_id);
    }

    pub fn remove_cleared_items(&mut self) {
        self.transaction_bundles.retain(|bundle| bundle.change != 0);
    }

    fn too_expensive(&self, res: &Res, book_account_id: &BookAccountId) -> bool {
        let transaction_amount: Currency = self.transaction_amount();

        let book_account = &res.book_accounts[book_account_id];

        match book_account.account_type {
            BookAccountType::Liabilities => book_account.balance < transaction_amount,
            _ => false,
        }
    }

    fn create_submit_button<M: 'static + Clone>(
        show_penguin: bool,
        style: Attrs,
        on_click: Option<M>,
    ) -> Node<M> {
        button![
            C![C.wide_button, C.border_on_focus],
            IF![
                show_penguin =>
                div![
                    C![C.penguin, C.penguin_small],
                    style! {
                        St::Position => "absolute",
                        St::MarginTop => "-0.25em",
                        St::Filter => "invert(100%)",
                    },
                ]
            ],
            style,
            IF![on_click.is_none() => attrs! { At::Disabled => true }],
            ev(Ev::Click, move |_| on_click),
            "Slutför Köp",
        ]
    }

    pub fn view(&self, rs: &ResourceStore) -> Node<CheckoutMsg> {
        let res = match Res::acquire_now(rs) {
            Ok(res) => res,
            // TODO: proper loading component?
            Err(_) => return div!["loading"],
        };

        div![
            C![C.new_transaction_view],
            self.transaction_bundles
                .iter()
                .enumerate()
                .rev() // display newest bundle first
                .map(|(bundle_index, bundle)| {
                    let mut items = bundle.item_ids.keys().map(|id| &res.inventory[id]);

                    // TODO: Properly display more complicated bundles

                    let (item_name, item_price) = match items.next() {
                        None => (None, 0),
                        Some(InventoryItem { name, price, .. }) => {
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
                        if bundle.change == 0 {
                            C![C.line_through, C.transaction_entry]
                        } else {
                            C![C.transaction_entry]
                        },
                        input![
                            C![C.new_transaction_bundle_amount_field, C.border_on_focus],
                            attrs! { At::Value => -bundle.change },
                            attrs! { At::Type => "number" },
                            input_ev(Ev::Input, move |input| {
                                CheckoutMsg::SetBundleChange {
                                    bundle_index,
                                    change: -input.parse().unwrap_or(0),
                                }
                            }),
                        ],
                        span![C![C.transaction_entry_item_name], format!("x {}", name),],
                        span![C![C.transaction_entry_item_price], format!("{}:-", price),],
                    ]
                })
                .collect::<Vec<_>>(),
            div![
                C![C.new_transaction_total_row],
                span![C![C.new_transaction_total_text], strings::TRANSACTION_TOTAL],
                {
                    // input field
                    let color = if self.override_transaction_total {
                        "color: #762;"
                    } else {
                        "color: black;"
                    };
                    let attrs = attrs! {
                        At::Style => color,
                        At::Class => [
                                C.new_transaction_total_field,
                                C.border_on_focus,
                            ].join(" "),
                    };
                    self.transaction_total_input
                        .view(attrs)
                        .map_msg(CheckoutMsg::TotalInputMsg)
                },
                button![
                    C![C.new_transaction_clear_button, C.border_on_focus],
                    simple_ev(Ev::Click, CheckoutMsg::ClearCart),
                ],
            ],
            if self.waiting_for_izettle {
                Self::create_submit_button(true, C![], None)
            } else {
                match &self.debited_account {
                    Some(account) if !self.transaction_bundles.is_empty() => {
                        if self.too_expensive(&res, account) {
                            if self.overpay_confirm_enabled {
                                // Show foldout button to confirm overpay purchase.
                                div![
                                    C![C.wide_button_foldout_container],
                                    Self::create_submit_button(false, C![C.button_danger], None),
                                    button![
                                        C![
                                            C.wide_button,
                                            C.border_on_focus,
                                            C.button_danger,
                                            C.wide_button_foldout
                                        ],
                                        simple_ev(Ev::Click, CheckoutMsg::ConfirmPurchase),
                                        "Godkänn överbetalning"
                                    ],
                                ]
                            } else {
                                // Overpay purchase first confirmation.
                                Self::create_submit_button(
                                    false,
                                    C![C.button_danger],
                                    Some(CheckoutMsg::OverpayConfirmPurchase),
                                )
                            }
                        } else {
                            // Standard tillgodo purchase.
                            Self::create_submit_button(
                                false,
                                C![],
                                Some(CheckoutMsg::ConfirmPurchase),
                            )
                        }
                    }
                    // We don't have anything to purchase or not selected an account.
                    _ => Self::create_submit_button(false, C![C.greyed_out], None),
                }
            },
            if let Some(message) = &self.confirm_button_message {
                div![C![C.wide_button_message], message]
            } else {
                empty![]
            },
        ]
    }
}
