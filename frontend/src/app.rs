use crate::i18n::{I18n, use_i18n};
use crate::state::{AppAction, AppState, AuthStatus, UserProfile};
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
use crate::components::admin_shell::AdminShell;
use std::collections::HashSet;

#[derive(Clone, Routable, PartialEq, Eq, Hash)]
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
                match Request::get("/auth/me").send().await {
                    Ok(res) if res.ok() => {
                        if let Ok(user) = res.json::<UserProfile>().await {
                            app_state.dispatch(AppAction::SetSession(Some(user)));
                        } else {
                            app_state.dispatch(AppAction::SetSession(None));
                        }
                    }
                    _ => {
                        app_state.dispatch(AppAction::SetSession(None));
                    }
                }
            });
            || ()
        });
    }

    {
        let auth_status = app_state.auth_status;
        use_effect_with(auth_status, move |auth_status| {
            if *auth_status == AuthStatus::Authenticated {
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
        <ContextProvider<I18n> context={I18n::default()}>
            <ContextProvider<UseReducerHandle<AppState>> context={app_state.clone()}>
                {
                    match app_state.auth_status {
                        AuthStatus::Checking => html! { <AuthChecking /> },
                        AuthStatus::Guest => html! { <LoginPanel /> },
                        AuthStatus::Authenticated => {
                            let is_admin = app_state
                                .user
                                .as_ref()
                                .map(|u| u.role == "admin")
                                .unwrap_or(false);
                            if is_admin && app_state.admin_mode {
                                html! {
                                    <div id="app-shell" class="min-h-screen">
                                        <AdminShell />
                                    </div>
                                }
                            } else {
                                html! {
                                    <div id="app-shell" class="min-h-screen">
                                        <Switch<View> render={|_| html! { <AppShell /> }} />
                                        <MobileNav />
                                    </div>
                                }
                            }
                        }
                    }
                }

                <div id="toast" class={classes!("toast", "surface", "border", "rounded-md", "px-3", "py-2", "text-sm", "font-medium", app_state.toast.show.then_some("show"))}>
                    { &app_state.toast.message }
                </div>
            </ContextProvider<UseReducerHandle<AppState>>>
        </ContextProvider<I18n>>
    }
}

#[function_component(AuthChecking)]
fn auth_checking() -> Html {
    let i18n = use_i18n();

    html! {
        <section id="auth-checking" class="min-h-screen flex items-center justify-center p-4">
            <div class="surface border rounded-md w-full max-w-md p-6 text-center">
                <iconify-icon icon="radix-icons:reload" class="radix-icon text-primary text-3xl animate-spin mx-auto block" aria-hidden="true"></iconify-icon>
                <p class="mt-4 text-sm text-muted">{i18n.t("auth.checking_session")}</p>
            </div>
        </section>
    }
}

fn normalize_view(view: Option<View>) -> View {
    match view {
        Some(View::NotFound) | None => View::Dashboard,
        Some(view) => view,
    }
}

#[derive(Properties, PartialEq)]
struct RouteViewProps {
    active: bool,
    mounted: bool,
    children: Children,
}

#[function_component(RouteView)]
fn route_view(props: &RouteViewProps) -> Html {
    if !props.mounted {
        return html! {};
    }

    html! {
        <div class={classes!("route-view", (!props.active).then_some("hidden-view"))} aria-hidden={(!props.active).to_string()}>
            { for props.children.iter() }
        </div>
    }
}

#[function_component(AppShell)]
fn app_shell() -> Html {
    let current = normalize_view(use_route::<View>());
    let mounted = use_state(|| HashSet::from([current.clone()]));

    {
        let mounted = mounted.clone();
        let current = current.clone();
        use_effect_with(current.clone(), move |view| {
            if !mounted.contains(view) {
                let mut next = (*mounted).clone();
                next.insert(view.clone());
                mounted.set(next);
            }
            || ()
        });
    }

    let is_mounted = |view: &View| mounted.contains(view);
    let is_active = |view: View| current == view;

    html! {
        <>
            <Sidebar current_view={current.clone()} />
            <main class="lg:ml-56 px-4 sm:px-6 lg:px-6 py-5 pb-20 max-w-none">
                <RouteView active={is_active(View::Dashboard)} mounted={is_mounted(&View::Dashboard)}>
                    <Dashboard />
                </RouteView>
                <RouteView active={is_active(View::Flow)} mounted={is_mounted(&View::Flow)}>
                    <UnifiedFlow />
                </RouteView>
                <RouteView active={is_active(View::Settings)} mounted={is_mounted(&View::Settings)}>
                    <Settings />
                </RouteView>
                <RouteView active={is_active(View::Keys)} mounted={is_mounted(&View::Keys)}>
                    <ApiKeys />
                </RouteView>
                <RouteView active={is_active(View::Archive)} mounted={is_mounted(&View::Archive)}>
                    <Archive />
                </RouteView>
                <RouteView active={is_active(View::AccountSettings)} mounted={is_mounted(&View::AccountSettings)}>
                    <AccountSettings />
                </RouteView>
            </main>
        </>
    }
}

#[function_component(MobileNav)]
fn mobile_nav() -> Html {
    let active_view = use_route::<View>();
    let i18n = use_i18n();

    html! {
        <nav class="lg:hidden fixed bottom-0 inset-x-0 z-50 surface border-t grid grid-cols-5 px-2 py-2 rounded-none">
            <Link<View> to={View::Dashboard} classes={classes!("nav-item", "rounded-md", "px-2", "py-2", "text-xs", "font-semibold", "text-center", (active_view == Some(View::Dashboard)).then_some("active"))}>
                <iconify-icon icon="radix-icons:dashboard" class="radix-icon block mx-auto"></iconify-icon>
                <span class="sr-only">{i18n.t("nav.dashboard")}</span>
            </Link<View>>
            <Link<View> to={View::Flow} classes={classes!("nav-item", "rounded-md", "px-2", "py-2", "text-xs", "font-semibold", "text-center", (active_view == Some(View::Flow)).then_some("active"))}>
                <iconify-icon icon="radix-icons:loop" class="radix-icon block mx-auto"></iconify-icon>
                <span class="sr-only">{i18n.t("nav.flow")}</span>
            </Link<View>>
            <Link<View> to={View::Archive} classes={classes!("nav-item", "rounded-md", "px-2", "py-2", "text-xs", "font-semibold", "text-center", (active_view == Some(View::Archive)).then_some("active"))}>
                <iconify-icon icon="radix-icons:archive" class="radix-icon block mx-auto"></iconify-icon>
                <span class="sr-only">{i18n.t("nav.archive")}</span>
            </Link<View>>
            <Link<View> to={View::Settings} classes={classes!("nav-item", "rounded-md", "px-2", "py-2", "text-xs", "font-semibold", "text-center", (active_view == Some(View::Settings)).then_some("active"))}>
                <iconify-icon icon="radix-icons:gear" class="radix-icon block mx-auto"></iconify-icon>
                <span class="sr-only">{i18n.t("nav.settings")}</span>
            </Link<View>>
            <Link<View> to={View::Keys} classes={classes!("nav-item", "rounded-md", "px-2", "py-2", "text-xs", "font-semibold", "text-center", (active_view == Some(View::Keys)).then_some("active"))}>
                <iconify-icon icon="radix-icons:lock-closed" class="radix-icon block mx-auto"></iconify-icon>
                <span class="sr-only">{i18n.t("nav.api_keys")}</span>
            </Link<View>>
        </nav>
    }
}
