use crate::api::toast;
use crate::i18n::use_i18n;
use crate::state::{AppAction, AppState};
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};
use web_sys::HtmlInputElement;
use yew::prelude::*;

#[derive(Deserialize, Clone, PartialEq)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub role: String,
    pub display_name: Option<String>,
    pub created_at: String,
}

#[derive(Serialize)]
struct CreateUserReq {
    username: String,
    password: String,
    role: String,
    display_name: Option<String>,
}

#[derive(Serialize)]
struct UpdateUserReq {
    role: Option<String>,
    password: Option<String>,
}

#[function_component(AdminShell)]
pub fn admin_shell() -> Html {
    let app_state = use_context::<UseReducerHandle<AppState>>().unwrap();
    let i18n = use_i18n();
    let users = use_state(Vec::<UserInfo>::new);
    let loading = use_state(|| false);
    let error = use_state(String::new);

    // Create-user form state
    let new_username = use_state(String::new);
    let new_password = use_state(String::new);
    let new_display_name = use_state(String::new);
    let new_role = use_state(|| "user".to_string());
    let creating = use_state(|| false);

    // Edit-user state
    let editing_id = use_state(|| None::<String>);
    let edit_role = use_state(String::new);
    let edit_password = use_state(String::new);
    let saving = use_state(|| false);

    let current_user_id = app_state.user.as_ref().map(|u| u.id.clone());

    let refresh_users = {
        let users = users.clone();
        let loading = loading.clone();
        let error = error.clone();
        Callback::from(move |_| {
            let users = users.clone();
            let loading = loading.clone();
            let error = error.clone();
            wasm_bindgen_futures::spawn_local(async move {
                loading.set(true);
                error.set(String::new());
                match Request::get("/admin/users").send().await {
                    Ok(res) if res.ok() => {
                        if let Ok(data) = res.json::<Vec<UserInfo>>().await {
                            users.set(data);
                        }
                    }
                    Ok(res) => {
                        error.set(format!("HTTP {}", res.status()));
                    }
                    Err(e) => {
                        error.set(e.to_string());
                    }
                }
                loading.set(false);
            });
        })
    };

    {
        let refresh_users = refresh_users.clone();
        use_effect_with((), move |_| {
            refresh_users.emit(());
            || ()
        });
    }

    let on_create = {
        let app_state = app_state.clone();
        let new_username = new_username.clone();
        let new_password = new_password.clone();
        let new_display_name = new_display_name.clone();
        let new_role = new_role.clone();
        let creating = creating.clone();
        let refresh_users = refresh_users.clone();
        let i18n = i18n.clone();

        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let app_state = app_state.clone();
            let username = (*new_username).clone();
            let password = (*new_password).clone();
            let display_name = (*new_display_name).clone();
            let role = (*new_role).clone();
            let creating = creating.clone();
            let refresh_users = refresh_users.clone();
            let i18n = i18n.clone();

            if username.trim().is_empty() || password.len() < 8 {
                toast(&app_state, i18n.t("admin.validation_username_password"));
                return;
            }

            let new_username = new_username.clone();
            let new_password = new_password.clone();
            let new_display_name = new_display_name.clone();
            let new_role = new_role.clone();
            wasm_bindgen_futures::spawn_local(async move {
                creating.set(true);
                let req = CreateUserReq {
                    username: username.trim().to_string(),
                    password: password.clone(),
                    role: role.clone(),
                    display_name: if display_name.trim().is_empty() {
                        None
                    } else {
                        Some(display_name.trim().to_string())
                    },
                };
                match Request::post("/admin/users")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        new_username.set(String::new());
                        new_password.set(String::new());
                        new_display_name.set(String::new());
                        new_role.set("user".to_string());
                        toast(&app_state, i18n.t("admin.user_created"));
                        refresh_users.emit(());
                    }
                    Ok(res) => {
                        let msg = res
                            .text()
                            .await
                            .unwrap_or_else(|_| i18n.t("admin.create_failed"));
                        toast(&app_state, msg);
                    }
                    Err(e) => toast(&app_state, e.to_string()),
                }
                creating.set(false);
            });
        })
    };

    let start_edit = {
        let editing_id = editing_id.clone();
        let edit_role = edit_role.clone();
        let edit_password = edit_password.clone();
        Callback::from(move |user: UserInfo| {
            editing_id.set(Some(user.id));
            edit_role.set(user.role);
            edit_password.set(String::new());
        })
    };

    let cancel_edit = {
        let editing_id = editing_id.clone();
        let edit_password = edit_password.clone();
        Callback::from(move |_| {
            editing_id.set(None);
            edit_password.set(String::new());
        })
    };

    let save_edit = {
        let app_state = app_state.clone();
        let editing_id = editing_id.clone();
        let edit_role = edit_role.clone();
        let edit_password = edit_password.clone();
        let saving = saving.clone();
        let refresh_users = refresh_users.clone();
        let i18n = i18n.clone();

        Callback::from(move |_| {
            let app_state = app_state.clone();
            let id = match (*editing_id).clone() {
                Some(id) => id,
                None => return,
            };
            let role = (*edit_role).clone();
            let password = (*edit_password).clone();
            let saving = saving.clone();
            let refresh_users = refresh_users.clone();
            let editing_id = editing_id.clone();
            let edit_password = edit_password.clone();
            let i18n = i18n.clone();

            wasm_bindgen_futures::spawn_local(async move {
                saving.set(true);
                let req = UpdateUserReq {
                    role: Some(role),
                    password: if password.is_empty() {
                        None
                    } else {
                        Some(password)
                    },
                };
                match Request::patch(&format!("/admin/users/{}", id))
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        toast(&app_state, i18n.t("admin.user_updated"));
                        editing_id.set(None);
                        edit_password.set(String::new());
                        refresh_users.emit(());
                    }
                    Ok(res) => {
                        let msg = res
                            .text()
                            .await
                            .unwrap_or_else(|_| i18n.t("admin.update_failed"));
                        toast(&app_state, msg);
                    }
                    Err(e) => toast(&app_state, e.to_string()),
                }
                saving.set(false);
            });
        })
    };

    let on_delete = {
        let app_state = app_state.clone();
        let refresh_users = refresh_users.clone();
        let i18n = i18n.clone();
        let current_user_id = current_user_id.clone();

        Callback::from(move |user: UserInfo| {
            let app_state = app_state.clone();
            let refresh_users = refresh_users.clone();
            let i18n = i18n.clone();
            let current_user_id = current_user_id.clone();

            if Some(user.id.clone()) == current_user_id {
                toast(&app_state, i18n.t("admin.cannot_delete_self"));
                return;
            }

            let confirm_msg = format!("{} {}?", i18n.t("admin.confirm_delete"), user.username);
            if !web_sys::window()
                .unwrap()
                .confirm_with_message(&confirm_msg)
                .unwrap()
            {
                return;
            }

            wasm_bindgen_futures::spawn_local(async move {
                match Request::delete(&format!("/admin/users/{}", user.id))
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        toast(&app_state, i18n.t("admin.user_deleted"));
                        refresh_users.emit(());
                    }
                    Ok(res) => {
                        let msg = res
                            .text()
                            .await
                            .unwrap_or_else(|_| i18n.t("admin.delete_failed"));
                        toast(&app_state, msg);
                    }
                    Err(e) => toast(&app_state, e.to_string()),
                }
            });
        })
    };

    let logout = {
        let app_state = app_state.clone();
        Callback::from(move |_| {
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if Request::post("/auth/logout").send().await.is_ok() {
                    app_state.dispatch(AppAction::SetSession(None));
                }
            });
        })
    };

    let admin_name = app_state
        .user
        .as_ref()
        .map(|u| u.display_name.clone().unwrap_or(u.username.clone()))
        .unwrap_or_else(|| i18n.t("account.fallback_name"));

    html! {
        <div class="min-h-screen flex flex-col lg:flex-row">
            // Admin sidebar — separate from standard app nav
            <nav class="hidden lg:flex fixed left-0 top-0 z-50 h-full w-56 flex-col border-r surface p-4 rounded-none">
                <div class="flex items-center gap-2 px-2 py-2 mb-4">
                    <iconify-icon icon="radix-icons:gear" class="radix-icon shrink-0 text-primary text-xl" aria-hidden="true"></iconify-icon>
                    <div class="truncate text-lg font-semibold tracking-tight">{i18n.t("admin.title")}</div>
                </div>
                <div class="space-y-1 flex-1">
                    <div class="nav-item w-full grid grid-cols-[1.5rem_minmax(0,1fr)] items-center gap-3 rounded-md px-3 py-2 text-sm font-semibold text-left active">
                        <iconify-icon icon="radix-icons:person" class="radix-icon justify-self-center"></iconify-icon>
                        <span class="justify-self-start text-left">{i18n.t("admin.users")}</span>
                    </div>
                </div>
                <div class="border-t border-token pt-3 mt-3">
                    <button
                        type="button"
                        onclick={let app_state = app_state.clone(); Callback::from(move |_| app_state.dispatch(AppAction::SetAdminMode(false)))}
                        class="nav-item w-full grid grid-cols-[1.5rem_minmax(0,1fr)] items-center gap-3 rounded-md px-3 py-2 text-sm font-semibold text-left"
                    >
                        <iconify-icon icon="radix-icons:arrow-left" class="radix-icon justify-self-center"></iconify-icon>
                        <span class="justify-self-start text-left">{i18n.t("admin.switch_to_app")}</span>
                    </button>
                </div>
                <div class="border-t border-token pt-3">
                    <div class="px-3 py-2 text-xs text-muted truncate">{admin_name}</div>
                    <button
                        type="button"
                        onclick={logout.clone()}
                        class="account-menu-item account-menu-item--danger w-full rounded-md px-3 py-2 text-sm font-semibold text-left"
                    >
                        <iconify-icon icon="radix-icons:exit" class="radix-icon" aria-hidden="true"></iconify-icon>
                        <span>{i18n.t("nav.logout")}</span>
                    </button>
                </div>
            </nav>

            // Mobile header — visible only on small screens since sidebar is hidden
            <header class="lg:hidden sticky top-0 z-50 surface border-b px-4 py-3 flex items-center justify-between">
                <div class="flex items-center gap-2">
                    <iconify-icon icon="radix-icons:gear" class="radix-icon text-primary text-lg" aria-hidden="true"></iconify-icon>
                    <span class="font-semibold text-sm">{i18n.t("admin.title")}</span>
                </div>
                <div class="flex items-center gap-2">
                    <button
                        type="button"
                        onclick={let app_state = app_state.clone(); Callback::from(move |_| app_state.dispatch(AppAction::SetAdminMode(false)))}
                        class="rounded-md border border-token px-3 py-1.5 text-xs font-semibold"
                    >
                        {i18n.t("admin.switch_to_app")}
                    </button>
                    <button
                        type="button"
                        onclick={logout.clone()}
                        class="rounded-md text-danger border border-token px-3 py-1.5 text-xs font-semibold"
                    >
                        <iconify-icon icon="radix-icons:exit" class="radix-icon" aria-hidden="true"></iconify-icon>
                    </button>
                </div>
            </header>

            <main class="lg:ml-56 px-4 sm:px-6 lg:px-6 py-5 pb-20 max-w-none flex-1">
                <div class="mb-6">
                    <h1 class="text-xl font-semibold tracking-tight">{i18n.t("admin.user_management")}</h1>
                    <p class="text-muted mt-2">{i18n.t("admin.subtitle")}</p>
                </div>

                // Create user form
                <div class="surface border rounded-md p-4 mb-6">
                    <h2 class="text-sm font-semibold mb-3 card-kicker">{i18n.t("admin.create_user")}</h2>
                    <form onsubmit={on_create} class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
                        <div>
                            <label class="block text-xs text-muted mb-1">{i18n.t("auth.username")}</label>
                            <input
                                class="w-full rounded-md border px-3 py-2 text-sm"
                                value={(*new_username).clone()}
                                oninput={Callback::from({
                                    let s = new_username.clone();
                                    move |e: InputEvent| if let Some(t) = e.target_dyn_into::<HtmlInputElement>() { s.set(t.value()); }
                                })}
                                placeholder="alice"
                            />
                        </div>
                        <div>
                            <label class="block text-xs text-muted mb-1">{i18n.t("auth.password")}</label>
                            <input
                                type="password"
                                class="w-full rounded-md border px-3 py-2 text-sm"
                                value={(*new_password).clone()}
                                oninput={Callback::from({
                                    let s = new_password.clone();
                                    move |e: InputEvent| if let Some(t) = e.target_dyn_into::<HtmlInputElement>() { s.set(t.value()); }
                                })}
                                placeholder="********"
                            />
                        </div>
                        <div>
                            <label class="block text-xs text-muted mb-1">{i18n.t("account.settings")}</label>
                            <input
                                class="w-full rounded-md border px-3 py-2 text-sm"
                                value={(*new_display_name).clone()}
                                oninput={Callback::from({
                                    let s = new_display_name.clone();
                                    move |e: InputEvent| if let Some(t) = e.target_dyn_into::<HtmlInputElement>() { s.set(t.value()); }
                                })}
                                placeholder={i18n.t("admin.display_name_optional")}
                            />
                        </div>
                        <div>
                            <label class="block text-xs text-muted mb-1">{i18n.t("admin.role")}</label>
                            <select
                                class="w-full rounded-md border px-3 py-2 text-sm"
                                value={(*new_role).clone()}
                                onchange={Callback::from({
                                    let s = new_role.clone();
                                    move |e: Event| {
                                        if let Some(t) = e.target_dyn_into::<web_sys::HtmlSelectElement>() {
                                            s.set(t.value());
                                        }
                                    }
                                })}
                            >
                                <option value="user">{i18n.t("role.user")}</option>
                                <option value="admin">{i18n.t("role.admin")}</option>
                            </select>
                        </div>
                        <div class="sm:col-span-2 lg:col-span-4">
                            <button
                                type="submit"
                                disabled={*creating}
                                class="rounded-md bg-primary-solid px-4 py-2 font-medium text-sm disabled:opacity-50"
                            >
                                {if *creating { i18n.t("admin.creating") } else { i18n.t("admin.create_user") }}
                            </button>
                        </div>
                    </form>
                </div>

                // Error banner
                if !(*error).is_empty() {
                    <div class="surface border border-danger rounded-md p-3 mb-4 text-sm text-danger">{&*error}</div>
                }

                // Users table
                <div class="surface border rounded-md overflow-hidden">
                    <div class="overflow-x-auto">
                        <table class="w-full text-sm">
                            <thead>
                                <tr class="border-b border-token text-left text-xs text-muted">
                                    <th class="px-4 py-3 font-semibold">{i18n.t("auth.username")}</th>
                                    <th class="px-4 py-3 font-semibold">{i18n.t("admin.role")}</th>
                                    <th class="px-4 py-3 font-semibold">{i18n.t("admin.created_at")}</th>
                                    <th class="px-4 py-3 font-semibold text-right">{i18n.t("admin.actions")}</th>
                                </tr>
                            </thead>
                            <tbody>
                                {
                                    if *loading && users.is_empty() {
                                        html! {
                                            <tr><td colspan="4" class="px-4 py-8 text-center text-muted">{i18n.t("admin.loading")}</td></tr>
                                        }
                                    } else if users.is_empty() {
                                        html! {
                                            <tr><td colspan="4" class="px-4 py-8 text-center text-muted">{i18n.t("admin.no_users")}</td></tr>
                                        }
                                    } else {
                                        html! {
                                            for users.iter().map(|user| {
                                                let user = user.clone();
                                                let is_editing = (*editing_id).as_ref() == Some(&user.id);
                                                let is_self = current_user_id.as_ref() == Some(&user.id);

                                                html! {
                                                    <tr class="border-b border-token last:border-0">
                                                        <td class="px-4 py-3">
                                                            <div class="font-semibold">{&user.username}</div>
                                                            if let Some(name) = &user.display_name {
                                                                <div class="text-xs text-muted">{name}</div>
                                                            }
                                                            if is_self {
                                                                <div class="text-xs text-primary mt-0.5">{i18n.t("admin.you")}</div>
                                                            }
                                                        </td>
                                                        <td class="px-4 py-3">
                                                            if is_editing {
                                                                <select
                                                                    class="rounded-md border px-2 py-1 text-sm"
                                                                    value={(*edit_role).clone()}
                                                                    onchange={Callback::from({
                                                                        let s = edit_role.clone();
                                                                        move |e: Event| {
                                                                            if let Some(t) = e.target_dyn_into::<web_sys::HtmlSelectElement>() {
                                                                                s.set(t.value());
                                                                            }
                                                                        }
                                                                    })}
                                                                >
                                                                    <option value="user">{i18n.t("role.user")}</option>
                                                                    <option value="admin">{i18n.t("role.admin")}</option>
                                                                </select>
                                                            } else {
                                                                <span class={classes!("inline-flex", "items-center", "rounded-md", "px-2", "py-1", "text-xs", "font-semibold", if user.role == "admin" { vec!["bg-primary-solid", "text-white"] } else { vec!["bg-[var(--surface-muted)]"] })}>
                                                                    {if user.role == "admin" { i18n.t("role.admin") } else { i18n.t("role.user") }}
                                                                </span>
                                                            }
                                                        </td>
                                                        <td class="px-4 py-3 text-muted">{&user.created_at}</td>
                                                        <td class="px-4 py-3">
                                                            if is_editing {
                                                                <div class="flex flex-col gap-2">
                                                                    <input
                                                                        type="password"
                                                                        class="rounded-md border px-2 py-1 text-sm"
                                                                        placeholder={i18n.t("admin.new_password_optional")}
                                                                        value={(*edit_password).clone()}
                                                                        oninput={Callback::from({
                                                                            let s = edit_password.clone();
                                                                            move |e: InputEvent| if let Some(t) = e.target_dyn_into::<HtmlInputElement>() { s.set(t.value()); }
                                                                        })}
                                                                    />
                                                                    <div class="flex gap-2">
                                                                        <button
                                                                            type="button"
                                                                            onclick={save_edit.clone()}
                                                                            disabled={*saving}
                                                                            class="rounded-md bg-primary-solid px-3 py-1 text-xs font-medium disabled:opacity-50"
                                                                        >
                                                                            {i18n.t("common.save")}
                                                                        </button>
                                                                        <button
                                                                            type="button"
                                                                            onclick={cancel_edit.clone()}
                                                                            class="rounded-md border px-3 py-1 text-xs font-medium"
                                                                        >
                                                                            {i18n.t("common.cancel")}
                                                                        </button>
                                                                    </div>
                                                                </div>
                                                            } else {
                                                                <div class="flex justify-end gap-2">
                                                                    <button
                                                                        type="button"
                                                                        onclick={let start_edit = start_edit.clone(); let user = user.clone(); Callback::from(move |_| start_edit.emit(user.clone()))}
                                                                        class="text-primary hover:underline text-xs font-medium"
                                                                    >
                                                                        {i18n.t("admin.edit")}
                                                                    </button>
                                                                    <button
                                                                        type="button"
                                                                        onclick={let on_delete = on_delete.clone(); Callback::from(move |_| on_delete.emit(user.clone()))}
                                                                        disabled={is_self}
                                                                        class="text-danger hover:underline text-xs font-medium disabled:opacity-30 disabled:no-underline"
                                                                    >
                                                                        {i18n.t("common.delete")}
                                                                    </button>
                                                                </div>
                                                            }
                                                        </td>
                                                    </tr>
                                                }
                                            })
                                        }
                                    }
                                }
                            </tbody>
                        </table>
                    </div>
                </div>
            </main>
        </div>
    }
}
