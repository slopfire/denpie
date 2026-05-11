use crate::app::View;
use crate::state::{AppAction, AppState};
use gloo_net::http::Request;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct SidebarProps {
    pub current_view: View,
    pub on_navigate: Callback<View>,
}

#[function_component(Sidebar)]
pub fn sidebar(props: &SidebarProps) -> Html {
    let app_state = use_context::<UseReducerHandle<AppState>>().unwrap();
    let menu_open = use_state(|| false);

    let on_nav = |view: View, cb: Callback<View>| {
        Callback::from(move |_| cb.emit(view.clone()))
    };

    let toggle_menu = {
        let menu_open = menu_open.clone();
        Callback::from(move |_| menu_open.set(!*menu_open))
    };

    let logout = {
        let app_state = app_state.clone();
        Callback::from(move |_| {
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if Request::post("/auth/logout").send().await.is_ok() {
                    app_state.dispatch(AppAction::SetAuthed(false));
                    app_state.dispatch(AppAction::SetUser(None));
                    app_state.dispatch(AppAction::ShowToast("Logged out".to_string()));
                    let state_clone = app_state.clone();
                    gloo_timers::callback::Timeout::new(2400, move || {
                        state_clone.dispatch(AppAction::HideToast);
                    }).forget();
                }
            });
        })
    };

    let user = app_state.user.clone();
    let username = user.as_ref().map(|u| u.display_name.clone().unwrap_or(u.username.clone())).unwrap_or_else(|| "Account".to_string());
    let role = user.as_ref().map(|u| u.role.clone()).unwrap_or_else(|| "User".to_string());
    let avatar_content = if let Some(Some(avatar)) = user.as_ref().map(|u| u.avatar_data.clone()) {
        html! { <img src={avatar} class="h-full w-full rounded-full object-cover" /> }
    } else {
        html! { {username.chars().next().unwrap_or('?').to_uppercase().to_string()} }
    };

    html! {
        <nav class="hidden lg:flex fixed left-0 top-0 z-50 h-full w-56 flex-col border-r surface p-4">
            <div class="flex items-center gap-2 px-2 py-2 mb-4">
                <div class="flex min-w-0 items-center gap-2">
                    <iconify-icon icon="radix-icons:lightning-bolt" class="radix-icon shrink-0 text-primary text-xl" aria-hidden="true"></iconify-icon>
                    <div class="truncate text-lg font-semibold tracking-tight">{"Denpie"}</div>
                </div>
            </div>
            <div class="space-y-1 flex-1">
                <button onclick={on_nav(View::Dashboard, props.on_navigate.clone())} class={classes!("nav-item", "w-full", "grid", "grid-cols-[1.5rem_minmax(0,1fr)]", "items-center", "gap-3", "rounded-md", "px-3", "py-2", "text-sm", "font-semibold", "text-left", (props.current_view == View::Dashboard).then_some("active"))}>
                    <iconify-icon icon="radix-icons:dashboard" class="radix-icon justify-self-center"></iconify-icon><span class="justify-self-start text-left">{"Dashboard"}</span>
                </button>
                <button onclick={on_nav(View::Flow, props.on_navigate.clone())} class={classes!("nav-item", "w-full", "grid", "grid-cols-[1.5rem_minmax(0,1fr)]", "items-center", "gap-3", "rounded-md", "px-3", "py-2", "text-sm", "font-semibold", "text-left", (props.current_view == View::Flow).then_some("active"))}>
                    <iconify-icon icon="radix-icons:loop" class="radix-icon justify-self-center"></iconify-icon><span class="justify-self-start text-left">{"Unified Flow"}</span>
                </button>
                <button onclick={on_nav(View::Settings, props.on_navigate.clone())} class={classes!("nav-item", "w-full", "grid", "grid-cols-[1.5rem_minmax(0,1fr)]", "items-center", "gap-3", "rounded-md", "px-3", "py-2", "text-sm", "font-semibold", "text-left", (props.current_view == View::Settings).then_some("active"))}>
                    <iconify-icon icon="radix-icons:gear" class="radix-icon justify-self-center"></iconify-icon><span class="justify-self-start text-left">{"Settings"}</span>
                </button>
                <button onclick={on_nav(View::Keys, props.on_navigate.clone())} class={classes!("nav-item", "w-full", "grid", "grid-cols-[1.5rem_minmax(0,1fr)]", "items-center", "gap-3", "rounded-md", "px-3", "py-2", "text-sm", "font-semibold", "text-left", (props.current_view == View::Keys).then_some("active"))}>
                    <iconify-icon icon="radix-icons:lock-closed" class="radix-icon justify-self-center"></iconify-icon><span class="justify-self-start text-left">{"API Keys"}</span>
                </button>
            </div>
            
            <div class="relative">
                <button id="account-menu-btn" onclick={toggle_menu} class="w-full rounded-md border border-token p-2 hover:opacity-90 flex items-center gap-2 text-left" title="Account">
                    <span id="account-avatar" class={classes!("h-8", "w-8", "shrink-0", "rounded-full", "flex", "items-center", "justify-center", "text-sm", "font-semibold", (user.as_ref().map(|u| u.avatar_data.is_none()).unwrap_or(true)).then_some("bg-primary-solid"))}>{avatar_content}</span>
                    <span class="min-w-0 flex-1">
                        <span id="account-name" class="block truncate text-sm font-semibold">{username}</span>
                        <span id="account-role" class="block truncate text-xs text-muted">{role}</span>
                    </span>
                    <iconify-icon icon="radix-icons:chevron-up" class="radix-icon shrink-0" aria-hidden="true"></iconify-icon>
                </button>
                <div id="account-menu" class={classes!("absolute", "bottom-12", "left-0", "right-0", "z-50", "surface", "border", "rounded-md", "p-1", "shadow-lg", (!*menu_open).then_some("hidden"))}>
                    <button id="account-settings-btn" onclick={on_nav(View::AccountSettings, props.on_navigate.clone())} type="button" class="w-full rounded px-3 py-2 text-sm text-left hover:bg-[var(--surface-muted)] flex items-center gap-2">
                        <iconify-icon icon="radix-icons:person" class="radix-icon" aria-hidden="true"></iconify-icon>
                        <span>{"Account Settings"}</span>
                    </button>
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
        </nav>
    }
}
