//! Client-side reference app for df-pascal-core, built by Trunk into `dist/`.

#![deny(clippy::float_arithmetic)]

mod app;
mod events;
mod fmt;
mod icons;
mod pages;
mod state;
mod ui;
mod wallet;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(app::App);
}
