use crate::api::toast;
use crate::components::select::{SelectOption, ShadcnSelect};
use crate::state::AppState;
use gloo_net::http::Request;
use gloo_storage::{LocalStorage, Storage};
use gloo_timers::callback::Timeout;
use serde::{Deserialize, Serialize};
use web_sys::{HtmlInputElement, HtmlTextAreaElement};
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

impl SettingsRes {
    fn diff_from(&self, prev: &Self) -> UpdateSettingsPatch {
        macro_rules! changed {
            ($patch:ident, $field:ident) => {
                if self.$field != prev.$field {
                    $patch.$field = Some(self.$field.clone());
                }
            };
            ($patch:ident, $field:ident, copy) => {
                if self.$field != prev.$field {
                    $patch.$field = Some(self.$field);
                }
            };
        }

        let mut patch = UpdateSettingsPatch::default();
        changed!(patch, model);
        changed!(patch, compress_model);
        changed!(patch, template);
        changed!(patch, api_key);
        changed!(patch, base_url);
        changed!(patch, compress_base_url);
        changed!(patch, reasoning_effort);
        changed!(patch, compress_reasoning_effort);
        changed!(patch, compression_level);
        changed!(patch, color_scheme);
        changed!(patch, transparency);
        changed!(patch, blur_intensity);
        changed!(patch, autoupdate_enabled, copy);
        changed!(patch, autoupdate_repo);
        changed!(patch, autoupdate_branch);
        changed!(patch, autoupdate_check_interval_secs, copy);
        changed!(patch, autoupdate_command);
        changed!(patch, daily_time_zone);
        changed!(patch, daily_update_time);
        changed!(patch, max_active_cards, copy);
        patch
    }

    fn apply_patch(&mut self, patch: &UpdateSettingsPatch) {
        macro_rules! apply {
            ($field:ident) => {
                if let Some(value) = &patch.$field {
                    self.$field = value.clone();
                }
            };
        }

        apply!(reasoning_effort);
        apply!(compress_reasoning_effort);
        apply!(compression_level);
        apply!(color_scheme);
        apply!(transparency);
        apply!(blur_intensity);
    }
}

#[derive(Serialize)]
struct ForceDailyRefreshRequest {
    topics: String,
    tipcard_type: Option<String>,
}

const PENDING_SETTINGS_KEY: &str = "denpie-pending-settings";

#[derive(Serialize, Deserialize, Clone, Default)]
struct UpdateSettingsPatch {
    model: Option<String>,
    compress_model: Option<String>,
    template: Option<String>,
    api_key: Option<String>,
    base_url: Option<String>,
    compress_base_url: Option<String>,
    reasoning_effort: Option<String>,
    compress_reasoning_effort: Option<String>,
    compression_level: Option<String>,
    color_scheme: Option<String>,
    transparency: Option<String>,
    blur_intensity: Option<String>,
    autoupdate_enabled: Option<bool>,
    autoupdate_repo: Option<String>,
    autoupdate_branch: Option<String>,
    autoupdate_check_interval_secs: Option<u64>,
    autoupdate_command: Option<String>,
    daily_time_zone: Option<String>,
    daily_update_time: Option<String>,
    max_active_cards: Option<u64>,
}

impl UpdateSettingsPatch {
    fn durable_selects(&self) -> Self {
        Self {
            reasoning_effort: self.reasoning_effort.clone(),
            compress_reasoning_effort: self.compress_reasoning_effort.clone(),
            compression_level: self.compression_level.clone(),
            color_scheme: self.color_scheme.clone(),
            transparency: self.transparency.clone(),
            blur_intensity: self.blur_intensity.clone(),
            ..Default::default()
        }
    }

    fn is_empty(&self) -> bool {
        self.model.is_none()
            && self.compress_model.is_none()
            && self.template.is_none()
            && self.api_key.is_none()
            && self.base_url.is_none()
            && self.compress_base_url.is_none()
            && self.reasoning_effort.is_none()
            && self.compress_reasoning_effort.is_none()
            && self.compression_level.is_none()
            && self.color_scheme.is_none()
            && self.transparency.is_none()
            && self.blur_intensity.is_none()
            && self.autoupdate_enabled.is_none()
            && self.autoupdate_repo.is_none()
            && self.autoupdate_branch.is_none()
            && self.autoupdate_check_interval_secs.is_none()
            && self.autoupdate_command.is_none()
            && self.daily_time_zone.is_none()
            && self.daily_update_time.is_none()
            && self.max_active_cards.is_none()
    }

    fn merge_from(&mut self, other: Self) {
        macro_rules! merge {
            ($field:ident) => {
                if other.$field.is_some() {
                    self.$field = other.$field;
                }
            };
        }

        merge!(model);
        merge!(compress_model);
        merge!(template);
        merge!(api_key);
        merge!(base_url);
        merge!(compress_base_url);
        merge!(reasoning_effort);
        merge!(compress_reasoning_effort);
        merge!(compression_level);
        merge!(color_scheme);
        merge!(transparency);
        merge!(blur_intensity);
        merge!(autoupdate_enabled);
        merge!(autoupdate_repo);
        merge!(autoupdate_branch);
        merge!(autoupdate_check_interval_secs);
        merge!(autoupdate_command);
        merge!(daily_time_zone);
        merge!(daily_update_time);
        merge!(max_active_cards);
    }
}

fn remember_pending_selects(patch: &UpdateSettingsPatch) {
    let durable = patch.durable_selects();
    if !durable.is_empty() {
        let _ = LocalStorage::set(PENDING_SETTINGS_KEY, durable);
    }
}

fn load_pending_selects() -> Option<UpdateSettingsPatch> {
    LocalStorage::get(PENDING_SETTINGS_KEY).ok()
}

fn clear_pending_selects() {
    LocalStorage::delete(PENDING_SETTINGS_KEY);
}

fn refresh_autoupdate_status(update_status: UseStateHandle<Option<AutoupdateStatus>>) {
    wasm_bindgen_futures::spawn_local(async move {
        let Ok(status_res) = Request::get("/admin/autoupdate/status").send().await else {
            return;
        };
        let Ok(status) = status_res.json::<AutoupdateStatus>().await else {
            return;
        };
        if status.phase == "unknown" && status.message.is_empty() && status.updated_at.is_empty() {
            update_status.set(None);
        } else {
            update_status.set(Some(status));
        }
    });
}

fn autoupdate_status_is_active(status: &AutoupdateStatus) -> bool {
    matches!(
        status.phase.as_str(),
        "starting"
            | "queued"
            | "checking"
            | "preparing"
            | "pulling"
            | "cloning"
            | "compiling"
            | "installing"
            | "restarting"
            | "running"
    )
}

fn autoupdate_phase_label(phase: &str) -> &'static str {
    match phase {
        "active" => "Update active",
        "baseline" => "Baseline recorded",
        "checking" => "Checking",
        "cloning" => "Cloning",
        "compiling" => "Building",
        "current" => "Already current",
        "disabled" => "Disabled",
        "failed" => "Failed",
        "idle" => "Idle",
        "installing" => "Installing",
        "invalid" => "Invalid settings",
        "preparing" => "Preparing",
        "pulling" => "Pulling",
        "queued" => "Queued",
        "restarting" => "Restarting",
        "running" => "Running",
        "starting" => "Starting",
        _ => "Status",
    }
}

fn short_commit(sha: &str) -> String {
    sha.chars().take(12).collect()
}

#[derive(Clone)]
struct SaveRequest {
    patch: UpdateSettingsPatch,
    snapshot: SettingsRes,
}

#[derive(Deserialize, Clone, PartialEq, Default)]
struct TriggerAutoupdateRes {
    message: String,
    restarting: bool,
    updating: bool,
    target_sha: Option<String>,
    build_sha: String,
}

#[derive(Deserialize, Clone, PartialEq, Default)]
struct AutoupdateStatus {
    phase: String,
    message: String,
    target_sha: String,
    updated_at: String,
}

pub fn apply_appearance(settings: &SettingsRes) {
    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            if let Some(html) = document.document_element() {
                let _ = html.set_attribute("data-theme", &settings.color_scheme);
                let _ = html.set_attribute("data-transparency", &settings.transparency);
                let _ = html.set_attribute("data-blur-intensity", &settings.blur_intensity);
            }
        }
    }
}

fn save_settings_now(
    app_state: UseReducerHandle<AppState>,
    settings_state: UseStateHandle<Option<SettingsRes>>,
    status: UseStateHandle<String>,
    last_saved: UseStateHandle<Option<SettingsRes>>,
    request: SaveRequest,
) {
    status.set("Saving...".to_string());
    wasm_bindgen_futures::spawn_local(async move {
        match Request::post("/admin/settings")
            .json(&request.patch)
            .unwrap()
            .send()
            .await
        {
            Ok(res) if res.ok() => {
                match Request::get("/admin/settings").send().await {
                    Ok(refresh) if refresh.ok() => {
                        if let Ok(server_settings) = refresh.json::<SettingsRes>().await {
                            clear_pending_selects();
                            apply_appearance(&server_settings);
                            settings_state.set(Some(server_settings.clone()));
                            last_saved.set(Some(server_settings));
                            status.set("Saved".to_string());
                        } else {
                            // Keep optimistic snapshot if refresh payload is invalid.
                            settings_state.set(Some(request.snapshot.clone()));
                            last_saved.set(Some(request.snapshot));
                            status.set("Saved".to_string());
                        }
                    }
                    _ => {
                        clear_pending_selects();
                        // Keep optimistic snapshot if refresh request fails.
                        settings_state.set(Some(request.snapshot.clone()));
                        last_saved.set(Some(request.snapshot));
                        status.set("Saved".to_string());
                    }
                }
            }
            Ok(res) => {
                let message = res
                    .text()
                    .await
                    .unwrap_or_else(|_| "Failed to save settings".to_string());
                status.set("Save failed".to_string());
                toast(&app_state, message);
            }
            Err(err) => {
                status.set("Save failed".to_string());
                toast(&app_state, err.to_string());
            }
        }
    });
}

#[function_component(Settings)]
pub fn settings() -> Html {
    let app_state = use_context::<UseReducerHandle<AppState>>().unwrap();
    let settings = use_state(|| None::<SettingsRes>);
    let last_saved = use_state(|| None::<SettingsRes>);
    let update_status = use_state(|| None::<AutoupdateStatus>);
    let update_result = use_state(|| None::<TriggerAutoupdateRes>);
    let save_status = use_state(String::new);
    let save_timer = use_mut_ref(|| None::<Timeout>);
    let pending_patch = use_mut_ref(UpdateSettingsPatch::default);
    let pending_snapshot = use_mut_ref(|| None::<SettingsRes>);

    {
        let app_state = app_state.clone();
        let settings = settings.clone();
        let last_saved = last_saved.clone();
        let save_status = save_status.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                match Request::get("/admin/settings").send().await {
                    Ok(res) if res.ok() => {
                        if let Ok(data) = res.json::<SettingsRes>().await {
                            let mut data = data;
                            let pending = load_pending_selects();
                            if let Some(pending) = &pending {
                                data.apply_patch(pending);
                            }
                            let snapshot = data.clone();
                            apply_appearance(&data);
                            last_saved.set(Some(data.clone()));
                            settings.set(Some(data));
                            save_status.set(String::new());
                            if let Some(pending) = pending {
                                save_settings_now(
                                    app_state.clone(),
                                    settings.clone(),
                                    save_status.clone(),
                                    last_saved.clone(),
                                    SaveRequest {
                                        patch: pending,
                                        snapshot,
                                    },
                                );
                            }
                        } else {
                            save_status.set("Failed to load settings".to_string());
                            toast(&app_state, "Failed to parse settings response");
                        }
                    }
                    Ok(res) => {
                        save_status.set("Failed to load settings".to_string());
                        let message = res
                            .text()
                            .await
                            .unwrap_or_else(|_| "Failed to load settings".to_string());
                        toast(&app_state, message);
                    }
                    Err(err) => {
                        save_status.set("Failed to load settings".to_string());
                        toast(&app_state, err.to_string());
                    }
                }
            });
            || ()
        });
    }

    {
        let update_status = update_status.clone();
        use_effect_with((), move |_| {
            refresh_autoupdate_status(update_status);
            || ()
        });
    }

    {
        let update_status_handle = update_status.clone();
        let current_status = (*update_status).clone();
        use_effect_with(current_status, move |status| {
            let timer = status
                .as_ref()
                .filter(|status| autoupdate_status_is_active(status))
                .map(|_| {
                    let update_status_handle = update_status_handle.clone();
                    Timeout::new(2500, move || {
                        refresh_autoupdate_status(update_status_handle);
                    })
                });
            move || drop(timer)
        });
    }

    let on_submit = {
        let app_state = app_state.clone();
        let settings = settings.clone();
        let last_saved = last_saved.clone();
        let save_status = save_status.clone();
        let save_timer = save_timer.clone();
        let pending_patch = pending_patch.clone();
        let pending_snapshot = pending_snapshot.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            if let Some(s) = (*settings).clone() {
                if let Some(timer) = save_timer.borrow_mut().take() {
                    timer.cancel();
                }
                *pending_patch.borrow_mut() = UpdateSettingsPatch::default();
                pending_snapshot.borrow_mut().take();
                let prev = (*last_saved).clone().unwrap_or_default();
                let patch = s.diff_from(&prev);
                save_settings_now(
                    app_state.clone(),
                    settings.clone(),
                    save_status.clone(),
                    last_saved.clone(),
                    SaveRequest { patch, snapshot: s },
                );
            }
        })
    };

    let schedule_save = {
        let app_state = app_state.clone();
        let settings = settings.clone();
        let last_saved = last_saved.clone();
        let save_status = save_status.clone();
        let save_timer = save_timer.clone();
        let pending_patch = pending_patch.clone();
        let pending_snapshot = pending_snapshot.clone();
        Callback::from(move |request: SaveRequest| {
            save_status.set("Unsaved changes".to_string());
            pending_patch.borrow_mut().merge_from(request.patch);
            *pending_snapshot.borrow_mut() = Some(request.snapshot);
            if let Some(timer) = save_timer.borrow_mut().take() {
                timer.cancel();
            }

            let app_state = app_state.clone();
            let settings = settings.clone();
            let last_saved = last_saved.clone();
            let save_status = save_status.clone();
            let save_timer = save_timer.clone();
            let save_timer_for_callback = save_timer.clone();
            let pending_patch = pending_patch.clone();
            let pending_snapshot = pending_snapshot.clone();
            let timer = Timeout::new(600, move || {
                let patch = std::mem::take(&mut *pending_patch.borrow_mut());
                let Some(snapshot) = pending_snapshot.borrow_mut().take() else {
                    return;
                };
                save_timer_for_callback.borrow_mut().take();
                save_settings_now(
                    app_state,
                    settings,
                    save_status,
                    last_saved,
                    SaveRequest { patch, snapshot },
                );
            });
            *save_timer.borrow_mut() = Some(timer);
        })
    };

    let save_immediately = {
        let app_state = app_state.clone();
        let settings = settings.clone();
        let last_saved = last_saved.clone();
        let save_status = save_status.clone();
        let save_timer = save_timer.clone();
        let pending_patch = pending_patch.clone();
        let pending_snapshot = pending_snapshot.clone();
        Callback::from(move |request: SaveRequest| {
            if let Some(timer) = save_timer.borrow_mut().take() {
                timer.cancel();
            }
            remember_pending_selects(&request.patch);
            pending_patch.borrow_mut().merge_from(request.patch);
            *pending_snapshot.borrow_mut() = Some(request.snapshot);

            let patch = std::mem::take(&mut *pending_patch.borrow_mut());
            let Some(snapshot) = pending_snapshot.borrow_mut().take() else {
                return;
            };
            save_settings_now(
                app_state.clone(),
                settings.clone(),
                save_status.clone(),
                last_saved.clone(),
                SaveRequest { patch, snapshot },
            );
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
                if let Ok(res) = Request::post("/app/daily-refresh")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    if res.ok() {
                        toast(&app_state, "Force refresh triggered");
                    } else {
                        toast(&app_state, "Failed to refresh");
                    }
                }
            });
        })
    };

    let on_check_server = {
        let app_state = app_state.clone();
        let update_status = update_status.clone();
        let update_result = update_result.clone();
        Callback::from(move |_| {
            let app_state = app_state.clone();
            let update_status = update_status.clone();
            let update_result = update_result.clone();
            wasm_bindgen_futures::spawn_local(async move {
                update_status.set(Some(AutoupdateStatus {
                    phase: "checking".to_string(),
                    message: "Checking GitHub for server updates".to_string(),
                    target_sha: String::new(),
                    updated_at: String::new(),
                }));
                if let Ok(res) = Request::post("/admin/autoupdate").send().await {
                    if res.ok() {
                        if let Ok(result) = res.json::<TriggerAutoupdateRes>().await {
                            toast(&app_state, result.message.clone());
                            update_result.set(Some(result));
                        } else {
                            toast(&app_state, "Autoupdate checked");
                        }
                        refresh_autoupdate_status(update_status);
                    } else {
                        toast(&app_state, "Failed to check updates");
                        refresh_autoupdate_status(update_status);
                    }
                } else {
                    update_status.set(Some(AutoupdateStatus {
                        phase: "failed".to_string(),
                        message: "Failed to reach server updater endpoint".to_string(),
                        target_sha: String::new(),
                        updated_at: String::new(),
                    }));
                }
            });
        })
    };

    let Some(s) = (*settings).clone() else {
        return html! {
            <section id="view-settings">
                <h1 class="text-xl font-semibold tracking-tight mb-4">
                    {"Settings"}
                </h1>
                <div class="text-sm text-muted">{"Loading settings..."}</div>
            </section>
        };
    };

    let on_input = |field: &'static str| {
        let settings = settings.clone();
        let schedule_save = schedule_save.clone();
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
                        "max_active_cards" => {
                            current.max_active_cards = target.value().parse().unwrap_or(0)
                        }
                        "autoupdate_repo" => current.autoupdate_repo = target.value(),
                        "autoupdate_branch" => current.autoupdate_branch = target.value(),
                        "autoupdate_check_interval_secs" => {
                            current.autoupdate_check_interval_secs =
                                target.value().parse().unwrap_or(60)
                        }
                        _ => {}
                    }
                    settings.set(Some(current.clone()));
                    let mut patch = UpdateSettingsPatch::default();
                    match field {
                        "model" => patch.model = Some(current.model.clone()),
                        "compress_model" => {
                            patch.compress_model = Some(current.compress_model.clone())
                        }
                        "api_key" => patch.api_key = Some(current.api_key.clone()),
                        "base_url" => patch.base_url = Some(current.base_url.clone()),
                        "compress_base_url" => {
                            patch.compress_base_url = Some(current.compress_base_url.clone())
                        }
                        "daily_time_zone" => {
                            patch.daily_time_zone = Some(current.daily_time_zone.clone())
                        }
                        "daily_update_time" => {
                            patch.daily_update_time = Some(current.daily_update_time.clone())
                        }
                        "max_active_cards" => {
                            patch.max_active_cards = Some(current.max_active_cards)
                        }
                        "autoupdate_repo" => {
                            patch.autoupdate_repo = Some(current.autoupdate_repo.clone())
                        }
                        "autoupdate_branch" => {
                            patch.autoupdate_branch = Some(current.autoupdate_branch.clone())
                        }
                        "autoupdate_check_interval_secs" => {
                            patch.autoupdate_check_interval_secs =
                                Some(current.autoupdate_check_interval_secs)
                        }
                        _ => {}
                    }
                    schedule_save.emit(SaveRequest {
                        patch,
                        snapshot: current,
                    });
                }
            }
        })
    };

    let on_checkbox = |field: &'static str| {
        let settings = settings.clone();
        let save_immediately = save_immediately.clone();
        Callback::from(move |e: InputEvent| {
            if let Some(target) = e.target_dyn_into::<HtmlInputElement>() {
                if let Some(mut current) = (*settings).clone() {
                    if field == "autoupdate_enabled" {
                        current.autoupdate_enabled = target.checked();
                    }
                    settings.set(Some(current.clone()));
                    let mut patch = UpdateSettingsPatch::default();
                    patch.autoupdate_enabled = Some(current.autoupdate_enabled);
                    save_immediately.emit(SaveRequest {
                        patch,
                        snapshot: current,
                    });
                }
            }
        })
    };

    let on_select = |field: &'static str| {
        let settings = settings.clone();
        let save_immediately = save_immediately.clone();
        Callback::from(move |value: String| {
            if let Some(mut current) = (*settings).clone() {
                match field {
                    "reasoning_effort" => current.reasoning_effort = value,
                    "compress_reasoning_effort" => current.compress_reasoning_effort = value,
                    "compression_level" => current.compression_level = value,
                    "color_scheme" => current.color_scheme = value,
                    "transparency" => current.transparency = value,
                    "blur_intensity" => current.blur_intensity = value,
                    _ => {}
                }
                apply_appearance(&current);
                settings.set(Some(current.clone()));
                let mut patch = UpdateSettingsPatch::default();
                match field {
                    "reasoning_effort" => {
                        patch.reasoning_effort = Some(current.reasoning_effort.clone())
                    }
                    "compress_reasoning_effort" => {
                        patch.compress_reasoning_effort =
                            Some(current.compress_reasoning_effort.clone())
                    }
                    "compression_level" => {
                        patch.compression_level = Some(current.compression_level.clone())
                    }
                    "color_scheme" => patch.color_scheme = Some(current.color_scheme.clone()),
                    "transparency" => patch.transparency = Some(current.transparency.clone()),
                    "blur_intensity" => patch.blur_intensity = Some(current.blur_intensity.clone()),
                    _ => {}
                }
                save_immediately.emit(SaveRequest {
                    patch,
                    snapshot: current,
                });
            }
        })
    };

    let on_textarea = |field: &'static str| {
        let settings = settings.clone();
        let schedule_save = schedule_save.clone();
        Callback::from(move |e: InputEvent| {
            if let Some(target) = e.target_dyn_into::<HtmlTextAreaElement>() {
                if let Some(mut current) = (*settings).clone() {
                    match field {
                        "template" => current.template = target.value(),
                        "autoupdate_command" => current.autoupdate_command = target.value(),
                        _ => {}
                    }
                    settings.set(Some(current.clone()));
                    let mut patch = UpdateSettingsPatch::default();
                    match field {
                        "template" => patch.template = Some(current.template.clone()),
                        "autoupdate_command" => {
                            patch.autoupdate_command = Some(current.autoupdate_command.clone())
                        }
                        _ => {}
                    }
                    schedule_save.emit(SaveRequest {
                        patch,
                        snapshot: current,
                    });
                }
            }
        })
    };

    html! {
        <section id="view-settings">
            <h1 class="text-xl font-semibold tracking-tight mb-4">
                {"Settings"}
            </h1>
            <form id="settings-form" onsubmit={on_submit} autocomplete="off" class="surface border rounded-md p-4 max-w-5xl space-y-5">
                if !save_status.is_empty() {
                    <div class="text-sm text-muted">{(*save_status).clone()}</div>
                }
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
                        <ShadcnSelect
                            id="reasoning-effort-input"
                            name="reasoning-effort-input"
                            onchange={on_select("reasoning_effort")}
                            value={s.reasoning_effort.clone()}
                            options={vec![
                                SelectOption { value: "none".into(), label: "None".into() },
                                SelectOption { value: "minimal".into(), label: "Minimal".into() },
                                SelectOption { value: "low".into(), label: "Low".into() },
                                SelectOption { value: "medium".into(), label: "Medium".into() },
                                SelectOption { value: "high".into(), label: "High".into() },
                                SelectOption { value: "xhigh".into(), label: "XHigh".into() },
                            ]}
                        />
                    </div>
                    <div>
                        <label class="block card-kicker mb-2">{"Compression Level"}</label>
                        <ShadcnSelect
                            id="compression-level-input"
                            name="compression-level-input"
                            onchange={on_select("compression_level")}
                            value={s.compression_level.clone()}
                            options={vec![
                                SelectOption { value: "light".into(), label: "Light".into() },
                                SelectOption { value: "balanced".into(), label: "Balanced".into() },
                                SelectOption { value: "strong".into(), label: "Strong".into() },
                                SelectOption { value: "ultra".into(), label: "Ultra".into() },
                            ]}
                        />
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
                    <ShadcnSelect
                        id="theme-select-settings"
                        name="theme-select-settings"
                        value={s.color_scheme.clone()}
                        class="theme-select"
                        onchange={on_select("color_scheme")}
                        options={vec![
                            SelectOption { value: "shadcn".into(), label: "Shadcn (Dark)".into() },
                            SelectOption { value: "shadcn-light".into(), label: "Shadcn (Light)".into() },
                            SelectOption { value: "carbonfox".into(), label: "Carbonfox".into() },
                            SelectOption { value: "ayu".into(), label: "Ayu".into() },
                            SelectOption { value: "solarized-light".into(), label: "Solarized Light".into() },
                            SelectOption { value: "solarized-dark".into(), label: "Solarized Dark".into() },
                            SelectOption { value: "amoled".into(), label: "AMOLED".into() },
                            SelectOption { value: "slate".into(), label: "Slate".into() },
                        ]}
                    />
                </div>
                <div>
                    <label class="block card-kicker mb-2">{"Transparency"}</label>
                    <ShadcnSelect
                        id="transparency-input"
                        name="transparency-input"
                        onchange={on_select("transparency")}
                        value={s.transparency.clone()}
                        options={vec![
                            SelectOption { value: "none".into(), label: "None".into() },
                            SelectOption { value: "low".into(), label: "Low".into() },
                            SelectOption { value: "medium".into(), label: "Medium".into() },
                            SelectOption { value: "full".into(), label: "Full".into() },
                        ]}
                    />
                </div>
                <div>
                    <label class="block card-kicker mb-2">{"Blur Intensity"}</label>
                    <ShadcnSelect
                        id="blur-intensity-input"
                        name="blur-intensity-input"
                        onchange={on_select("blur_intensity")}
                        value={s.blur_intensity.clone()}
                        options={vec![
                            SelectOption { value: "none".into(), label: "None".into() },
                            SelectOption { value: "low".into(), label: "Low".into() },
                            SelectOption { value: "medium".into(), label: "Medium".into() },
                            SelectOption { value: "full".into(), label: "Full".into() },
                        ]}
                    />
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
                    if let Some(result) = (*update_result).clone() {
                        <div class="muted-surface rounded-md p-3 text-sm">
                            <div class="font-medium">{result.message}</div>
                            <div class="text-xs text-muted mt-1">{format!("Build: {}{}", result.build_sha, result.target_sha.map(|sha| format!(" -> {}", sha)).unwrap_or_default())}</div>
                        </div>
                    }
                    if let Some(status) = (*update_status).clone() {
                        <div id="autoupdate-progress" class="muted-surface rounded-md p-3 space-y-2">
                            <div class="flex items-center justify-between gap-3 card-kicker">
                                <span>{format!("Updater Log: {}", autoupdate_phase_label(&status.phase))}</span>
                                if !status.updated_at.is_empty() {
                                    <span>{status.updated_at}</span>
                                }
                            </div>
                            <div class="flex flex-wrap items-center gap-2 text-xs text-muted">
                                <span>{format!("Phase: {}", status.phase)}</span>
                                if !status.target_sha.is_empty() {
                                    <span>{format!("Target: {}", short_commit(&status.target_sha))}</span>
                                }
                            </div>
                            <div class="text-sm">{status.message}</div>
                        </div>
                    }
                </div>
                <button type="submit" class="rounded-md bg-primary-solid px-5 py-3 font-medium">
                    {"Save Now"}
                </button>
            </form>
        </section>
    }
}
