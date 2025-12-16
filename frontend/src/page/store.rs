use crate::app::Msg;
use crate::components::checkout::{Checkout, CheckoutMsg};
use crate::components::izettle_pay::{IZettlePay, IZettlePayErr, IZettlePayMsg};
use crate::fuzzy_search::{FuzzyScore, FuzzySearch};
use crate::generated::css_classes::C;
use crate::notification_manager::{Notification, NotificationMessage};
use crate::page::loading::Loading;
use crate::strings;
use crate::util::{compare_fuzzy, simple_ev};
use crate::views::{view_inventory_bundle, view_inventory_item, view_tillgodo};
use seed::prelude::*;
use seed::*;
use seed_fetcher::{DontFetch, NotAvailable, ResourceStore, Resources, event};
use std::collections::HashMap;
use strecklistan_api::{
    book_account::{BookAccount, BookAccountId, MasterAccounts},
    inventory::{
        InventoryBundle, InventoryBundleId, InventoryItemId, InventoryItemStock as InventoryItem,
    },
    member::{Member, MemberId},
};

#[derive(Clone, Debug)]
enum StoreItemId {
    Item(InventoryItemId),
    Bundle(InventoryBundleId),
}

enum StoreItem<'a> {
    Item(&'a InventoryItem),
    Bundle(&'a InventoryBundle),
}

impl StoreItemId {
    fn acquire<'a>(&self, state: &'a Res) -> StoreItem<'a> {
        match self {
            StoreItemId::Item(id) => StoreItem::Item(&state.inventory[id]),
            StoreItemId::Bundle(id) => StoreItem::Bundle(&state.bundles[id]),
        }
    }
}

impl StoreItem<'_> {
    pub fn get_name(&self) -> &str {
        match self {
            StoreItem::Item(item) => &item.name,
            StoreItem::Bundle(bundle) => &bundle.name,
        }
    }

    pub fn in_stock(&self) -> bool {
        match self {
            StoreItem::Item(item) => item.stock > 0,
            StoreItem::Bundle(_) => true,
        }
    }
}

impl FuzzySearch for StoreItem<'_> {
    fn compare_fuzzy(&self, search: &str) -> FuzzyScore {
        compare_fuzzy(self.get_name().chars(), search.chars())
    }
}

impl FuzzySearch for Member {
    fn compare_fuzzy(&self, search: &str) -> FuzzyScore {
        match &self.nickname {
            Some(nick) => compare_fuzzy(nick.chars(), search.chars()),
            None => compare_fuzzy(
                self.first_name
                    .chars()
                    .chain(std::iter::once(' '))
                    .chain(self.last_name.chars()),
                search.chars(),
            ),
        }
    }
}

#[derive(Clone, Debug)]
pub enum StoreMsg {
    ResFetched(event::Fetched),
    #[allow(dead_code)] // not currently used
    ResMarkDirty(event::MarkDirty),

    SearchDebit(String),
    DebitKeyDown(web_sys::KeyboardEvent),
    DebitSelect(SelectedDebit),

    IZettleMsg(IZettlePayMsg),
    CancelIZettle {
        message_title: String,
        message_body: Option<String>,
    },

    SearchInput(String),
    SearchKeyDown(web_sys::KeyboardEvent),

    CheckoutMsg(CheckoutMsg),
}

pub struct StorePage {
    checkout: Checkout,

    inventory_search_string: String,
    inventory_search: Vec<(FuzzyScore, StoreItemId)>,

    tillgodolista_search_string: String,
    tillgodolista_search: Vec<(FuzzyScore, BookAccountId, MemberId)>,

    selected_debit: Option<SelectedDebit>,

    izettle_pay: IZettlePay,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SelectedDebit {
    IZettleEPay,
    OtherEPay,
    Tillgodo(BookAccountId),

    #[allow(dead_code)]
    Cash,
}

#[derive(Resources)]
struct Res<'a> {
    #[url = "/api/inventory/bundles"]
    bundles: &'a HashMap<InventoryBundleId, InventoryBundle>,

    #[url = "/api/inventory/items"]
    #[policy = "SilentRefetch"]
    inventory: &'a HashMap<InventoryItemId, InventoryItem>,

    #[url = "/api/book_accounts"]
    #[policy = "SilentRefetch"]
    book_accounts: &'a HashMap<BookAccountId, BookAccount>,

    #[url = "/api/book_accounts/masters"]
    master_accounts: &'a MasterAccounts,

    #[url = "/api/members"]
    members: &'a HashMap<MemberId, Member>,

    #[url = "/api/transactions"]
    #[allow(dead_code)]
    transactions: DontFetch,
}

impl StorePage {
    pub fn new(rs: &ResourceStore, orders: &mut impl Orders<StoreMsg>) -> Self {
        orders.subscribe(StoreMsg::ResFetched);
        orders.subscribe(StoreMsg::ResMarkDirty);
        let mut p = StorePage {
            checkout: Checkout::new(rs, &mut orders.proxy(StoreMsg::CheckoutMsg)),

            inventory_search_string: String::new(),
            inventory_search: vec![],

            tillgodolista_search_string: String::new(),
            tillgodolista_search: vec![],

            selected_debit: None,

            izettle_pay: IZettlePay::new(),
        };
        if let Ok(state) = Res::acquire(rs, orders) {
            p.rebuild_data(&state);
        }
        p
    }

    pub fn update(
        &mut self,
        msg: StoreMsg,
        rs: &ResourceStore,
        orders: &mut impl Orders<Msg>,
    ) -> Result<(), NotAvailable> {
        let res = Res::acquire(rs, orders)?;

        let mut orders_local = orders.proxy(Msg::Store);

        match msg {
            StoreMsg::ResFetched(event::Fetched(resource)) => {
                if Res::has_resource(resource) {
                    self.rebuild_data(&res);
                }
            }
            StoreMsg::ResMarkDirty(_) => {}
            StoreMsg::SearchDebit(input) => {
                self.tillgodolista_search_string = input;
                self.sort_tillgodolista_search(&res);
            }
            StoreMsg::DebitKeyDown(ev) => match ev.key().as_str() {
                "Enter" => {
                    if let Some((_, acc_id, _)) = self.tillgodolista_search.first() {
                        let msg = StoreMsg::DebitSelect(SelectedDebit::Tillgodo(*acc_id));
                        self.update(msg, rs, orders)?;
                    }
                }
                _ => {}
            },
            StoreMsg::DebitSelect(selected) => {
                self.selected_debit = Some(selected);
                self.tillgodolista_search_string = String::new();
                self.checkout.set_debited(selected.acc_id(&res));
            }

            StoreMsg::SearchInput(input) => {
                self.inventory_search_string = input;
                self.sort_store_list(&res);
            }
            StoreMsg::SearchKeyDown(ev) => match ev.key().as_str() {
                "Enter" => match self.inventory_search.first() {
                    Some((_, StoreItemId::Item(item_id))) => {
                        let msg = StoreMsg::CheckoutMsg(CheckoutMsg::AddItem {
                            item_id: *item_id,
                            amount: 1,
                        });
                        self.update(msg, rs, orders)?;
                    }
                    Some((_, StoreItemId::Bundle(bundle_id))) => {
                        let msg = StoreMsg::CheckoutMsg(CheckoutMsg::AddBundle {
                            bundle_id: *bundle_id,
                            amount: 1,
                        });
                        self.update(msg, rs, orders)?;
                    }
                    None => {}
                },
                _ => {}
            },
            StoreMsg::IZettleMsg(msg) => {
                let reaction = match &msg {
                    &IZettlePayMsg::PaymentCompleted { transaction_id } => {
                        Some(StoreMsg::CheckoutMsg(CheckoutMsg::PurchaseSent {
                            transaction_id,
                        }))
                    }
                    IZettlePayMsg::PaymentCancelled => Some(StoreMsg::CancelIZettle {
                        message_title: strings::PAYMENT_CANCELLED.to_string(),
                        message_body: None,
                    }),
                    IZettlePayMsg::Error(IZettlePayErr::PaymentFailed { reason, .. }) => {
                        Some(StoreMsg::CancelIZettle {
                            message_title: strings::PAYMENT_FAILED.to_string(),
                            message_body: Some(reason.clone()),
                        })
                    }
                    IZettlePayMsg::Error(IZettlePayErr::NoTransaction { .. }) => {
                        Some(StoreMsg::CancelIZettle {
                            message_title: strings::SERVER_ERROR.to_string(),
                            message_body: Some(strings::NO_PENDING_TRANSACTION.to_string()),
                        })
                    }
                    IZettlePayMsg::Error(IZettlePayErr::NetworkError { reason }) => {
                        Some(StoreMsg::CancelIZettle {
                            message_title: strings::SERVER_ERROR.to_string(),
                            message_body: Some(reason.clone()),
                        })
                    }
                    IZettlePayMsg::PollPendingPayment(_) => None,
                };

                if let Some(msg) = reaction {
                    orders_local.send_msg(msg);
                }

                self.izettle_pay
                    .update(msg, orders_local.proxy(StoreMsg::IZettleMsg));
            }

            StoreMsg::CancelIZettle {
                message_title,
                message_body,
            } => {
                self.checkout.waiting_for_izettle = false;
                self.checkout.confirm_button_message = None;
                orders.send_msg(Msg::Notification(NotificationMessage::ShowNotification {
                    duration_ms: 10000,
                    notification: Notification {
                        title: message_title,
                        body: message_body,
                    },
                }));
            }

            StoreMsg::CheckoutMsg(msg) => {
                let forward_msg = match msg {
                    // if iZettle integration is enabled we intercept and handle the purchase here
                    CheckoutMsg::ConfirmPurchase
                        if self.selected_debit == Some(SelectedDebit::IZettleEPay) =>
                    {
                        if let Some(transaction) = self.checkout.build_transaction(rs) {
                            self.checkout.waiting_for_izettle = true;
                            self.checkout.remove_cleared_items();
                            self.checkout.confirm_button_message =
                                Some(strings::WAITING_FOR_PAYMENT);
                            self.izettle_pay
                                .pay(transaction, orders_local.proxy(StoreMsg::IZettleMsg));
                        }
                        None // don't forward the message
                    }
                    // show a notification & reload inventory when a purchase completes
                    CheckoutMsg::PurchaseSent { .. } => {
                        rs.mark_as_dirty(Res::inventory_url(), orders);
                        rs.mark_as_dirty(Res::book_accounts_url(), orders);
                        rs.mark_as_dirty(Res::transactions_url(), orders);
                        orders.send_msg(Msg::Notification(NotificationMessage::ShowNotification {
                            duration_ms: 5000,
                            notification: Notification {
                                title: strings::PURCHASE_COMPLETE.to_string(),
                                body: Some(format!(
                                    "Total: {}:-",
                                    self.checkout.transaction_amount(),
                                )),
                            },
                        }));
                        self.checkout = Checkout::new(
                            rs,
                            &mut orders.proxy(Msg::Store).proxy(StoreMsg::CheckoutMsg),
                        );
                        self.selected_debit = None;
                        None
                    }
                    msg => Some(msg),
                };

                if let Some(msg) = forward_msg {
                    self.checkout.update(
                        msg,
                        rs,
                        &mut orders.proxy(Msg::Store).proxy(StoreMsg::CheckoutMsg),
                    );
                }
            }
        }

        Ok(())
    }

    fn rebuild_data(&mut self, res: &Res) {
        let items = res
            .inventory
            .values()
            // Don't show deleted items
            .filter(|item| item.deleted_at.is_none())
            // Don't show items without a default price
            .filter(|item| item.price.is_some())
            .map(|item| (Default::default(), StoreItemId::Item(item.id)));

        let bundles = res
            .bundles
            .values()
            .map(|bundle| (Default::default(), StoreItemId::Bundle(bundle.id)));

        self.inventory_search = bundles.chain(items).collect();

        self.tillgodolista_search = res
            .book_accounts
            .values()
            .filter_map(|acc| {
                acc.creditor
                    .map(|member_id| (Default::default(), acc.id, member_id))
            })
            .collect();

        self.sort_tillgodolista_search(res);
        self.sort_store_list(res);
    }

    fn sort_tillgodolista_search(&mut self, res: &Res) {
        for (score, _acc, member_id) in self.tillgodolista_search.iter_mut() {
            *score = res.members[member_id].compare_fuzzy(&self.tillgodolista_search_string);
        }

        self.tillgodolista_search
            .sort_by(|(scr_a, acc_a_id, _), (scr_b, acc_b_id, _)| {
                scr_b.cmp(scr_a).then(acc_a_id.cmp(acc_b_id))
            });
    }

    fn sort_store_list(&mut self, state: &Res) {
        for (score, item) in self.inventory_search.iter_mut() {
            *score = item
                .acquire(state)
                .compare_fuzzy(&self.inventory_search_string);
        }
        self.inventory_search
            .sort_by(|(score_a, item_a), (score_b, item_b)| {
                // sort first by comparison score
                score_b
                    .cmp(score_a)
                    // then by if it is in stock
                    .then(
                        item_b
                            .acquire(state)
                            .in_stock()
                            .cmp(&item_a.acquire(state).in_stock()),
                    )
                    // then alphabetically on name
                    .then(
                        item_a
                            .acquire(state)
                            .get_name()
                            .cmp(item_b.acquire(state).get_name()),
                    )
            });
    }

    pub fn view(&self, rs: &ResourceStore) -> Node<Msg> {
        let res = match Res::acquire_now(rs) {
            Ok(res) => res,
            Err(_) => return Loading::view(),
        };

        let apply_selection_class_on = |f: &dyn Fn(SelectedDebit) -> bool| match self.selected_debit
        {
            Some(sd) if f(sd) => C![C.debit_selected],
            _ => C![],
        };

        div![
            C![C.store_page],
            div![
                C![C.store_top_box],
                div![
                    C![C.pay_method_select_box, C.margin_hcenter],
                    input![
                        C![C.tillgodolista_search_field, C.rounded_t, C.border_on_focus],
                        apply_selection_class_on(&|sd| matches!(sd, SelectedDebit::Tillgodo(_))),
                        attrs! {At::Value => self.tillgodolista_search_string},
                        {
                            attrs! {
                                At::Placeholder => match self.selected_debit {
                                    Some(SelectedDebit::Tillgodo(acc_id)) => res
                                        .book_accounts
                                        .get(&acc_id)
                                        .map(|acc| format!("{}: {}:-", acc.name, acc.balance))
                                        .unwrap_or_else(|| "[MISSING]".to_string()),
                                    _ => "Tillgodolista".into(),
                                },
                            }
                        },
                        input_ev(Ev::Input, |input| Msg::Store(StoreMsg::SearchDebit(input))),
                        keyboard_ev(Ev::KeyDown, |ev| Msg::Store(StoreMsg::DebitKeyDown(ev))),
                    ],
                    div![
                        C![C.select_debit_container],
                        if !self.tillgodolista_search_string.is_empty() {
                            div![
                                C![C.tillgodo_drop_down],
                                div![
                                    C![C.tillgodo_list],
                                    self.tillgodolista_search
                                        .iter()
                                        .flat_map(|(_, acc_id, member_id)| res
                                            .book_accounts
                                            .get(acc_id)
                                            .and_then(|acc| res
                                                .members
                                                .get(member_id)
                                                .map(|mem| (acc, mem))))
                                        .map(|(acc, member)| view_tillgodo(
                                            acc,
                                            member,
                                            Msg::Store(StoreMsg::DebitSelect(
                                                SelectedDebit::Tillgodo(acc.id)
                                            )),
                                        ))
                                        .collect::<Vec<_>>(),
                                ],
                            ]
                        } else {
                            empty![]
                        },
                        button![
                            apply_selection_class_on(&|sd| sd == SelectedDebit::IZettleEPay),
                            C![C.select_debit_button, C.border_on_focus, C.rounded_bl],
                            simple_ev(
                                Ev::Click,
                                Msg::Store(StoreMsg::DebitSelect(SelectedDebit::IZettleEPay))
                            ),
                            strings::IZETTLE,
                        ],
                        button![
                            apply_selection_class_on(&|sd| sd == SelectedDebit::OtherEPay),
                            C![C.select_debit_button, C.border_on_focus, C.rounded_br],
                            simple_ev(
                                Ev::Click,
                                Msg::Store(StoreMsg::DebitSelect(SelectedDebit::OtherEPay)),
                            ),
                            strings::OTHER_EPAY,
                        ],
                    ]
                ],
                input![
                    C![C.inventory_search_field, C.rounded, C.border_on_focus],
                    attrs! {At::Value => self.inventory_search_string},
                    attrs! {At::Placeholder => "sök varor"},
                    input_ev(Ev::Input, |input| Msg::Store(StoreMsg::SearchInput(input))),
                    keyboard_ev(Ev::KeyDown, |ev| Msg::Store(StoreMsg::SearchKeyDown(ev))),
                ],
            ],
            div![
                C![C.inventory_view],
                self.inventory_search
                    .iter()
                    .map(|(fuzzy, element)| match element {
                        StoreItemId::Item(item_id) => view_inventory_item(
                            &res.inventory[item_id],
                            fuzzy.matches.iter().map(|m| m.base_str_index),
                            |item_id, amount| Msg::Store(StoreMsg::CheckoutMsg(
                                CheckoutMsg::AddItem { item_id, amount }
                            ))
                        ),
                        StoreItemId::Bundle(bundle_id) => view_inventory_bundle(
                            &res.bundles[bundle_id],
                            fuzzy.matches.iter().map(|m| m.base_str_index),
                            |bundle_id, amount| Msg::Store(StoreMsg::CheckoutMsg(
                                CheckoutMsg::AddBundle { bundle_id, amount }
                            ))
                        ),
                    })
                    .collect::<Vec<_>>(),
            ],
            self.checkout
                .view(rs)
                .map_msg(StoreMsg::CheckoutMsg)
                .map_msg(Msg::Store),
        ]
    }
}

impl SelectedDebit {
    fn acc_id(&self, res: &Res) -> BookAccountId {
        match self {
            SelectedDebit::Cash => res.master_accounts.cash_account_id,
            SelectedDebit::IZettleEPay => res.master_accounts.bank_account_id,
            SelectedDebit::OtherEPay => res.master_accounts.bank_account_id,
            &SelectedDebit::Tillgodo(acc_id) => acc_id,
        }
    }
}
