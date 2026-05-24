use crate::state::{AppAction, AppState, UserProfile};
use gloo_net::http::Request;
use yew::prelude::*;
use yew_router::prelude::*;

use crate::components::account::AccountSettings;
use crate::components::api_keys::ApiKeys;
use crate::components::archive::Archive;
use crate::components::dashboard::Dashboard;
use crate::components::login::LoginPanel;
use crate::components::settings::{Settings, SettingsRes, apply_appearance};
use crate::components::sidebar::Sidebar;
use crate::components::unified_flow::UnifiedFlow;

#[derive(Clone, Routable, PartialEq)]
pub enum View {
    #[at("/")]
    Dashboard,
    #[at("/flow")]
    Flow,
    #[at("/settings")]
    Settings,
    #[at("/keys")]
    Keys,
    #[at("/archive")]
    Archive,
    #[at("/account")]
    AccountSettings,
    #[not_found]
    #[at("/404")]
    NotFound,
}

#[function_component(App)]
pub fn app() -> Html {
    html! {
        <BrowserRouter>
            <AppRoot />
        </BrowserRouter>
    }
}

#[function_component(AppRoot)]
fn app_root() -> Html {
    let app_state = use_reducer(AppState::default);

    {
        let app_state = app_state.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(res) = Request::get("/auth/me").send().await {
                    if res.ok() {
                        if let Ok(user) = res.json::<UserProfile>().await {
                            app_state.dispatch(AppAction::SetUser(Some(user)));
                            app_state.dispatch(AppAction::SetAuthed(true));
                            if let Ok(settings_res) = Request::get("/admin/settings").send().await {
                                if settings_res.ok() {
                                    if let Ok(settings) = settings_res.json::<SettingsRes>().await {
                                        apply_appearance(&settings);
                                    }
                                }
                            }
                        }
                    }
                }
            });
            || ()
        });
    }

    {
        let authed = app_state.authed;
        use_effect_with(authed, move |authed| {
            if *authed {
                wasm_bindgen_futures::spawn_local(async move {
                    if let Ok(settings_res) = Request::get("/admin/settings").send().await {
                        if settings_res.ok() {
                            if let Ok(settings) = settings_res.json::<SettingsRes>().await {
                                apply_appearance(&settings);
                            }
                        }
                    }
                });
            }
            || ()
        });
    }

    html! {
        <ContextProvider<UseReducerHandle<AppState>> context={app_state.clone()}>
            if !app_state.authed {
                <LoginPanel />
            } else {
                <div id="app-shell" class="min-h-screen">
                    <Switch<View> render={switch_shell} />
                    <MobileNav />
                </div>
            }

            <div id="toast" class={classes!("toast", "surface", "border", "rounded-md", "px-3", "py-2", "text-sm", "font-medium", app_state.toast.show.then_some("show"))}>
                { &app_state.toast.message }
            </div>
        </ContextProvider<UseReducerHandle<AppState>>>
    }
}

fn switch_shell(view: View) -> Html {
    let current = if view == View::NotFound {
        View::Dashboard
    } else {
        view
    };
    html! {
        <>
            <Sidebar current_view={current.clone()} />
            <main class="lg:ml-56 px-4 sm:px-6 lg:px-6 py-5 pb-20 max-w-none">
                {
                    match current {
                        View::Dashboard | View::NotFound => html! { <Dashboard /> },
                        View::Flow => html! { <UnifiedFlow /> },
                        View::Settings => html! { <Settings /> },
                        View::Keys => html! { <ApiKeys /> },
                        View::Archive => html! { <Archive /> },
                        View::AccountSettings => html! { <AccountSettings /> },
                    }
                }
            </main>
        </>
    }
}

#[function_component(MobileNav)]
fn mobile_nav() -> Html {
    let active_view = use_route::<View>();

    html! {
        <nav class="lg:hidden fixed bottom-0 inset-x-0 z-50 surface border-t grid grid-cols-5 px-2 py-2 rounded-none">
            <Link<View> to={View::Dashboard} classes={classes!("nav-item", "rounded-md", "px-2", "py-2", "text-xs", "font-semibold", "text-center", (active_view == Some(View::Dashboard)).then_some("active"))}>
                <iconify-icon icon="radix-icons:dashboard" class="radix-icon block mx-auto"></iconify-icon>
            </Link<View>>
            <Link<View> to={View::Flow} classes={classes!("nav-item", "rounded-md", "px-2", "py-2", "text-xs", "font-semibold", "text-center", (active_view == Some(View::Flow)).then_some("active"))}>
                <iconify-icon icon="radix-icons:loop" class="radix-icon block mx-auto"></iconify-icon>
            </Link<View>>
            <Link<View> to={View::Archive} classes={classes!("nav-item", "rounded-md", "px-2", "py-2", "text-xs", "font-semibold", "text-center", (active_view == Some(View::Archive)).then_some("active"))}>
                <iconify-icon icon="radix-icons:archive" class="radix-icon block mx-auto"></iconify-icon>
            </Link<View>>
            <Link<View> to={View::Settings} classes={classes!("nav-item", "rounded-md", "px-2", "py-2", "text-xs", "font-semibold", "text-center", (active_view == Some(View::Settings)).then_some("active"))}>
                <iconify-icon icon="radix-icons:gear" class="radix-icon block mx-auto"></iconify-icon>
            </Link<View>>
            <Link<View> to={View::Keys} classes={classes!("nav-item", "rounded-md", "px-2", "py-2", "text-xs", "font-semibold", "text-center", (active_view == Some(View::Keys)).then_some("active"))}>
                <iconify-icon icon="radix-icons:lock-closed" class="radix-icon block mx-auto"></iconify-icon>
            </Link<View>>
        </nav>
    }
}
