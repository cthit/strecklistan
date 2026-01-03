use crate::schema::tables::{inventory_bundle_items, inventory_bundles};
use serde::{Deserialize, Serialize};

#[derive(Queryable, Serialize, Deserialize, Debug, PartialEq)]
pub struct InventoryBundle {
    pub id: i32,
    pub name: String,
    pub price: i32,
    pub image_url: Option<String>,
}

#[derive(Insertable, AsChangeset, Serialize, Deserialize, Debug, PartialEq)]
#[diesel(table_name = inventory_bundles)]
#[diesel(treat_none_as_null = true)]
pub struct NewInventoryBundle {
    pub name: String,
    pub price: i32,
    pub image_url: Option<String>,
}

#[derive(Queryable, Serialize, Deserialize, Debug, PartialEq)]
pub struct InventoryBundleItem {
    pub id: i32,
    pub bundle_id: i32,
    pub item_id: i32,
}

#[derive(Insertable, AsChangeset, Serialize, Deserialize, Debug, PartialEq)]
#[diesel(table_name = inventory_bundle_items)]
pub struct NewInventoryBundleItem {
    pub bundle_id: i32,
    pub item_id: i32,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct InventoryCSVItem {
    pub name: String,
    pub price: Option<f64>,
    pub stock: i32,
}

impl InventoryCSVItem {
    /// Create from database values (price in öre)
    pub fn from_db(name: String, price_ore: Option<i32>, stock: i32) -> Self {
        InventoryCSVItem {
            name,
            price: price_ore.map(|p| p as f64 / 100.0),
            stock,
        }
    }

    /// Convert price from kronor to öre for database storage
    pub fn price_in_ore(&self) -> Option<i32> {
        self.price.map(|p| (p * 100.0).round() as i32)
    }
}


