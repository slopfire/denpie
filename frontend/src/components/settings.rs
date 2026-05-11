use crate::state::{AppAction, AppState};
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};
use web_sys::{HtmlInputElement, HtmlSelectElement, HtmlTextAreaElement};
use yew::prelude::*;

#[derive(Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct SettingsRes {
    pub server_version: String,
    pub build_sha: String,
    pub model: String,
    pub compress_model: String,
    pub template: String,
    pub api_key: String,
    pub base_url: String,
    pub compress_base_url: String,
    pub reasoning_effort: String,
    pub compress_reasoning_effort: String,
    pub compression_level: String,
    pub color_scheme: String,
    pub transparency: String,
    pub blur_intensity: String,
    pub autoupdate_enabled: bool,
    pub autoupdate_repo: String,
    pub autoupdate_branch: String,
    pub autoupdate_check_interval_secs: u64,
    pub autoupdate_command: String,
    pub autoupdate_last_seen_sha: String,
    pub daily_time_zone: String,
    pub daily_update_time: String,
    pub max_active_cards: u64,
}

#[derive(Serialize)]
struct ForceDailyRefreshRequest {
    topics: String,
    tipcard_type: Option<String>,
}

#[function_component(Settings)]
pub fn settings() -> Html {
    let app_state = use_context::<UseReducerHandle<AppState>>().unwrap();
    let settings = use_state(|| None::<SettingsRes>);

    {
        let settings = settings.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(res) = Request::get("/admin/settings").send().await {
                    if let Ok(data) = res.json::<SettingsRes>().await {
                        settings.set(Some(data));
                    }
                }
            });
            || ()
        });
    }

    let on_submit = {
        let app_state = app_state.clone();
        let settings = settings.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let app_state = app_state.clone();
            if let Some(s) = (*settings).clone() {
                wasm_bindgen_futures::spawn_local(async move {
                    if let Ok(res) = Request::post("/admin/settings").json(&s).unwrap().send().await {
                        if res.ok() {
                            app_state.dispatch(AppAction::ShowToast("Settings saved".to_string()));
                            let state_clone = app_state.clone();
                            gloo_timers::callback::Timeout::new(2400, move || {
                                state_clone.dispatch(AppAction::HideToast);
                            }).forget();
                        } else {
                            app_state.dispatch(AppAction::ShowToast("Failed to save".to_string()));
                            let state_clone = app_state.clone();
                            gloo_timers::callback::Timeout::new(2400, move || {
                                state_clone.dispatch(AppAction::HideToast);
                            }).forget();
                        }
                    }
                });
            }
        })
    };

    let on_force_refresh = {
        let app_state = app_state.clone();
        Callback::from(move |_| {
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = ForceDailyRefreshRequest {
                    topics: String::new(),
                    tipcard_type: None,
                };
                if let Ok(res) = Request::post("/app/daily-refresh").json(&req).unwrap().send().await {
                    if res.ok() {
                        app_state.dispatch(AppAction::ShowToast("Force refresh triggered".to_string()));
                        let state_clone = app_state.clone();
                        gloo_timers::callback::Timeout::new(2400, move || {
                            state_clone.dispatch(AppAction::HideToast);
                        }).forget();
                    } else {
                        app_state.dispatch(AppAction::ShowToast("Failed to refresh".to_string()));
                        let state_clone = app_state.clone();
                        gloo_timers::callback::Timeout::new(2400, move || {
                            state_clone.dispatch(AppAction::HideToast);
                        }).forget();
                    }
                }
            });
        })
    };

    let on_check_server = {
        let app_state = app_state.clone();
        Callback::from(move |_| {
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(res) = Request::post("/admin/autoupdate").send().await {
                    if res.ok() {
                        app_state.dispatch(AppAction::ShowToast("Autoupdate checked".to_string()));
                        let state_clone = app_state.clone();
                        gloo_timers::callback::Timeout::new(2400, move || {
                            state_clone.dispatch(AppAction::HideToast);
                        }).forget();
                    } else {
                        app_state.dispatch(AppAction::ShowToast("Failed to check updates".to_string()));
                        let state_clone = app_state.clone();
                        gloo_timers::callback::Timeout::new(2400, move || {
                            state_clone.dispatch(AppAction::HideToast);
                        }).forget();
                    }
                }
            });
        })
    };

    let s = (*settings).clone().unwrap_or_default();

    let on_input = |field: &'static str| {
        let settings = settings.clone();
        Callback::from(move |e: InputEvent| {
            if let Some(target) = e.target_dyn_into::<HtmlInputElement>() {
                if let Some(mut current) = (*settings).clone() {
                    match field {
                        "model" => current.model = target.value(),
                        "compress_model" => current.compress_model = target.value(),
                        "api_key" => current.api_key = target.value(),
                        "base_url" => current.base_url = target.value(),
                        "compress_base_url" => current.compress_base_url = target.value(),
                        "daily_time_zone" => current.daily_time_zone = target.value(),
                        "daily_update_time" => current.daily_update_time = target.value(),
                        "max_active_cards" => current.max_active_cards = target.value().parse().unwrap_or(0),
                        "autoupdate_repo" => current.autoupdate_repo = target.value(),
                        "autoupdate_branch" => current.autoupdate_branch = target.value(),
                        "autoupdate_check_interval_secs" => current.autoupdate_check_interval_secs = target.value().parse().unwrap_or(60),
                        _ => {}
                    }
                    settings.set(Some(current));
                }
            }
        })
    };

    let on_checkbox = |field: &'static str| {
        let settings = settings.clone();
        Callback::from(move |e: InputEvent| {
            if let Some(target) = e.target_dyn_into::<HtmlInputElement>() {
                if let Some(mut current) = (*settings).clone() {
                    if field == "autoupdate_enabled" {
                        current.autoupdate_enabled = target.checked();
                    }
                    settings.set(Some(current));
                }
            }
        })
    };

    let on_select = |field: &'static str| {
        let settings = settings.clone();
        Callback::from(move |e: Event| {
            if let Some(target) = e.target_dyn_into::<HtmlSelectElement>() {
                if let Some(mut current) = (*settings).clone() {
                    match field {
                        "reasoning_effort" => current.reasoning_effort = target.value(),
                        "compress_reasoning_effort" => current.compress_reasoning_effort = target.value(),
                        "compression_level" => current.compression_level = target.value(),
                        "color_scheme" => {
                            current.color_scheme = target.value();
                            if let Some(window) = web_sys::window() {
                                if let Some(document) = window.document() {
                                    if let Some(html) = document.document_element() {
                                        let _ = html.set_attribute("data-theme", &current.color_scheme);
                                    }
                                }
                            }
                        },
                        "transparency" => current.transparency = target.value(),
                        "blur_intensity" => current.blur_intensity = target.value(),
                        _ => {}
                    }
                    settings.set(Some(current));
                }
            }
        })
    };
    
    let on_textarea = |field: &'static str| {
        let settings = settings.clone();
        Callback::from(move |e: InputEvent| {
            if let Some(target) = e.target_dyn_into::<HtmlTextAreaElement>() {
                if let Some(mut current) = (*settings).clone() {
                    match field {
                        "template" => current.template = target.value(),
                        "autoupdate_command" => current.autoupdate_command = target.value(),
                        _ => {}
                    }
                    settings.set(Some(current));
                }
            }
        })
    };

    html! {
        <section id="view-settings">
            <h1 class="text-xl font-semibold tracking-tight mb-4">
                {"Settings"}
            </h1>
            <form id="settings-form" onsubmit={on_submit} class="surface border rounded-md p-4 max-w-5xl space-y-5">
                <div>
                    <label class="block card-kicker mb-2">{"LLM Model"}</label>
                    <input id="model-input" oninput={on_input("model")} value={s.model.clone()} class="w-full rounded-md border px-3 py-2" />
                </div>
                <div>
                    <label class="block card-kicker mb-2">{"Compression Model"}</label>
                    <input id="compress-model-input" oninput={on_input("compress_model")} value={s.compress_model.clone()} class="w-full rounded-md border px-3 py-2" />
                </div>
                <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                    <div>
                        <label class="block card-kicker mb-2">{"LLM Reasoning"}</label>
                        <select id="reasoning-effort-input" onchange={on_select("reasoning_effort")} value={s.reasoning_effort.clone()} class="w-full rounded-md border px-3 py-2">
                            <option value="none">{"None"}</option>
                            <option value="minimal">{"Minimal"}</option>
                            <option value="low">{"Low"}</option>
                            <option value="medium">{"Medium"}</option>
                            <option value="high">{"High"}</option>
                            <option value="xhigh">{"XHigh"}</option>
                        </select>
                    </div>
                    <div>
                        <label class="block card-kicker mb-2">{"Compression Level"}</label>
                        <select id="compression-level-input" onchange={on_select("compression_level")} value={s.compression_level.clone()} class="w-full rounded-md border px-3 py-2">
                            <option value="light">{"Light"}</option>
                            <option value="balanced">{"Balanced"}</option>
                            <option value="strong">{"Strong"}</option>
                            <option value="ultra">{"Ultra"}</option>
                        </select>
                    </div>
                </div>
                <div>
                    <label class="block card-kicker mb-2">{"Prompt Template"}</label>
                    <textarea id="template-input" oninput={on_textarea("template")} value={s.template.clone()} class="w-full rounded-md border px-3 py-2 h-20 resize-y"></textarea>
                </div>
                <div>
                    <label class="block card-kicker mb-2">{"LLM API Key"}</label>
                    <input id="api-key-input" oninput={on_input("api_key")} type="password" value={s.api_key.clone()} class="w-full rounded-md border px-3 py-2" />
                </div>
                <div>
                    <label class="block card-kicker mb-2">{"LLM Base URL"}</label>
                    <input id="base-url-input" oninput={on_input("base_url")} value={s.base_url.clone()} class="w-full rounded-md border px-3 py-2" />
                </div>
                <div>
                    <label class="block card-kicker mb-2">{"Compression Base URL"}</label>
                    <input id="compress-base-url-input" oninput={on_input("compress_base_url")} value={s.compress_base_url.clone()} class="w-full rounded-md border px-3 py-2" />
                </div>
                <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                    <div>
                        <label class="block card-kicker mb-2">{"Card Refresh Time Zone"}</label>
                        <input id="daily-time-zone-input" oninput={on_input("daily_time_zone")} list="time-zone-options" value={s.daily_time_zone.clone()} class="w-full rounded-md border px-3 py-2" placeholder="UTC" />
                    </div>
                    <div>
                        <label class="block card-kicker mb-2">{"Card Refresh Time"}</label>
                        <input id="daily-update-time-input" oninput={on_input("daily_update_time")} type="time" value={s.daily_update_time.clone()} class="w-full rounded-md border px-3 py-2" />
                    </div>
                </div>
                <div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
                    <div class="text-sm text-muted">{"Force-refresh generated topics now."}</div>
                    <button id="force-daily-refresh-btn" type="button" onclick={on_force_refresh} class="rounded-md border border-token px-4 py-2 font-medium flex items-center justify-center gap-2">
                        <iconify-icon icon="radix-icons:update" class="radix-icon" aria-hidden="true"></iconify-icon>
                        <span>{"Force Daily Refresh"}</span>
                    </button>
                </div>
                <div>
                    <label class="block card-kicker mb-2">{"Max Active Cards"}</label>
                    <input id="max-active-cards-input" oninput={on_input("max_active_cards")} type="number" min="0" step="1" value={s.max_active_cards.to_string()} class="w-full rounded-md border px-3 py-2" placeholder="0" />
                    <div class="mt-2 text-xs text-muted">{"0 means unlimited. When full, existing due cards still show but new cards are not created."}</div>
                </div>
                <div>
                    <label class="block card-kicker mb-2" for="theme-select-settings">{"Color Scheme"}</label>
                    <select
                        id="theme-select-settings"
                        value={s.color_scheme.clone()}
                        class="theme-select w-full rounded-md border px-3 py-2"
                        onchange={on_select("color_scheme")}
                    >
                        <option value="shadcn">{"Shadcn (Dark)"}</option>
                        <option value="shadcn-light">{"Shadcn (Light)"}</option>
                        <option value="carbonfox">{"Carbonfox"}</option>
                        <option value="ayu">{"Ayu"}</option>
                        <option value="solarized-light">{"Solarized Light"}</option>
                        <option value="solarized-dark">{"Solarized Dark"}</option>
                        <option value="amoled">{"AMOLED"}</option>
                        <option value="slate">{"Slate"}</option>
                    </select>
                </div>
                <div>
                    <label class="block card-kicker mb-2">{"Transparency"}</label>
                    <select id="transparency-input" onchange={on_select("transparency")} value={s.transparency.clone()} class="w-full rounded-md border px-3 py-2">
                        <option value="none">{"None"}</option>
                        <option value="low">{"Low"}</option>
                        <option value="medium">{"Medium"}</option>
                        <option value="full">{"Full"}</option>
                    </select>
                </div>
                <div>
                    <label class="block card-kicker mb-2">{"Blur Intensity"}</label>
                    <select id="blur-intensity-input" onchange={on_select("blur_intensity")} value={s.blur_intensity.clone()} class="w-full rounded-md border px-3 py-2">
                        <option value="none">{"None"}</option>
                        <option value="low">{"Low"}</option>
                        <option value="medium">{"Medium"}</option>
                        <option value="full">{"Full"}</option>
                    </select>
                </div>
                <div class="border border-token rounded-md p-4 space-y-4">
                    <label class="flex items-center gap-3 text-sm font-medium">
                        <input id="autoupdate-enabled-input" oninput={on_checkbox("autoupdate_enabled")} type="checkbox" checked={s.autoupdate_enabled} class="h-4 w-4" />
                        <span>{"Enable Server Self-Updates"}</span>
                    </label>
                    <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                        <div>
                            <label class="block card-kicker mb-2">{"GitHub Repo"}</label>
                            <input id="autoupdate-repo-input" oninput={on_input("autoupdate_repo")} value={s.autoupdate_repo.clone()} class="w-full rounded-md border px-3 py-2" placeholder="slopfire/denpie" />
                        </div>
                        <div>
                            <label class="block card-kicker mb-2">{"Branch"}</label>
                            <input id="autoupdate-branch-input" oninput={on_input("autoupdate_branch")} value={s.autoupdate_branch.clone()} class="w-full rounded-md border px-3 py-2" placeholder="master" />
                        </div>
                    </div>
                    <div>
                        <label class="block card-kicker mb-2">{"Server Update Check Interval Seconds"}</label>
                        <input id="autoupdate-interval-input" oninput={on_input("autoupdate_check_interval_secs")} type="number" min="60" value={s.autoupdate_check_interval_secs.to_string()} class="w-full rounded-md border px-3 py-2" />
                    </div>
                    <div>
                        <label class="block card-kicker mb-2">{"Server Update Command"}</label>
                        <textarea id="autoupdate-command-input" oninput={on_textarea("autoupdate_command")} value={s.autoupdate_command.clone()} class="w-full rounded-md border px-3 py-2 h-20 resize-y" placeholder="Optional command for non-systemd installs"></textarea>
                        <div class="mt-2 text-xs text-muted">{"Leave empty to use the installed systemd server updater service."}</div>
                    </div>
                    <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                        <div class="text-xs text-muted">
                            {"Last server build seen: "}
                            <span id="autoupdate-last-sha">{if s.autoupdate_last_seen_sha.is_empty() { "not recorded".to_string() } else { s.autoupdate_last_seen_sha.clone() }}</span>
                        </div>
                        <button id="autoupdate-trigger-btn" type="button" onclick={on_check_server} class="rounded-md border border-token px-4 py-2 font-medium flex items-center justify-center gap-2">
                            <iconify-icon icon="radix-icons:update" class="radix-icon" aria-hidden="true"></iconify-icon>
                            <span>{"Check Server Now"}</span>
                        </button>
                    </div>
                </div>
                <button type="submit" class="rounded-md bg-primary-solid px-5 py-3 font-medium">
                    {"Save Settings"}
                </button>
            </form>
        </section>
    }
}