//! Grounding settings panel (fact sources, strategies, image sources).
use super::image_sources::{ImageSourceKind, ImageSourceSettings, parse_image_sources};
use crate::api::toast;
use crate::components::select::{SelectOption, ShadcnSelect};
use crate::state::AppState;
use gloo_net::http::Request;
use gloo_timers::callback::Timeout;
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;
use web_sys::{HtmlInputElement, HtmlTextAreaElement};
use yew::prelude::*;

fn default_scrape_provider() -> String {
    "scrapling".to_string()
}

#[derive(Deserialize, Clone, PartialEq)]
struct GroundingSettingsRes {
    grounding_strategy: String,
    grounding_model: String,
    grounding_reasoning_effort: String,
    image_strategy: String,
    search_provider: String,
    #[serde(default = "default_scrape_provider")]
    scrape_provider: String,
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
    search_provider: Option<String>,
    scrape_provider: Option<String>,
    search_api_key: Option<String>,
    search_base_url: Option<String>,
    image_sources: Option<String>,
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
        merge!(search_provider);
        merge!(scrape_provider);
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
pub fn grounding_settings() -> Html {
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
                "search_provider" => {
                    current.search_provider = value.clone();
                    let known_default = current.search_base_url.trim().is_empty()
                        || matches!(
                            current.search_base_url.trim_end_matches('/'),
                            "https://api.tavily.com" | "https://api.firecrawl.dev"
                        );
                    if known_default {
                        current.search_base_url = if value == "firecrawl" {
                            "https://api.firecrawl.dev".to_string()
                        } else {
                            "https://api.tavily.com".to_string()
                        };
                        patch.search_base_url = Some(current.search_base_url.clone());
                    }
                    // Prefer Firecrawl for link scrape when the web provider is Firecrawl.
                    if value == "firecrawl" && current.scrape_provider != "firecrawl" {
                        current.scrape_provider = "firecrawl".to_string();
                        patch.scrape_provider = Some("firecrawl".to_string());
                    }
                    patch.search_provider = Some(value);
                }
                "scrape_provider" => {
                    current.scrape_provider = value.clone();
                    patch.scrape_provider = Some(value);
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
                <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-3">
                    <div>
                        <label class="block card-kicker mb-2" for="search-provider-input">{"Web Provider"}</label>
                        <ShadcnSelect
                            id="search-provider-input"
                            name="search-provider-input"
                            onchange={on_select("search_provider")}
                            value={settings.search_provider.clone()}
                            options={vec![
                                SelectOption { value: "tavily".into(), label: "Tavily".into() },
                                SelectOption { value: "firecrawl".into(), label: "Firecrawl".into() },
                            ]}
                        />
                        <div class="mt-2 text-xs text-muted">{"Used for fact grounding and image search."}</div>
                    </div>
                    <div>
                        <label class="block card-kicker mb-2" for="scrape-provider-input">{"Link Scraper"}</label>
                        <ShadcnSelect
                            id="scrape-provider-input"
                            name="scrape-provider-input"
                            onchange={on_select("scrape_provider")}
                            value={settings.scrape_provider.clone()}
                            options={vec![
                                SelectOption { value: "scrapling".into(), label: "Scrapling (local, main)".into() },
                                SelectOption { value: "firecrawl".into(), label: "Firecrawl (cloud)".into() },
                                SelectOption { value: "direct".into(), label: "Direct HTTP (legacy)".into() },
                            ]}
                        />
                        <div class="mt-2 text-xs text-muted">
                            {if settings.scrape_provider == "scrapling" {
                                "Main option: turns linked pages into clean Markdown via the Scrapling CLI when installed (pip install \"scrapling[fetchers,shell]\")."
                            } else if settings.scrape_provider == "firecrawl" {
                                "Uses Firecrawl /v2/scrape (pages and supported remote files such as PDFs). Requires the API key below."
                            } else {
                                "Simple capped HTTP GET with HTML tags stripped. No external tools."
                            }}
                        </div>
                    </div>
                    <div>
                        <label class="block card-kicker mb-2" for="search-api-key-input">{format!("{} API Key", if settings.search_provider == "firecrawl" || settings.scrape_provider == "firecrawl" { "Firecrawl / Tavily" } else { "Tavily" })}</label>
                        <input id="search-api-key-input" oninput={on_input("search_api_key")} type="password" value={settings.search_api_key.clone()} class="w-full rounded-md border px-3 py-2" placeholder={if settings.search_provider == "firecrawl" || settings.scrape_provider == "firecrawl" { "fc-… or tvly-…" } else { "tvly-…" }} />
                        <div class="mt-2 text-xs text-muted">{"Required for the selected external web provider and for Firecrawl link scraping. Without a key, fact grounding uses the LLM provider's web search."}</div>
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
                            ("web_search", "Web Image Search", "Uses the configured web provider and the card's generated image query."),
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
