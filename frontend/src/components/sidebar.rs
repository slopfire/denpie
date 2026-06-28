use crate::api::toast_key;
use crate::app::View;
use crate::components::tooltip::ShadcnTooltip;
use crate::i18n::use_i18n;
use crate::state::{AppAction, AppState, UserProfile};
use gloo_net::http::Request;
use wasm_bindgen::JsCast;
use web_sys::KeyboardEvent;
use yew::prelude::*;
use yew_router::prelude::*;

#[derive(Properties, PartialEq)]
pub struct SidebarProps {
    pub current_view: View,
}

#[function_component(Sidebar)]
pub fn sidebar(props: &SidebarProps) -> Html {
    let app_state = use_context::<UseReducerHandle<AppState>>().unwrap();
    let i18n = use_i18n();
    let navigator = use_navigator().unwrap();
    let menu_open = use_state(|| false);
    let refreshing = use_state(|| false);

    let close_menu = {
        let menu_open = menu_open.clone();
        Callback::from(move |_| menu_open.set(false))
    };

    let toggle_menu = {
        let menu_open = menu_open.clone();
        Callback::from(move |_| menu_open.set(!*menu_open))
    };

    let container_ref = use_node_ref();

    let onfocusout = {
        let menu_open = menu_open.clone();
        let container_ref = container_ref.clone();
        Callback::from(move |e: FocusEvent| {
            if let Some(container) = container_ref.cast::<web_sys::HtmlElement>() {
                if let Some(related_target) = e.related_target() {
                    if let Ok(target_node) = related_target.dyn_into::<web_sys::Node>() {
                        let container_node: web_sys::Node = container.into();
                        if container_node.contains(Some(&target_node)) {
                            return;
                        }
                    }
                }
            }
            menu_open.set(false);
        })
    };

    let on_menu_keydown = {
        let close_menu = close_menu.clone();
        Callback::from(move |e: KeyboardEvent| {
            if e.key() == "Escape" {
                e.prevent_default();
                close_menu.emit(());
            }
        })
    };

    let logout = {
        let app_state = app_state.clone();
        let i18n = i18n.clone();
        let close_menu = close_menu.clone();
        Callback::from(move |_| {
            close_menu.emit(());
            let app_state = app_state.clone();
            let i18n = i18n.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if Request::post("/auth/logout").send().await.is_ok() {
                    app_state.dispatch(AppAction::SetSession(None));
                    toast_key(&app_state, &i18n, "toast.logged_out");
                }
            });
        })
    };

    let refresh_profile = {
        let app_state = app_state.clone();
        let i18n = i18n.clone();
        let close_menu = close_menu.clone();
        let refreshing = refreshing.clone();
        Callback::from(move |_| {
            if *refreshing {
                return;
            }
            refreshing.set(true);
            let app_state = app_state.clone();
            let i18n = i18n.clone();
            let refreshing = refreshing.clone();
            let close_menu = close_menu.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let result = match Request::get("/auth/me").send().await {
                    Ok(res) if res.ok() => res.json::<UserProfile>().await.ok(),
                    _ => None,
                };
                if let Some(user) = result {
                    app_state.dispatch(AppAction::SetUser(Some(user)));
                    toast_key(&app_state, &i18n, "toast.profile_refreshed");
                } else {
                    toast_key(&app_state, &i18n, "toast.profile_refresh_failed");
                }
                refreshing.set(false);
                close_menu.emit(());
            });
        })
    };

    let user = app_state.user.clone();
    let username = user
        .as_ref()
        .map(|u| u.display_name.clone().unwrap_or(u.username.clone()))
        .unwrap_or_else(|| i18n.t("account.fallback_name"));
    let role = user
        .as_ref()
        .map(|u| match u.role.as_str() {
            "admin" => i18n.t("role.admin"),
            "user" => i18n.t("role.user"),
            _ => u.role.clone(),
        })
        .unwrap_or_else(|| i18n.t("role.user"));
    let build_sha = user
        .as_ref()
        .map(|u| u.build_sha.clone())
        .unwrap_or_default();
    let build_sha_short = if build_sha.len() >= 7 {
        build_sha.chars().take(7).collect::<String>()
    } else {
        build_sha.clone()
    };
    let avatar_content = if let Some(Some(avatar)) = user.as_ref().map(|u| u.avatar_data.clone()) {
        html! { <img src={avatar} class="h-full w-full rounded-full object-cover" alt="" /> }
    } else {
        html! { {username.chars().next().unwrap_or('?').to_uppercase().to_string()} }
    };
    let menu_expanded = if *menu_open { "true" } else { "false" };
    let account_settings_active = props.current_view == View::AccountSettings;

    let open_account_settings = {
        let navigator = navigator.clone();
        let close_menu = close_menu.clone();
        Callback::from(move |_| {
            close_menu.emit(());
            navigator.push(&View::AccountSettings);
        })
    };

    html! {
        <nav class="hidden lg:flex fixed left-0 top-0 z-50 h-full w-56 flex-col border-r surface p-4 rounded-none">
            <div class="flex items-center gap-2 px-2 py-2 mb-4">
                <div class="flex min-w-0 items-center gap-2">
                    <iconify-icon icon="radix-icons:lightning-bolt" class="radix-icon shrink-0 text-primary text-xl" aria-hidden="true"></iconify-icon>
                    <div class="truncate text-lg font-semibold tracking-tight">{i18n.t("app.name")}</div>
                </div>
            </div>
            <div class="space-y-1 flex-1">
                <Link<View> to={View::Dashboard} classes={classes!("nav-item", "w-full", "grid", "grid-cols-[1.5rem_minmax(0,1fr)]", "items-center", "gap-3", "rounded-md", "px-3", "py-2", "text-sm", "font-semibold", "text-left", (props.current_view == View::Dashboard).then_some("active"))}>
                    <iconify-icon icon="radix-icons:dashboard" class="radix-icon justify-self-center"></iconify-icon><span class="justify-self-start text-left">{i18n.t("nav.dashboard")}</span>
                </Link<View>>
                <Link<View> to={View::Flow} classes={classes!("nav-item", "w-full", "grid", "grid-cols-[1.5rem_minmax(0,1fr)]", "items-center", "gap-3", "rounded-md", "px-3", "py-2", "text-sm", "font-semibold", "text-left", (props.current_view == View::Flow).then_some("active"))}>
                    <iconify-icon icon="radix-icons:loop" class="radix-icon justify-self-center"></iconify-icon><span class="justify-self-start text-left">{i18n.t("nav.flow")}</span>
                </Link<View>>
                <Link<View> to={View::Settings} classes={classes!("nav-item", "w-full", "grid", "grid-cols-[1.5rem_minmax(0,1fr)]", "items-center", "gap-3", "rounded-md", "px-3", "py-2", "text-sm", "font-semibold", "text-left", (props.current_view == View::Settings).then_some("active"))}>
                    <iconify-icon icon="radix-icons:gear" class="radix-icon justify-self-center"></iconify-icon><span class="justify-self-start text-left">{i18n.t("nav.settings")}</span>
                </Link<View>>
                <Link<View> to={View::Keys} classes={classes!("nav-item", "w-full", "grid", "grid-cols-[1.5rem_minmax(0,1fr)]", "items-center", "gap-3", "rounded-md", "px-3", "py-2", "text-sm", "font-semibold", "text-left", (props.current_view == View::Keys).then_some("active"))}>
                    <iconify-icon icon="radix-icons:lock-closed" class="radix-icon justify-self-center"></iconify-icon><span class="justify-self-start text-left">{i18n.t("nav.api_keys")}</span>
                </Link<View>>
                <Link<View> to={View::Archive} classes={classes!("nav-item", "w-full", "grid", "grid-cols-[1.5rem_minmax(0,1fr)]", "items-center", "gap-3", "rounded-md", "px-3", "py-2", "text-sm", "font-semibold", "text-left", (props.current_view == View::Archive).then_some("active"))}>
                    <iconify-icon icon="radix-icons:archive" class="radix-icon justify-self-center"></iconify-icon><span class="justify-self-start text-left">{i18n.t("nav.archive")}</span>
                </Link<View>>
            </div>
            if user.as_ref().map(|u| u.role == "admin").unwrap_or(false) {
                <div class="border-t border-token pt-3 mt-3">
                    <button
                        type="button"
                        onclick={let app_state = app_state.clone(); Callback::from(move |_| app_state.dispatch(AppAction::SetAdminMode(true)))}
                        class="nav-item w-full grid grid-cols-[1.5rem_minmax(0,1fr)] items-center gap-3 rounded-md px-3 py-2 text-sm font-semibold text-left"
                    >
                        <iconify-icon icon="radix-icons:gear" class="radix-icon justify-self-center"></iconify-icon>
                        <span class="justify-self-start text-left">{i18n.t("admin.switch_to_admin")}</span>
                    </button>
                </div>
            }

            <div ref={container_ref.clone()} class="relative" {onfocusout}>
                <ShadcnTooltip content={i18n.t("account.menu_title")} class={classes!("w-full")}>
                    <button
                        id="account-menu-btn"
                        type="button"
                        onclick={toggle_menu}
                        class="account-menu-trigger w-full rounded-md border border-token p-2 hover:opacity-90 flex items-center gap-2 text-left"
                        aria-haspopup="menu"
                        aria-expanded={menu_expanded}
                        aria-controls="account-menu"
                    >
                        <span id="account-avatar" class={classes!("h-8", "w-8", "shrink-0", "rounded-full", "flex", "items-center", "justify-center", "text-sm", "font-semibold", (user.as_ref().map(|u| u.avatar_data.is_none()).unwrap_or(true)).then_some("bg-primary-solid"))}>{avatar_content}</span>
                        <span class="min-w-0 flex-1">
                            <span id="account-name" class="block truncate text-sm font-semibold">{username.clone()}</span>
                            <span id="account-role" class="block truncate text-xs text-muted">{role.clone()}</span>
                        </span>
                        <iconify-icon icon="radix-icons:chevron-up" class="account-menu-chevron radix-icon shrink-0" aria-hidden="true"></iconify-icon>
                    </button>
                </ShadcnTooltip>
                <div
                    id="account-menu"
                    role="menu"
                    aria-label={i18n.t("account.menu_title")}
                    class={classes!("account-menu", "surface-popover", (!*menu_open).then_some("account-menu--closed"))}
                    onkeydown={on_menu_keydown}
                >
                    <div class="account-menu-header" role="presentation">
                        <span class="account-menu-header-name">{username}</span>
                        <span class="account-menu-header-role">{role}</span>
                    </div>
                    <div class="account-menu-separator" role="separator"></div>
                    <button
                        type="button"
                        role="menuitem"
                        onclick={open_account_settings}
                        class={classes!(
                            "account-menu-item",
                            account_settings_active.then_some("active"),
                        )}
                    >
                        <iconify-icon icon="radix-icons:person" class="radix-icon" aria-hidden="true"></iconify-icon>
                        <span>{i18n.t("account.settings")}</span>
                    </button>
                    <button
                        id="account-refresh-btn"
                        type="button"
                        role="menuitem"
                        onclick={refresh_profile}
                        disabled={*refreshing}
                        class="account-menu-item"
                    >
                        <iconify-icon
                            icon="radix-icons:reload"
                            class={classes!("radix-icon", (*refreshing).then_some("animate-spin"))}
                            aria-hidden="true"
                        ></iconify-icon>
                        <span>{i18n.t("account.refresh")}</span>
                    </button>
                    <div class="account-menu-separator" role="separator"></div>
                    <button
                        id="logout-btn"
                        type="button"
                        role="menuitem"
                        onclick={logout}
                        class="account-menu-item account-menu-item--danger"
                    >
                        <iconify-icon icon="radix-icons:exit" class="radix-icon" aria-hidden="true"></iconify-icon>
                        <span>{i18n.t("nav.logout")}</span>
                    </button>
                </div>
            </div>
            if !build_sha.is_empty() && build_sha != "unknown" {
                <div class="mt-4 pt-3 border-t border-token flex justify-center">
                    <ShadcnTooltip content={i18n.t("account.view_commit")}>
                        <a href={format!("https://github.com/slopfire/dailytipdraft/commit/{}", build_sha)}
                           target="_blank"
                           rel="noopener noreferrer"
                           class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[10px] font-mono font-medium tracking-wider text-muted hover:text-primary border border-token hover:border-primary/30 bg-[var(--surface-muted)] hover:bg-[var(--surface-hover)] transition-all duration-200 shadow-sm">
                            <iconify-icon icon="radix-icons:commit" class="radix-icon text-xs opacity-70 shrink-0"></iconify-icon>
                            <span>{build_sha_short}</span>
                        </a>
                    </ShadcnTooltip>
                </div>
            }
        </nav>
    }
}
