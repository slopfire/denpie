
mod components;
mod app;
mod state;
mod passkeys;
mod markdown;

use app::App;

fn main() {
    yew::Renderer::<App>::new().render();
}
