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
