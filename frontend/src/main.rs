mod api;
mod app;
mod components;
mod markdown;
mod passkeys;
mod state;

use app::App;

fn main() {
    yew::Renderer::<App>::new().render();
}
