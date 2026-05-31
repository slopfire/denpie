use crate::api::{toast, toast_key};
use crate::i18n::use_i18n;
use crate::state::AppState;
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};
use web_sys::HtmlInputElement;
use yew::prelude::*;

#[derive(Deserialize, Clone, PartialEq)]
pub struct ApiKeyInfo {
    pub id: i64,
    pub client_name: String,
    pub created_at: String,
}

#[derive(Serialize)]
struct CreateKeyReq {
    client_name: Option<String>,
}

#[derive(Serialize)]
struct DeleteKeyReq {
    id: i64,
}

#[function_component(ApiKeys)]
pub fn api_keys() -> Html {
    let app_state = use_context::<UseReducerHandle<AppState>>().unwrap();
    let i18n = use_i18n();
    let keys = use_state(Vec::<ApiKeyInfo>::new);
    let new_key = use_state(|| None::<String>);
    let key_name_input = use_state(String::new);

    let refresh_keys = {
        let keys = keys.clone();
        Callback::from(move |_| {
            let keys = keys.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(res) = Request::get("/admin/keys").send().await {
                    if let Ok(data) = res.json::<Vec<ApiKeyInfo>>().await {
                        keys.set(data);
                    }
                }
            });
        })
    };

    {
        let refresh_keys = refresh_keys.clone();
        use_effect_with((), move |_| {
            refresh_keys.emit(());
            || ()
        });
    }

    let on_submit = {
        let app_state = app_state.clone();
        let key_name_input = key_name_input.clone();
        let new_key = new_key.clone();
        let refresh_keys = refresh_keys.clone();
        let i18n = i18n.clone();

        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let app_state = app_state.clone();
            let key_name = (*key_name_input).clone();
            let new_key = new_key.clone();
            let refresh_keys = refresh_keys.clone();
            let i18n = i18n.clone();

            wasm_bindgen_futures::spawn_local(async move {
                let req = CreateKeyReq {
                    client_name: if key_name.is_empty() {
                        None
                    } else {
                        Some(key_name)
                    },
                };
                match Request::post("/admin/keys")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        if let Ok(key) = res.json::<String>().await {
                            new_key.set(Some(key));
                            toast_key(&app_state, &i18n, "toast.key_generated");
                            refresh_keys.emit(());
                        }
                    }
                    Ok(res) => {
                        let msg = res
                            .text()
                            .await
                            .unwrap_or_else(|_| i18n.t("toast.failed_generate_key"));
                        toast(&app_state, msg);
                    }
                    Err(e) => {
                        toast(&app_state, e.to_string());
                    }
                }
            });
        })
    };

    let on_delete = |id: i64| {
        let app_state = app_state.clone();
        let refresh_keys = refresh_keys.clone();
        let i18n = i18n.clone();
        Callback::from(move |_| {
            if web_sys::window()
                .unwrap()
                .confirm_with_message(&i18n.t("confirm.delete_api_key"))
                .unwrap()
            {
                let app_state = app_state.clone();
                let refresh_keys = refresh_keys.clone();
                let i18n = i18n.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let req = DeleteKeyReq { id };
                    if Request::delete("/admin/keys")
                        .json(&req)
                        .unwrap()
                        .send()
                        .await
                        .is_ok()
                    {
                        toast_key(&app_state, &i18n, "toast.key_deleted");
                        refresh_keys.emit(());
                    }
                });
            }
        })
    };

    html! {
        <section id="view-keys">
            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-3 mb-4">
                <h1 class="text-xl font-semibold tracking-tight">
                    {i18n.t("api_keys.title")}
                </h1>
                <form id="key-form" onsubmit={on_submit} class="flex w-full flex-col gap-2 sm:w-auto sm:flex-row">
                    <input
                        id="key-name"
                        class="min-w-0 rounded-md border px-4 py-2 sm:w-56"
                        placeholder="desktop_widget"
                        aria-label={i18n.t("api_keys.client_name_label")}
                        value={(*key_name_input).clone()}
                        oninput={Callback::from(move |e: InputEvent| {
                            if let Some(target) = e.target_dyn_into::<HtmlInputElement>() {
                                key_name_input.set(target.value());
                            }
                        })}
                    />
                    <button type="submit" class="flex w-full items-center justify-center gap-2 rounded-md bg-primary-solid px-4 py-2 font-medium sm:w-auto">
                        <iconify-icon icon="radix-icons:plus" class="radix-icon" aria-hidden="true"></iconify-icon>
                        {i18n.t("api_keys.generate")}
                    </button>
                </form>
            </div>

            if let Some(key) = &*new_key {
                <div id="new-key-box" class="surface border rounded-md p-4 mb-4">
                    <div class="card-kicker mb-2">{i18n.t("api_keys.new_key")}</div>
                    <code id="new-key" class="block rounded-md muted-surface border p-3 break-all">{key}</code>
                </div>
            }

            <div id="keys-list" class="grid grid-cols-1 lg:grid-cols-2 gap-3">
                {
                    if keys.is_empty() {
                        html! { <div class="col-span-full surface border rounded-md p-10 text-center text-muted">{i18n.t("api_keys.empty")}</div> }
                    } else {
                        html! {
                            for keys.iter().map(|k| html! {
                                <div class="surface border rounded-md p-4 flex justify-between items-center">
                                    <div>
                                        <div class="font-semibold">{&k.client_name}</div>
                                        <div class="text-xs text-muted mt-1">{&k.created_at}</div>
                                    </div>
                                    <button onclick={on_delete(k.id)} class="text-danger hover:underline">{i18n.t("common.delete")}</button>
                                </div>
                            })
                        }
                    }
                }
            </div>
        </section>
    }
}
