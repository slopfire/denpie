//! Grounding settings panel (fact sources and image strategies).
use crate::api::toast;
use crate::api_v1;
use crate::components::select::{SelectOption, ShadcnSelect};
use crate::i18n::use_i18n;
use crate::state::AppState;
use gloo_timers::callback::Timeout;
use web_sys::HtmlInputElement;
use yew::prelude::*;

fn default_scrape_provider() -> String {
    "scrapling".to_string()
}

#[derive(Clone, PartialEq)]
struct GroundingSettingsRes {
    grounding_strategy: String,
    grounding_model: String,
    grounding_reasoning_effort: String,
    image_strategy: String,
    search_provider: String,
    scrape_provider: String,
    search_api_key: String,
    search_base_url: String,
}

impl From<api_v1::SettingsView> for GroundingSettingsRes {
    fn from(s: api_v1::SettingsView) -> Self {
        Self {
            grounding_strategy: s.grounding_strategy,
            grounding_model: s.grounding_model,
            grounding_reasoning_effort: s.grounding_reasoning_effort,
            image_strategy: s.image_strategy,
            search_provider: s.search_provider,
            scrape_provider: if s.scrape_provider.is_empty() {
                default_scrape_provider()
            } else {
                s.scrape_provider
            },
            search_api_key: s.search_api_key,
            search_base_url: s.search_base_url,
        }
    }
}

#[derive(Default)]
struct GroundingSettingsPatch {
    grounding_strategy: Option<String>,
    grounding_model: Option<String>,
    grounding_reasoning_effort: Option<String>,
    image_strategy: Option<String>,
    search_provider: Option<String>,
    scrape_provider: Option<String>,
    search_api_key: Option<String>,
    search_base_url: Option<String>,
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
    }

    fn to_v1(&self) -> api_v1::SettingsPatch {
        api_v1::SettingsPatch {
            grounding_strategy: self.grounding_strategy.clone(),
            grounding_model: self.grounding_model.clone(),
            grounding_reasoning_effort: self.grounding_reasoning_effort.clone(),
            image_strategy: self.image_strategy.clone(),
            search_provider: self.search_provider.clone(),
            scrape_provider: self.scrape_provider.clone(),
            search_api_key: self.search_api_key.clone(),
            search_base_url: self.search_base_url.clone(),
            ..Default::default()
        }
    }
}

fn save_grounding_settings(
    app_state: UseReducerHandle<AppState>,
    status: UseStateHandle<String>,
    patch: GroundingSettingsPatch,
) {
    status.set("Saving...".to_string());
    wasm_bindgen_futures::spawn_local(async move {
        match api_v1::update_settings(patch.to_v1()).await {
            Ok(()) => status.set("Saved".to_string()),
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
    let i18n = use_i18n();
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
                match api_v1::get_settings().await {
                    Ok(data) => {
                        settings.set(Some(GroundingSettingsRes::from(data)));
                        save_status.set(String::new());
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

    let Some(settings) = (*settings).clone() else {
        return html! {
            <div class="surface border rounded-md p-4">
                <h2 class="text-lg font-semibold flex items-center gap-2">
                    <iconify-icon icon="radix-icons:mixer-horizontal" class="radix-icon text-primary" aria-hidden="true"></iconify-icon>
                    {"Grounding Settings"}
                </h2>
                <div class="mt-2 text-sm text-muted">
                    {if save_status.is_empty() { "Loading settings..." } else { save_status.as_str() }}
                </div>
            </div>
        };
    };

    let selected_image_strategy = settings.image_strategy.clone();
    let select_image_strategy = on_select("image_strategy");

    html! {
        <div id="grounding-settings" class="surface border rounded-md p-4 flex flex-col gap-5">
            <div class="flex items-start justify-between gap-3">
                <div>
                    <h2 class="text-lg font-semibold flex items-center gap-2">
                        <iconify-icon icon="radix-icons:mixer-horizontal" class="radix-icon text-primary" aria-hidden="true"></iconify-icon>
                        {"Grounding Settings"}
                    </h2>
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
                        <div class="mt-2 text-xs text-muted">{i18n.t("grounding.search_provider.help")}</div>
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
                        <div class="mt-2 text-xs text-muted">{i18n.t("grounding.search_api_key.help")}</div>
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
                            ("none", "grounding.image_strategy.none", "grounding.image_strategy.none_description"),
                            ("pool", "grounding.image_strategy.pool", "grounding.image_strategy.pool_description"),
                            ("bing_html", "grounding.image_strategy.bing_html", "grounding.image_strategy.bing_html_description"),
                            ("bing_playwright", "grounding.image_strategy.bing_playwright", "grounding.image_strategy.bing_playwright_description"),
                            ("ddgs_text_og", "grounding.image_strategy.ddgs_text_og", "grounding.image_strategy.ddgs_text_og_description"),
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
                                        class="image-source-mode-radio mt-1 size-5 shrink-0"
                                    />
                                    <span class="flex flex-col gap-1">
                                        <span class="font-medium">{i18n.t(title)}</span>
                                        <span class="text-xs text-muted">{i18n.t(description)}</span>
                                    </span>
                                </label>
                            }
                        })
                    }
                </div>

                if selected_image_strategy == "pool" {
                    <p id="image-source-mode-help" class="text-sm text-muted">
                        {i18n.t("grounding.image_strategy.pool_help")}
                    </p>
                } else if selected_image_strategy == "bing_html" {
                    <p id="image-source-mode-help" class="text-sm text-muted">
                        {i18n.t("grounding.image_strategy.bing_html_help")}
                    </p>
                } else if selected_image_strategy == "bing_playwright" {
                    <p id="image-source-mode-help" class="text-sm text-muted">
                        {i18n.t("grounding.image_strategy.bing_playwright_help")}
                    </p>
                } else if selected_image_strategy == "ddgs_text_og" {
                    <p id="image-source-mode-help" class="text-sm text-muted">
                        {i18n.t("grounding.image_strategy.ddgs_text_og_help")}
                    </p>
                } else {
                    <p id="image-source-mode-help" class="text-sm text-muted">
                        {i18n.t("grounding.image_strategy.none_help")}
                    </p>
                }
            </section>
        </div>
    }
}
