#![deny(unreachable_patterns)]
#![allow(clippy::single_match)]

mod app;
mod components;
mod fuzzy_search;
mod generated;
mod models;
mod notification_manager;
mod page;
mod strings;
mod util;
mod views;

use seed::{App, prelude::*};

#[wasm_bindgen(start)]
pub fn start() {
    set_panic_hook();

    App::start("app", app::init, app::update, app::view);
}

// see cargo.toml for more info
cfg_if::cfg_if! {
    // When the `console_error_panic_hook` feature is enabled, we can call the
    // `set_panic_hook` function to get better error messages if we ever panic.
    if #[cfg(feature = "console_error_panic_hook")] {
        use console_error_panic_hook::set_once as set_panic_hook;
    } else {
        #[inline]
        fn set_panic_hook() {}
    }
}

// see cargo.toml for more info
cfg_if::cfg_if! {
    // When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
    // allocator.
    if #[cfg(feature = "wee_alloc")] {
        #[global_allocator]
        static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;
    }
}
