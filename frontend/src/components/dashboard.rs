use crate::api::toast;
use crate::app::View;
use crate::components::select::{SelectOption, ShadcnSelect};
use crate::components::tooltip::ShadcnTooltip;
use crate::i18n::{I18n, use_i18n};
use crate::state::AppState;
use crate::topic_visual::display_icon;
use gloo_net::http::Request;
use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};
use web_sys::{HtmlDialogElement, HtmlInputElement, HtmlTextAreaElement};
use yew::prelude::*;
use yew_router::prelude::*;

#[derive(Deserialize, Clone, PartialEq)]
pub struct AppSummary {
    pub topics: i64,
    pub total_cards: i64,
    pub due_cards: i64,
    pub active_cards: i64,
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
    pub completed_cards: i64,
    pub daily_card_count: u32,
    pub daily_time_zone: String,
    pub daily_update_time: String,
    pub compression_level: String,
}

#[derive(Serialize)]
struct UpdateTopicReq {
    id: i64,
    prompt_template: Option<String>,
    daily_card_count: Option<u32>,
    daily_time_zone: Option<String>,
    daily_update_time: Option<String>,
    compression_level: Option<String>,
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

#[function_component(Dashboard)]
pub fn dashboard() -> Html {
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
    let dialog_ref = use_node_ref();

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

    {
        let summary = summary.clone();
        let token_spend = token_spend.clone();
        let topics = topics.clone();
        use_effect_with((), move |_| {
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
            });
            || ()
        });
    }

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

    html! {
        <section id="view-dashboard">
            <div class="mb-4">
                <h1 class="text-xl font-semibold tracking-tight">
                    {"Welcome back"}
                </h1>
                <p class="text-muted mt-2">
                    {"Cards, topics, and queue state from local server."}
                </p>
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
                    <button
                        type="button"
                        class="nav-shortcut rounded-md bg-primary-solid px-3 py-2 font-semibold"
                        onclick={Callback::from({
                            let navigator = navigator.clone();
                            move |_| {
                                if let Some(nav) = navigator.clone() {
                                    nav.push(&View::Flow);
                                }
                            }
                        })}
                    >
                        <iconify-icon icon="radix-icons:plus" class="radix-icon" aria-hidden="true"></iconify-icon>
                    </button>
                </div>
            </div>

            <div id="topics-grid" class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4 gap-3">
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
                                let on_regenerate_icon = on_regenerate_icon.clone();
                                html! {
                                <div class="surface border rounded-md p-4 flex flex-col">
                                    <div class="flex justify-between items-start mb-2 gap-2">
                                        <h3 class="font-semibold text-lg truncate flex items-center gap-2 min-w-0">
                                            <ShadcnTooltip content="Pick new icon with AI">
                                            <button
                                                type="button"
                                                class="topic-icon-btn shrink-0 inline-flex items-center justify-center rounded-sm border border-transparent hover:border-token disabled:opacity-50"
                                                disabled={icon_loading}
                                                onclick={Callback::from(move |_| on_regenerate_icon.emit(topic_id))}
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
                                    <div class="mt-3 grid grid-cols-3 gap-2">
                                        <button
                                            type="button"
                                            class="rounded-md border border-token px-2 py-1.5 text-xs font-medium"
                                            onclick={Callback::from({
                                                let navigator = navigator.clone();
                                                move |_| {
                                                    let _ = LocalStorage::set("denpie_prefill_topic", &topic_for_load.name);
                                                    let _ = LocalStorage::set("denpie_prefill_type", &topic_for_load.tipcard_type);
                                                    if let Some(nav) = navigator.clone() {
                                                        nav.push(&View::Flow);
                                                    }
                                                }
                                            })}
                                        >
                                            {"Load"}
                                        </button>
                                        <button
                                            type="button"
                                            class="rounded-md border border-token px-2 py-1.5 text-xs font-medium"
                                            onclick={Callback::from({
                                                let editing = editing.clone();
                                                move |_| editing.set(Some(topic_for_edit.clone()))
                                            })}
                                        >
                                            {"Edit"}
                                        </button>
                                        <button
                                            type="button"
                                            class="rounded-md border border-token px-2 py-1.5 text-xs font-medium text-danger"
                                            onclick={Callback::from({
                                                let confirm_delete = confirm_delete.clone();
                                                let topic = topic_for_delete.clone();
                                                move |_| {
                                                    confirm_delete.set(Some(topic.clone()));
                                                }
                                            })}
                                        >
                                            {"Delete"}
                                        </button>
                                    </div>
                                </div>
                                }
                            })
                        }
                    }
                }
            </div>

            if let Some(topic) = (*editing).clone() {
                <TopicEditor topic={topic} on_close={Callback::from({
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
                        <div class="flex-shrink-0 flex items-center justify-center w-10 h-10 rounded-full bg-red-100 dark:bg-red-900/30 text-red-600 dark:text-red-400">
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
                                <button
                                    type="button"
                                    class="rounded-md border border-token px-4 py-2 text-sm font-medium hover:bg-accent-hsl"
                                    onclick={on_cancel_delete}
                                >
                                    {"Cancel"}
                                </button>
                                <button
                                    type="button"
                                    class="rounded-md bg-red-600 hover:bg-red-700 text-white px-4 py-2 text-sm font-medium"
                                    onclick={on_confirm_delete}
                                >
                                    {"Delete"}
                                </button>
                            </div>
                        </div>
                    </div>
                }
            </dialog>
        </section>
    }
}

#[derive(Properties, PartialEq)]
struct TopicEditorProps {
    topic: AppTopicInfo,
    on_close: Callback<()>,
    on_saved: Callback<()>,
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

    let on_submit = {
        let app_state = app_state.clone();
        let on_saved = props.on_saved.clone();
        let topic_id = props.topic.id;
        let prompt_template = prompt_template.clone();
        let daily_card_count = daily_card_count.clone();
        let daily_time_zone = daily_time_zone.clone();
        let daily_update_time = daily_update_time.clone();
        let compression_level = compression_level.clone();
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

    html! {
        <div class="fixed inset-0 z-[80] bg-black/60 p-4 flex items-center justify-center">
            <form onsubmit={on_submit} class="surface border rounded-md w-full max-w-2xl p-4 space-y-4">
                <div class="flex items-start justify-between gap-3">
                    <div>
                        <h2 class="text-lg font-semibold">{format!("Topic: {}", props.topic.name)}</h2>
                        <p class="text-sm text-muted">{tip_type_label(&i18n, &props.topic.tipcard_type)}</p>
                    </div>
                    <button type="button" class="border border-token rounded-md px-2 py-1" onclick={let on_close = props.on_close.clone(); Callback::from(move |_| on_close.emit(()))}>{"Close"}</button>
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
                        <label class="block card-kicker mb-2">{"Time Zone"}</label>
                        <input value={(*daily_time_zone).clone()} oninput={Callback::from({ let state = daily_time_zone.clone(); move |e: InputEvent| if let Some(t) = e.target_dyn_into::<HtmlInputElement>() { state.set(t.value()); }})} class="w-full rounded-md border px-3 py-2" />
                    </div>
                    <div>
                        <label class="block card-kicker mb-2">{"Update Time"}</label>
                        <input value={(*daily_update_time).clone()} oninput={Callback::from({ let state = daily_update_time.clone(); move |e: InputEvent| if let Some(t) = e.target_dyn_into::<HtmlInputElement>() { state.set(t.value()); }})} type="time" class="w-full rounded-md border px-3 py-2" />
                    </div>
                </div>
                <button type="submit" class="rounded-md bg-primary-solid px-4 py-2 font-medium">{"Save Topic"}</button>
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
