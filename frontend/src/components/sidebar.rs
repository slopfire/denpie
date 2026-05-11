use yew::prelude::*;
use crate::app::View;

#[derive(Properties, PartialEq)]
pub struct SidebarProps {
    pub current_view: View,
    pub on_navigate: Callback<View>,
}

#[function_component(Sidebar)]
pub fn sidebar(props: &SidebarProps) -> Html {
    let menu_open = use_state(|| false);

    let on_nav = |view: View, cb: Callback<View>| {
        Callback::from(move |_| cb.emit(view.clone()))
    };

    let toggle_menu = {
        let menu_open = menu_open.clone();
        Callback::from(move |_| menu_open.set(!*menu_open))
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
                    <span id="account-avatar" class="h-8 w-8 shrink-0 rounded-full bg-primary-solid flex items-center justify-center text-sm font-semibold">{"?"}</span>
                    <span class="min-w-0 flex-1">
                        <span id="account-name" class="block truncate text-sm font-semibold">{"Account"}</span>
                        <span id="account-role" class="block truncate text-xs text-muted">{"User"}</span>
                    </span>
                    <iconify-icon icon="radix-icons:chevron-up" class="radix-icon shrink-0" aria-hidden="true"></iconify-icon>
                </button>
                <div id="account-menu" class={classes!("absolute", "bottom-12", "left-0", "right-0", "z-50", "surface", "border", "rounded-md", "p-1", "shadow-lg", (!*menu_open).then_some("hidden"))}>
                    <button id="account-settings-btn" type="button" class="w-full rounded px-3 py-2 text-sm text-left hover:bg-[var(--surface-muted)] flex items-center gap-2">
                        <iconify-icon icon="radix-icons:person" class="radix-icon" aria-hidden="true"></iconify-icon>
                        <span>{"Account Settings"}</span>
                    </button>
                    <button id="account-refresh-btn" type="button" class="w-full rounded px-3 py-2 text-sm text-left hover:bg-[var(--surface-muted)] flex items-center gap-2">
                        <iconify-icon icon="radix-icons:reload" class="radix-icon" aria-hidden="true"></iconify-icon>
                        <span>{"Refresh"}</span>
                    </button>
                    <button id="logout-btn" type="button" class="w-full rounded px-3 py-2 text-sm text-left hover:bg-[var(--surface-muted)] flex items-center gap-2">
                        <iconify-icon icon="radix-icons:exit" class="radix-icon" aria-hidden="true"></iconify-icon>
                        <span>{"Logout"}</span>
                    </button>
                </div>
            </div>
        </nav>
    }
}
