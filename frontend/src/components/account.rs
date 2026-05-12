use crate::api::toast;
use crate::passkeys::registerPasskey;
use crate::state::{AppAction, AppState, UserProfile};
use gloo_file::{callbacks::FileReader, File};
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};
use web_sys::HtmlInputElement;
use yew::prelude::*;

#[derive(Deserialize, Clone, PartialEq)]
pub struct PasskeyInfo {
    pub id: String,
    pub name: String,
}

#[derive(Serialize)]
struct UpdateMeReq {
    display_name: Option<String>,
    avatar_data: Option<String>,
    password: Option<String>,
}

#[function_component(AccountSettings)]
pub fn account_settings() -> Html {
    let app_state = use_context::<UseReducerHandle<AppState>>().unwrap();
    let passkeys = use_state(Vec::<PasskeyInfo>::new);

    let display_name = use_state(|| {
        app_state
            .user
            .as_ref()
            .and_then(|u| u.display_name.clone())
            .unwrap_or_default()
    });
    let avatar_data = use_state(|| {
        app_state
            .user
            .as_ref()
            .and_then(|u| u.avatar_data.clone())
            .unwrap_or_default()
    });
    let password = use_state(String::new);
    let avatar_reader = use_state(|| None::<FileReader>);

    let refresh_passkeys = {
        let passkeys = passkeys.clone();
        Callback::from(move |_| {
            let passkeys = passkeys.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(res) = Request::get("/auth/passkeys").send().await {
                    if let Ok(data) = res.json::<Vec<PasskeyInfo>>().await {
                        passkeys.set(data);
                    }
                }
            });
        })
    };

    {
        let refresh_passkeys = refresh_passkeys.clone();
        use_effect_with((), move |_| {
            refresh_passkeys.emit(());
            || ()
        });
    }

    let on_save_profile = {
        let app_state = app_state.clone();
        let display_name = display_name.clone();
        let avatar_data = avatar_data.clone();
        let password = password.clone();

        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let app_state = app_state.clone();
            let dname = (*display_name).clone();
            let av = (*avatar_data).clone();
            let pass = (*password).clone();

            wasm_bindgen_futures::spawn_local(async move {
                let req = UpdateMeReq {
                    display_name: if dname.is_empty() { None } else { Some(dname) },
                    avatar_data: if av.is_empty() { None } else { Some(av) },
                    password: if pass.is_empty() { None } else { Some(pass) },
                };
                if let Ok(res) = Request::patch("/auth/me").json(&req).unwrap().send().await {
                    if res.ok() {
                        if let Ok(user) = res.json::<UserProfile>().await {
                            app_state.dispatch(AppAction::SetUser(Some(user)));
                            toast(&app_state, "Profile updated");
                        }
                    } else {
                        let err = res.text().await.unwrap_or_default();
                        toast(&app_state, err);
                    }
                }
            });
        })
    };

    let on_add_passkey = {
        let app_state = app_state.clone();
        let refresh_passkeys = refresh_passkeys.clone();
        Callback::from(move |_| {
            let app_state = app_state.clone();
            let refresh_passkeys = refresh_passkeys.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let challenge_res =
                    match Request::post("/auth/passkeys/register/start").send().await {
                        Ok(r) if r.ok() => r.text().await.unwrap_or_default(),
                        _ => {
                            toast(&app_state, "Failed to start passkey registration");
                            return;
                        }
                    };

                let credential_json = match registerPasskey(&challenge_res).await {
                    Ok(val) => val.as_string().unwrap_or_default(),
                    Err(e) => {
                        let err_msg = if let Some(err) = e.as_string() {
                            err
                        } else {
                            "Passkey error".to_string()
                        };
                        toast(&app_state, err_msg);
                        return;
                    }
                };

                match Request::post("/auth/passkeys/register/finish")
                    .header("Content-Type", "application/json")
                    .body(credential_json)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        toast(&app_state, "Passkey added");
                        refresh_passkeys.emit(());
                    }
                    Ok(res) => {
                        let err = res.text().await.unwrap_or_default();
                        toast(&app_state, err);
                    }
                    Err(_) => {}
                }
            });
        })
    };

    let on_delete_passkey = |id: String| {
        let app_state = app_state.clone();
        let refresh_passkeys = refresh_passkeys.clone();
        Callback::from(move |_| {
            if web_sys::window()
                .unwrap()
                .confirm_with_message("Delete this passkey?")
                .unwrap()
            {
                let app_state = app_state.clone();
                let refresh_passkeys = refresh_passkeys.clone();
                let id = id.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    if Request::delete(&format!("/auth/passkeys/{}", id))
                        .send()
                        .await
                        .is_ok()
                    {
                        toast(&app_state, "Passkey deleted");
                        refresh_passkeys.emit(());
                    }
                });
            }
        })
    };

    let on_avatar_file = {
        let avatar_data = avatar_data.clone();
        let avatar_reader = avatar_reader.clone();
        Callback::from(move |e: Event| {
            let Some(input) = e.target_dyn_into::<HtmlInputElement>() else {
                return;
            };
            let Some(files) = input.files() else {
                return;
            };
            let Some(file) = files.get(0) else {
                return;
            };
            let avatar_data = avatar_data.clone();
            let reader = gloo_file::callbacks::read_as_data_url(&File::from(file), move |result| {
                if let Ok(data) = result {
                    avatar_data.set(data);
                }
            });
            avatar_reader.set(Some(reader));
        })
    };

    let on_delete_account = {
        let app_state = app_state.clone();
        Callback::from(move |_| {
            if web_sys::window()
                .and_then(|w| {
                    w.confirm_with_message("Delete this account and all data?")
                        .ok()
                })
                .unwrap_or(false)
            {
                let app_state = app_state.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match Request::delete("/auth/me").send().await {
                        Ok(res) if res.ok() => {
                            app_state.dispatch(AppAction::SetAuthed(false));
                            app_state.dispatch(AppAction::SetUser(None));
                            toast(&app_state, "Account deleted");
                        }
                        Ok(res) => toast(
                            &app_state,
                            res.text()
                                .await
                                .unwrap_or_else(|_| "Failed to delete account".to_string()),
                        ),
                        Err(err) => toast(&app_state, err.to_string()),
                    }
                });
            }
        })
    };

    html! {
        <section id="view-account-settings">
            <h1 class="text-xl font-semibold tracking-tight mb-4">{"Account Settings"}</h1>
            <div class="grid grid-cols-1 xl:grid-cols-2 gap-4 items-start">
                <form onsubmit={on_save_profile} class="surface border rounded-md p-4 space-y-4">
                    <div class="flex items-center gap-4 mb-2">
                        <div class="h-16 w-16 shrink-0 rounded-full bg-primary-solid flex items-center justify-center text-xl font-semibold overflow-hidden">
                            if !(*avatar_data).is_empty() {
                                <img src={(*avatar_data).clone()} class="h-full w-full object-cover" />
                            } else {
                                {app_state.user.as_ref().map(|u| u.username.chars().next().unwrap_or('?').to_uppercase().to_string()).unwrap_or_default()}
                            }
                        </div>
                        <div>
                            <h2 class="text-lg font-semibold">{app_state.user.as_ref().map(|u| u.username.clone()).unwrap_or_default()}</h2>
                            <p class="text-sm text-muted">{"Profile details"}</p>
                        </div>
                    </div>
                    <div>
                        <label class="block card-kicker mb-2">{"Display Name"}</label>
                        <input value={(*display_name).clone()} oninput={Callback::from({let d = display_name.clone(); move |e: InputEvent| if let Some(t) = e.target_dyn_into::<HtmlInputElement>() { d.set(t.value()); }})} class="w-full rounded-md border px-3 py-2" placeholder="Leave blank to use username" />
                    </div>
                    <div>
                        <label class="block card-kicker mb-2">{"Avatar"}</label>
                        <div class="flex flex-col sm:flex-row gap-2">
                            <label class="rounded-md border border-token px-3 py-2 font-medium cursor-pointer inline-flex items-center gap-2">
                                <iconify-icon icon="radix-icons:image" class="radix-icon"></iconify-icon>
                                <span>{"Upload"}</span>
                                <input type="file" accept="image/*" class="hidden" onchange={on_avatar_file} />
                            </label>
                            <button type="button" class="rounded-md border border-token px-3 py-2 font-medium" onclick={Callback::from({ let avatar_data = avatar_data.clone(); move |_| avatar_data.set(String::new()) })}>{"Remove"}</button>
                        </div>
                    </div>
                    <div>
                        <label class="block card-kicker mb-2">{"Change Password"}</label>
                        <input value={(*password).clone()} oninput={Callback::from({let p = password.clone(); move |e: InputEvent| if let Some(t) = e.target_dyn_into::<HtmlInputElement>() { p.set(t.value()); }})} type="password" class="w-full rounded-md border px-3 py-2" placeholder="Leave blank to keep current password" />
                    </div>
                    <div class="flex flex-col sm:flex-row gap-2 sm:items-center sm:justify-between">
                        <button type="submit" class="rounded-md bg-primary-solid px-4 py-2 font-medium">{"Save Profile"}</button>
                        <button type="button" onclick={on_delete_account} class="rounded-md border border-token px-4 py-2 font-medium text-danger">{"Delete Account"}</button>
                    </div>
                </form>

                <div class="surface border rounded-md p-4 space-y-4">
                    <div class="flex items-center justify-between gap-3 mb-2">
                        <div>
                            <h2 class="text-lg font-semibold">{"Passkeys"}</h2>
                            <p class="text-sm text-muted">{"Sign in with biometrics or a security key"}</p>
                        </div>
                        <button type="button" onclick={on_add_passkey} class="rounded-md border border-token px-3 py-2 font-medium flex items-center gap-2">
                            <iconify-icon icon="radix-icons:plus" class="radix-icon"></iconify-icon>
                            <span>{"Add Passkey"}</span>
                        </button>
                    </div>
                    <div class="space-y-2">
                        {
                            if passkeys.is_empty() {
                                html! { <div class="text-sm text-muted py-2">{"No passkeys registered."}</div> }
                            } else {
                                passkeys.iter().map(|pk| {
                                    let id = pk.id.clone();
                                    html! {
                                        <div class="flex items-center justify-between p-2 rounded-md muted-surface border border-token">
                                            <div class="flex items-center gap-2">
                                                <iconify-icon icon="radix-icons:lock-closed" class="text-muted"></iconify-icon>
                                                <span class="text-sm font-medium">{format!("Passkey {}", pk.name)}</span>
                                            </div>
                                            <button onclick={on_delete_passkey(id)} class="text-danger hover:text-red-600 p-1">
                                                <iconify-icon icon="radix-icons:trash" class="radix-icon"></iconify-icon>
                                            </button>
                                        </div>
                                    }
                                }).collect::<Html>()
                            }
                        }
                    </div>
                </div>
            </div>
        </section>
    }
}
