use yew::prelude::*;
use crate::components::login::LoginPanel;
use crate::components::sidebar::Sidebar;
use crate::components::dashboard::Dashboard;
use crate::components::unified_flow::UnifiedFlow;
use crate::components::settings::Settings;
use crate::components::api_keys::ApiKeys;
use crate::state::AppState;

#[derive(Clone, PartialEq, Default)]
pub enum View {
    #[default]
    Dashboard,
    Flow,
    Settings,
    Keys,
    Archive,
}

#[function_component(App)]
pub fn app() -> Html {
    let _app_state = use_reducer(AppState::default);
    let view = use_state(|| View::Dashboard);

    let logged_in = use_state(|| false); // Simplified for initial scaffolding

    if !*logged_in {
        return html! {
            <LoginPanel on_login={
                let logged_in = logged_in.clone();
                Callback::from(move |_| logged_in.set(true))
            } />
        };
    }

    let on_navigate = {
        let view = view.clone();
        Callback::from(move |new_view: View| view.set(new_view))
    };

    html! {
        <div id="app-shell" class="min-h-screen">
            <Sidebar current_view={(*view).clone()} on_navigate={on_navigate} />
            <main class="lg:ml-56 px-4 sm:px-6 lg:px-6 py-5 pb-20 max-w-none">
                {
                    match *view {
                        View::Dashboard => html! { <Dashboard /> },
                        View::Flow => html! { <UnifiedFlow /> },
                        View::Settings => html! { <Settings /> },
                        View::Keys => html! { <ApiKeys /> },
                        View::Archive => html! { <div class="p-10 text-center text-muted">{"Archive View (Coming Soon)"}</div> },
                    }
                }
            </main>
        </div>
    }
}
