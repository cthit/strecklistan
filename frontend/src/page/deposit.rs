use crate::app::Msg;
use crate::components::izettle_pay::{IZettlePay, IZettlePayErr, IZettlePayMsg};
use crate::components::parsed_input::{ParsedInput, ParsedInputMsg};
use crate::fuzzy_search::{FuzzyScore, FuzzySearch};
use crate::generated::css_classes::C;
use crate::notification_manager::{Notification, NotificationMessage};
use crate::page::loading::Loading;
use crate::strings;
use crate::util::simple_ev;
use crate::views::view_tillgodo;
use gloo_net::http::Request;
use seed::prelude::*;
use seed::*;
use seed_fetcher::Resources;
use seed_fetcher::{event, NotAvailable, ResourceStore};
use std::collections::HashMap;
use strecklistan_api::{
    book_account::{BookAccount, BookAccountId, MasterAccounts},
    currency::AbsCurrency,
    member::{Member, MemberId, NewMember},
    transaction::{NewTransaction, TransactionId},
};

#[derive(Clone)]
pub struct DepositionPage {
    accs_search: Vec<(FuzzyScore, BookAccountId)>,
    search_string: String,

    debit: Option<DebitOption>,
    credit_account: Option<BookAccountId>,
    amount_input: ParsedInput<AbsCurrency>,
    izettle_pay: IZettlePay,

    new_member: Option<(String, String, String, Option<String>)>,

    request_in_progress: bool,
}

#[derive(Clone, Debug)]
pub enum DepositionMsg {
    SearchDebit(String),

    CreditKeyDown(web_sys::KeyboardEvent),
    CreditSelect(BookAccountId),
    SelectDebit(DebitOption),

    AmountInputMsg(ParsedInputMsg),

    Deposit,
    DepositSent {
        transaction_id: TransactionId,
    },
    DepositFailed {
        message_title: String,
        message_body: Option<String>,
    },

    IZettlePay(IZettlePayMsg),

    ShowNewMemberMenu,
    NewMember(NewMemberMsg),
    NewMemberCreated((MemberId, BookAccountId)),

    // -- Resource Messages -- //
    ResFetched(event::Fetched),
    ResMarkDirty(event::MarkDirty),
}

#[derive(Clone, Debug)]
pub enum NewMemberMsg {
    FirstNameInput(String),
    LastNameInput(String),
    NicknameInput(String),
    AccNameInput(String),
    Create,
    HideMenu,
}

#[derive(Clone, Copy, Debug)]
pub enum DebitOption {
    IZettleEPay,
    OtherEPay,
    #[allow(dead_code)]
    Cash,
}

#[derive(Resources)]
struct Res<'a> {
    #[url = "/api/book_accounts"]
    book_accounts: &'a HashMap<BookAccountId, BookAccount>,

    #[url = "/api/book_accounts/masters"]
    master_accounts: &'a MasterAccounts,

    #[url = "/api/members"]
    members: &'a HashMap<MemberId, Member>,
}

impl DepositionPage {
    pub fn new(rs: &ResourceStore, orders: &mut impl Orders<DepositionMsg>) -> Self {
        orders.subscribe(DepositionMsg::ResFetched);
        orders.subscribe(DepositionMsg::ResMarkDirty);

        let mut page = DepositionPage {
            debit: None,
            izettle_pay: IZettlePay::new(),
            credit_account: None,
            search_string: String::new(),
            accs_search: vec![],
            amount_input: ParsedInput::new_with_text("0")
                .with_error_message(strings::INVALID_MONEY_MESSAGE_LONG),
            new_member: None,
            request_in_progress: false,
        };

        if let Ok(res) = Res::acquire(rs, orders) {
            page.rebuild_data(&res);
        }

        page
    }

    pub fn update(
        &mut self,
        msg: DepositionMsg,
        rs: &ResourceStore,
        orders: &mut impl Orders<Msg>,
    ) -> Result<(), NotAvailable> {
        let res = Res::acquire(rs, orders)?;

        let mut orders_local = orders.proxy(Msg::Deposition);

        match msg {
            DepositionMsg::SearchDebit(input) => {
                self.search_string = input;
                for (score, acc_id) in self.accs_search.iter_mut() {
                    let acc = &res.book_accounts[acc_id];
                    *score = res.members[&acc.creditor.unwrap()].compare_fuzzy(&self.search_string);
                }

                self.accs_search
                    .sort_by(|(scr_a, acc_a_id), (scr_b, acc_b_id)| {
                        scr_b.cmp(scr_a).then(acc_a_id.cmp(acc_b_id))
                    });
            }
            DepositionMsg::CreditKeyDown(ev) => match ev.key().as_str() {
                "Enter" => {
                    if let Some((_, acc_id)) = self.accs_search.first() {
                        orders_local.send_msg(DepositionMsg::CreditSelect(*acc_id));
                    }
                }
                _ => {}
            },
            DepositionMsg::CreditSelect(acc_id) => {
                self.search_string = String::new();
                self.credit_account = Some(acc_id);
            }
            DepositionMsg::SelectDebit(debit) => {
                self.debit = Some(debit);
            }
            DepositionMsg::AmountInputMsg(msg) => {
                self.amount_input.update(msg);
            }
            DepositionMsg::Deposit => {
                if let Some(((credit_acc, &amount), debit)) = self
                    .credit_account
                    .zip(self.amount_input.parsed())
                    .zip(self.debit)
                {
                    let transaction = NewTransaction {
                        description: Some(strings::TRANSACTION_DEPOSIT.to_string()),
                        amount: amount.into(),
                        credited_account: credit_acc,
                        debited_account: match debit {
                            DebitOption::Cash => res.master_accounts.cash_account_id,
                            DebitOption::IZettleEPay | DebitOption::OtherEPay => {
                                res.master_accounts.bank_account_id
                            }
                        },
                        bundles: vec![],
                    };

                    self.request_in_progress = true;

                    if let DebitOption::IZettleEPay = debit {
                        self.izettle_pay
                            .pay(transaction, orders_local.proxy(DepositionMsg::IZettlePay));
                    } else {
                        orders_local.perform_cmd(async move {
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
                                    Some(DepositionMsg::DepositSent { transaction_id })
                                }
                                Err(e) => {
                                    gloo_console::error!(format!(
                                        "Failed to post transaction: {e}"
                                    ));
                                    Some(DepositionMsg::DepositFailed {
                                        message_title: strings::SERVER_ERROR.to_string(),
                                        message_body: Some(
                                            strings::POSTING_TRANSACTION_FAILED.to_string(),
                                        ),
                                    })
                                }
                            }
                        });
                    }
                }
            }

            DepositionMsg::DepositSent { .. } => {
                orders.send_msg(Msg::Notification(NotificationMessage::ShowNotification {
                    duration_ms: 5000,
                    notification: Notification {
                        title: strings::DEPOSIT_COMPLETE.to_string(),
                        body: self
                            .amount_input
                            .parsed()
                            .map(|value| format!("{}:-", value)),
                    },
                }));

                self.request_in_progress = false;
                self.amount_input.set_value(Default::default());
                self.credit_account = None;
                rs.mark_as_dirty(Res::book_accounts_url(), orders);
                rs.mark_as_dirty(Res::members_url(), orders);
            }

            DepositionMsg::DepositFailed {
                message_title,
                message_body,
            } => {
                self.request_in_progress = false;
                orders.send_msg(Msg::Notification(NotificationMessage::ShowNotification {
                    duration_ms: 10000,
                    notification: Notification {
                        title: message_title,
                        body: message_body,
                    },
                }));
            }

            DepositionMsg::IZettlePay(msg) => {
                let reaction = match &msg {
                    &IZettlePayMsg::PaymentCompleted { transaction_id } => {
                        Some(DepositionMsg::DepositSent { transaction_id })
                    }
                    IZettlePayMsg::PaymentCancelled => Some(DepositionMsg::DepositFailed {
                        message_title: strings::PAYMENT_CANCELLED.to_string(),
                        message_body: None,
                    }),
                    IZettlePayMsg::Error(IZettlePayErr::PaymentFailed { reason, .. }) => {
                        Some(DepositionMsg::DepositFailed {
                            message_title: strings::PAYMENT_FAILED.to_string(),
                            message_body: Some(reason.clone()),
                        })
                    }
                    IZettlePayMsg::Error(IZettlePayErr::NoTransaction { .. }) => {
                        Some(DepositionMsg::DepositFailed {
                            message_title: strings::SERVER_ERROR.to_string(),
                            message_body: Some(strings::NO_PENDING_TRANSACTION.to_string()),
                        })
                    }
                    IZettlePayMsg::Error(IZettlePayErr::NetworkError { reason }) => {
                        Some(DepositionMsg::DepositFailed {
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
                    .update(msg, orders_local.proxy(DepositionMsg::IZettlePay));
            }

            DepositionMsg::ShowNewMemberMenu => {
                self.new_member = Some((String::new(), String::new(), String::new(), None));
            }

            DepositionMsg::NewMember(msg) => {
                if let Some((first_name, last_name, nickname, acc_name)) = &mut self.new_member {
                    match msg {
                        NewMemberMsg::FirstNameInput(input) => *first_name = input,
                        NewMemberMsg::LastNameInput(input) => *last_name = input,
                        NewMemberMsg::NicknameInput(input) => *nickname = input,
                        NewMemberMsg::AccNameInput(input) => {
                            *acc_name = if input.is_empty() { None } else { Some(input) }
                        }
                        NewMemberMsg::HideMenu => {
                            self.new_member = None;
                        }
                        NewMemberMsg::Create => {
                            if first_name.is_empty() || last_name.is_empty() {
                                gloo_console::log!(
                                    "Missing fields: `first_name` and `last_name` required"
                                );
                            } else {
                                let msg = (
                                    NewMember {
                                        first_name: first_name.clone(),
                                        last_name: last_name.clone(),
                                        nickname: match nickname.as_str() {
                                            "" => None,
                                            nickname => Some(nickname.to_string()),
                                        },
                                    },
                                    acc_name.clone().unwrap_or_else(|| {
                                        generate_tillgodo_acc_name(first_name, nickname)
                                    }),
                                );
                                orders_local.perform_cmd(async move {
                                    let response = async {
                                        Request::post("/api/add_member_with_book_account")
                                            .json(&msg)?
                                            .send()
                                            .await?
                                            .json()
                                            .await
                                    }
                                    .await;

                                    match response {
                                        Ok(data) => Some(DepositionMsg::NewMemberCreated(data)),
                                        Err(e) => {
                                            gloo_console::error!(format!(
                                                "Failed to create new member: {e}"
                                            ));
                                            None
                                        }
                                    }
                                });
                            }
                        }
                    }
                } else {
                    gloo_console::error!(
                        "Tried to edit new member fields while in incorrect state."
                    );
                }
            }
            DepositionMsg::NewMemberCreated((member_id, book_account_id)) => {
                gloo_console::log!("New member ID: ", member_id);
                gloo_console::log!("New book account ID: ", book_account_id);
                self.new_member = None;
                rs.mark_as_dirty(Res::book_accounts_url(), orders);
                rs.mark_as_dirty(Res::members_url(), orders);
            }

            DepositionMsg::ResFetched(event::Fetched(resource)) => {
                if Res::has_resource(resource) {
                    self.rebuild_data(&res);
                }
            }
            DepositionMsg::ResMarkDirty(_) => {}
        }

        Ok(())
    }

    pub fn view(&self, rs: &ResourceStore) -> Node<Msg> {
        let res = match Res::acquire_now(rs) {
            Ok(res) => res,
            Err(_) => return Loading::view(),
        };

        if let Some((first_name, last_name, nickname, acc_name)) = &self.new_member {
            div![
                C![C.new_member_view],
                button![
                    C![C.border_on_focus, C.wide_button, C.new_member_view_item],
                    simple_ev(Ev::Click, NewMemberMsg::HideMenu),
                    strings::ABORT,
                ],
                input![
                    C![C.border_on_focus, C.new_member_view_item],
                    attrs! {At::Placeholder => strings::FIRST_NAME},
                    attrs! {At::Value => first_name},
                    input_ev(Ev::Input, NewMemberMsg::FirstNameInput),
                ],
                input![
                    C![C.border_on_focus, C.new_member_view_item],
                    attrs! {At::Placeholder => strings::LAST_NAME},
                    attrs! {At::Value => last_name},
                    input_ev(Ev::Input, NewMemberMsg::LastNameInput),
                ],
                input![
                    C![C.border_on_focus, C.new_member_view_item],
                    attrs! {At::Placeholder => strings::NICKNAME},
                    attrs! {At::Value => nickname},
                    input_ev(Ev::Input, NewMemberMsg::NicknameInput),
                ],
                input![
                    C![C.border_on_focus, C.new_member_view_item],
                    attrs! {At::Placeholder => strings::ACCOUNT_NAME},
                    attrs! {At::Value => match acc_name {
                        Some(name) => name.to_string(),
                        None => generate_tillgodo_acc_name(first_name, nickname),
                    }},
                    input_ev(Ev::Input, NewMemberMsg::AccNameInput),
                ],
                button![
                    C![C.border_on_focus, C.wide_button, C.new_member_view_item],
                    if first_name.is_empty() || last_name.is_empty() {
                        attrs! {At::Disabled => true}
                    } else {
                        attrs! {}
                    },
                    simple_ev(Ev::Click, NewMemberMsg::Create),
                    strings::CONFIRM,
                ],
            ]
            .map_msg(DepositionMsg::NewMember)
        } else {
            div![
                C![C.deposit_page],
                div![
                    C![C.tillgodo_list],
                    input![
                        C![C.tillgodolista_search_field, C.rounded, C.border_on_focus],
                        if self.credit_account.is_some() {
                            C![C.debit_selected]
                        } else {
                            C![]
                        },
                        attrs! {At::Value => self.search_string},
                        {
                            let s = if let Some(acc_id) = self.credit_account {
                                res.book_accounts
                                    .get(&acc_id)
                                    .map(|acc| acc.name.as_str())
                                    .unwrap_or("[MISSING]")
                            } else {
                                strings::CHOOSE_TILLGODO_ACC
                            };
                            attrs! {At::Placeholder => s}
                        },
                        input_ev(Ev::Input, DepositionMsg::SearchDebit),
                        keyboard_ev(Ev::KeyDown, DepositionMsg::CreditKeyDown),
                    ],
                    button![
                        C![C.new_member_button, C.wide_button, C.border_on_focus],
                        simple_ev(Ev::Click, DepositionMsg::ShowNewMemberMenu),
                        "+",
                    ],
                    self.accs_search
                        .iter()
                        .filter_map(|(_, acc_id)| res.book_accounts.get(acc_id))
                        .filter_map(|acc| acc.creditor.map(|creditor| (acc, creditor)))
                        .filter_map(|(acc, creditor)| res
                            .members
                            .get(&creditor)
                            .map(|member| (acc, member)))
                        .map(|(acc, member)| div![
                            if self.credit_account == Some(acc.id) {
                                C![C.border_highlight]
                            } else {
                                C![]
                            },
                            view_tillgodo(acc, member, DepositionMsg::CreditSelect(acc.id)),
                        ])
                        .collect::<Vec<_>>(),
                ],
                div![
                    C![C.pay_method_select_box],
                    div![
                        C![C.select_debit_container],
                        button![
                            if let Some(DebitOption::IZettleEPay) = self.debit {
                                C![C.debit_selected]
                            } else {
                                C![]
                            },
                            C![C.select_debit_button, C.border_on_focus, C.rounded_l],
                            simple_ev(
                                Ev::Click,
                                DepositionMsg::SelectDebit(DebitOption::IZettleEPay)
                            ),
                            strings::IZETTLE,
                        ],
                        button![
                            if let Some(DebitOption::OtherEPay) = self.debit {
                                C![C.debit_selected]
                            } else {
                                C![]
                            },
                            C![C.select_debit_button, C.border_on_focus, C.rounded_r],
                            simple_ev(
                                Ev::Click,
                                DepositionMsg::SelectDebit(DebitOption::OtherEPay),
                            ),
                            strings::OTHER_EPAY,
                        ],
                    ],
                    self.amount_input
                        .view(C![C.deposit_amount_input, C.rounded, C.border_on_focus])
                        .map_msg(DepositionMsg::AmountInputMsg),
                    if self.request_in_progress {
                        button![
                            C![C.wide_button, C.border_on_focus],
                            attrs! {At::Disabled => true},
                            div![
                                C![C.penguin, C.penguin_small],
                                style! {
                                    St::Position => "absolute",
                                    St::MarginTop => "-0.25em",
                                    St::Filter => "invert(100%)",
                                },
                            ],
                            strings::DEPOSIT,
                        ]
                    } else {
                        button![
                            C![C.wide_button, C.border_on_focus],
                            {
                                let disabled = match self.amount_input.parsed().copied() {
                                    None => true,
                                    Some(x) if x == Default::default() => true,
                                    Some(_) if self.credit_account.is_none() => true,
                                    Some(_) => self.debit.is_none(),
                                };

                                if disabled {
                                    attrs! { At::Disabled => true }
                                } else {
                                    attrs! {}
                                }
                            },
                            simple_ev(Ev::Click, DepositionMsg::Deposit),
                            strings::DEPOSIT,
                        ]
                    },
                    if self.izettle_pay.pending().is_some() {
                        div![C![C.wide_button_message], strings::WAITING_FOR_PAYMENT]
                    } else {
                        empty![]
                    },
                ],
            ]
        }
        .map_msg(Msg::Deposition)
    }

    fn rebuild_data(&mut self, res: &Res) {
        self.search_string = String::new();
        self.accs_search = res
            .book_accounts
            .values()
            .filter(|acc| acc.creditor.is_some())
            .map(|acc| (Default::default(), acc.id))
            .collect();
    }
}

fn generate_tillgodo_acc_name(first_name: &str, nickname: &str) -> String {
    format!(
        "{}/{}",
        strings::TRANSACTION_TILLGODO,
        match nickname {
            "" => first_name,
            nn => nn,
        }
    )
}
