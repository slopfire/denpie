use crate::state::{AppAction, AppState, UserProfile};
use gloo_net::http::Request;
use yew::prelude::*;

use crate::components::api_keys::ApiKeys;
use crate::components::dashboard::Dashboard;
use crate::components::login::LoginPanel;
use crate::components::settings::Settings;
use crate::components::sidebar::Sidebar;
use crate::components::unified_flow::UnifiedFlow;

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
    let app_state = use_reducer(AppState::default);
    let view = use_state(|| View::Dashboard);

    {
        let app_state = app_state.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(res) = Request::get("/auth/me").send().await {
                    if res.ok() {
                        if let Ok(user) = res.json::<UserProfile>().await {
                            app_state.dispatch(AppAction::SetUser(Some(user)));
                            app_state.dispatch(AppAction::SetAuthed(true));
                        }
                    }
                }
            });
            || ()
        });
    }

    let on_navigate = {
        let view = view.clone();
        Callback::from(move |new_view: View| view.set(new_view))
    };

    html! {
        <ContextProvider<UseReducerHandle<AppState>> context={app_state.clone()}>
            if !app_state.authed {
                <LoginPanel />
            } else {
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

            <div id="toast" class={classes!("toast", "surface", "border", "rounded-md", "px-3", "py-2", "text-sm", "font-medium", app_state.toast.show.then_some("show"))}>
                { &app_state.toast.message }
            </div>
        </ContextProvider<UseReducerHandle<AppState>>>
    }
}
