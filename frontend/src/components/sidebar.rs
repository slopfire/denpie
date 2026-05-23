use crate::api::toast;
use crate::app::View;
use crate::state::{AppAction, AppState};
use gloo_net::http::Request;
use wasm_bindgen::JsCast;
use yew::prelude::*;
use yew_router::prelude::*;

#[derive(Properties, PartialEq)]
pub struct SidebarProps {
    pub current_view: View,
}

#[function_component(Sidebar)]
pub fn sidebar(props: &SidebarProps) -> Html {
    let app_state = use_context::<UseReducerHandle<AppState>>().unwrap();
    let menu_open = use_state(|| false);

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

    let logout = {
        let app_state = app_state.clone();
        Callback::from(move |_| {
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if Request::post("/auth/logout").send().await.is_ok() {
                    app_state.dispatch(AppAction::SetAuthed(false));
                    app_state.dispatch(AppAction::SetUser(None));
                    toast(&app_state, "Logged out");
                }
            });
        })
    };

    let user = app_state.user.clone();
    let username = user
        .as_ref()
        .map(|u| u.display_name.clone().unwrap_or(u.username.clone()))
        .unwrap_or_else(|| "Account".to_string());
    let role = user
        .as_ref()
        .map(|u| u.role.clone())
        .unwrap_or_else(|| "User".to_string());
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
        html! { <img src={avatar} class="h-full w-full rounded-full object-cover" /> }
    } else {
        html! { {username.chars().next().unwrap_or('?').to_uppercase().to_string()} }
    };

    html! {
        <nav class="hidden lg:flex fixed left-0 top-0 z-50 h-full w-56 flex-col border-r surface p-4 rounded-none">
            <div class="flex items-center gap-2 px-2 py-2 mb-4">
                <div class="flex min-w-0 items-center gap-2">
                    <iconify-icon icon="radix-icons:lightning-bolt" class="radix-icon shrink-0 text-primary text-xl" aria-hidden="true"></iconify-icon>
                    <div class="truncate text-lg font-semibold tracking-tight">{"Denpie"}</div>
                </div>
            </div>
            <div class="space-y-1 flex-1">
                <Link<View> to={View::Dashboard} classes={classes!("nav-item", "w-full", "grid", "grid-cols-[1.5rem_minmax(0,1fr)]", "items-center", "gap-3", "rounded-md", "px-3", "py-2", "text-sm", "font-semibold", "text-left", (props.current_view == View::Dashboard).then_some("active"))}>
                    <iconify-icon icon="radix-icons:dashboard" class="radix-icon justify-self-center"></iconify-icon><span class="justify-self-start text-left">{"Dashboard"}</span>
                </Link<View>>
                <Link<View> to={View::Flow} classes={classes!("nav-item", "w-full", "grid", "grid-cols-[1.5rem_minmax(0,1fr)]", "items-center", "gap-3", "rounded-md", "px-3", "py-2", "text-sm", "font-semibold", "text-left", (props.current_view == View::Flow).then_some("active"))}>
                    <iconify-icon icon="radix-icons:loop" class="radix-icon justify-self-center"></iconify-icon><span class="justify-self-start text-left">{"Unified Flow"}</span>
                </Link<View>>
                <Link<View> to={View::Settings} classes={classes!("nav-item", "w-full", "grid", "grid-cols-[1.5rem_minmax(0,1fr)]", "items-center", "gap-3", "rounded-md", "px-3", "py-2", "text-sm", "font-semibold", "text-left", (props.current_view == View::Settings).then_some("active"))}>
                    <iconify-icon icon="radix-icons:gear" class="radix-icon justify-self-center"></iconify-icon><span class="justify-self-start text-left">{"Settings"}</span>
                </Link<View>>
                <Link<View> to={View::Keys} classes={classes!("nav-item", "w-full", "grid", "grid-cols-[1.5rem_minmax(0,1fr)]", "items-center", "gap-3", "rounded-md", "px-3", "py-2", "text-sm", "font-semibold", "text-left", (props.current_view == View::Keys).then_some("active"))}>
                    <iconify-icon icon="radix-icons:lock-closed" class="radix-icon justify-self-center"></iconify-icon><span class="justify-self-start text-left">{"API Keys"}</span>
                </Link<View>>
                <Link<View> to={View::Archive} classes={classes!("nav-item", "w-full", "grid", "grid-cols-[1.5rem_minmax(0,1fr)]", "items-center", "gap-3", "rounded-md", "px-3", "py-2", "text-sm", "font-semibold", "text-left", (props.current_view == View::Archive).then_some("active"))}>
                    <iconify-icon icon="radix-icons:archive" class="radix-icon justify-self-center"></iconify-icon><span class="justify-self-start text-left">{"Archive"}</span>
                </Link<View>>
            </div>

            <div ref={container_ref.clone()} class="relative" {onfocusout}>
                <button id="account-menu-btn" onclick={toggle_menu} class="w-full rounded-md border border-token p-2 hover:opacity-90 flex items-center gap-2 text-left" title="Account">
                    <span id="account-avatar" class={classes!("h-8", "w-8", "shrink-0", "rounded-full", "flex", "items-center", "justify-center", "text-sm", "font-semibold", (user.as_ref().map(|u| u.avatar_data.is_none()).unwrap_or(true)).then_some("bg-primary-solid"))}>{avatar_content}</span>
                    <span class="min-w-0 flex-1">
                        <span id="account-name" class="block truncate text-sm font-semibold">{username}</span>
                        <span id="account-role" class="block truncate text-xs text-muted">{role}</span>
                    </span>
                    <iconify-icon icon="radix-icons:chevron-up" class="radix-icon shrink-0" aria-hidden="true"></iconify-icon>
                </button>
                <div id="account-menu" class={classes!("absolute", "bottom-12", "left-0", "right-0", "z-50", "surface", "border", "rounded-md", "p-1", "shadow-lg", (!*menu_open).then_some("hidden"))}>
                    <Link<View> to={View::AccountSettings} classes="w-full rounded px-3 py-2 text-sm text-left hover:bg-[var(--surface-muted)] flex items-center gap-2">
                        <iconify-icon icon="radix-icons:person" class="radix-icon" aria-hidden="true"></iconify-icon>
                        <span>{"Account Settings"}</span>
                    </Link<View>>
                    <button id="account-refresh-btn" type="button" class="w-full rounded px-3 py-2 text-sm text-left hover:bg-[var(--surface-muted)] flex items-center gap-2">
                        <iconify-icon icon="radix-icons:reload" class="radix-icon" aria-hidden="true"></iconify-icon>
                        <span>{"Refresh"}</span>
                    </button>
                    <button id="logout-btn" onclick={logout} type="button" class="w-full rounded px-3 py-2 text-sm text-left hover:bg-[var(--surface-muted)] flex items-center gap-2">
                        <iconify-icon icon="radix-icons:exit" class="radix-icon" aria-hidden="true"></iconify-icon>
                        <span>{"Logout"}</span>
                    </button>
                </div>
            </div>
            if !build_sha.is_empty() && build_sha != "unknown" {
                <div class="mt-4 pt-3 border-t border-token flex justify-center">
                    <a href={format!("https://github.com/slopfire/dailytipdraft/commit/{}", build_sha)}
                       target="_blank"
                       rel="noopener noreferrer"
                       class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[10px] font-mono font-medium tracking-wider text-muted hover:text-primary border border-token hover:border-primary/30 bg-[var(--surface-muted)] hover:bg-[var(--surface-hover)] transition-all duration-200 shadow-sm"
                       title="View commit on GitHub">
                        <iconify-icon icon="radix-icons:commit" class="radix-icon text-xs opacity-70 shrink-0"></iconify-icon>
                        <span>{build_sha_short}</span>
                    </a>
                </div>
            }
        </nav>
    }
}
