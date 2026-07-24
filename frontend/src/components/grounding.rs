use crate::api::toast;
use crate::app::View;
use crate::components::archive::ArchiveQuery;
use crate::components::button::{ButtonSize, ButtonType, ButtonVariant, ShadcnButton};
use crate::components::image_lightbox::ImageLightbox;
use crate::components::select::{SelectOption, ShadcnSelect};
use crate::components::tooltip::ShadcnTooltip;
use crate::i18n::{I18n, use_i18n};
use crate::state::AppState;
use crate::topic_visual::display_icon;
use gloo_net::http::Request;
use gloo_timers::callback::Timeout;
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{FileReader, HtmlDialogElement, HtmlInputElement, HtmlTextAreaElement};
use yew::prelude::*;
use yew_router::prelude::*;

#[derive(Deserialize, Clone, PartialEq)]
pub struct AppSummary {
    pub due_cards: i64,
    pub active_cards: i64,
    pub total_cards: i64,
    pub topics: i64,
}

#[derive(Deserialize, Clone, PartialEq)]
pub struct TokenSpend {
    pub daily: i64,
    pub monthly: i64,
    pub total: i64,
}

#[derive(Deserialize, Clone, PartialEq)]
pub struct AppTopicInfo {
    pub id: i64,
    pub name: String,
    pub tipcard_type: String,
    pub icon_id: String,
    pub topic_color: String,
    pub prompt_template: String,
    pub total_cards: i64,
    pub due_cards: i64,
    pub pending_cards: i64,
    pub completed_cards: i64,
    pub daily_card_count: u32,
    pub daily_time_zone: String,
    pub daily_update_time: String,
    pub compression_level: String,
    pub grounding_strategy: String,
    pub image_strategy: String,
}

#[derive(Serialize)]
struct UpdateTopicReq {
    id: i64,
    prompt_template: Option<String>,
    daily_card_count: Option<u32>,
    daily_time_zone: Option<String>,
    daily_update_time: Option<String>,
    compression_level: Option<String>,
    grounding_strategy: Option<String>,
    image_strategy: Option<String>,
}

#[derive(Serialize)]
struct DeleteTopicReq {
    id: i64,
}

#[derive(Serialize)]
struct RegenerateTopicIconReq {
    id: i64,
}

#[derive(Deserialize)]
struct RegenerateTopicIconRes {
    icon_id: String,
    topic_color: String,
}

#[derive(Serialize)]
struct ForceDailyRefreshReq {
    topics: String,
    tipcard_type: Option<String>,
}

#[derive(Deserialize)]
struct ForceDailyRefreshRes {
    refreshed_cards: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct DocumentInfo {
    id: i64,
    #[serde(default)]
    topic_ids: Vec<i64>,
    source_type: String,
    title: String,
    url: Option<String>,
    created_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct DocumentDetail {
    id: i64,
    source_type: String,
    title: String,
    url: Option<String>,
    content: String,
    created_at: String,
}

#[derive(Serialize)]
struct AddDocumentReq {
    topic_ids: Vec<i64>,
    source_type: String,
    title: String,
    url: Option<String>,
    content: String,
}

#[derive(Serialize)]
struct ExploreLinkReq {
    url: String,
}

#[derive(Deserialize)]
struct ExploredLink {
    url: String,
}

#[derive(Serialize)]
struct DeleteDocumentReq {
    id: i64,
}

#[derive(Serialize)]
struct UploadDocumentReq {
    filename: String,
    title: Option<String>,
    data_url: String,
    topic_ids: Vec<i64>,
}

#[derive(Serialize)]
struct AttachDocumentReq {
    topic_id: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct PoolImageInfo {
    id: i64,
    name: String,
    description: Option<String>,
    tags: Vec<String>,
    created_at: String,
}
#[derive(Serialize)]
struct AddPoolImageReq {
    image_data: String,
    name: String,
}

#[derive(Serialize)]
struct RenamePoolImageReq {
    id: i64,
    name: String,
    description: Option<String>,
}

#[derive(Serialize)]
struct RemovePoolImageTagReq {
    id: i64,
    tag: String,
}

#[derive(Serialize)]
struct DeletePoolImageReq {
    id: i64,
}

/// Derive a human-readable title from a URL's last path segment.
/// e.g. "https://site.com/start-here/installation/" → "Installation"
fn title_from_url(url: &str) -> String {
    let path = url
        .split("://")
        .nth(1)
        .and_then(|rest| {
            let slash_idx = rest.find('/')?;
            Some(&rest[slash_idx..])
        })
        .unwrap_or(url);

    let last = path.split('/').rfind(|s| !s.is_empty()).unwrap_or("");

    if last.is_empty() {
        return url.to_string();
    }

    last.replace(['-', '_'], " ")
        .split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn set_fullscreen_body_class(fullscreen: bool) {
    let Some(body) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.body())
    else {
        return;
    };
    let _ = body
        .class_list()
        .toggle_with_force("has-fullscreen-card", fullscreen);
}

#[derive(Deserialize, Clone, PartialEq)]
struct GroundingSettingsRes {
    grounding_strategy: String,
    grounding_model: String,
    grounding_reasoning_effort: String,
    image_strategy: String,
    search_api_key: String,
    search_base_url: String,
    image_sources: String,
}

#[derive(Serialize, Default)]
struct GroundingSettingsPatch {
    grounding_strategy: Option<String>,
    grounding_model: Option<String>,
    grounding_reasoning_effort: Option<String>,
    image_strategy: Option<String>,
    search_api_key: Option<String>,
    search_base_url: Option<String>,
    image_sources: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ImageSourceKind {
    Api,
    WebSearch,
}

#[derive(Deserialize, Serialize, Clone, PartialEq, Eq)]
struct ImageSourceSettings {
    id: String,
    name: String,
    kind: ImageSourceKind,
    enabled: bool,
    #[serde(default)]
    endpoint: String,
    #[serde(default)]
    query_parameter: String,
    #[serde(default)]
    json_path: String,
    #[serde(default)]
    default_tags: String,
    #[serde(default)]
    api_hosts: String,
    #[serde(default)]
    download_hosts: String,
    #[serde(default)]
    instructions: String,
}

const DEFAULT_IMAGE_SOURCES: &str = r#"[{"id":"danbooru","name":"Danbooru","kind":"api","enabled":false,"endpoint":"https://danbooru.donmai.us/posts/random.json","query_parameter":"tags","json_path":"file_url","default_tags":"rating:general","api_hosts":"danbooru.donmai.us","download_hosts":"cdn.donmai.us","instructions":"Use concise Danbooru tags separated by spaces. Prefer tags that describe the card topic without naming UI text."},{"id":"safebooru","name":"Safebooru","kind":"api","enabled":false,"endpoint":"https://safebooru.org/index.php?page=dapi&s=post&q=index&json=1","query_parameter":"tags","json_path":"file_url","default_tags":"rating:safe","api_hosts":"safebooru.org","download_hosts":"safebooru.org","instructions":"Use concise booru tags separated by spaces."},{"id":"web-search","name":"Web Image Search","kind":"web_search","enabled":false,"endpoint":"","query_parameter":"","json_path":"","default_tags":"","api_hosts":"","download_hosts":"","instructions":"Prefer official project documentation and repositories. Return a direct image asset, never a webpage, logo, tracking pixel, or placeholder."}]"#;

fn parse_image_sources(value: &str) -> Vec<ImageSourceSettings> {
    serde_json::from_str(value)
        .ok()
        .filter(|sources: &Vec<ImageSourceSettings>| !sources.is_empty())
        .unwrap_or_else(|| {
            serde_json::from_str(DEFAULT_IMAGE_SOURCES)
                .expect("built-in frontend image sources are valid")
        })
}

impl GroundingSettingsPatch {
    fn merge_from(&mut self, other: Self) {
        macro_rules! merge {
            ($field:ident) => {
                if other.$field.is_some() {
                    self.$field = other.$field;
                }
            };
        }

        merge!(grounding_strategy);
        merge!(grounding_model);
        merge!(grounding_reasoning_effort);
        merge!(image_strategy);
        merge!(search_api_key);
        merge!(search_base_url);
        merge!(image_sources);
    }
}

fn save_grounding_settings(
    app_state: UseReducerHandle<AppState>,
    status: UseStateHandle<String>,
    patch: GroundingSettingsPatch,
) {
    status.set("Saving...".to_string());
    wasm_bindgen_futures::spawn_local(async move {
        match Request::post("/admin/settings")
            .json(&patch)
            .unwrap()
            .send()
            .await
        {
            Ok(response) if response.ok() => status.set("Saved".to_string()),
            Ok(response) => {
                let message = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Failed to save grounding settings".to_string());
                status.set("Save failed".to_string());
                toast(&app_state, message);
            }
            Err(error) => {
                status.set("Save failed".to_string());
                toast(&app_state, error.to_string());
            }
        }
    });
}

#[function_component(GroundingSettings)]
fn grounding_settings() -> Html {
    let app_state = use_context::<UseReducerHandle<AppState>>().unwrap();
    let settings = use_state(|| None::<GroundingSettingsRes>);
    let save_status = use_state(String::new);
    let save_timer = use_mut_ref(|| None::<Timeout>);
    let pending_patch = use_mut_ref(GroundingSettingsPatch::default);

    {
        let app_state = app_state.clone();
        let settings = settings.clone();
        let save_status = save_status.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                match Request::get("/admin/settings").send().await {
                    Ok(response) if response.ok() => {
                        match response.json::<GroundingSettingsRes>().await {
                            Ok(data) => {
                                settings.set(Some(data));
                                save_status.set(String::new());
                            }
                            Err(error) => {
                                save_status.set("Failed to load settings".to_string());
                                toast(&app_state, error.to_string());
                            }
                        }
                    }
                    Ok(response) => {
                        let message = response
                            .text()
                            .await
                            .unwrap_or_else(|_| "Failed to load grounding settings".to_string());
                        save_status.set("Failed to load settings".to_string());
                        toast(&app_state, message);
                    }
                    Err(error) => {
                        save_status.set("Failed to load settings".to_string());
                        toast(&app_state, error.to_string());
                    }
                }
            });
            || ()
        });
    }

    let save_immediately = {
        let app_state = app_state.clone();
        let save_status = save_status.clone();
        let save_timer = save_timer.clone();
        let pending_patch = pending_patch.clone();
        Callback::from(move |patch: GroundingSettingsPatch| {
            if let Some(timer) = save_timer.borrow_mut().take() {
                timer.cancel();
            }
            pending_patch.borrow_mut().merge_from(patch);
            let patch = std::mem::take(&mut *pending_patch.borrow_mut());
            save_grounding_settings(app_state.clone(), save_status.clone(), patch);
        })
    };

    let schedule_save = {
        let app_state = app_state.clone();
        let save_status = save_status.clone();
        let save_timer = save_timer.clone();
        let pending_patch = pending_patch.clone();
        Callback::from(move |patch: GroundingSettingsPatch| {
            save_status.set("Unsaved changes".to_string());
            pending_patch.borrow_mut().merge_from(patch);
            if let Some(timer) = save_timer.borrow_mut().take() {
                timer.cancel();
            }
            let app_state = app_state.clone();
            let save_status = save_status.clone();
            let save_timer_for_callback = save_timer.clone();
            let pending_patch = pending_patch.clone();
            let timer = Timeout::new(600, move || {
                save_timer_for_callback.borrow_mut().take();
                let patch = std::mem::take(&mut *pending_patch.borrow_mut());
                save_grounding_settings(app_state, save_status, patch);
            });
            *save_timer.borrow_mut() = Some(timer);
        })
    };

    let on_select = |field: &'static str| {
        let settings = settings.clone();
        let save_immediately = save_immediately.clone();
        Callback::from(move |value: String| {
            let Some(mut current) = (*settings).clone() else {
                return;
            };
            let mut patch = GroundingSettingsPatch::default();
            match field {
                "grounding_reasoning_effort" => {
                    current.grounding_reasoning_effort = value.clone();
                    patch.grounding_reasoning_effort = Some(value);
                }
                "grounding_strategy" => {
                    current.grounding_strategy = value.clone();
                    patch.grounding_strategy = Some(value);
                }
                "image_strategy" => {
                    current.image_strategy = value.clone();
                    patch.image_strategy = Some(value);
                }
                _ => return,
            }
            settings.set(Some(current));
            save_immediately.emit(patch);
        })
    };

    let on_input = |field: &'static str| {
        let settings = settings.clone();
        let schedule_save = schedule_save.clone();
        Callback::from(move |event: InputEvent| {
            let Some(target) = event.target_dyn_into::<HtmlInputElement>() else {
                return;
            };
            let Some(mut current) = (*settings).clone() else {
                return;
            };
            let value = target.value();
            let mut patch = GroundingSettingsPatch::default();
            match field {
                "grounding_model" => {
                    current.grounding_model = value.clone();
                    patch.grounding_model = Some(value);
                }
                "search_api_key" => {
                    current.search_api_key = value.clone();
                    patch.search_api_key = Some(value);
                }
                "search_base_url" => {
                    current.search_base_url = value.clone();
                    patch.search_base_url = Some(value);
                }
                _ => return,
            }
            settings.set(Some(current));
            schedule_save.emit(patch);
        })
    };

    let update_source = {
        let settings = settings.clone();
        let schedule_save = schedule_save.clone();
        Callback::from(
            move |(source_id, field, value): (String, &'static str, String)| {
                let Some(mut current) = (*settings).clone() else {
                    return;
                };
                let mut sources = parse_image_sources(&current.image_sources);
                let Some(source) = sources.iter_mut().find(|source| source.id == source_id) else {
                    return;
                };
                match field {
                    "name" => source.name = value,
                    "enabled" => source.enabled = value == "true",
                    "endpoint" => source.endpoint = value,
                    "query_parameter" => source.query_parameter = value,
                    "json_path" => source.json_path = value,
                    "default_tags" => source.default_tags = value,
                    "api_hosts" => source.api_hosts = value,
                    "download_hosts" => source.download_hosts = value,
                    "instructions" => source.instructions = value,
                    _ => return,
                }
                let Ok(image_sources) = serde_json::to_string(&sources) else {
                    return;
                };
                current.image_sources = image_sources.clone();
                settings.set(Some(current));
                schedule_save.emit(GroundingSettingsPatch {
                    image_sources: Some(image_sources),
                    ..Default::default()
                });
            },
        )
    };

    let on_source_input = |source_id: String, field: &'static str| {
        let update_source = update_source.clone();
        Callback::from(move |event: InputEvent| {
            let Some(target) = event.target() else {
                return;
            };
            let value = target
                .dyn_ref::<HtmlInputElement>()
                .map(HtmlInputElement::value)
                .or_else(|| {
                    target
                        .dyn_ref::<HtmlTextAreaElement>()
                        .map(HtmlTextAreaElement::value)
                });
            if let Some(value) = value {
                update_source.emit((source_id.clone(), field, value));
            }
        })
    };

    let on_source_enabled = |source_id: String| {
        let update_source = update_source.clone();
        Callback::from(move |event: Event| {
            let Some(target) = event.target_dyn_into::<HtmlInputElement>() else {
                return;
            };
            update_source.emit((source_id.clone(), "enabled", target.checked().to_string()));
        })
    };

    let remove_source = {
        let settings = settings.clone();
        let save_immediately = save_immediately.clone();
        Callback::from(move |source_id: String| {
            let Some(mut current) = (*settings).clone() else {
                return;
            };
            let mut sources = parse_image_sources(&current.image_sources);
            sources.retain(|source| source.id != source_id);
            let Ok(image_sources) = serde_json::to_string(&sources) else {
                return;
            };
            current.image_sources = image_sources.clone();
            settings.set(Some(current));
            save_immediately.emit(GroundingSettingsPatch {
                image_sources: Some(image_sources),
                ..Default::default()
            });
        })
    };

    let add_source = {
        let settings = settings.clone();
        let save_immediately = save_immediately.clone();
        Callback::from(move |kind: ImageSourceKind| {
            let Some(mut current) = (*settings).clone() else {
                return;
            };
            let mut sources = parse_image_sources(&current.image_sources);
            let mut suffix = sources.len() + 1;
            let source_id = loop {
                let candidate = format!("custom-{suffix}");
                if sources.iter().all(|source| source.id != candidate) {
                    break candidate;
                }
                suffix += 1;
            };
            let is_api = kind == ImageSourceKind::Api;
            sources.push(ImageSourceSettings {
                id: source_id,
                name: if is_api {
                    "Custom Image API".to_string()
                } else {
                    "Custom Web Search".to_string()
                },
                kind,
                enabled: false,
                endpoint: String::new(),
                query_parameter: if is_api {
                    "tags".to_string()
                } else {
                    String::new()
                },
                json_path: if is_api {
                    "file_url".to_string()
                } else {
                    String::new()
                },
                default_tags: String::new(),
                api_hosts: String::new(),
                download_hosts: String::new(),
                instructions: String::new(),
            });
            let Ok(image_sources) = serde_json::to_string(&sources) else {
                return;
            };
            current.image_sources = image_sources.clone();
            settings.set(Some(current));
            save_immediately.emit(GroundingSettingsPatch {
                image_sources: Some(image_sources),
                ..Default::default()
            });
        })
    };

    let Some(settings) = (*settings).clone() else {
        return html! {
            <div class="surface border rounded-md p-4 mb-4">
                <h2 class="text-lg font-semibold">{"Grounding Settings"}</h2>
                <div class="mt-2 text-sm text-muted">
                    {if save_status.is_empty() { "Loading settings..." } else { save_status.as_str() }}
                </div>
            </div>
        };
    };

    let image_sources = parse_image_sources(&settings.image_sources);
    let selected_image_strategy = settings.image_strategy.clone();
    let selected_source_kind = match selected_image_strategy.as_str() {
        "programmatic" => Some(ImageSourceKind::Api),
        "agentic" => Some(ImageSourceKind::WebSearch),
        _ => None,
    };
    let visible_image_sources = image_sources
        .into_iter()
        .filter(|source| Some(source.kind) == selected_source_kind)
        .collect::<Vec<_>>();
    let select_image_strategy = on_select("image_strategy");
    let add_api_source = {
        let add_source = add_source.clone();
        Callback::from(move |_: MouseEvent| add_source.emit(ImageSourceKind::Api))
    };
    let add_web_source = {
        let add_source = add_source.clone();
        Callback::from(move |_: MouseEvent| add_source.emit(ImageSourceKind::WebSearch))
    };

    html! {
        <div id="grounding-settings" class="surface border rounded-md p-4 mb-4 flex flex-col gap-5">
            <div class="flex items-start justify-between gap-3">
                <div>
                    <h2 class="text-lg font-semibold">{"Grounding Settings"}</h2>
                    <p class="text-sm text-muted">{"Defaults for sourcing facts and illustrating generated cards."}</p>
                </div>
                if !save_status.is_empty() {
                    <span id="grounding-settings-save-status" class="text-sm text-muted">{(*save_status).clone()}</span>
                }
            </div>

            <section class="flex flex-col gap-3" aria-labelledby="fact-sources-heading">
                <div>
                    <h3 id="fact-sources-heading" class="font-semibold">{"Fact Sources"}</h3>
                    <p class="text-sm text-muted">{"Control how generated card facts are researched."}</p>
                </div>
                <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                    <div>
                        <label class="block card-kicker mb-2" for="grounding-model-input">{"Grounding Agent Model"}</label>
                        <input
                            id="grounding-model-input"
                            oninput={on_input("grounding_model")}
                            value={settings.grounding_model.clone()}
                            class="w-full rounded-md border px-3 py-2"
                            placeholder="Defaults to the LLM model"
                        />
                        <div class="mt-2 text-xs text-muted">{"Model used for grounded research. Leave empty to use the LLM model."}</div>
                    </div>
                    <div>
                        <label class="block card-kicker mb-2" for="grounding-reasoning-effort-input">{"Grounding Agent Reasoning"}</label>
                        <ShadcnSelect
                            id="grounding-reasoning-effort-input"
                            name="grounding-reasoning-effort-input"
                            onchange={on_select("grounding_reasoning_effort")}
                            value={settings.grounding_reasoning_effort.clone()}
                            options={vec![
                                SelectOption { value: "".into(), label: "Use LLM setting".into() },
                                SelectOption { value: "none".into(), label: "None".into() },
                                SelectOption { value: "minimal".into(), label: "Minimal".into() },
                                SelectOption { value: "low".into(), label: "Low".into() },
                                SelectOption { value: "medium".into(), label: "Medium".into() },
                                SelectOption { value: "high".into(), label: "High".into() },
                                SelectOption { value: "xhigh".into(), label: "XHigh".into() },
                            ]}
                        />
                    </div>
                </div>
                <div>
                    <label class="block card-kicker mb-2" for="grounding-strategy-input">{"Grounding Strategy"}</label>
                    <ShadcnSelect
                        id="grounding-strategy-input"
                        name="grounding-strategy-input"
                        onchange={on_select("grounding_strategy")}
                        value={settings.grounding_strategy.clone()}
                        options={vec![
                            SelectOption { value: "factual".into(), label: "Factual (no grounding)".into() },
                            SelectOption { value: "create_and_ground".into(), label: "Factcheck (web fact-check)".into() },
                            SelectOption { value: "agentic".into(), label: "Agentic (research + backlog)".into() },
                            SelectOption { value: "rag".into(), label: "From My Data (user documents)".into() },
                        ]}
                    />
                </div>
                <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                    <div>
                        <label class="block card-kicker mb-2" for="search-api-key-input">{"Search API Key (Tavily)"}</label>
                        <input id="search-api-key-input" oninput={on_input("search_api_key")} type="password" value={settings.search_api_key.clone()} class="w-full rounded-md border px-3 py-2" placeholder="tvly-…" />
                        <div class="mt-2 text-xs text-muted">{"Optional. Used for external fact grounding, Web Image Search, and Isolated Image Search."}</div>
                    </div>
                    <div>
                        <label class="block card-kicker mb-2" for="search-base-url-input">{"Search Base URL"}</label>
                        <input id="search-base-url-input" oninput={on_input("search_base_url")} value={settings.search_base_url.clone()} class="w-full rounded-md border px-3 py-2" placeholder="https://api.tavily.com" />
                    </div>
                </div>
            </section>

            <section id="image-sources-settings" class="flex flex-col gap-4" aria-labelledby="image-sources-heading">
                <div>
                    <h3 id="image-sources-heading" class="font-semibold">{"Image Sources"}</h3>
                    <p class="text-sm text-muted">{"Choose how Denpie should find illustrations for newly generated cards."}</p>
                    <p class="text-xs text-muted mt-1">{"For every enabled mode, the card-generation model first decides whether an image would add learning value. Decorative lookups are skipped."}</p>
                </div>
                <div id="image-source-mode-cards" class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-5 gap-3">
                    {
                        for [
                            ("none", "No Images", "Generate cards without illustrations."),
                            ("pool", "Local Image Pool", "Choose from images uploaded to this account."),
                            ("programmatic", "Tag-based Image APIs", "The model writes search tags. Denpie tries each enabled API below and uses the first image returned."),
                            ("web_search", "Web Image Search", "Uses the configured Tavily-compatible search API and the card's generated image query."),
                            ("agentic", "Isolated Image Search", "Uses the configured web search API to find images only inside the allowed domains below. Denpie validates and downloads the first usable result."),
                        ]
                        .into_iter()
                        .map(|(value, title, description)| {
                            let selected = selected_image_strategy == value;
                            let select_image_strategy = select_image_strategy.clone();
                            let onchange = Callback::from(move |_: Event| {
                                select_image_strategy.emit(value.to_string());
                            });
                            html! {
                                <label
                                    class={classes!(
                                        "border", "border-token", "rounded-md", "p-3", "cursor-pointer",
                                        "flex", "items-start", "gap-3",
                                        selected.then_some("ring-1"),
                                        selected.then_some("ring-primary")
                                    )}
                                >
                                    <input
                                        type="radio"
                                        name="image-source-mode"
                                        value={value}
                                        checked={selected}
                                        onchange={onchange}
                                        class="mt-1"
                                    />
                                    <span class="flex flex-col gap-1">
                                        <span class="font-medium">{title}</span>
                                        <span class="text-xs text-muted">{description}</span>
                                    </span>
                                </label>
                            }
                        })
                    }
                </div>

                if let Some(source_kind) = selected_source_kind {
                    <div class="flex flex-wrap items-center justify-between gap-3">
                        <div>
                            <h4 class="font-medium">
                                {if source_kind == ImageSourceKind::Api { "Image API Providers" } else { "Allowed Search Sites" }}
                            </h4>
                            <p class="text-xs text-muted">
                                {if source_kind == ImageSourceKind::Api {
                                    "Enable at least one provider. Denpie asks the model for tags, calls each API in order, and stops at the first valid image."
                                } else {
                                    "Enable at least one site and list its exact image-host domains. Denpie rejects URLs from every other host."
                                }}
                            </p>
                        </div>
                        if source_kind == ImageSourceKind::Api {
                            <button type="button" onclick={add_api_source} class="rounded-md border border-token px-3 py-2 text-sm font-medium">
                                {"Add Image API"}
                            </button>
                        } else {
                            <button type="button" onclick={add_web_source} class="rounded-md border border-token px-3 py-2 text-sm font-medium">
                                {"Add Search Site"}
                            </button>
                        }
                    </div>
                } else if selected_image_strategy == "pool" {
                    <p id="image-source-mode-help" class="text-sm text-muted">
                        {"Manage uploaded images in the Image Pool below. Denpie asks the model to choose the closest match for each new card."}
                    </p>
                } else if selected_image_strategy == "web_search" {
                    <p id="image-source-mode-help" class="text-sm text-muted">
                        {"Configure the Search API Key and Base URL above. Denpie searches only when the generated card decides an image adds learning value."}
                    </p>
                } else {
                    <p id="image-source-mode-help" class="text-sm text-muted">
                        {"Image lookup is disabled. Existing card images are not removed."}
                    </p>
                }

                if selected_source_kind.is_some() && visible_image_sources.is_empty() {
                    <p id="image-source-empty" class="rounded-md border border-token p-3 text-sm text-muted">
                        {"No providers exist for this mode yet. Add one to make image lookup available."}
                    </p>
                }

                <div id="image-source-cards" class="grid grid-cols-1 lg:grid-cols-2 gap-3">
                    {
                        for visible_image_sources.into_iter().map(|source| {
                            let source_id = source.id.clone();
                            let source_kind = source.kind;
                            let remove = {
                                let remove_source = remove_source.clone();
                                let source_id = source_id.clone();
                                Callback::from(move |_: MouseEvent| remove_source.emit(source_id.clone()))
                            };
                            let source_hint = if source_kind == ImageSourceKind::Api {
                                source.endpoint.clone()
                            } else if !source.download_hosts.is_empty() {
                                source.download_hosts.clone()
                            } else {
                                "No image hosts configured".to_string()
                            };
                            html! {
                                <details
                                    data-image-source-id={source.id.clone()}
                                    data-image-source-kind={if source_kind == ImageSourceKind::Api { "api" } else { "web_search" }}
                                    class={classes!(
                                        "group", "border", "border-token", "rounded-md", "overflow-hidden",
                                        "open:lg:col-span-2",
                                        source.enabled.then_some("ring-1"),
                                        source.enabled.then_some("ring-primary")
                                    )}
                                >
                                    <summary
                                        data-image-source-summary=""
                                        class="list-none cursor-pointer px-3 py-3 select-none [&::-webkit-details-marker]:hidden"
                                    >
                                        <div class="flex items-center gap-3">
                                            <input
                                                type="checkbox"
                                                checked={source.enabled}
                                                onchange={on_source_enabled(source_id.clone())}
                                                onclick={Callback::from(|event: MouseEvent| event.stop_propagation())}
                                                aria-label={format!("Enable {}", source.name)}
                                            />
                                            <div class="min-w-0 flex-1">
                                                <div class="flex min-w-0 items-center gap-2">
                                                    <span class="truncate font-medium">{source.name.clone()}</span>
                                                    <span class="shrink-0 rounded-full border border-token px-2 py-0.5 text-[0.6875rem] text-muted">
                                                        {if source_kind == ImageSourceKind::Api { "Image API" } else { "Search site" }}
                                                    </span>
                                                </div>
                                                <p class="truncate text-xs text-muted">{source_hint}</p>
                                            </div>
                                            <span class={classes!(
                                                "hidden", "shrink-0", "text-xs", "sm:inline",
                                                source.enabled.then_some("text-primary"),
                                                (!source.enabled).then_some("text-muted")
                                            )}>
                                                {if source.enabled { "Enabled" } else { "Disabled" }}
                                            </span>
                                            <span aria-hidden="true" class="shrink-0 text-lg leading-none text-muted transition-transform group-open:rotate-90">{"›"}</span>
                                        </div>
                                    </summary>
                                    <div data-image-source-fields="" class="flex flex-col gap-4 border-t border-token p-4">
                                        <div class="flex items-end gap-3">
                                            <div class="min-w-0 flex-1">
                                                <label class="block card-kicker mb-2">{"Source Name"}</label>
                                                <input
                                                    value={source.name.clone()}
                                                    oninput={on_source_input(source_id.clone(), "name")}
                                                    class="w-full rounded-md border px-3 py-2"
                                                />
                                            </div>
                                            <button type="button" onclick={remove} class="shrink-0 rounded-md border border-token px-3 py-2 text-sm">
                                                {"Remove"}
                                            </button>
                                        </div>
                                        if source_kind == ImageSourceKind::Api {
                                            <div class="flex flex-col gap-3">
                                                <div>
                                                    <label class="block card-kicker mb-2">{"API Endpoint"}</label>
                                                    <input value={source.endpoint.clone()} oninput={on_source_input(source_id.clone(), "endpoint")} class="w-full rounded-md border px-3 py-2" placeholder="https://example.com/posts/random.json" />
                                                </div>
                                                <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
                                                    <div>
                                                        <label class="block card-kicker mb-2">{"Query Parameter"}</label>
                                                        <input value={source.query_parameter.clone()} oninput={on_source_input(source_id.clone(), "query_parameter")} class="w-full rounded-md border px-3 py-2" placeholder="tags" />
                                                    </div>
                                                    <div>
                                                        <label class="block card-kicker mb-2">{"JSON Image Path"}</label>
                                                        <input value={source.json_path.clone()} oninput={on_source_input(source_id.clone(), "json_path")} class="w-full rounded-md border px-3 py-2" placeholder="file_url" />
                                                    </div>
                                                </div>
                                                <div>
                                                    <label class="block card-kicker mb-2">{"Fixed Tags"}</label>
                                                    <input value={source.default_tags.clone()} oninput={on_source_input(source_id.clone(), "default_tags")} class="w-full rounded-md border px-3 py-2" placeholder="rating:general" />
                                                </div>
                                                <div>
                                                    <label class="block card-kicker mb-2">{"API Hosts"}</label>
                                                    <input value={source.api_hosts.clone()} oninput={on_source_input(source_id.clone(), "api_hosts")} class="w-full rounded-md border px-3 py-2" placeholder="api.example.com" />
                                                </div>
                                            </div>
                                        }
                                        <div>
                                            <label class="block card-kicker mb-2">{"Image Download Hosts"}</label>
                                            <input value={source.download_hosts.clone()} oninput={on_source_input(source_id.clone(), "download_hosts")} class="w-full rounded-md border px-3 py-2" placeholder="cdn.example.com,images.example.com" />
                                            <p class="mt-2 text-xs text-muted">{"Exact comma-separated hosts allowed to serve image bytes."}</p>
                                        </div>
                                        <div>
                                            <label class="block card-kicker mb-2">
                                                {if source_kind == ImageSourceKind::Api { "Tag Instructions" } else { "Search Instructions" }}
                                            </label>
                                            <textarea value={source.instructions.clone()} oninput={on_source_input(source_id, "instructions")} class="w-full rounded-md border px-3 py-2 min-h-20 resize-y"></textarea>
                                        </div>
                                    </div>
                                </details>
                            }
                        })
                    }
                </div>
            </section>
        </div>
    }
}

#[function_component(Grounding)]
pub fn grounding() -> Html {
    let app_state = use_context::<UseReducerHandle<AppState>>().unwrap();
    let i18n = use_i18n();
    let navigator = use_navigator();
    let summary = use_state(|| None::<AppSummary>);
    let token_spend = use_state(|| None::<TokenSpend>);
    let topics = use_state(Vec::<AppTopicInfo>::new);
    let search = use_state(String::new);
    let editing = use_state(|| None::<AppTopicInfo>);
    let confirm_delete = use_state(|| None::<AppTopicInfo>);
    let regenerating_icon = use_state(|| None::<i64>);
    let loading_topic = use_state(|| None::<i64>);
    let dialog_ref = use_node_ref();

    let documents = use_state(Vec::<DocumentInfo>::new);
    let pool_images = use_state(Vec::<PoolImageInfo>::new);
    let sources_loaded = use_state(|| false);

    let doc_title = use_state(String::new);
    let doc_url = use_state(String::new);
    let doc_content = use_state(String::new);
    let doc_source_type = use_state(|| "document".to_string());
    let bulk_urls = use_state(String::new);
    let bulk_progress = use_state(|| None::<(usize, usize)>);
    let viewing_document = use_state(|| None::<DocumentDetail>);
    let doc_viewer_ref = use_node_ref();
    let doc_file_input_ref = use_node_ref();
    let doc_file_data = use_state(String::new);
    let doc_file_name = use_state(String::new);
    let doc_file_uploading = use_state(|| false);

    let img_name = use_state(String::new);
    let img_data = use_state(String::new);
    let file_input_ref = use_node_ref();
    let editing_image = use_state(|| None::<PoolImageInfo>);
    let rename_name = use_state(String::new);
    let rename_desc = use_state(String::new);
    let file_input_ref_for_click = file_input_ref.clone();
    let doc_file_input_ref_for_click = doc_file_input_ref.clone();
    let pool_expanded = use_state(|| false);
    let pool_search = use_state(String::new);
    let pool_lightbox_index = use_state(|| None::<usize>);
    {
        let pool_expanded = pool_expanded.clone();
        use_effect_with(pool_expanded.clone(), move |expanded| {
            set_fullscreen_body_class(**expanded);
            move || set_fullscreen_body_class(false)
        });
    }
    let rename_dialog_ref = use_node_ref();
    {
        let rename_dialog_ref = rename_dialog_ref.clone();
        let editing_image = editing_image.clone();
        use_effect_with(editing_image.clone(), move |ei| {
            if let Some(dialog) = rename_dialog_ref.cast::<HtmlDialogElement>() {
                if ei.is_some() {
                    let _ = dialog.show_modal();
                } else {
                    let _ = dialog.close();
                }
            }
            || ()
        });
    }
    {
        let doc_viewer_ref = doc_viewer_ref.clone();
        let viewing_document = viewing_document.clone();
        use_effect_with(viewing_document.clone(), move |vd| {
            if let Some(dialog) = doc_viewer_ref.cast::<HtmlDialogElement>() {
                if vd.is_some() {
                    let _ = dialog.show_modal();
                } else {
                    let _ = dialog.close();
                }
            }
            || ()
        });
    }

    let filtered_pool_images: Vec<PoolImageInfo> = {
        let search = (*pool_search).to_lowercase();
        if search.is_empty() {
            (*pool_images).clone()
        } else {
            (*pool_images)
                .iter()
                .filter(|img| {
                    img.name.to_lowercase().contains(&search)
                        || img
                            .description
                            .as_ref()
                            .map(|d| d.to_lowercase().contains(&search))
                            .unwrap_or(false)
                        || img.tags.iter().any(|t| t.to_lowercase().contains(&search))
                })
                .cloned()
                .collect()
        }
    };

    {
        let dialog_ref = dialog_ref.clone();
        let confirm_delete = confirm_delete.clone();
        use_effect_with(confirm_delete.clone(), move |cd| {
            if let Some(dialog) = dialog_ref.cast::<HtmlDialogElement>() {
                if cd.is_some() {
                    let _ = dialog.show_modal();
                } else {
                    let _ = dialog.close();
                }
            }
            || ()
        });
    }

    let refresh = {
        let summary = summary.clone();
        let token_spend = token_spend.clone();
        let topics = topics.clone();
        let documents = documents.clone();
        let pool_images = pool_images.clone();
        let sources_loaded = sources_loaded.clone();
        Callback::from(move |_| {
            let summary = summary.clone();
            let token_spend = token_spend.clone();
            let topics = topics.clone();
            let documents = documents.clone();
            let pool_images = pool_images.clone();
            let sources_loaded = sources_loaded.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(res) = Request::get("/app/summary").send().await {
                    if let Ok(data) = res.json::<AppSummary>().await {
                        summary.set(Some(data));
                    }
                }
                if let Ok(res) = Request::get("/admin/token-spend").send().await {
                    if let Ok(data) = res.json::<TokenSpend>().await {
                        token_spend.set(Some(data));
                    }
                }
                if let Ok(res) = Request::get("/app/topics").send().await {
                    if let Ok(data) = res.json::<Vec<AppTopicInfo>>().await {
                        topics.set(data);
                    }
                }
                if let Ok(res) = Request::get("/app/documents").send().await {
                    if let Ok(data) = res.json::<Vec<DocumentInfo>>().await {
                        documents.set(data);
                    }
                }
                if let Ok(res) = Request::get("/app/image-pool").send().await {
                    if let Ok(data) = res.json::<Vec<PoolImageInfo>>().await {
                        pool_images.set(data);
                    }
                }
                sources_loaded.set(true);
            });
        })
    };

    crate::hooks::use_view_refresh(View::Grounding, refresh.clone());

    let refresh_topics = {
        let topics = topics.clone();
        Callback::from(move |_| {
            let topics = topics.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(res) = Request::get("/app/topics").send().await {
                    if let Ok(data) = res.json::<Vec<AppTopicInfo>>().await {
                        topics.set(data);
                    }
                }
            });
        })
    };

    let on_regenerate_icon = {
        let app_state = app_state.clone();
        let topics = topics.clone();
        let regenerating_icon = regenerating_icon.clone();
        Callback::from(move |topic_id: i64| {
            if regenerating_icon.is_some() {
                return;
            }
            regenerating_icon.set(Some(topic_id));
            let app_state = app_state.clone();
            let topics = topics.clone();
            let regenerating_icon = regenerating_icon.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = RegenerateTopicIconReq { id: topic_id };
                let result = Request::post("/app/topics/regenerate-icon")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await;
                match result {
                    Ok(res) if res.ok() => {
                        if let Ok(data) = res.json::<RegenerateTopicIconRes>().await {
                            topics.set(
                                topics
                                    .iter()
                                    .map(|topic| {
                                        if topic.id == topic_id {
                                            AppTopicInfo {
                                                icon_id: data.icon_id.clone(),
                                                topic_color: data.topic_color.clone(),
                                                ..topic.clone()
                                            }
                                        } else {
                                            topic.clone()
                                        }
                                    })
                                    .collect(),
                            );
                            toast(&app_state, "Topic icon and color updated");
                        } else {
                            toast(&app_state, "Failed to read icon response");
                        }
                    }
                    Ok(res) => {
                        toast(
                            &app_state,
                            res.text()
                                .await
                                .unwrap_or_else(|_| "Failed to update topic icon".to_string()),
                        );
                    }
                    Err(err) => toast(&app_state, err.to_string()),
                }
                regenerating_icon.set(None);
            });
        })
    };

    let on_load_topic = {
        let app_state = app_state.clone();
        let loading_topic = loading_topic.clone();
        let navigator = navigator.clone();
        Callback::from(move |topic: AppTopicInfo| {
            if loading_topic.is_some() {
                return;
            }
            loading_topic.set(Some(topic.id));
            let app_state = app_state.clone();
            let loading_topic = loading_topic.clone();
            let navigator = navigator.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = ForceDailyRefreshReq {
                    topics: topic.name,
                    tipcard_type: Some(topic.tipcard_type),
                };
                let result = Request::post("/app/daily-refresh")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await;
                match result {
                    Ok(res) if res.ok() => match res.json::<ForceDailyRefreshRes>().await {
                        Ok(result) if result.refreshed_cards > 0 => {
                            toast(&app_state, "New card loaded");
                            if let Some(nav) = navigator {
                                nav.push(&View::Flow);
                            }
                        }
                        Ok(_) => toast(&app_state, "No card loaded: max active cards reached"),
                        Err(_) => toast(&app_state, "Failed to read refresh response"),
                    },
                    Ok(res) => toast(
                        &app_state,
                        res.text()
                            .await
                            .unwrap_or_else(|_| "Failed to load card".to_string()),
                    ),
                    Err(err) => toast(&app_state, err.to_string()),
                }
                loading_topic.set(None);
            });
        })
    };

    let on_dialog_close = {
        let confirm_delete = confirm_delete.clone();
        Callback::from(move |_| {
            confirm_delete.set(None);
        })
    };

    let on_cancel_delete = {
        let confirm_delete = confirm_delete.clone();
        Callback::from(move |_| {
            confirm_delete.set(None);
        })
    };

    let on_confirm_delete = {
        let confirm_delete = confirm_delete.clone();
        let app_state = app_state.clone();
        let refresh_topics = refresh_topics.clone();
        Callback::from(move |_| {
            if let Some(topic) = &*confirm_delete {
                let app_state = app_state.clone();
                let refresh_topics = refresh_topics.clone();
                let confirm_delete = confirm_delete.clone();
                let req = DeleteTopicReq { id: topic.id };
                wasm_bindgen_futures::spawn_local(async move {
                    match Request::delete("/app/topics")
                        .json(&req)
                        .unwrap()
                        .send()
                        .await
                    {
                        Ok(res) if res.ok() => {
                            toast(&app_state, "Topic deleted");
                            refresh_topics.emit(());
                            confirm_delete.set(None);
                        }
                        Ok(res) => {
                            toast(
                                &app_state,
                                res.text()
                                    .await
                                    .unwrap_or_else(|_| "Failed to delete topic".to_string()),
                            );
                            confirm_delete.set(None);
                        }
                        Err(err) => {
                            toast(&app_state, err.to_string());
                            confirm_delete.set(None);
                        }
                    }
                });
            }
        })
    };

    let filtered_topics: Vec<_> = topics
        .iter()
        .filter(|topic| {
            let q = search.to_lowercase();
            q.is_empty()
                || topic.name.to_lowercase().contains(&q)
                || topic.tipcard_type.to_lowercase().contains(&q)
        })
        .cloned()
        .collect();

    let on_add_document = {
        let doc_title = doc_title.clone();
        let doc_url = doc_url.clone();
        let doc_content = doc_content.clone();
        let doc_source_type = doc_source_type.clone();
        let refresh = {
            let documents = documents.clone();
            Callback::from(move |_| {
                let documents = documents.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    if let Ok(res) = Request::get("/app/documents").send().await {
                        if let Ok(data) = res.json::<Vec<DocumentInfo>>().await {
                            documents.set(data);
                        }
                    }
                });
            })
        };
        let app_state = app_state.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let title = (*doc_title).clone();
            let url_val = (*doc_url).clone();
            let content = (*doc_content).clone();
            let source_type = (*doc_source_type).clone();
            let refresh = refresh.clone();
            let doc_title = doc_title.clone();
            let doc_url = doc_url.clone();
            let doc_content = doc_content.clone();
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = AddDocumentReq {
                    topic_ids: Vec::new(),
                    source_type: source_type.clone(),
                    title,
                    url: if url_val.is_empty() {
                        None
                    } else {
                        Some(url_val)
                    },
                    content,
                };
                match Request::post("/app/documents")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        toast(&app_state, "Document added");
                        doc_title.set(String::new());
                        doc_url.set(String::new());
                        doc_content.set(String::new());
                        refresh.emit(());
                    }
                    _ => toast(&app_state, "Failed to add document"),
                }
            });
        })
    };

    let on_add_bulk_links = {
        let bulk_urls = bulk_urls.clone();
        let bulk_progress = bulk_progress.clone();
        let app_state = app_state.clone();
        let documents = documents.clone();
        Callback::from(move |_: MouseEvent| {
            let raw = (*bulk_urls).clone();
            let urls: Vec<String> = raw
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(|l| l.to_string())
                .collect();
            if urls.is_empty() {
                toast(&app_state, "No URLs to add");
                return;
            }
            let total = urls.len();
            let bulk_progress = bulk_progress.clone();
            let bulk_urls = bulk_urls.clone();
            let app_state = app_state.clone();
            let documents = documents.clone();
            bulk_progress.set(Some((0, total)));
            wasm_bindgen_futures::spawn_local(async move {
                let mut ok = 0usize;
                let mut fail = 0usize;
                for (i, url) in urls.iter().enumerate() {
                    bulk_progress.set(Some((i, total)));
                    let title = title_from_url(url);
                    let req = AddDocumentReq {
                        topic_ids: Vec::new(),
                        source_type: "link".into(),
                        title,
                        url: Some(url.clone()),
                        content: String::new(),
                    };
                    match Request::post("/app/documents")
                        .json(&req)
                        .unwrap()
                        .send()
                        .await
                    {
                        Ok(res) if res.ok() => ok += 1,
                        _ => fail += 1,
                    }
                }
                bulk_progress.set(Some((total, total)));
                if let Ok(res) = Request::get("/app/documents").send().await {
                    if let Ok(data) = res.json::<Vec<DocumentInfo>>().await {
                        documents.set(data);
                    }
                }
                bulk_progress.set(None);
                bulk_urls.set(String::new());
                toast(
                    &app_state,
                    format!(
                        "Imported {} link(s){}",
                        ok,
                        if fail > 0 {
                            format!(", {} failed", fail)
                        } else {
                            String::new()
                        }
                    ),
                );
            });
        })
    };

    let on_delete_document = {
        let documents = documents.clone();
        let app_state = app_state.clone();
        Callback::from(move |id: i64| {
            let documents = documents.clone();
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = DeleteDocumentReq { id };
                match Request::delete("/app/documents")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        toast(&app_state, "Document deleted");
                        if let Ok(res) = Request::get("/app/documents").send().await {
                            if let Ok(data) = res.json::<Vec<DocumentInfo>>().await {
                                documents.set(data);
                            }
                        }
                    }
                    _ => toast(&app_state, "Failed to delete document"),
                }
            });
        })
    };

    let on_view_document = {
        let viewing_document = viewing_document.clone();
        let app_state = app_state.clone();
        Callback::from(move |id: i64| {
            let viewing_document = viewing_document.clone();
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match Request::get(&format!("/app/documents/{}", id)).send().await {
                    Ok(res) if res.ok() => {
                        if let Ok(doc) = res.json::<DocumentDetail>().await {
                            viewing_document.set(Some(doc));
                        }
                    }
                    _ => toast(&app_state, "Failed to load document"),
                }
            });
        })
    };

    let on_doc_file_selected = {
        let doc_file_data = doc_file_data.clone();
        let doc_file_name = doc_file_name.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            let doc_file_data = doc_file_data.clone();
            let doc_file_name = doc_file_name.clone();
            if let Some(files) = input.files() {
                if let Some(file) = files.get(0) {
                    let name = file.name();
                    let reader = FileReader::new().unwrap();
                    let reader_clone = reader.clone();
                    let closure = Closure::wrap(Box::new(move || {
                        if let Ok(result) = reader_clone.result() {
                            if let Some(data_url) = result.as_string() {
                                doc_file_data.set(data_url);
                            }
                        }
                    }) as Box<dyn FnMut()>);
                    reader.set_onloadend(Some(closure.as_ref().unchecked_ref()));
                    let _ = reader.read_as_data_url(&file);
                    closure.forget();
                    doc_file_name.set(name);
                }
            }
        })
    };

    let on_upload_doc_file = {
        let doc_file_data = doc_file_data.clone();
        let doc_file_name = doc_file_name.clone();
        let doc_file_uploading = doc_file_uploading.clone();
        let app_state = app_state.clone();
        let documents = documents.clone();
        Callback::from(move |_: MouseEvent| {
            let data_url = (*doc_file_data).clone();
            let filename = (*doc_file_name).clone();
            if data_url.is_empty() || filename.is_empty() {
                toast(&app_state, "Choose a file first");
                return;
            }
            let doc_file_data = doc_file_data.clone();
            let doc_file_name = doc_file_name.clone();
            let doc_file_uploading = doc_file_uploading.clone();
            let app_state = app_state.clone();
            let documents = documents.clone();
            doc_file_uploading.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                let req = UploadDocumentReq {
                    filename: filename.clone(),
                    title: None,
                    data_url,
                    topic_ids: Vec::new(),
                };
                match Request::post("/app/documents/upload")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        toast(&app_state, "File uploaded");
                        doc_file_data.set(String::new());
                        doc_file_name.set(String::new());
                        if let Ok(res) = Request::get("/app/documents").send().await {
                            if let Ok(data) = res.json::<Vec<DocumentInfo>>().await {
                                documents.set(data);
                            }
                        }
                    }
                    _ => toast(&app_state, "Failed to upload file"),
                }
                doc_file_uploading.set(false);
            });
        })
    };

    let on_add_image = {
        let img_name = img_name.clone();
        let img_data = img_data.clone();
        let file_input_ref = file_input_ref.clone();
        let refresh = {
            let pool_images = pool_images.clone();
            Callback::from(move |_| {
                let pool_images = pool_images.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    if let Ok(res) = Request::get("/app/image-pool").send().await {
                        if let Ok(data) = res.json::<Vec<PoolImageInfo>>().await {
                            pool_images.set(data);
                        }
                    }
                });
            })
        };
        let app_state = app_state.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let name = (*img_name).clone();
            let image_data = (*img_data).clone();
            let refresh = refresh.clone();
            let img_name = img_name.clone();
            let img_data = img_data.clone();
            let file_input_ref = file_input_ref.clone();
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = AddPoolImageReq { image_data, name };
                match Request::post("/app/image-pool")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        toast(&app_state, "Image added");
                        img_name.set(String::new());
                        img_data.set(String::new());
                        if let Some(input) = file_input_ref.cast::<HtmlInputElement>() {
                            input.set_value("");
                        }
                        refresh.emit(());
                    }
                    Ok(res) => {
                        let message = res
                            .text()
                            .await
                            .unwrap_or_else(|_| "Failed to add image".to_string());
                        toast(&app_state, message);
                    }
                    Err(err) => toast(&app_state, err.to_string()),
                }
            });
        })
    };

    let on_delete_image = {
        let pool_images = pool_images.clone();
        let app_state = app_state.clone();
        Callback::from(move |id: i64| {
            let pool_images = pool_images.clone();
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = DeletePoolImageReq { id };
                match Request::delete("/app/image-pool")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        toast(&app_state, "Image deleted");
                        if let Ok(res) = Request::get("/app/image-pool").send().await {
                            if let Ok(data) = res.json::<Vec<PoolImageInfo>>().await {
                                pool_images.set(data);
                            }
                        }
                    }
                    _ => toast(&app_state, "Failed to delete image"),
                }
            });
        })
    };

    let on_rename_image = {
        let pool_images = pool_images.clone();
        let editing_image = editing_image.clone();
        let rename_name = rename_name.clone();
        let rename_desc = rename_desc.clone();
        let app_state = app_state.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let Some(img) = (*editing_image).clone() else {
                return;
            };
            let name = (*rename_name).clone();
            let description = {
                let d = (*rename_desc).clone();
                if d.is_empty() { None } else { Some(d) }
            };
            let pool_images = pool_images.clone();
            let editing_image = editing_image.clone();
            let rename_name = rename_name.clone();
            let rename_desc = rename_desc.clone();
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = RenamePoolImageReq {
                    id: img.id,
                    name,
                    description,
                };
                match Request::patch("/app/image-pool")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        toast(&app_state, "Image renamed");
                        editing_image.set(None);
                        rename_name.set(String::new());
                        rename_desc.set(String::new());
                        if let Ok(res) = Request::get("/app/image-pool").send().await {
                            if let Ok(data) = res.json::<Vec<PoolImageInfo>>().await {
                                pool_images.set(data);
                            }
                        }
                    }
                    _ => toast(&app_state, "Failed to rename image"),
                }
            });
        })
    };

    let on_remove_tag = {
        let pool_images = pool_images.clone();
        let app_state = app_state.clone();
        Callback::from(move |(id, tag): (i64, String)| {
            let pool_images = pool_images.clone();
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = RemovePoolImageTagReq { id, tag };
                match Request::delete("/app/image-pool/tag")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        if let Ok(res) = Request::get("/app/image-pool").send().await {
                            if let Ok(data) = res.json::<Vec<PoolImageInfo>>().await {
                                pool_images.set(data);
                            }
                        }
                    }
                    _ => toast(&app_state, "Failed to remove tag"),
                }
            });
        })
    };

    let on_start_rename = {
        let editing_image = editing_image.clone();
        let rename_name = rename_name.clone();
        let rename_desc = rename_desc.clone();
        Callback::from(move |img: PoolImageInfo| {
            rename_name.set(img.name.clone());
            rename_desc.set(img.description.clone().unwrap_or_default());
            editing_image.set(Some(img));
        })
    };

    let on_file_selected = {
        let img_data = img_data.clone();
        let img_name = img_name.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            let img_data = img_data.clone();
            let img_name = img_name.clone();
            if let Some(files) = input.files() {
                if let Some(file) = files.get(0) {
                    let name = file.name();
                    let reader = FileReader::new().unwrap();
                    let reader_clone = reader.clone();
                    let closure = Closure::wrap(Box::new(move || {
                        if let Ok(result) = reader_clone.result() {
                            if let Some(data_url) = result.as_string() {
                                img_data.set(data_url);
                            }
                        }
                    }) as Box<dyn FnMut()>);
                    reader.set_onloadend(Some(closure.as_ref().unchecked_ref()));
                    let _ = reader.read_as_data_url(&file);
                    closure.forget();
                    if (*img_name).is_empty() {
                        img_name.set(name);
                    }
                }
            }
        })
    };

    html! {
        <section id="view-grounding">
            <div class="mb-4">
                <h1 class="text-xl font-semibold tracking-tight">{"Grounding"}</h1>
                <p class="text-muted mt-2">{"Cards, topics, and sources."}</p>
            </div>

            <div class="grid grid-cols-2 md:grid-cols-4 gap-2 sm:gap-3 mb-3 sm:mb-4" id="stats-grid">
                { if let Some(s) = &*summary {
                    html! {
                        <>
                            <div class="surface border rounded-md p-4 flex flex-col justify-center">
                                <div class="text-3xl font-bold text-primary">{s.due_cards}</div>
                                <div class="text-sm font-medium text-muted mt-1">{"Due Now"}</div>
                            </div>
                            <div class="surface border rounded-md p-4 flex flex-col justify-center">
                                <div class="text-3xl font-bold">{s.active_cards}</div>
                                <div class="text-sm font-medium text-muted mt-1">{"Active Queue"}</div>
                            </div>
                            <div class="surface border rounded-md p-4 flex flex-col justify-center">
                                <div class="text-3xl font-bold">{s.total_cards}</div>
                                <div class="text-sm font-medium text-muted mt-1">{"Total Cards"}</div>
                            </div>
                            <div class="surface border rounded-md p-4 flex flex-col justify-center">
                                <div class="text-3xl font-bold">{s.topics}</div>
                                <div class="text-sm font-medium text-muted mt-1">{"Topics"}</div>
                            </div>
                        </>
                    }
                } else {
                    html! { <div class="col-span-full surface border rounded-md p-4 text-center text-muted">{"Loading stats..."}</div> }
                }}
            </div>

            <div id="token-spend-row" class="grid grid-cols-1 sm:grid-cols-3 gap-2 sm:gap-3 mb-4">
                { if let Some(t) = &*token_spend {
                    html! {
                        <>
                            <div class="muted-surface border border-token rounded-md p-3 flex justify-between items-center">
                                <span class="text-sm font-medium text-muted">{"Daily Spend"}</span>
                                <span class="font-semibold text-primary">{format!("{} tokens", t.daily)}</span>
                            </div>
                            <div class="muted-surface border border-token rounded-md p-3 flex justify-between items-center">
                                <span class="text-sm font-medium text-muted">{"Monthly"}</span>
                                <span class="font-semibold">{format!("{} tokens", t.monthly)}</span>
                            </div>
                            <div class="muted-surface border border-token rounded-md p-3 flex justify-between items-center">
                                <span class="text-sm font-medium text-muted">{"Total Lifetime"}</span>
                                <span class="font-semibold">{format!("{} tokens", t.total)}</span>
                            </div>
                        </>
                    }
                } else {
                    html! {}
                }}
            </div>

            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-3 mb-4">
                <h2 class="text-xl font-semibold flex items-center gap-2">
                    <iconify-icon icon="radix-icons:layers" class="radix-icon text-primary" aria-hidden="true"></iconify-icon>
                    {"Active Topics"}
                </h2>
                <div class="flex gap-2">
                    <input
                        id="topic-search"
                        class="rounded-md border px-4 py-2 w-full sm:w-72"
                        placeholder="Find topic"
                        aria-label="Find topic"
                        value={(*search).clone()}
                        oninput={Callback::from({ let search = search.clone(); move |e: InputEvent| {
                            if let Some(target) = e.target_dyn_into::<HtmlInputElement>() {
                                search.set(target.value());
                            }
                        }})}
                    />
                    <ShadcnButton
                        variant={ButtonVariant::Default}
                        size={ButtonSize::Icon}
                        onclick={Callback::from({
                            let navigator = navigator.clone();
                            move |_: MouseEvent| {
                                if let Some(nav) = navigator.clone() {
                                    nav.push(&View::Flow);
                                }
                            }
                        })}
                    >
                        <iconify-icon icon="radix-icons:plus" class="radix-icon" aria-hidden="true"></iconify-icon>
                        <span class="sr-only">{"New topic"}</span>
                    </ShadcnButton>
                </div>
            </div>

            <div id="topics-grid" class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4 gap-3 mb-8">
                {
                    if filtered_topics.is_empty() {
                        html! { <div class="col-span-full surface border rounded-md p-10 text-center text-muted">{"No topics found."}</div> }
                    } else {
                        html! {
                            for filtered_topics.iter().map(|t| {
                                let topic_for_edit = t.clone();
                                let topic_for_load = t.clone();
                                let topic_for_delete = t.clone();
                                let topic_id = t.id;
                                let icon_loading = *regenerating_icon == Some(topic_id);
                                let card_loading = *loading_topic == Some(topic_id);
                                let on_regenerate_icon = on_regenerate_icon.clone();
                                let on_load_topic = on_load_topic.clone();
                                let pending_topic_name = t.name.clone();
                                let pending_count = t.pending_cards;
                                let pending_navigator = navigator.clone();
                                html! {
                                <div class="surface border rounded-md p-4 flex flex-col">
                                    <div class="flex justify-between items-start mb-2 gap-2">
                                        <h3 class="font-semibold text-lg truncate flex items-center gap-2 min-w-0">
                                            <ShadcnTooltip content="Pick new icon with AI">
                                            <button
                                                type="button"
                                                class="topic-icon-btn shrink-0 inline-flex items-center justify-center rounded-sm border border-transparent hover:border-token disabled:opacity-50"
                                                disabled={icon_loading}
                                                onclick={Callback::from(move |_: MouseEvent| on_regenerate_icon.emit(topic_id))}
                                            >
                                                if icon_loading {
                                                    <iconify-icon
                                                        icon="radix-icons:reload"
                                                        class="topic-icon radix-icon animate-spin opacity-70"
                                                        aria-hidden="true"
                                                    ></iconify-icon>
                                                } else {
                                                    <iconify-icon
                                                        icon={display_icon(&t.icon_id).to_string()}
                                                        class="topic-icon radix-icon"
                                                        style={format!("color: {}", t.topic_color)}
                                                        aria-hidden="true"
                                                    ></iconify-icon>
                                                }
                                            </button>
                                            </ShadcnTooltip>
                                            <span class="truncate">{&t.name}</span>
                                        </h3>
                                        <span class="badge shrink-0">{tip_type_label(&i18n, &t.tipcard_type)}</span>
                                    </div>
                                    <div class="text-sm text-muted">
                                        {format!("{} due / {} total", t.due_cards, t.total_cards)}
                                    </div>
                                    <ShadcnButton
                                        variant={ButtonVariant::Outline}
                                        size={ButtonSize::Sm}
                                        class="mt-3 w-full"
                                        disabled={pending_count == 0}
                                        onclick={Callback::from(move |_: MouseEvent| {
                                            if pending_count == 0 {
                                                return;
                                            }
                                            if let Some(nav) = pending_navigator.clone() {
                                                let _ = nav.push_with_query(
                                                    &View::Archive,
                                                    &ArchiveQuery {
                                                        status: Some("pending".to_string()),
                                                        topic: Some(pending_topic_name.clone()),
                                                    },
                                                );
                                            }
                                        })}
                                    >
                                        <iconify-icon icon="radix-icons:eye-open" class="radix-icon" aria-hidden="true"></iconify-icon>
                                        {format!("Show {pending_count} pending cards")}
                                    </ShadcnButton>
                                    <div class="mt-3 grid grid-cols-3 gap-2">
                                        <ShadcnButton
                                            variant={ButtonVariant::Secondary}
                                            size={ButtonSize::Sm}
                                            disabled={loading_topic.is_some()}
                                            onclick={Callback::from(move |_: MouseEvent| on_load_topic.emit(topic_for_load.clone()))}
                                        >
                                            if card_loading {
                                                <iconify-icon icon="radix-icons:reload" class="radix-icon animate-spin" aria-hidden="true"></iconify-icon>
                                                {"Loading..."}
                                            } else {
                                                {"Load"}
                                            }
                                        </ShadcnButton>
                                        <ShadcnButton
                                            variant={ButtonVariant::Outline}
                                            size={ButtonSize::Sm}
                                            onclick={Callback::from({
                                                let editing = editing.clone();
                                                move |_: MouseEvent| editing.set(Some(topic_for_edit.clone()))
                                            })}
                                        >
                                            {"Edit"}
                                        </ShadcnButton>
                                        <ShadcnButton
                                            variant={ButtonVariant::Destructive}
                                            size={ButtonSize::Sm}
                                            onclick={Callback::from({
                                                let confirm_delete = confirm_delete.clone();
                                                let topic = topic_for_delete.clone();
                                                move |_: MouseEvent| {
                                                    confirm_delete.set(Some(topic.clone()));
                                                }
                                            })}
                                        >
                                            {"Delete"}
                                        </ShadcnButton>
                                    </div>
                                </div>
                                }
                            })
                        }
                    }
                }
            </div>

            <div class="space-y-6">
                <div>
                    <h2 class="text-xl font-semibold flex items-center gap-2">
                        <iconify-icon icon="radix-icons:file-text" class="radix-icon text-primary" aria-hidden="true"></iconify-icon>
                        {"Sources"}
                    </h2>
                    <p class="text-sm text-muted">{"Documents and images used for grounding and card illustration."}</p>
                </div>

                <div class="grid grid-cols-1 lg:grid-cols-2 gap-6 items-start">
                    <div class="hidden" aria-hidden="true">
                        <div>
                            <h3 class="text-lg font-semibold flex items-center gap-2">
                                <iconify-icon icon="radix-icons:file-text" class="radix-icon text-primary" aria-hidden="true"></iconify-icon>
                                {"Documents"}
                            </h3>
                            <p class="text-sm text-muted">{"Chunked and indexed for retrieval."}</p>
                        </div>

                        <form onsubmit={on_add_document} class="space-y-3">
                            <div class="space-y-1">
                                <label class="text-sm font-medium">{"Source Type"}</label>
                                <ShadcnSelect
                                    value={(*doc_source_type).clone()}
                                    onchange={Callback::from({
                                        let s = doc_source_type.clone();
                                        move |value: String| s.set(value)
                                    })}
                                    options={vec![
                                        SelectOption { value: "document".into(), label: "Document (paste text)".into() },
                                        SelectOption { value: "link".into(), label: "Link (fetch from URL)".into() },
                                    ]}
                                />
                                { if *doc_source_type == "link" {
                                    html! { <p class="text-xs text-muted">{"The server fetches the URL body and indexes it. Leave Content empty to auto-fetch."}</p> }
                                } else {
                                    html! { <p class="text-xs text-muted">{"Paste text directly. URL is optional metadata."}</p> }
                                } }
                            </div>
                            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                                <div class="space-y-1">
                                    <label class="text-sm font-medium">{"Title"}</label>
                                    <input
                                        type="text"
                                        class="w-full rounded-md border px-3 py-2 h-10"
                                        placeholder="My Document"
                                        value={(*doc_title).clone()}
                                        oninput={{
                                            let s = doc_title.clone();
                                            Callback::from(move |e: InputEvent| {
                                                let t: HtmlInputElement = e.target_unchecked_into();
                                                s.set(t.value());
                                            })
                                        }}
                                    />
                                </div>
                                <div class="space-y-1">
                                    <label class="text-sm font-medium">
                                        { if *doc_source_type == "link" { "URL" } else { "URL (optional)" } }
                                    </label>
                                    <input
                                        type="text"
                                        class="w-full rounded-md border px-3 py-2 h-10"
                                        placeholder="https://…"
                                        value={(*doc_url).clone()}
                                        oninput={{
                                            let s = doc_url.clone();
                                            Callback::from(move |e: InputEvent| {
                                                let t: HtmlInputElement = e.target_unchecked_into();
                                                s.set(t.value());
                                            })
                                        }}
                                    />
                                </div>
                            </div>
                            <div class="space-y-1">
                                <label class="text-sm font-medium">{"Content"}</label>
                                <textarea
                                    class="w-full rounded-md border px-3 py-2 min-h-[100px] resize-y"
                                    placeholder={ if *doc_source_type == "link" { "Leave empty to fetch from URL automatically…" } else { "Paste document text here…" } }
                                    value={(*doc_content).clone()}
                                    oninput={{
                                        let s = doc_content.clone();
                                        Callback::from(move |e: InputEvent| {
                                            let t: HtmlTextAreaElement = e.target_unchecked_into();
                                            s.set(t.value());
                                        })
                                    }}
                                />
                            </div>
                            <div class="flex justify-end">
                                <ShadcnButton r#type={ButtonType::Submit}>
                                    <iconify-icon icon="radix-icons:plus" class="radix-icon" aria-hidden="true"></iconify-icon>
                                    { if *doc_source_type == "link" { "Add Link" } else { "Add Document" } }
                                </ShadcnButton>
                            </div>
                        </form>

                        <div class="border-t border-token pt-4 space-y-3">
                            <div>
                                <label class="text-sm font-medium">{"Bulk Import Links"}</label>
                                <p class="text-xs text-muted">{"One URL per line. Titles are auto-derived from the URL path. The server fetches each page."}</p>
                            </div>
                            <textarea
                                class="w-full rounded-md border px-3 py-2 min-h-[80px] resize-y font-mono text-sm"
                                placeholder={"https://helix-editor.vercel.app/start-here/installation/\nhttps://helix-editor.vercel.app/reference/typed-commands/\n…"}
                                value={(*bulk_urls).clone()}
                                oninput={{
                                    let s = bulk_urls.clone();
                                    Callback::from(move |e: InputEvent| {
                                        let t: HtmlTextAreaElement = e.target_unchecked_into();
                                        s.set(t.value());
                                    })
                                }}
                            />
                            <div class="flex items-center justify-between">
                                { if let Some((done, total)) = *bulk_progress {
                                    html! { <span class="text-sm text-muted">{format!("Importing {} / {}…", done, total)}</span> }
                                } else {
                                    html! { <span></span> }
                                } }
                                <ShadcnButton
                                    variant={ButtonVariant::Secondary}
                                    onclick={on_add_bulk_links}
                                    disabled={(*bulk_progress).is_some()}
                                >
                                    <iconify-icon icon="radix-icons:link-2" class="radix-icon" aria-hidden="true"></iconify-icon>
                                    {"Import Links"}
                                </ShadcnButton>
                            </div>
                        </div>

                        <div class="border-t border-token pt-4 space-y-3">
                            <div>
                                <label class="text-sm font-medium">{"Upload File"}</label>
                                <p class="text-xs text-muted">{"PDF, HTML, or text files. Text is extracted server-side and indexed for retrieval."}</p>
                            </div>
                            <div class="flex items-center gap-3">
                                <input
                                    type="file"
                                    accept=".pdf,.html,.htm,.txt,text/plain,text/html,application/pdf"
                                    class="hidden"
                                    ref={doc_file_input_ref}
                                    onchange={on_doc_file_selected}
                                />
                                <ShadcnButton
                                    variant={ButtonVariant::Secondary}
                                    onclick={Callback::from(move |_: MouseEvent| {
                                        if let Some(input) = doc_file_input_ref_for_click.cast::<HtmlInputElement>() {
                                            let _ = input.click();
                                        }
                                    })}
                                    disabled={*doc_file_uploading}
                                >
                                    <iconify-icon icon="radix-icons:upload" class="radix-icon" aria-hidden="true"></iconify-icon>
                                    {"Choose File"}
                                </ShadcnButton>
                                <span class="text-sm text-muted truncate">
                                    { if (*doc_file_name).is_empty() { "No file chosen".to_string() } else { (*doc_file_name).clone() } }
                                </span>
                                { if !(*doc_file_data).is_empty() && !(*doc_file_name).is_empty() {
                                    html! {
                                        <ShadcnButton
                                            variant={ButtonVariant::Default}
                                            onclick={on_upload_doc_file}
                                            disabled={*doc_file_uploading}
                                        >
                                            { if *doc_file_uploading { "Uploading…" } else { "Upload" } }
                                        </ShadcnButton>
                                    }
                                } else {
                                    html! {}
                                } }
                            </div>
                        </div>
                        <div class="border-t border-token pt-4 space-y-3">
                            <div>
                                <label class="text-sm font-medium">{"Indexed Documents"}</label>
                                <p class="text-xs text-muted">{"Added documents appear here."}</p>
                            </div>

                        { if !*sources_loaded {
                            html! { <p class="text-sm text-muted">{"Loading documents…"}</p> }
                        } else if documents.is_empty() {
                            html! { <p class="text-sm text-muted italic">{"No documents yet."}</p> }
                        } else {
                            html! {
                                <div class="space-y-2">
                                    { for documents.iter().map(|doc| {
                                        let id = doc.id;
                                        let on_delete = on_delete_document.clone();
                                        let on_view = on_view_document.clone();
                                        html! {
                                            <div class="flex items-start gap-3 p-3 surface border rounded-md">
                                                <iconify-icon
                                                    icon={ if doc.source_type == "link" { "radix-icons:link-2" } else { "radix-icons:file-text" } }
                                                    class="radix-icon text-muted shrink-0 mt-0.5"
                                                    aria-hidden="true"
                                                ></iconify-icon>
                                                <div class="flex-1 min-w-0">
                                                    <div class="font-medium truncate">
                                                        {&doc.title}
                                                        <span class="badge ml-2 text-xs">{ if doc.source_type == "link" { "link" } else { "doc" } }</span>
                                                    </div>
                                                    <div class="text-xs text-muted">
                                                        { doc.url.as_ref().map(|url| html! {
                                                            <a href={url.clone()} target="_blank" class="underline mr-2">{url}</a>
                                                        }) }
                                                        <span>{&doc.created_at}</span>
                                                    </div>
                                                </div>
                                                <ShadcnButton
                                                    variant={ButtonVariant::Ghost}
                                                    size={ButtonSize::Icon}
                                                    onclick={Callback::from(move |_: MouseEvent| on_view.emit(id))}
                                                >
                                                    <iconify-icon icon="radix-icons:eye-open" class="radix-icon" aria-hidden="true"></iconify-icon>
                                                    <span class="sr-only">{"View"}</span>
                                                </ShadcnButton>
                                                <ShadcnButton
                                                    variant={ButtonVariant::Ghost}
                                                    size={ButtonSize::Icon}
                                                    onclick={Callback::from(move |_: MouseEvent| on_delete.emit(id))}
                                                >
                                                    <iconify-icon icon="radix-icons:trash" class="radix-icon text-destructive" aria-hidden="true"></iconify-icon>
                                                    <span class="sr-only">{"Delete"}</span>
                                                </ShadcnButton>
                                            </div>
                                        }
                                    }) }
                                </div>
                            }
                        } }
                        </div>
                    </div>

                    <div class="surface border rounded-md p-4 space-y-4">
                        <div>
                            <h3 class="text-lg font-semibold flex items-center gap-2">
                                <iconify-icon icon="radix-icons:image" class="radix-icon text-primary" aria-hidden="true"></iconify-icon>
                                {"Local Image Pool"}
                            </h3>
                            <p class="text-sm text-muted">{"Images available to the Local Image Pool source."}</p>
                        </div>

                        <form onsubmit={on_add_image} class="space-y-3">
                            <div class="space-y-1">
                                <label class="text-sm font-medium">{"Fallback Name"}</label>
                                <input
                                    type="text"
                                    class="w-full rounded-md border px-3 py-2 h-10"
                                    placeholder="sunset.jpg"
                                    value={(*img_name).clone()}
                                    oninput={{
                                        let s = img_name.clone();
                                        Callback::from(move |e: InputEvent| {
                                            let t: HtmlInputElement = e.target_unchecked_into();
                                            s.set(t.value());
                                        })
                                    }}
                                />
                                <div class="text-xs text-muted">{"Used if vision annotation is unavailable. Name, description, and tags are auto-generated by the vision model."}</div>
                            </div>
                            <div class="space-y-1">
                                <label class="text-sm font-medium">{"Image File"}</label>
                                <div class="flex items-center gap-3">
                                    <input
                                        type="file"
                                        accept="image/*"
                                        class="hidden"
                                        ref={file_input_ref}
                                        onchange={on_file_selected}
                                    />
                                    <ShadcnButton
                                        variant={ButtonVariant::Secondary}
                                        onclick={Callback::from(move |_: MouseEvent| {
                                            if let Some(input) = file_input_ref_for_click.cast::<HtmlInputElement>() {
                                                let _ = input.click();
                                            }
                                        })}
                                    >
                                        <iconify-icon icon="radix-icons:upload" class="radix-icon" aria-hidden="true"></iconify-icon>
                                        {"Choose File"}
                                    </ShadcnButton>
                                    <span class="text-sm text-muted truncate">
                                        { if (*img_data).is_empty() { "No file chosen".to_string() } else { (*img_name).clone() } }
                                    </span>
                                </div>
                            </div>
                            <div class="flex justify-end">
                                <ShadcnButton r#type={ButtonType::Submit}>
                                    <iconify-icon icon="radix-icons:plus" class="radix-icon" aria-hidden="true"></iconify-icon>
                                    {"Add Image"}
                                </ShadcnButton>
                            </div>
                        </form>

                        { if !*sources_loaded {
                            html! { <p class="text-sm text-muted">{"Loading images…"}</p> }
                        } else if pool_images.is_empty() {
                            html! { <p class="text-sm text-muted italic">{"No pool images yet."}</p> }
                        } else {
                            html! {
                                <>
                                    <div class="flex items-center justify-between">
                                        <span class="text-sm text-muted">
                                            {format!("{} image{}", pool_images.len(), if pool_images.len() == 1 { "" } else { "s" })}
                                        </span>
                                        <ShadcnTooltip content="Fullscreen">
                                            <button
                                                type="button"
                                                class="border border-token p-2"
                                                onclick={Callback::from({
                                                    let pool_expanded = pool_expanded.clone();
                                                    move |_: MouseEvent| pool_expanded.set(true)
                                                })}
                                            >
                                                <iconify-icon icon="radix-icons:enter-full-screen" class="radix-icon" aria-hidden="true"></iconify-icon>
                                            </button>
                                        </ShadcnTooltip>
                                    </div>
                                    <div class="max-h-[30rem] overflow-y-auto pr-1">
                                        <div class="grid grid-cols-2 gap-3">
                                            { for pool_images.iter().take(20).enumerate().map(|(idx, img)| {
                                                let id = img.id;
                                                let on_delete = on_delete_image.clone();
                                                let on_start_rename = on_start_rename.clone();
                                                let on_remove_tag = on_remove_tag.clone();
                                                let img_for_rename = img.clone();
                                                let img_url = format!("/app/pool-images/{}", id);
                                                let pool_lightbox_index = pool_lightbox_index.clone();
                                                html! {
                                                    <div class="surface border rounded-md p-3 flex flex-col text-center">
                                                        <button
                                                            type="button"
                                                            class="block w-full mb-2"
                                                            onclick={Callback::from(move |_: MouseEvent| pool_lightbox_index.set(Some(idx)))}
                                                        >
                                                            <img
                                                                src={img_url.clone()}
                                                                alt={img.name.clone()}
                                                                class="w-full h-24 object-cover rounded-md"
                                                                loading="lazy"
                                                            />
                                                        </button>
                                                        <div class="font-medium text-sm truncate w-full">{&img.name}</div>
                                                        { img.description.as_ref().map(|desc| html! {
                                                            <div class="text-xs text-muted truncate w-full">{desc}</div>
                                                        }) }
                                                        { if !img.tags.is_empty() {
                                                            html! {
                                                                <div class="flex flex-wrap gap-1 justify-center mt-1">
                                                                    { for img.tags.iter().map(|tag| {
                                                                        let tag_clone = tag.clone();
                                                                        let on_remove_tag = on_remove_tag.clone();
                                                                        html! {
                                                                            <span class="inline-flex items-center gap-1 rounded-full bg-token/10 text-xs px-2 py-0.5">
                                                                                {tag}
                                                                                <button
                                                                                    type="button"
                                                                                    class="text-muted hover:text-destructive"
                                                                                    onclick={Callback::from(move |_: MouseEvent| on_remove_tag.emit((id, tag_clone.clone())))}
                                                                                >
                                                                                    <iconify-icon icon="radix-icons:cross-2" class="radix-icon text-xs" aria-hidden="true"></iconify-icon>
                                                                                </button>
                                                                            </span>
                                                                        }
                                                                    }) }
                                                                </div>
                                                            }
                                                        } else {
                                                            html! {}
                                                        } }
                                                        <div class="mt-2 flex gap-2 justify-center">
                                                            <ShadcnButton
                                                                variant={ButtonVariant::Outline}
                                                                size={ButtonSize::Sm}
                                                                onclick={Callback::from(move |_: MouseEvent| on_start_rename.emit(img_for_rename.clone()))}
                                                            >
                                                                {"Rename"}
                                                            </ShadcnButton>
                                                            <ShadcnButton
                                                                variant={ButtonVariant::Destructive}
                                                                size={ButtonSize::Sm}
                                                                onclick={Callback::from(move |_: MouseEvent| on_delete.emit(id))}
                                                            >
                                                                {"Delete"}
                                                            </ShadcnButton>
                                                        </div>
                                                    </div>
                                                }
                                            }) }
                                        </div>
                                        { if pool_images.len() > 20 {
                                            html! {
                                                <div class="text-center text-sm text-muted mt-3">
                                                    {format!("Showing 20 of {} — click fullscreen to see all", pool_images.len())}
                                                </div>
                                            }
                                        } else {
                                            html! {}
                                        } }
                                    </div>
                                </>
                            }
                        } }
                    </div>
                </div>
            </div>

            <GroundingSettings />

            { if *pool_expanded {
                html! {
                    <div class="flow-card is-fullscreen fullscreen-card-enter surface border fixed top-0 right-0 bottom-0 z-[70] overflow-hidden flex flex-col p-6">
                        <div class="flex items-center justify-between mb-4">
                            <h3 class="text-lg font-semibold flex items-center gap-2">
                                <iconify-icon icon="radix-icons:image" class="radix-icon text-primary" aria-hidden="true"></iconify-icon>
                                {"Local Image Pool"}
                                <span class="text-sm text-muted font-normal">
                                    {format!("({} image{})", pool_images.len(), if pool_images.len() == 1 { "" } else { "s" })}
                                </span>
                            </h3>
                            <ShadcnTooltip content="Exit fullscreen">
                                <button
                                    type="button"
                                    class="border border-token p-2"
                                    onclick={Callback::from({
                                        let pool_expanded = pool_expanded.clone();
                                        move |_: MouseEvent| pool_expanded.set(false)
                                    })}
                                >
                                    <iconify-icon icon="radix-icons:exit-full-screen" class="radix-icon" aria-hidden="true"></iconify-icon>
                                </button>
                            </ShadcnTooltip>
                        </div>
                        <input
                            type="text"
                            class="w-full rounded-md border px-3 py-2 h-10 mb-4"
                            placeholder="Search by name, description, or tags…"
                            value={(*pool_search).clone()}
                            oninput={{
                                let s = pool_search.clone();
                                Callback::from(move |e: InputEvent| {
                                    let t: HtmlInputElement = e.target_unchecked_into();
                                    s.set(t.value());
                                })
                            }}
                        />
                        { if filtered_pool_images.is_empty() {
                            html! { <p class="text-sm text-muted italic">{"No images match your search."}</p> }
                        } else {
                            html! {
                                <div class="flex-1 overflow-y-auto pr-1">
                                    <div class="grid grid-cols-3 gap-3">
                                        { for filtered_pool_images.iter().enumerate().map(|(idx, img)| {
                                            let id = img.id;
                                            let on_delete = on_delete_image.clone();
                                            let on_start_rename = on_start_rename.clone();
                                            let on_remove_tag = on_remove_tag.clone();
                                            let img_for_rename = img.clone();
                                            let img_url = format!("/app/pool-images/{}", id);
                                            let pool_lightbox_index = pool_lightbox_index.clone();
                                            html! {
                                                <div class="surface border rounded-md p-3 flex flex-col text-center">
                                                    <button
                                                        type="button"
                                                        class="block w-full mb-2"
                                                        onclick={Callback::from(move |_: MouseEvent| pool_lightbox_index.set(Some(idx)))}
                                                    >
                                                        <img
                                                            src={img_url.clone()}
                                                            alt={img.name.clone()}
                                                            class="w-full h-28 object-cover rounded-md"
                                                            loading="lazy"
                                                        />
                                                    </button>
                                                    <div class="font-medium text-sm truncate w-full">{&img.name}</div>
                                                    { img.description.as_ref().map(|desc| html! {
                                                        <div class="text-xs text-muted truncate w-full">{desc}</div>
                                                    }) }
                                                    { if !img.tags.is_empty() {
                                                        html! {
                                                            <div class="flex flex-wrap gap-1 justify-center mt-1">
                                                                { for img.tags.iter().map(|tag| {
                                                                    let tag_clone = tag.clone();
                                                                    let on_remove_tag = on_remove_tag.clone();
                                                                    html! {
                                                                        <span class="inline-flex items-center gap-1 rounded-full bg-token/10 text-xs px-2 py-0.5">
                                                                            {tag}
                                                                            <button
                                                                                type="button"
                                                                                class="text-muted hover:text-destructive"
                                                                                onclick={Callback::from(move |_: MouseEvent| on_remove_tag.emit((id, tag_clone.clone())))}
                                                                            >
                                                                                <iconify-icon icon="radix-icons:cross-2" class="radix-icon text-xs" aria-hidden="true"></iconify-icon>
                                                                            </button>
                                                                        </span>
                                                                    }
                                                                }) }
                                                            </div>
                                                        }
                                                    } else {
                                                        html! {}
                                                    } }
                                                    <div class="mt-2 flex gap-2 justify-center">
                                                        <ShadcnButton
                                                            variant={ButtonVariant::Outline}
                                                            size={ButtonSize::Sm}
                                                            onclick={Callback::from(move |_: MouseEvent| on_start_rename.emit(img_for_rename.clone()))}
                                                        >
                                                            {"Rename"}
                                                        </ShadcnButton>
                                                        <ShadcnButton
                                                            variant={ButtonVariant::Destructive}
                                                            size={ButtonSize::Sm}
                                                            onclick={Callback::from(move |_: MouseEvent| on_delete.emit(id))}
                                                        >
                                                            {"Delete"}
                                                        </ShadcnButton>
                                                    </div>
                                                </div>
                                            }
                                        }) }
                                    </div>
                                </div>
                            }
                        } }
                    </div>
                }
            } else {
                html! {}
            } }

            { if let Some(index) = *pool_lightbox_index {
                let pool_image_urls: Vec<String> = if *pool_expanded {
                    filtered_pool_images.iter().map(|img| format!("/app/pool-images/{}", img.id)).collect()
                } else {
                    pool_images.iter().take(20).map(|img| format!("/app/pool-images/{}", img.id)).collect()
                };
                let on_close = {
                    let pool_lightbox_index = pool_lightbox_index.clone();
                    Callback::from(move |_| pool_lightbox_index.set(None))
                };
                html! {
                    <ImageLightbox
                        images={pool_image_urls}
                        initial_index={index}
                        on_close={on_close}
                    />
                }
            } else {
                html! {}
            } }

            if let Some(topic) = (*editing).clone() {
                <TopicEditor topic={topic} documents={(*documents).clone()} sources_loaded={*sources_loaded} on_refresh_documents={Callback::from({
                    let refresh = refresh.clone();
                    move |_| refresh.emit(())
                })} on_view_document={on_view_document.clone()} on_close={Callback::from({
                    let editing = editing.clone();
                    move |_| editing.set(None)
                })} on_saved={Callback::from({
                    let editing = editing.clone();
                    let refresh_topics = refresh_topics.clone();
                    move |_| {
                        editing.set(None);
                        refresh_topics.emit(());
                    }
                })} />
            }

            <dialog ref={dialog_ref} onclose={on_dialog_close} class="tailscale-dialog">
                if let Some(topic) = &*confirm_delete {
                    <div class="flex items-start gap-4">
                        <div class="flex-shrink-0 flex items-center justify-center size-10 rounded-full bg-destructive/10 text-destructive">
                            <iconify-icon icon="lucide:alert-triangle" class="text-xl"></iconify-icon>
                        </div>
                        <div class="flex-1">
                            <h3 class="text-lg font-semibold leading-6 text-foreground mb-1">
                                {"Delete topic"}
                            </h3>
                            <p class="text-sm text-muted mb-4">
                                {format!("Are you sure you want to delete topic \"{}\" and all its cards? This action cannot be undone.", topic.name)}
                            </p>
                            <div class="flex justify-end gap-3">
                                <ShadcnButton
                                    variant={ButtonVariant::Outline}
                                    onclick={on_cancel_delete}
                                >
                                    {"Cancel"}
                                </ShadcnButton>
                                <ShadcnButton
                                    variant={ButtonVariant::Destructive}
                                    onclick={on_confirm_delete}
                                >
                                    {"Delete"}
                                </ShadcnButton>
                            </div>
                        </div>
                    </div>
                }
            </dialog>
            <dialog ref={rename_dialog_ref} class="tailscale-dialog" onclose={Callback::from({
                let editing_image = editing_image.clone();
                move |_| editing_image.set(None)
            })}>
                if (*editing_image).is_some() {
                    <form onsubmit={on_rename_image} class="space-y-4">
                        <h3 class="text-lg font-semibold">{"Rename Image"}</h3>
                        <div class="space-y-1">
                            <label class="text-sm font-medium">{"Name"}</label>
                            <input
                                type="text"
                                class="w-full rounded-md border px-3 py-2 h-10"
                                value={(*rename_name).clone()}
                                oninput={{
                                    let s = rename_name.clone();
                                    Callback::from(move |e: InputEvent| {
                                        let t: HtmlInputElement = e.target_unchecked_into();
                                        s.set(t.value());
                                    })
                                }}
                            />
                        </div>
                        <div class="space-y-1">
                            <label class="text-sm font-medium">{"Description"}</label>
                            <input
                                type="text"
                                class="w-full rounded-md border px-3 py-2 h-10"
                                value={(*rename_desc).clone()}
                                oninput={{
                                    let s = rename_desc.clone();
                                    Callback::from(move |e: InputEvent| {
                                        let t: HtmlInputElement = e.target_unchecked_into();
                                        s.set(t.value());
                                    })
                                }}
                            />
                        </div>
                        <div class="flex justify-end gap-3">
                            <ShadcnButton
                                variant={ButtonVariant::Outline}
                                r#type={ButtonType::Button}
                                onclick={Callback::from({
                                    let editing_image = editing_image.clone();
                                    move |_: MouseEvent| editing_image.set(None)
                                })}
                            >
                                {"Cancel"}
                            </ShadcnButton>
                            <ShadcnButton r#type={ButtonType::Submit}>
                                {"Save"}
                            </ShadcnButton>
                        </div>
                    </form>
                }
            </dialog>

            <dialog ref={doc_viewer_ref} class="tailscale-dialog max-w-3xl" onclose={Callback::from({
                let viewing_document = viewing_document.clone();
                move |_| viewing_document.set(None)
            })}>
                { if let Some(doc) = &*viewing_document {
                    let viewing_document = viewing_document.clone();
                    html! {
                        <div class="space-y-4">
                            <div class="flex items-start justify-between gap-4">
                                <div class="min-w-0">
                                    <h3 class="text-lg font-semibold truncate">{&doc.title}</h3>
                                    <div class="text-xs text-muted mt-1">
                                        <span class="badge mr-2">{ if doc.source_type == "link" { "link" } else { "doc" } }</span>
                                        { doc.url.as_ref().map(|url| html! {
                                            <a href={url.clone()} target="_blank" class="underline mr-2">{url}</a>
                                        }) }
                                        <span>{&doc.created_at}</span>
                                    </div>
                                </div>
                                <ShadcnButton
                                    variant={ButtonVariant::Ghost}
                                    size={ButtonSize::Icon}
                                    onclick={Callback::from(move |_: MouseEvent| viewing_document.set(None))}
                                >
                                    <iconify-icon icon="radix-icons:cross-2" class="radix-icon" aria-hidden="true"></iconify-icon>
                                    <span class="sr-only">{"Close"}</span>
                                </ShadcnButton>
                            </div>
                            <div class="border-t pt-3">
                                <pre class="text-sm whitespace-pre-wrap break-words max-h-[60vh] overflow-y-auto font-mono p-3 surface rounded-md">
                                    {&doc.content}
                                </pre>
                            </div>
                        </div>
                    }
                } else {
                    html! {}
                } }
            </dialog>
        </section>
    }
}

#[derive(Properties, PartialEq)]
struct TopicEditorProps {
    topic: AppTopicInfo,
    documents: Vec<DocumentInfo>,
    sources_loaded: bool,
    on_refresh_documents: Callback<()>,
    on_view_document: Callback<i64>,
    on_close: Callback<()>,
    on_saved: Callback<()>,
}

fn source_links(input: &str) -> Vec<String> {
    let values = input.split_whitespace().collect::<Vec<_>>();
    if values.is_empty()
        || values
            .iter()
            .any(|value| !value.starts_with("https://") && !value.starts_with("http://"))
    {
        Vec::new()
    } else {
        values.into_iter().map(str::to_string).collect()
    }
}

fn link_title(url: &str) -> String {
    url.trim_end_matches('/')
        .strip_prefix("https://")
        .or_else(|| url.trim_end_matches('/').strip_prefix("http://"))
        .unwrap_or(url)
        .chars()
        .take(100)
        .collect()
}

fn source_detection_label(input: &str) -> String {
    let links = source_links(input);
    if links.is_empty() {
        "Detected: pasted document".to_string()
    } else if links.len() == 1 {
        "Detected: 1 link".to_string()
    } else {
        format!("Detected: {} links", links.len())
    }
}

fn source_button_label(input: &str, adding: bool) -> &'static str {
    if adding {
        "Adding..."
    } else if source_links(input).len() > 1 {
        "Add Links"
    } else if source_links(input).len() == 1 {
        "Add Link"
    } else {
        "Add Document"
    }
}

#[function_component(TopicEditor)]
fn topic_editor(props: &TopicEditorProps) -> Html {
    let app_state = use_context::<UseReducerHandle<AppState>>().unwrap();
    let i18n = use_i18n();
    let prompt_template = use_state(|| props.topic.prompt_template.clone());
    let daily_card_count = use_state(|| props.topic.daily_card_count.to_string());
    let daily_time_zone = use_state(|| props.topic.daily_time_zone.clone());
    let daily_update_time = use_state(|| props.topic.daily_update_time.clone());
    let compression_level = use_state(|| props.topic.compression_level.clone());
    let grounding_strategy = use_state(|| props.topic.grounding_strategy.clone());
    let image_strategy = use_state(|| props.topic.image_strategy.clone());
    let source_input = use_state(String::new);
    let source_adding = use_state(|| false);
    let source_exploring = use_state(|| false);
    let doc_file_data = use_state(String::new);
    let doc_file_name = use_state(String::new);
    let doc_file_uploading = use_state(|| false);
    let doc_file_input_ref = use_node_ref();

    let on_submit = {
        let app_state = app_state.clone();
        let on_saved = props.on_saved.clone();
        let topic_id = props.topic.id;
        let prompt_template = prompt_template.clone();
        let daily_card_count = daily_card_count.clone();
        let daily_time_zone = daily_time_zone.clone();
        let daily_update_time = daily_update_time.clone();
        let compression_level = compression_level.clone();
        let grounding_strategy = grounding_strategy.clone();
        let image_strategy = image_strategy.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let app_state = app_state.clone();
            let on_saved = on_saved.clone();
            let req = UpdateTopicReq {
                id: topic_id,
                prompt_template: Some((*prompt_template).clone()),
                daily_card_count: Some(daily_card_count.parse().unwrap_or(0)),
                daily_time_zone: Some((*daily_time_zone).clone()),
                daily_update_time: Some((*daily_update_time).clone()),
                compression_level: Some((*compression_level).clone()),
                grounding_strategy: Some((*grounding_strategy).clone()),
                image_strategy: Some((*image_strategy).clone()),
            };
            wasm_bindgen_futures::spawn_local(async move {
                match Request::patch("/app/topics")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        toast(&app_state, "Topic saved");
                        on_saved.emit(());
                    }
                    Ok(res) => toast(
                        &app_state,
                        res.text()
                            .await
                            .unwrap_or_else(|_| "Failed to save topic".to_string()),
                    ),
                    Err(err) => toast(&app_state, err.to_string()),
                }
            });
        })
    };

    let refresh_documents = props.on_refresh_documents.clone();
    let on_add_document = {
        let app_state = app_state.clone();
        let source_input = source_input.clone();
        let source_adding = source_adding.clone();
        let refresh_documents = refresh_documents.clone();
        let topic_id = props.topic.id;
        Callback::from(move |_: MouseEvent| {
            let input = source_input.trim().to_string();
            if input.is_empty() {
                return;
            }
            let links = source_links(&input);
            let requests = if links.is_empty() {
                vec![AddDocumentReq {
                    topic_ids: vec![topic_id],
                    source_type: "document".to_string(),
                    title: String::new(),
                    url: None,
                    content: input,
                }]
            } else {
                links
                    .into_iter()
                    .map(|url| AddDocumentReq {
                        topic_ids: vec![topic_id],
                        source_type: "link".to_string(),
                        title: link_title(&url),
                        url: Some(url),
                        content: String::new(),
                    })
                    .collect()
            };
            let app_state = app_state.clone();
            let source_input = source_input.clone();
            let source_adding = source_adding.clone();
            let refresh_documents = refresh_documents.clone();
            source_adding.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                let count = requests.len();
                for req in requests {
                    match Request::post("/app/documents")
                        .json(&req)
                        .unwrap()
                        .send()
                        .await
                    {
                        Ok(res) if res.ok() => {}
                        Ok(res) => {
                            toast(
                                &app_state,
                                res.text()
                                    .await
                                    .unwrap_or_else(|_| "Failed to add source".to_string()),
                            );
                            source_adding.set(false);
                            return;
                        }
                        Err(err) => {
                            toast(&app_state, err.to_string());
                            source_adding.set(false);
                            return;
                        }
                    }
                }
                toast(
                    &app_state,
                    if count == 1 {
                        "Source added to topic"
                    } else {
                        "Sources added to topic"
                    },
                );
                source_input.set(String::new());
                source_adding.set(false);
                refresh_documents.emit(());
            });
        })
    };

    let on_explore_link = {
        let app_state = app_state.clone();
        let source_input = source_input.clone();
        let source_exploring = source_exploring.clone();
        Callback::from(move |_: MouseEvent| {
            let links = source_links(source_input.trim());
            if links.len() != 1 {
                return;
            }
            let req = ExploreLinkReq {
                url: links[0].clone(),
            };
            let app_state = app_state.clone();
            let source_input = source_input.clone();
            let source_exploring = source_exploring.clone();
            source_exploring.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                match Request::post("/app/documents/explore")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => match res.json::<Vec<ExploredLink>>().await {
                        Ok(links) => {
                            let count = links.len();
                            source_input.set(
                                links
                                    .into_iter()
                                    .map(|link| link.url)
                                    .collect::<Vec<_>>()
                                    .join("\n"),
                            );
                            toast(&app_state, &format!("Found {count} documentation pages"));
                        }
                        Err(err) => toast(&app_state, err.to_string()),
                    },
                    Ok(res) => toast(
                        &app_state,
                        res.text()
                            .await
                            .unwrap_or_else(|_| "Could not explore link".to_string()),
                    ),
                    Err(err) => toast(&app_state, err.to_string()),
                }
                source_exploring.set(false);
            });
        })
    };

    let on_file_selected = {
        let data = doc_file_data.clone();
        let name = doc_file_name.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            if let Some(file) = input.files().and_then(|files| files.get(0)) {
                name.set(file.name());
                let reader = FileReader::new().unwrap();
                let reader_clone = reader.clone();
                let data = data.clone();
                let closure = Closure::wrap(Box::new(move || {
                    if let Ok(result) = reader_clone.result() {
                        if let Some(value) = result.as_string() {
                            data.set(value);
                        }
                    }
                }) as Box<dyn FnMut()>);
                reader.set_onloadend(Some(closure.as_ref().unchecked_ref()));
                let _ = reader.read_as_data_url(&file);
                closure.forget();
            }
        })
    };

    let on_upload_file = {
        let app_state = app_state.clone();
        let data = doc_file_data.clone();
        let name = doc_file_name.clone();
        let uploading = doc_file_uploading.clone();
        let refresh_documents = refresh_documents.clone();
        let topic_id = props.topic.id;
        Callback::from(move |_: MouseEvent| {
            if (*data).is_empty() || (*name).is_empty() {
                toast(&app_state, "Choose a file first");
                return;
            }
            uploading.set(true);
            let req = UploadDocumentReq {
                filename: (*name).clone(),
                title: None,
                data_url: (*data).clone(),
                topic_ids: vec![topic_id],
            };
            let app_state = app_state.clone();
            let data = data.clone();
            let name = name.clone();
            let uploading = uploading.clone();
            let refresh_documents = refresh_documents.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match Request::post("/app/documents/upload")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        toast(&app_state, "File uploaded to topic");
                        data.set(String::new());
                        name.set(String::new());
                        refresh_documents.emit(());
                    }
                    _ => toast(&app_state, "Failed to upload file"),
                }
                uploading.set(false);
            });
        })
    };

    let on_attach = {
        let app_state = app_state.clone();
        let refresh_documents = refresh_documents.clone();
        let topic_id = props.topic.id;
        Callback::from(move |id: i64| {
            let app_state = app_state.clone();
            let refresh_documents = refresh_documents.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = AttachDocumentReq { topic_id };
                match Request::post(&format!("/app/documents/{id}/topics"))
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => refresh_documents.emit(()),
                    _ => toast(&app_state, "Failed to attach source"),
                }
            });
        })
    };
    let on_detach = {
        let app_state = app_state.clone();
        let refresh_documents = refresh_documents.clone();
        let topic_id = props.topic.id;
        Callback::from(move |id: i64| {
            let app_state = app_state.clone();
            let refresh_documents = refresh_documents.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match Request::delete(&format!("/app/documents/{id}/topics/{topic_id}"))
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => refresh_documents.emit(()),
                    _ => toast(&app_state, "Failed to detach source"),
                }
            });
        })
    };

    html! {
        <div class="fixed inset-0 z-[80] bg-black/60 p-4 flex items-center justify-center">
            <form onsubmit={on_submit} class="surface border rounded-md w-full max-w-2xl p-4 space-y-4 max-h-[90vh] overflow-y-auto">
                <div class="flex items-start justify-between gap-3">
                    <div>
                        <h2 class="text-lg font-semibold">{format!("Topic: {}", props.topic.name)}</h2>
                        <p class="text-sm text-muted">{tip_type_label(&i18n, &props.topic.tipcard_type)}</p>
                    </div>
                    <ShadcnButton
                        variant={ButtonVariant::Outline}
                        size={ButtonSize::Sm}
                        onclick={Callback::from({ let on_close = props.on_close.clone(); move |_: MouseEvent| on_close.emit(()) })}
                    >
                        {"Close"}
                    </ShadcnButton>
                </div>
                <div>
                    <label class="block card-kicker mb-2">{"Prompt Template"}</label>
                    <textarea value={(*prompt_template).clone()} oninput={Callback::from({ let state = prompt_template.clone(); move |e: InputEvent| if let Some(t) = e.target_dyn_into::<HtmlTextAreaElement>() { state.set(t.value()); }})} class="w-full rounded-md border px-3 py-2 h-24 resize-y"></textarea>
                </div>
                <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                    <div>
                        <label class="block card-kicker mb-2">{"Daily Card Count"}</label>
                        <input value={(*daily_card_count).clone()} oninput={Callback::from({ let state = daily_card_count.clone(); move |e: InputEvent| if let Some(t) = e.target_dyn_into::<HtmlInputElement>() { state.set(t.value()); }})} type="number" min="0" class="w-full rounded-md border px-3 py-2" />
                    </div>
                    <div>
                        <label class="block card-kicker mb-2">{"Compression Level"}</label>
                        <ShadcnSelect
                            value={(*compression_level).clone()}
                            onchange={Callback::from({
                                let state = compression_level.clone();
                                move |value: String| state.set(value)
                            })}
                            options={vec![
                                SelectOption { value: "light".into(), label: "Light".into() },
                                SelectOption { value: "balanced".into(), label: "Balanced".into() },
                                SelectOption { value: "strong".into(), label: "Strong".into() },
                                SelectOption { value: "ultra".into(), label: "Ultra".into() },
                            ]}
                        />
                    </div>
                    <div>
                        <label class="block card-kicker mb-2">{"Grounding Strategy"}</label>
                        <ShadcnSelect
                            value={(*grounding_strategy).clone()}
                            onchange={Callback::from({
                                let state = grounding_strategy.clone();
                                move |value: String| state.set(value)
                            })}
                            options={vec![
                                SelectOption { value: "".into(), label: "Inherit from settings".into() },
                                SelectOption { value: "factual".into(), label: "Factual".into() },
                                SelectOption { value: "create_and_ground".into(), label: "Factcheck".into() },
                                SelectOption { value: "agentic".into(), label: "Agentic".into() },
                                SelectOption { value: "rag".into(), label: "From My Data".into() },
                            ]}
                        />
                    </div>
                    if *grounding_strategy == "rag" {
                        <div class="md:col-span-2 surface border border-token rounded-md p-4 space-y-4">
                            <div>
                                <h3 class="font-semibold">{"Topic Sources"}</h3>
                                <p class="text-sm text-muted">{"Add sources for this topic, or attach one you have already added elsewhere. Sources can be detached without deleting them."}</p>
                            </div>
                            <div class="space-y-3">
                                <div class="relative">
                                    <label class="block card-kicker mb-2" for="topic-source-input">{"Paste a document or links"}</label>
                                    <textarea id="topic-source-input" value={(*source_input).clone()} oninput={Callback::from({ let state = source_input.clone(); move |e: InputEvent| if let Some(t) = e.target_dyn_into::<HtmlTextAreaElement>() { state.set(t.value()); }})} class="w-full rounded-md border px-3 py-2 min-h-28 resize-y" placeholder="Paste document text, or one or more https:// links…"></textarea>
                                    if !source_input.trim().is_empty() && props.documents.iter().any(|doc| !doc.topic_ids.contains(&props.topic.id) && doc.title.to_lowercase().contains(&source_input.trim().to_lowercase())) {
                                        <div id="existing-source-matches" class="absolute z-10 mt-1 w-full surface border border-token rounded-md shadow-lg overflow-hidden">
                                            {for props.documents.iter().filter(|doc| !doc.topic_ids.contains(&props.topic.id) && doc.title.to_lowercase().contains(&source_input.trim().to_lowercase())).take(5).map(|doc| {
                                                let id = doc.id; let on_attach = on_attach.clone(); let source_input = source_input.clone();
                                                html! { <button type="button" class="w-full flex items-center gap-2 px-3 py-2 text-left hover:bg-muted/20" onclick={Callback::from(move |_: MouseEvent| { on_attach.emit(id); source_input.set(String::new()); })}><span class="truncate flex-1 text-sm">{&doc.title}</span><span class="text-xs text-muted">{"Attach existing"}</span></button> }
                                            })}
                                        </div>
                                    }
                                </div>
                                <div class="flex items-center justify-between gap-3">
                                    <span id="topic-source-detection" class="text-xs text-muted">{source_detection_label(&source_input)}</span>
                                    <div class="flex items-center gap-2">
                                        if source_links(&source_input).len() == 1 {
                                            <ShadcnButton id="explore-topic-source" variant={ButtonVariant::Secondary} r#type={ButtonType::Button} onclick={on_explore_link} disabled={*source_adding || *source_exploring}><iconify-icon icon="radix-icons:magnifying-glass" class="radix-icon" aria-hidden="true"></iconify-icon>{if *source_exploring { "Exploring..." } else { "Explore Link" }}</ShadcnButton>
                                        }
                                        <ShadcnButton id="add-topic-source" r#type={ButtonType::Button} onclick={on_add_document} disabled={source_input.trim().is_empty() || *source_adding || *source_exploring}><iconify-icon icon="radix-icons:plus" class="radix-icon" aria-hidden="true"></iconify-icon>{source_button_label(&source_input, *source_adding)}</ShadcnButton>
                                    </div>
                                </div>
                            </div>
                            <div class="border-t border-token pt-3 flex items-center gap-3">
                                <input type="file" accept=".pdf,.html,.htm,.txt,text/plain,text/html,application/pdf" class="hidden" ref={doc_file_input_ref.clone()} onchange={on_file_selected} />
                                <ShadcnButton variant={ButtonVariant::Secondary} r#type={ButtonType::Button} onclick={Callback::from({ let input = doc_file_input_ref.clone(); move |_: MouseEvent| if let Some(input) = input.cast::<HtmlInputElement>() { let _ = input.click(); }})} disabled={*doc_file_uploading}>{"Choose File"}</ShadcnButton>
                                <span class="text-sm text-muted truncate flex-1">{if (*doc_file_name).is_empty() { "No file chosen".to_string() } else { (*doc_file_name).clone() }}</span>
                                if !(*doc_file_data).is_empty() { <ShadcnButton variant={ButtonVariant::Default} r#type={ButtonType::Button} onclick={on_upload_file} disabled={*doc_file_uploading}>{if *doc_file_uploading { "Uploading…" } else { "Upload" }}</ShadcnButton> }
                            </div>
                            <div class="border-t border-token pt-3 space-y-3">
                                <div class="text-sm font-medium">{"Assigned to this topic"}</div>
                                if !props.sources_loaded { <p class="text-sm text-muted">{"Loading sources…"}</p> }
                                else {
                                    <div class="space-y-2">
                                        {for props.documents.iter().filter(|doc| doc.topic_ids.contains(&props.topic.id)).map(|doc| {
                                            let id = doc.id; let on_detach = on_detach.clone(); let on_view = props.on_view_document.clone();
                                            html! { <div class="flex items-center gap-2 p-2 rounded-md border border-token"><iconify-icon icon="radix-icons:file-text" class="radix-icon text-muted" aria-hidden="true"></iconify-icon><span class="truncate flex-1 text-sm">{&doc.title}</span><button type="button" class="text-xs underline" onclick={Callback::from(move |_: MouseEvent| on_view.emit(id))}>{"View"}</button><button type="button" class="text-xs text-muted hover:text-destructive" onclick={Callback::from(move |_: MouseEvent| on_detach.emit(id))}>{"Detach"}</button></div> }
                                        })}
                                        if props.documents.iter().all(|doc| !doc.topic_ids.contains(&props.topic.id)) { <p class="text-sm text-muted italic">{"No sources assigned yet."}</p> }
                                    </div>
                                }
                            </div>
                        </div>
                    }
                    <div>
                        <label class="block card-kicker mb-2">{"Image Strategy"}</label>
                        <ShadcnSelect
                            value={(*image_strategy).clone()}
                            onchange={Callback::from({
                                let state = image_strategy.clone();
                                move |value: String| state.set(value)
                            })}
                            options={vec![
                                SelectOption { value: "".into(), label: "Inherit from settings".into() },
                                SelectOption { value: "none".into(), label: "None".into() },
                                SelectOption { value: "pool".into(), label: "Local Image Pool".into() },
                                SelectOption { value: "programmatic".into(), label: "Tag-based Image APIs".into() },
                                SelectOption { value: "web_search".into(), label: "Web Image Search".into() },
                                SelectOption { value: "agentic".into(), label: "Isolated Image Search".into() },
                            ]}
                        />
                    </div>
                    <div>
                        <label class="block card-kicker mb-2">{"Time Zone"}</label>
                        <input value={(*daily_time_zone).clone()} oninput={Callback::from({ let state = daily_time_zone.clone(); move |e: InputEvent| if let Some(t) = e.target_dyn_into::<HtmlInputElement>() { state.set(t.value()); }})} class="w-full rounded-md border px-3 py-2" />
                    </div>
                    <div>
                        <label class="block card-kicker mb-2">{"Update Time"}</label>
                        <input value={(*daily_update_time).clone()} oninput={Callback::from({ let state = daily_update_time.clone(); move |e: InputEvent| if let Some(t) = e.target_dyn_into::<HtmlInputElement>() { state.set(t.value()); }})} type="time" class="w-full rounded-md border px-3 py-2" />
                    </div>
                </div>
                <ShadcnButton r#type={ButtonType::Submit}>{"Save Topic"}</ShadcnButton>
            </form>
        </div>
    }
}

fn tip_type_label(i18n: &I18n, tipcard_type: &str) -> String {
    match tipcard_type {
        "casual_tip" | "repeatable_tip" | "manual_tip" | "custom_tip" => {
            i18n.t(&format!("tip_type.{tipcard_type}"))
        }
        _ => tipcard_type.to_string(),
    }
}
