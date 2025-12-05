use crate::generated::css_classes::C;
use crate::notification_manager::{NotificationManager, NotificationMessage};
use crate::page::{
    analytics::{AnalyticsMsg, AnalyticsPage},
    deposit::{DepositionMsg, DepositionPage},
    inventory::{InventoryMsg, InventoryPage},
    store::{StoreMsg, StorePage},
    transactions::{TransactionsMsg, TransactionsPage},
    Page,
};
use crate::util::compare_semver;
use gloo_net::http::Request;
use seed::prelude::*;
use seed::*;
use seed_fetcher::{ResourceMsg, ResourceStore};
use semver::Version;
use std::fmt::Debug;

const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct Model {
    pub page: Page,

    pub error: Option<(String, String)>,

    pub store_page: Option<StorePage>,
    pub transactions_page: Option<TransactionsPage>,
    pub analytics_page: Option<AnalyticsPage>,
    pub deposition_page: Option<DepositionPage>,
    pub inventory_page: Option<InventoryPage>,

    pub rs: ResourceStore,
    pub notifications: NotificationManager,
}

#[derive(Clone, Debug)]
pub enum Msg {
    ChangePage(Page),

    Resource(ResourceMsg),

    FetchedApiVersion(String),

    ShowError { header: String, dump: String },

    Analytics(AnalyticsMsg),
    Deposition(DepositionMsg),
    Transactions(TransactionsMsg),
    Store(StoreMsg),
    Inventory(InventoryMsg),

    Notification(NotificationMessage),
}

pub fn init(url: Url, orders: &mut impl Orders<Msg>) -> Model {
    orders
        .subscribe(|subs::UrlChanged(mut url)| {
            let page = match url.remaining_path_parts().as_slice() {
                [] | [""] | ["store"] => Page::Store,
                ["transactions"] => Page::TransactionHistory,
                ["analytics"] => Page::Analytics,
                ["deposit"] => Page::Deposit,
                ["inventory"] => Page::Inventory,
                _ => Page::NotFound,
            };

            Msg::ChangePage(page)
        })
        .notify(subs::UrlChanged(url));

    orders.perform_cmd(async move {
        let response: Result<String, gloo_net::Error> =
            async { Request::get("/api/version").send().await?.text().await }.await;
        match response {
            Ok(response) => Msg::FetchedApiVersion(response),
            Err(e) => Msg::ShowError {
                header: "Failed to contact server".to_owned(),
                dump: format!("{:#?}", e),
            },
        }
    });

    let rs = ResourceStore::new(&mut orders.proxy(Msg::Resource));
    Model {
        page: Page::Store,
        error: None,
        store_page: None,
        transactions_page: None,
        analytics_page: None,
        deposition_page: None,
        inventory_page: None,
        rs,
        notifications: Default::default(),
    }
}

pub fn update(msg: Msg, model: &mut Model, orders: &mut impl Orders<Msg>) {
    #[cfg(debug_assertions)]
    gloo_console::log!(format!("message {:?}", msg));

    let rs = &model.rs;
    match msg {
        Msg::ChangePage(page) => {
            model.page = page;

            model.transactions_page = None;

            match page {
                Page::Store => {
                    model
                        .store_page
                        .get_or_insert_with(|| StorePage::new(rs, &mut orders.proxy(Msg::Store)));
                }
                Page::TransactionHistory => {
                    model.transactions_page = Some(TransactionsPage::new(
                        &model.rs,
                        &mut orders.proxy(Msg::Transactions),
                    ))
                }
                Page::Analytics => {
                    model.analytics_page.get_or_insert_with(|| {
                        AnalyticsPage::new(rs, &mut orders.proxy(Msg::Analytics))
                    });
                }
                Page::Deposit => {
                    model.deposition_page.get_or_insert_with(|| {
                        DepositionPage::new(rs, &mut orders.proxy(Msg::Deposition))
                    });
                }
                Page::Inventory => {
                    model.inventory_page.get_or_insert_with(|| {
                        InventoryPage::new(rs, &mut orders.proxy(Msg::Inventory))
                    });
                }
                Page::NotFound => {}
            }
        }

        Msg::Resource(msg) => {
            model.rs.update(msg, &mut orders.proxy(Msg::Resource));
        }

        Msg::ShowError { header, dump } => {
            model.error = Some((header, dump));
        }

        Msg::FetchedApiVersion(response) => {
            if let Ok(api_version) = Version::parse(&response) {
                let frontend_version = Version::parse(PKG_VERSION).unwrap();

                gloo_console::log!("API version:", response.to_string());
                gloo_console::log!("Application version:", PKG_VERSION);

                if !compare_semver(frontend_version, api_version) {
                    model.error = Some((
                        "Mismatching api version.".to_string(),
                        format!(
                            "API version: {}\nApplication version: {}",
                            response, PKG_VERSION
                        ),
                    ));
                }
            } else {
                model.error = Some(("Failed to parse server api version.".to_string(), response));
            }
        }

        Msg::Deposition(msg) => {
            model
                .deposition_page
                .as_mut()
                .and_then(|p| p.update(msg, rs, orders).ok());
        }
        Msg::Analytics(msg) => {
            model
                .analytics_page
                .as_mut()
                .and_then(|p| p.update(msg, rs, orders).ok());
        }
        Msg::Transactions(msg) => {
            model
                .transactions_page
                .as_mut()
                .and_then(|p| p.update(msg, rs, orders).ok());
        }
        Msg::Store(msg) => {
            model
                .store_page
                .as_mut()
                .and_then(|p| p.update(msg, rs, orders).ok());
        }
        Msg::Inventory(msg) => {
            model
                .inventory_page
                .as_mut()
                .and_then(|p| p.update(msg, rs, orders).ok());
        }

        Msg::Notification(msg) => model.notifications.update(msg, orders),
    }
}

pub fn view(model: &Model) -> Vec<Node<Msg>> {
    vec![
        model.notifications.view(),
        div![
            div![
                C![C.header],
                if cfg!(debug_assertions) {
                    div![C![C.debug_banner], "DEBUG"]
                } else {
                    empty![]
                },
                div![
                    // links
                    //a!["hem", C![C.header_link], attrs! {At::Href => "/"}],
                    C![C.header_link_box],
                    a![
                        "försäljning",
                        C![C.header_link],
                        attrs! {At::Href => "/store"}
                    ],
                    a![
                        "tillgodo",
                        C![C.header_link],
                        attrs! {At::Href => "/deposit"}
                    ],
                    a![
                        "transaktioner",
                        C![C.header_link],
                        attrs! {At::Href => "/transactions"}
                    ],
                    a![
                        "utbud",
                        C![C.header_link],
                        attrs! {At::Href => "/inventory"}
                    ],
                    a![
                        "analys",
                        C![C.header_link],
                        attrs! {At::Href => "/analytics"}
                    ],
                ],
            ],
            match &model.error {
                None => match model.page {
                    Page::Analytics => model.analytics_page.as_ref().unwrap().view(&model.rs),
                    Page::Store => model.store_page.as_ref().unwrap().view(&model.rs),
                    Page::Deposit => model.deposition_page.as_ref().unwrap().view(&model.rs),
                    Page::TransactionHistory =>
                        model.transactions_page.as_ref().unwrap().view(&model.rs),
                    Page::Inventory => model.inventory_page.as_ref().unwrap().view(&model.rs),
                    Page::NotFound => {
                        div![C![C.not_found_message, C.unselectable], "404"]
                    }
                },

                Some((header, message)) => div![
                    C![C.error_page],
                    p!["An has error occured."],
                    p![header],
                    textarea![
                        C![C.code_box],
                        attrs! { At::ReadOnly => true, },
                        attrs! { At::Rows => message.lines().count(), },
                        message,
                    ],
                ],
            },
        ],
    ]
}
