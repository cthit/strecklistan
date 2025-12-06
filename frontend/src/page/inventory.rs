use crate::app::Msg;
use crate::components::parsed_input::{ParsedInput, ParsedInputMsg};
use crate::generated::css_classes::C;
use crate::notification_manager::{Notification, NotificationMessage};
use crate::page::loading::Loading;
use crate::strings;
use crate::util::simple_ev;
use gloo_net::http::Request;
use seed::prelude::*;
use seed::*;
use seed_fetcher::{NotAvailable, ResourceStore, Resources, event};
use std::collections::{BTreeMap, HashMap};
use strecklistan_api::{
    currency::Currency,
    inventory::{
        InventoryBundle, InventoryBundleId, InventoryItemId, InventoryItemStock as InventoryItem,
        NewInventoryBundle, NewInventoryItem,
    },
};

#[derive(Clone, Debug)]
pub enum InventoryMsg {
    #[allow(dead_code)] // not currently used
    ResFetched(event::Fetched),
    #[allow(dead_code)] // not currently used
    ResMarkDirty(event::MarkDirty),

    DeleteBundle(InventoryBundleId),
    SaveBundle(InventoryBundleId),
    NewBundle,

    DeleteItem(InventoryItemId),
    SaveItem(InventoryItemId),
    NewItem,

    ItemsChanged,
    BundlesChanged,
    ServerError(String),

    BundleInput(Field, InventoryBundleId, ParsedInputMsg),
    ItemInput(Field, InventoryItemId, ParsedInputMsg),
}

pub struct InventoryPage {
    bundle_rows: BTreeMap<InventoryBundleId, Row<InventoryBundle>>,
    item_rows: BTreeMap<InventoryItemId, Row<InventoryItem>>,
}

#[derive(Resources)]
struct Res<'a> {
    #[url = "/api/inventory/bundles"]
    #[policy = "SilentRefetch"]
    bundles: &'a HashMap<InventoryBundleId, InventoryBundle>,

    #[url = "/api/inventory/items"]
    #[policy = "SilentRefetch"]
    items: &'a HashMap<InventoryItemId, InventoryItem>,
}

#[derive(Clone)]
struct Row<T> {
    original: T,
    name: ParsedInput<String>,
    price: ParsedInput<Currency>,
    image: ParsedInput<String>,
}

#[derive(Clone, Debug)]
pub enum Field {
    Name,
    Price,
    Image,
}

impl InventoryPage {
    pub fn new(rs: &ResourceStore, orders: &mut impl Orders<InventoryMsg>) -> Self {
        orders.subscribe(InventoryMsg::ResFetched);
        orders.subscribe(InventoryMsg::ResMarkDirty);
        let mut p = InventoryPage {
            item_rows: Default::default(),
            bundle_rows: Default::default(),
        };
        if let Ok(state) = Res::acquire(rs, orders) {
            p.rebuild_data(&state);
        }
        p
    }

    pub fn update(
        &mut self,
        msg: InventoryMsg,
        rs: &ResourceStore,
        orders: &mut impl Orders<Msg>,
    ) -> Result<(), NotAvailable> {
        let res = Res::acquire(rs, orders)?;

        let mut orders_local = orders.proxy(Msg::Inventory);

        let delete_request = |uri: String, msg: InventoryMsg| async move {
            let result: Result<_, String> = async {
                let response = Request::delete(&uri)
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
                if response.ok() {
                    Ok(response)
                } else {
                    Err(format!("Bad status code: {}", response.status()))
                }
            }
            .await;

            match result {
                Ok(_) => msg,
                Err(e) => {
                    gloo_console::error!(format!("Failed to save inventory changes: {}", e));
                    InventoryMsg::ServerError(format!("{:?}", e))
                }
            }
        };

        match msg {
            InventoryMsg::ResFetched(_) => self.rebuild_data(&res),
            InventoryMsg::ResMarkDirty(_) => {}
            InventoryMsg::DeleteBundle(id) => {
                orders_local.perform_cmd(delete_request(
                    format!("/api/inventory/bundle/{}", id),
                    InventoryMsg::BundlesChanged,
                ));
            }
            InventoryMsg::SaveBundle(id) => {
                let row = &self.bundle_rows[&id];

                let not_empty = |s: &&String| !s.is_empty();

                // Send changes to server
                let new_bundle = NewInventoryBundle {
                    name: row.name.text().to_string(),
                    price: *row.price.parsed().unwrap_or(&row.original.price),
                    image_url: row.image.parsed().filter(not_empty).cloned(),
                    item_ids: row.original.item_ids.clone(), // TODO: allow editing bundle items
                };
                orders_local.perform_cmd(async move {
                    let result: Result<_, String> = async {
                        let response = Request::put(&format!("/api/inventory/bundle/{}", id))
                            .json(&new_bundle)
                            .map_err(|e| e.to_string())?
                            .send()
                            .await
                            .map_err(|e| e.to_string())?;
                        if response.ok() {
                            Ok(response)
                        } else {
                            Err(format!("Bad status code: {}", response.status()))
                        }
                    }
                    .await;

                    match result {
                        Ok(_) => InventoryMsg::BundlesChanged,
                        Err(e) => {
                            gloo_console::error!(format!("Failed to save inventory changes: {e}"));
                            InventoryMsg::ServerError(format!("{:?}", e))
                        }
                    }
                });
            }
            InventoryMsg::NewBundle => {
                orders_local.perform_cmd(async move {
                    let result: Result<_, String> = async {
                        let response = Request::post("/api/inventory/bundle")
                            .json(&default_bundle())
                            .map_err(|e| e.to_string())?
                            .send()
                            .await
                            .map_err(|e| e.to_string())?;
                        if response.ok() {
                            Ok(response)
                        } else {
                            Err(format!("Bad status code: {}", response.status()))
                        }
                    }
                    .await;

                    match result {
                        Ok(_) => InventoryMsg::BundlesChanged,
                        Err(e) => {
                            gloo_console::error!(format!("Failed to create new bundle: {e}"));
                            InventoryMsg::ServerError(format!("{:?}", e))
                        }
                    }
                });
            }
            InventoryMsg::DeleteItem(id) => {
                orders_local.perform_cmd(delete_request(
                    format!("/api/inventory/item/{}", id),
                    InventoryMsg::ItemsChanged,
                ));
            }
            InventoryMsg::SaveItem(id) => {
                let row = &self.item_rows[&id];

                let not_empty = |s: &&String| !s.is_empty();

                // Send changes to server
                let new_item = NewInventoryItem {
                    name: row.name.text().to_string(),
                    price: row.price.parsed().copied().map(i32::from),
                    image_url: row.image.parsed().filter(not_empty).cloned(),
                };
                orders_local.perform_cmd(async move {
                    let result: Result<_, String> = async {
                        let response = Request::put(&format!("/api/inventory/item/{}", id))
                            .json(&new_item)
                            .map_err(|e| e.to_string())?
                            .send()
                            .await
                            .map_err(|e| e.to_string())?;
                        if response.ok() {
                            Ok(response)
                        } else {
                            Err(format!("Bad status code: {}", response.status()))
                        }
                    }
                    .await;

                    match result {
                        Ok(_) => InventoryMsg::ItemsChanged,
                        Err(e) => {
                            gloo_console::error!(format!("Failed to save inventory changes: {e}"));
                            InventoryMsg::ServerError(format!("{:?}", e))
                        }
                    }
                });
            }
            InventoryMsg::NewItem => {
                orders_local.perform_cmd(async move {
                    let result: Result<_, String> = async {
                        let response = Request::post("/api/inventory/item")
                            .json(&default_item())
                            .map_err(|e| e.to_string())?
                            .send()
                            .await
                            .map_err(|e| e.to_string())?;
                        if response.ok() {
                            Ok(response)
                        } else {
                            Err(format!("Bad status code: {}", response.status()))
                        }
                    }
                    .await;

                    match result {
                        Ok(_) => InventoryMsg::ItemsChanged,
                        Err(e) => {
                            gloo_console::error!(format!("Failed to create new item: {e}"));
                            InventoryMsg::ServerError(format!("{:?}", e))
                        }
                    }
                });
            }
            InventoryMsg::ItemsChanged => {
                rs.mark_as_dirty(Res::items_url(), orders);
            }
            InventoryMsg::BundlesChanged => {
                rs.mark_as_dirty(Res::bundles_url(), orders);
            }
            InventoryMsg::ServerError(message) => {
                orders.send_msg(Msg::Notification(NotificationMessage::ShowNotification {
                    duration_ms: 10000,
                    notification: Notification {
                        title: strings::SERVER_ERROR.to_string(),
                        body: Some(message),
                    },
                }));
            }
            InventoryMsg::BundleInput(field, id, msg) => {
                let row = self.bundle_rows.get_mut(&id);
                match field {
                    Field::Name => row.map(|row| row.name.update(msg)),
                    Field::Price => row.map(|row| row.price.update(msg)),
                    Field::Image => row.map(|row| row.image.update(msg)),
                };
            }
            InventoryMsg::ItemInput(field, id, msg) => {
                let row = self.item_rows.get_mut(&id);
                match field {
                    Field::Name => row.map(|row| row.name.update(msg)),
                    Field::Price => row.map(|row| row.price.update(msg)),
                    Field::Image => row.map(|row| row.image.update(msg)),
                };
            }
        }

        Ok(())
    }

    fn rebuild_data(&mut self, res: &Res) {
        fn err<T>(input: ParsedInput<T>) -> ParsedInput<T> {
            input.with_error_message(strings::INVALID_MONEY_MESSAGE_SHORT)
        }

        // update bundle rows
        self.bundle_rows
            .retain(|id, _| res.bundles.contains_key(id));
        for (&id, bundle) in res.bundles.iter() {
            if let Some(row) = self.bundle_rows.get_mut(&id) {
                row.original = bundle.clone();
            } else {
                self.bundle_rows.insert(
                    id,
                    Row {
                        original: bundle.clone(),
                        name: ParsedInput::new_with_text(&bundle.name),
                        price: err(ParsedInput::new_with_value(bundle.price)),
                        image: ParsedInput::new_with_text(
                            bundle.image_url.as_deref().unwrap_or(""),
                        ),
                    },
                );
            }
        }

        // update item rows
        self.item_rows.retain(|id, _| res.items.contains_key(id));
        for (&id, item) in res.items.iter() {
            if let Some(row) = self.item_rows.get_mut(&id) {
                row.original = item.clone();
            } else {
                self.item_rows.insert(
                    id,
                    Row {
                        original: item.clone(),
                        name: ParsedInput::new_with_text(&item.name),
                        price: err(match item.price {
                            Some(price) => ParsedInput::new_with_value(Currency::from(price)),
                            None => ParsedInput::new(),
                        }),
                        image: ParsedInput::new_with_text(item.image_url.as_deref().unwrap_or("")),
                    },
                );
            }
        }

        // don't show deleted items
        self.item_rows
            .retain(|_, row| row.original.deleted_at.is_none());
    }

    pub fn view(&self, rs: &ResourceStore) -> Node<Msg> {
        use Field::*;
        use InventoryMsg::{BundleInput, ItemInput};

        let _res = match Res::acquire_now(rs) {
            Ok(res) => res,
            Err(_) => return Loading::view(),
        };

        fn view_input<T>(input: &ParsedInput<T>) -> Node<ParsedInputMsg> {
            td![input.view(C![C.inventory_page_input])]
        }

        let bundle_row = |(&id, row): (&InventoryBundleId, &Row<InventoryBundle>)| {
            tr![
                td![button![
                    C![C.inventory_page_save_button],
                    simple_ev(Ev::Click, InventoryMsg::SaveBundle(id)),
                    if row.is_dirty() {
                        attrs! {}
                    } else {
                        attrs! { At::Disabled => true }
                    },
                ]],
                td![id],
                view_input(&row.name).map_msg(move |msg| BundleInput(Name, id, msg)),
                view_input(&row.price).map_msg(move |msg| BundleInput(Price, id, msg)),
                view_input(&row.image).map_msg(move |msg| BundleInput(Image, id, msg)),
                td![button![
                    C![C.inventory_page_delete_button],
                    simple_ev(Ev::Click, InventoryMsg::DeleteBundle(id)),
                ]],
            ]
        };

        let item_row = |(&id, row): (&InventoryItemId, &Row<InventoryItem>)| {
            tr![
                td![button![
                    C![C.inventory_page_save_button],
                    simple_ev(Ev::Click, InventoryMsg::SaveItem(id)),
                    if row.is_dirty() {
                        attrs! {}
                    } else {
                        attrs! { At::Disabled => true }
                    },
                ]],
                td![id],
                view_input(&row.name).map_msg(move |msg| ItemInput(Name, id, msg)),
                view_input(&row.price).map_msg(move |msg| ItemInput(Price, id, msg)),
                view_input(&row.image).map_msg(move |msg| ItemInput(Image, id, msg)),
                td![button![
                    C![C.inventory_page_delete_button],
                    simple_ev(Ev::Click, InventoryMsg::DeleteItem(id)),
                ]],
            ]
        };

        let table_wide = || attrs! { At::ColSpan => 6 };

        let wide_button = |label: &str, msg: InventoryMsg| {
            tr![td![
                table_wide(),
                button![C![C.wide_button], simple_ev(Ev::Click, msg), label,],
            ]]
        };

        let header = || {
            tr![
                th![],
                th!["ID"],
                th!["Namn"],
                th!["Pris"],
                th!["Bild"],
                th![]
            ]
        };

        div![
            C![C.inventory_page],
            table![
                td![table_wide(), h1![strings::INVENTORY_BUNDLES]],
                header(),
                self.bundle_rows.iter().map(bundle_row),
                wide_button(strings::NEW_BUNDLE, InventoryMsg::NewBundle),
                td![table_wide(), h1![strings::INVENTORY_ITEMS]],
                header(),
                self.item_rows.iter().map(item_row),
                wide_button(strings::NEW_ITEM, InventoryMsg::NewItem),
            ],
        ]
        .map_msg(Msg::Inventory)
    }
}

fn default_bundle() -> NewInventoryBundle {
    NewInventoryBundle {
        name: strings::NEW_BUNDLE.to_string(),
        price: 1000.into(),
        image_url: None,
        item_ids: vec![],
    }
}

fn default_item() -> NewInventoryItem {
    NewInventoryItem {
        name: strings::NEW_ITEM.to_string(),
        price: None,
        image_url: None,
    }
}

impl Row<InventoryBundle> {
    fn is_dirty(&self) -> bool {
        [Field::Name, Field::Price, Field::Image]
            .into_iter()
            .any(|field| self.field_is_dirty(field))
    }

    fn field_is_dirty(&self, field: Field) -> bool {
        let original = &self.original;
        match field {
            Field::Name => Some(&original.name) != self.name.parsed(),
            Field::Price => Some(&original.price) != self.price.parsed(),
            Field::Image => {
                original.image_url.as_ref() != self.image.parsed().filter(|s| !s.is_empty())
            }
        }
    }
}

impl Row<InventoryItem> {
    fn is_dirty(&self) -> bool {
        [Field::Name, Field::Price, Field::Image]
            .into_iter()
            .any(|field| self.field_is_dirty(field))
    }

    fn field_is_dirty(&self, field: Field) -> bool {
        let original = &self.original;
        match field {
            Field::Name => Some(&original.name) != self.name.parsed(),
            Field::Price => original.price.map(Currency::from).as_ref() != self.price.parsed(),
            Field::Image => {
                original.image_url.as_ref() != self.image.parsed().filter(|s| !s.is_empty())
            }
        }
    }
}
