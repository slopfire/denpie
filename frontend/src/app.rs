use crate::i18n::{I18n, use_i18n};
use crate::state::{AppAction, AppState, AuthStatus, ToastKind, UserProfile};
use gloo_net::http::Request;
use yew::prelude::*;
use yew_router::prelude::*;

use crate::components::account::AccountSettings;
use crate::components::admin_shell::AdminShell;
use crate::components::api_keys::ApiKeys;
use crate::components::archive::Archive;
use crate::components::grounding::Grounding;
use crate::components::login::LoginPanel;
use crate::components::settings::{
    Settings, SettingsRes, apply_appearance, apply_local_appearance_overrides,
};
use crate::components::sidebar::Sidebar;
use crate::components::unified_flow::UnifiedFlow;
use std::collections::HashSet;

#[derive(Clone, Routable, PartialEq, Eq, Hash)]
pub enum View {
    #[at("/grounding")]
    Grounding,
    #[at("/")]
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
    #[cfg(feature = "lab-ui")]
    {
        return html! {
            <ContextProvider<I18n> context={I18n::default()}>
                <crate::components::card_lab::CardLab />
            </ContextProvider<I18n>>
        };
    }

    #[cfg(not(feature = "lab-ui"))]
    {
        html! {
            <BrowserRouter>
                <AppRoot />
            </BrowserRouter>
        }
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
                    if let Ok(settings_view) = crate::api_v1::get_settings().await {
                        let mut settings = SettingsRes::from(settings_view);
                        apply_local_appearance_overrides(&mut settings);
                        apply_appearance(&settings);
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
                                    <div id="app-shell" class="app-shell min-h-screen">
                                        <Switch<View> render={|_| html! { <AppShell /> }} />
                                        <MobileNav />
                                    </div>
                                }
                            }
                        }
                    }
                }

                <AppToast />
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
        Some(View::NotFound) | None => View::Flow,
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
            <main class="app-main lg:ml-56 px-4 sm:px-6 lg:px-6 py-5 pb-20 max-w-none">
                <RouteView active={is_active(View::Grounding)} mounted={is_mounted(&View::Grounding)}>
                    <Grounding />
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
        <nav class="mobile-bottom-nav lg:hidden z-50 w-full surface border-t grid grid-cols-5 rounded-none">
            <Link<View> to={View::Grounding} classes={classes!("nav-item", "rounded-md", "px-2", "py-2", "text-xs", "font-semibold", "text-center", (active_view == Some(View::Grounding)).then_some("active"))}>
                <iconify-icon icon="tabler:circuit-ground" class="radix-icon block mx-auto"></iconify-icon>
                <span class="sr-only">{i18n.t("nav.grounding")}</span>
            </Link<View>>
            <Link<View> to={View::Flow} classes={classes!("nav-item", "rounded-md", "px-2", "py-2", "text-xs", "font-semibold", "text-center", (active_view == Some(View::Flow)).then_some("active"))}>
                <iconify-icon icon="tabler:antenna" class="radix-icon block mx-auto"></iconify-icon>
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

#[function_component(AppToast)]
fn app_toast() -> Html {
    let app_state = use_context::<UseReducerHandle<AppState>>().expect("AppState context");
    let expanded = use_state(|| false);
    let copied = use_state(|| false);

    {
        let expanded = expanded.clone();
        let copied = copied.clone();
        let key = (
            app_state.toast.message.clone(),
            app_state.toast.detail.clone(),
            app_state.toast.show,
        );
        use_effect_with(key, move |_| {
            expanded.set(false);
            copied.set(false);
            || ()
        });
    }

    let toast = &app_state.toast;
    let detail = toast
        .detail
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(str::to_string);
    let has_detail = detail.is_some();
    let copy_text = detail.clone().unwrap_or_else(|| toast.message.clone());

    let kind_class = match toast.kind {
        ToastKind::Error => "toast-error",
        ToastKind::Success => "toast-success",
        ToastKind::Info => "toast-info",
    };
    let icon = match toast.kind {
        ToastKind::Error => "radix-icons:exclamation-triangle",
        ToastKind::Success => "radix-icons:check-circled",
        ToastKind::Info => "radix-icons:info-circled",
    };

    let on_dismiss = {
        let app_state = app_state.clone();
        Callback::from(move |_| app_state.dispatch(AppAction::HideToast))
    };
    let on_toggle = {
        let expanded = expanded.clone();
        Callback::from(move |_| expanded.set(!*expanded))
    };
    let on_copy = {
        let copied = copied.clone();
        Callback::from(move |_| {
            if copy_text.is_empty() {
                return;
            }
            if let Some(window) = web_sys::window() {
                let clipboard = window.navigator().clipboard();
                let _ = clipboard.write_text(&copy_text);
                copied.set(true);
                let copied = copied.clone();
                gloo_timers::callback::Timeout::new(1200, move || copied.set(false)).forget();
            }
        })
    };

    html! {
        <div
            id="toast"
            class={classes!(
                "toast",
                "surface",
                "border",
                kind_class,
                toast.show.then_some("show"),
                (*expanded).then_some("is-expanded"),
            )}
            role={if toast.kind == ToastKind::Error { "alert" } else { "status" }}
            aria-live={if toast.kind == ToastKind::Error { "assertive" } else { "polite" }}
        >
            <div class="toast-row">
                <span class="toast-icon" aria-hidden="true">
                    <iconify-icon icon={icon} class="radix-icon"></iconify-icon>
                </span>
                <div class="toast-body">
                    <p class="toast-message">{&toast.message}</p>
                    if *expanded {
                        if let Some(detail_text) = detail.clone() {
                            <pre class="toast-detail">{detail_text}</pre>
                        }
                    }
                    if has_detail || toast.kind == ToastKind::Error {
                        <div class="toast-actions">
                            if has_detail {
                                <button type="button" class="toast-action" onclick={on_toggle}>
                                    {if *expanded { "Hide details" } else { "Show details" }}
                                </button>
                            }
                            <button
                                type="button"
                                class={classes!("toast-action", (*copied).then_some("is-copied"))}
                                onclick={on_copy}
                            >
                                {if *copied { "Copied" } else { "Copy" }}
                            </button>
                        </div>
                    }
                </div>
                <button
                    type="button"
                    class="toast-dismiss"
                    aria-label="Dismiss notification"
                    onclick={on_dismiss}
                >
                    <iconify-icon icon="radix-icons:cross-2" class="radix-icon" aria-hidden="true"></iconify-icon>
                </button>
            </div>
        </div>
    }
}
