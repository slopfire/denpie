#![allow(
    clippy::collapsible_if,
    clippy::field_reassign_with_default,
    clippy::let_unit_value,
    clippy::trim_split_whitespace
)]

mod api;
mod app;
mod components;
mod markdown;
mod passkeys;
mod state;
mod topic_visual;

use app::App;

fn main() {
    yew::Renderer::<App>::new().render();
}
