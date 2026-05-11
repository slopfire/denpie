use yew::prelude::*;
use gloo_net::http::Request;
use serde::Deserialize;

#[derive(Deserialize, Clone, PartialEq)]
pub struct ApiKeyInfo {
    pub id: i64,
    pub client_name: String,
    pub created_at: String,
}

#[function_component(ApiKeys)]
pub fn api_keys() -> Html {
    let keys = use_state(Vec::<ApiKeyInfo>::new);
    let new_key = use_state(|| None::<String>);

    {
        let keys = keys.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(res) = Request::get("/admin/keys").send().await {
                    if let Ok(data) = res.json::<Vec<ApiKeyInfo>>().await {
                        keys.set(data);
                    }
                }
            });
            || ()
        });
    }

    html! {
        <section id="view-keys">
            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-3 mb-4">
                <h1 class="text-xl font-semibold tracking-tight">
                    {"API Keys"}
                </h1>
                <form id="key-form" class="flex w-full flex-col gap-2 sm:w-auto sm:flex-row">
                    <input id="key-name" class="min-w-0 rounded-md border px-4 py-2 sm:w-56" placeholder="desktop_widget" aria-label="API key client name" />
                    <button class="flex w-full items-center justify-center gap-2 rounded-md bg-primary-solid px-4 py-2 font-medium sm:w-auto">
                        <iconify-icon icon="radix-icons:plus" class="radix-icon" aria-hidden="true"></iconify-icon>
                        {"Generate"}
                    </button>
                </form>
            </div>
            
            if let Some(key) = &*new_key {
                <div id="new-key-box" class="surface border rounded-md p-4 mb-4">
                    <div class="card-kicker mb-2">{"New key"}</div>
                    <code id="new-key" class="block rounded-md muted-surface border p-3 break-all">{key}</code>
                </div>
            }

            <div id="keys-list" class="grid grid-cols-1 lg:grid-cols-2 gap-3">
                {
                    if keys.is_empty() {
                        html! { <div class="col-span-full surface border rounded-md p-10 text-center text-muted">{"No API keys generated."}</div> }
                    } else {
                        html! {
                            for keys.iter().map(|k| html! {
                                <div class="surface border rounded-md p-4 flex justify-between items-center">
                                    <div>
                                        <div class="font-semibold">{&k.client_name}</div>
                                        <div class="text-xs text-muted mt-1">{&k.created_at}</div>
                                    </div>
                                    <button class="text-danger hover:underline">{"Delete"}</button>
                                </div>
                            })
                        }
                    }
                }
            </div>
        </section>
    }
}
