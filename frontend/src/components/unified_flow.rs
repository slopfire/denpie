use crate::api::toast;
use crate::components::flow_card::{FlowCard, FlowCardSkeleton};
use crate::state::AppState;
use gloo_file::{File, callbacks::FileReader};
use gloo_net::http::Request;
use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};
use wasm_bindgen::JsCast;
use web_sys::{DragEvent, HtmlInputElement, HtmlTextAreaElement, KeyboardEvent};
use yew::prelude::*;

const PAGE_LIMIT: i64 = 48;
const DRAG_SCROLL_EDGE_PX: f64 = 96.0;
const DRAG_SCROLL_MAX_STEP_PX: f64 = 32.0;

#[derive(Deserialize, Clone, PartialEq)]
pub struct TipcardInfo {
    pub id: i64,
    pub topic_name: String,
    pub topic_icon: String,
    pub topic_color: String,
    pub title: String,
    pub full_content: String,
    pub compressed_content: String,
    pub image_data: Vec<String>,
    pub created_at: String,
    pub tipcard_type: String,
    pub status: String,
    pub next_review_at: String,
    pub repeat_count: u32,
    pub pinned: bool,
}

#[derive(Deserialize, Clone, PartialEq)]
struct FlowCardSummary {
    id: i64,
    topic_name: String,
    topic_icon: String,
    topic_color: String,
    title: String,
    compressed_content: String,
    created_at: String,
    tipcard_type: String,
    status: String,
    next_review_at: String,
    repeat_count: u32,
    pinned: bool,
    image_count: i64,
    thumbnail_urls: Vec<String>,
}

#[derive(Deserialize)]
struct FlowCardPage {
    cards: Vec<FlowCardSummary>,
    next_cursor: Option<String>,
    has_more: bool,
}

#[derive(Deserialize, Clone)]
struct FlowCardDetail {
    id: i64,
    topic_name: String,
    topic_icon: String,
    topic_color: String,
    title: String,
    full_content: String,
    compressed_content: String,
    created_at: String,
    tipcard_type: String,
    status: String,
    next_review_at: String,
    repeat_count: u32,
    pinned: bool,
    image_urls: Vec<String>,
}

impl From<FlowCardSummary> for TipcardInfo {
    fn from(card: FlowCardSummary) -> Self {
        Self {
            id: card.id,
            topic_name: card.topic_name,
            topic_icon: card.topic_icon,
            topic_color: card.topic_color,
            title: card.title,
            full_content: card.compressed_content.clone(),
            compressed_content: card.compressed_content,
            image_data: card.thumbnail_urls,
            created_at: card.created_at,
            tipcard_type: card.tipcard_type,
            status: card.status,
            next_review_at: card.next_review_at,
            repeat_count: card.repeat_count,
            pinned: card.pinned,
        }
    }
}

impl From<FlowCardDetail> for TipcardInfo {
    fn from(card: FlowCardDetail) -> Self {
        Self {
            id: card.id,
            topic_name: card.topic_name,
            topic_icon: card.topic_icon,
            topic_color: card.topic_color,
            title: card.title,
            full_content: card.full_content,
            compressed_content: card.compressed_content,
            image_data: card.image_urls,
            created_at: card.created_at,
            tipcard_type: card.tipcard_type,
            status: card.status,
            next_review_at: card.next_review_at,
            repeat_count: card.repeat_count,
            pinned: card.pinned,
        }
    }
}

#[derive(Serialize)]
struct CreateTipReq {
    topics: String,
    tipcard_type: Option<String>,
    manual_content: Option<String>,
    manual_image_data: Option<Vec<String>>,
    exclude_card_ids: Option<Vec<i64>>,
}

#[derive(Serialize)]
struct ReviewReq {
    card_id: i64,
    grade: Option<u8>,
    action: Option<String>,
}

#[derive(Serialize)]
struct PinReq {
    id: i64,
    pinned: Option<bool>,
    image_data: Option<Vec<String>>,
}

#[function_component(UnifiedFlow)]
pub fn unified_flow() -> Html {
    let app_state = use_context::<UseReducerHandle<AppState>>().unwrap();
    let cards = use_state(Vec::<TipcardInfo>::new);
    let detail_loaded = use_state(HashMap::<i64, bool>::new);
    let card_heights = use_state(HashMap::<i64, f64>::new);
    let next_cursor = use_state(|| None::<String>);
    let has_more = use_state(|| true);
    let loading = use_state(|| false);
    let pending_count = use_state(|| 0usize);
    let card_order =
        use_state(|| LocalStorage::get::<Vec<i64>>("denpie-card-order").unwrap_or_default());
    let pinned_card_order =
        use_state(|| LocalStorage::get::<Vec<i64>>("denpie-pinned-card-order").unwrap_or_default());
    let topics_input =
        use_state(|| LocalStorage::get::<String>("denpie_prefill_topic").unwrap_or_default());
    let tip_type = use_state(|| {
        LocalStorage::get::<String>("denpie_prefill_type")
            .unwrap_or_else(|_| "casual_tip".to_string())
    });
    let manual_content = use_state(String::new);
    let manual_images = use_state(Vec::<String>::new);
    let image_readers = use_state(Vec::<FileReader>::new);
    let layout = use_state(|| {
        LocalStorage::get::<String>("denpie-flow-layout").unwrap_or_else(|_| "grid".to_string())
    });
    let sort_by = use_state(|| {
        LocalStorage::get::<String>("denpie-flow-sort")
            .map(|value| normalize_flow_sort(&value))
            .unwrap_or_else(|_| "topic".to_string())
    });
    let fullscreen_card_id = use_state(|| None::<i64>);

    let load_cards = {
        let cards = cards.clone();
        let detail_loaded = detail_loaded.clone();
        let next_cursor = next_cursor.clone();
        let has_more = has_more.clone();
        let loading = loading.clone();
        Callback::from(move |reset: bool| {
            if *loading {
                return;
            }
            let cards = cards.clone();
            let detail_loaded = detail_loaded.clone();
            let next_cursor = next_cursor.clone();
            let has_more = has_more.clone();
            let loading = loading.clone();
            let cursor = if reset { None } else { (*next_cursor).clone() };
            if !reset && cursor.is_none() && !*has_more {
                return;
            }
            loading.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                let mut url = format!("/app/flow-cards?limit={PAGE_LIMIT}");
                if let Some(cursor) = cursor {
                    url.push_str("&cursor=");
                    url.push_str(
                        &js_sys::encode_uri_component(&cursor)
                            .as_string()
                            .unwrap_or_default(),
                    );
                }
                match Request::get(&url).send().await {
                    Ok(res) if res.ok() => {
                        if let Ok(page) = res.json::<FlowCardPage>().await {
                            let new_cards: Vec<TipcardInfo> =
                                page.cards.into_iter().map(Into::into).collect();
                            if reset {
                                let loaded = new_cards
                                    .iter()
                                    .map(|card| (card.id, false))
                                    .collect::<HashMap<_, _>>();
                                detail_loaded.set(loaded);
                                cards.set(new_cards);
                            } else {
                                let mut merged = (*cards).clone();
                                let mut loaded = (*detail_loaded).clone();
                                for card in new_cards {
                                    if !merged.iter().any(|existing| existing.id == card.id) {
                                        loaded.entry(card.id).or_insert(false);
                                        merged.push(card);
                                    }
                                }
                                detail_loaded.set(loaded);
                                cards.set(merged);
                            }
                            next_cursor.set(page.next_cursor);
                            has_more.set(page.has_more);
                        }
                    }
                    _ => {}
                }
                loading.set(false);
            });
        })
    };

    {
        let load_cards = load_cards.clone();
        use_effect_with((), move |_| {
            load_cards.emit(true);
            || ()
        });
    }

    let request_detail = {
        let cards = cards.clone();
        let detail_loaded = detail_loaded.clone();
        Callback::from(move |id: i64| {
            let cards = cards.clone();
            let detail_loaded = detail_loaded.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(res) = Request::get(&format!("/app/flow-cards/{id}")).send().await {
                    if res.ok() {
                        if let Ok(detail) = res.json::<FlowCardDetail>().await {
                            let updated_card: TipcardInfo = detail.into();
                            let mut next = (*cards).clone();
                            if let Some(card) = next.iter_mut().find(|card| card.id == id) {
                                *card = updated_card;
                            }
                            let mut loaded = (*detail_loaded).clone();
                            loaded.insert(id, true);
                            detail_loaded.set(loaded);
                            cards.set(next);
                        }
                    }
                }
            });
        })
    };

    let on_submit = {
        let app_state = app_state.clone();
        let topics_input = topics_input.clone();
        let tip_type = tip_type.clone();
        let manual_content = manual_content.clone();
        let manual_images = manual_images.clone();
        let load_cards = load_cards.clone();
        let pending_count = pending_count.clone();

        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            if *pending_count > 0 {
                return;
            }
            let app_state = app_state.clone();
            let topics = (*topics_input).clone();
            let ttype = (*tip_type).clone();
            let content = (*manual_content).clone();
            let images = (*manual_images).clone();
            let load_cards = load_cards.clone();
            let pending_count = pending_count.clone();

            let n_skeletons = if ttype == "manual_tip" {
                1
            } else {
                topics
                    .split(',')
                    .filter(|topic| !topic.trim().is_empty())
                    .count()
                    .max(1)
            };
            pending_count.set(n_skeletons);

            wasm_bindgen_futures::spawn_local(async move {
                let req = CreateTipReq {
                    topics,
                    tipcard_type: Some(ttype.clone()),
                    manual_content: if ttype == "manual_tip" {
                        Some(content)
                    } else {
                        None
                    },
                    manual_image_data: if ttype == "manual_tip" {
                        Some(images)
                    } else {
                        None
                    },
                    exclude_card_ids: None,
                };
                match Request::post("/app/tips").json(&req).unwrap().send().await {
                    Ok(res) if res.ok() => {
                        toast(&app_state, "Cards added");
                        LocalStorage::delete("denpie_prefill_topic");
                        LocalStorage::delete("denpie_prefill_type");
                        load_cards.emit(true);
                    }
                    _ => toast(&app_state, "Failed to add cards"),
                }
                pending_count.set(0);
            });
        })
    };

    let on_review_cb = {
        let cards = cards.clone();
        let app_state = app_state.clone();
        let load_cards = load_cards.clone();
        Callback::from(
            move |(id, grade, action): (i64, Option<u8>, Option<String>)| {
                let cards = cards.clone();
                let app_state = app_state.clone();
                let load_cards = load_cards.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let req = ReviewReq {
                        card_id: id,
                        grade,
                        action,
                    };
                    match Request::post("/app/review")
                        .json(&req)
                        .unwrap()
                        .send()
                        .await
                    {
                        Ok(res) if res.ok() => {
                            cards.set(cards.iter().filter(|card| card.id != id).cloned().collect());
                            load_cards.emit(false);
                        }
                        _ => toast(&app_state, "Review failed"),
                    }
                });
            },
        )
    };

    let on_toggle_pin_cb = {
        let cards = cards.clone();
        let card_order = card_order.clone();
        let pinned_card_order = pinned_card_order.clone();
        Callback::from(move |(id, pinned): (i64, bool)| {
            let cards = cards.clone();
            let card_order = card_order.clone();
            let pinned_card_order = pinned_card_order.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = PinReq {
                    id,
                    pinned: Some(pinned),
                    image_data: None,
                };
                if let Ok(res) = Request::patch("/admin/tipcards")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    if res.ok() {
                        let mut next = (*cards).clone();
                        if let Some(card) = next.iter_mut().find(|card| card.id == id) {
                            card.pinned = pinned;
                        }
                        let unpinned_ids: Vec<i64> = next
                            .iter()
                            .filter(|card| !card.pinned)
                            .map(|card| card.id)
                            .collect();
                        let pinned_ids: Vec<i64> = next
                            .iter()
                            .filter(|card| card.pinned)
                            .map(|card| card.id)
                            .collect();
                        cards.set(next);

                        if pinned {
                            let order = normalize_card_order((*card_order).clone(), &unpinned_ids);
                            let _ = LocalStorage::set("denpie-card-order", &order);
                            card_order.set(order);
                        } else {
                            let order =
                                normalize_card_order((*pinned_card_order).clone(), &pinned_ids);
                            let _ = LocalStorage::set("denpie-pinned-card-order", &order);
                            pinned_card_order.set(order);
                        }
                    }
                }
            });
        })
    };

    let on_update_images_cb = {
        let request_detail = request_detail.clone();
        let app_state = app_state.clone();
        let detail_loaded = detail_loaded.clone();
        Callback::from(move |(id, image_data): (i64, Vec<String>)| {
            let request_detail = request_detail.clone();
            let app_state = app_state.clone();
            let detail_loaded = detail_loaded.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = PinReq {
                    id,
                    pinned: None,
                    image_data: Some(image_data),
                };
                match Request::patch("/admin/tipcards")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        toast(&app_state, "Images updated");
                        let mut loaded = (*detail_loaded).clone();
                        loaded.insert(id, false);
                        detail_loaded.set(loaded);
                        request_detail.emit(id);
                    }
                    Ok(res) => toast(
                        &app_state,
                        res.text()
                            .await
                            .unwrap_or_else(|_| "Failed to update images".to_string()),
                    ),
                    Err(err) => toast(&app_state, err.to_string()),
                }
            });
        })
    };

    let on_delete_cb = {
        let app_state = app_state.clone();
        let cards = cards.clone();
        Callback::from(move |id: i64| {
            let app_state = app_state.clone();
            let cards = cards.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = serde_json::json!({ "id": id });
                if Request::delete("/admin/tipcards")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                    .is_ok()
                {
                    toast(&app_state, "Card deleted");
                    cards.set(cards.iter().filter(|card| card.id != id).cloned().collect());
                }
            });
        })
    };

    let on_reorder_cb = {
        let card_order = card_order.clone();
        let pinned_card_order = pinned_card_order.clone();
        let cards = cards.clone();
        let sort_by = sort_by.clone();
        Callback::from(move |(source_id, target_id): (i64, i64)| {
            let source_pinned = cards
                .iter()
                .find(|card| card.id == source_id)
                .map(|card| card.pinned);
            let target_pinned = cards
                .iter()
                .find(|card| card.id == target_id)
                .map(|card| card.pinned);
            let (Some(source_pinned), Some(target_pinned)) = (source_pinned, target_pinned) else {
                return;
            };
            if source_pinned != target_pinned {
                return;
            }

            if source_pinned {
                let pinned_ids: Vec<i64> = cards
                    .iter()
                    .filter(|card| card.pinned)
                    .map(|card| card.id)
                    .collect();
                let mut order = normalize_card_order((*pinned_card_order).clone(), &pinned_ids);
                if let (Some(from_idx), Some(to_idx)) = (
                    order.iter().position(|&id| id == source_id),
                    order.iter().position(|&id| id == target_id),
                ) {
                    let item = order.remove(from_idx);
                    order.insert(to_idx, item);
                    let _ = LocalStorage::set("denpie-pinned-card-order", &order);
                    pinned_card_order.set(order);
                }
                return;
            }

            let unpinned_ids: Vec<i64> = cards
                .iter()
                .filter(|card| !card.pinned)
                .map(|card| card.id)
                .collect();
            let mut order = normalize_card_order((*card_order).clone(), &unpinned_ids);

            if let (Some(from_idx), Some(to_idx)) = (
                order.iter().position(|&id| id == source_id),
                order.iter().position(|&id| id == target_id),
            ) {
                let item = order.remove(from_idx);
                order.insert(to_idx, item);
                let _ = LocalStorage::set("denpie-card-order", &order);
                card_order.set(order);
                let _ = LocalStorage::set("denpie-flow-sort", "drag");
                sort_by.set("drag".to_string());
            }
        })
    };

    let on_toggle_fullscreen = {
        let fullscreen_card_id = fullscreen_card_id.clone();
        let request_detail = request_detail.clone();
        Callback::from(move |id: i64| {
            if *fullscreen_card_id == Some(id) {
                set_fullscreen_body_class(false);
                fullscreen_card_id.set(None);
            } else {
                set_fullscreen_body_class(true);
                request_detail.emit(id);
                fullscreen_card_id.set(Some(id));
            }
        })
    };

    let on_measure = {
        let card_heights = card_heights.clone();
        Callback::from(move |(id, height): (i64, f64)| {
            if height <= 0.0 {
                return;
            }
            let current = card_heights.get(&id).copied().unwrap_or(0.0);
            if (current - height).abs() > 2.0 {
                let mut next = (*card_heights).clone();
                next.insert(id, height);
                card_heights.set(next);
            }
        })
    };

    {
        use_effect_with(*fullscreen_card_id, move |fullscreen| {
            set_fullscreen_body_class(fullscreen.is_some());
            move || {
                set_fullscreen_body_class(false);
            }
        });
    }

    let mut pinned_cards: Vec<TipcardInfo> =
        cards.iter().filter(|card| card.pinned).cloned().collect();
    let mut unpinned_cards: Vec<TipcardInfo> =
        cards.iter().filter(|card| !card.pinned).cloned().collect();
    let pinned_ids: Vec<i64> = pinned_cards.iter().map(|card| card.id).collect();
    let unpinned_ids: Vec<i64> = unpinned_cards.iter().map(|card| card.id).collect();

    if !(*pinned_card_order).is_empty() {
        let normalized_pinned_order =
            normalize_card_order((*pinned_card_order).clone(), &pinned_ids);
        pinned_cards.sort_by_key(|card| {
            normalized_pinned_order
                .iter()
                .position(|&id| id == card.id)
                .unwrap_or(usize::MAX)
        });
    } else {
        sort_flow_cards(&mut pinned_cards, sort_by.as_str(), &[]);
    }

    sort_flow_cards(
        &mut unpinned_cards,
        sort_by.as_str(),
        &normalize_card_order((*card_order).clone(), &unpinned_ids),
    );

    let mut flow_cards = pinned_cards;
    flow_cards.extend(unpinned_cards);
    let current_ids: Vec<i64> = flow_cards.iter().map(|card| card.id).collect();

    let list_mode = *layout == "list";
    let disable_flow_glass =
        should_disable_flow_glass(list_mode, flow_cards.len(), &card_heights, &current_ids);

    {
        let load_cards = load_cards.clone();
        let has_more = *has_more;
        let loading = *loading;
        let loaded_count = flow_cards.len();
        use_effect_with((has_more, loading, loaded_count), move |_| {
            if has_more && !loading && loaded_count == 0 {
                load_cards.emit(false);
            }
            || ()
        });
    }

    let on_manual_images = {
        let manual_images = manual_images.clone();
        let image_readers = image_readers.clone();
        Callback::from(move |e: Event| {
            let mut readers = Vec::new();
            let Some(input) = e.target_dyn_into::<HtmlInputElement>() else {
                return;
            };
            let Some(files) = input.files() else {
                return;
            };
            if files.length() == 0 {
                return;
            };
            let next_images = Rc::new(RefCell::new(Vec::<String>::new()));
            let remaining = Rc::new(Cell::new(files.length()));
            for index in 0..files.length() {
                let Some(file) = files.get(index) else {
                    continue;
                };
                let manual_images = manual_images.clone();
                let next_images = next_images.clone();
                let remaining = remaining.clone();
                let reader =
                    gloo_file::callbacks::read_as_data_url(&File::from(file), move |result| {
                        if let Ok(data) = result {
                            next_images.borrow_mut().push(data);
                        }
                        let left = remaining.get().saturating_sub(1);
                        remaining.set(left);
                        if left == 0 {
                            let mut next = (*manual_images).clone();
                            next.extend(next_images.borrow().iter().cloned());
                            manual_images.set(next);
                        }
                    });
                readers.push(reader);
            }
            input.set_value("");
            image_readers.set(readers);
        })
    };
    let on_flow_dragover = Callback::from(|e: DragEvent| {
        e.prevent_default();
        auto_scroll_for_drag(&e);
    });

    html! {
        <section
            id="view-flow"
            class={classes!(disable_flow_glass.then_some("flow-many-cards"))}
        >
            <div class="flex flex-col xl:flex-row xl:items-end justify-between gap-3 mb-4">
                <div>
                    <h1 class="text-xl font-semibold tracking-tight">{"Unified Flow"}</h1>
                    <p class="text-muted mt-2">{"All cards in one review surface."}</p>
                </div>
                <form id="tips-form" onsubmit={on_submit} class="surface border rounded-md p-4 grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-5 gap-3 w-full xl:w-auto">
                    <input
                        id="tips-topics"
                        class="rounded-md border px-3 py-2 xl:col-span-2"
                        placeholder="Rust, Python, System Design"
                        value={(*topics_input).clone()}
                        oninput={Callback::from({let t = topics_input.clone(); move |e: InputEvent| if let Some(target) = e.target_dyn_into::<HtmlInputElement>() { t.set(target.value()); }})}
                        onkeydown={Callback::from({
                            let tip_type = tip_type.clone();
                            move |e: KeyboardEvent| {
                                if *tip_type == "manual_tip" && e.key() == "Tab" && !e.shift_key() {
                                    if let Some(window) = web_sys::window() {
                                        if let Some(document) = window.document() {
                                            if let Some(el) = document.get_element_by_id("manual-card-content") {
                                                if let Ok(textarea) = el.dyn_into::<HtmlTextAreaElement>() {
                                                    let _ = textarea.focus();
                                                    e.prevent_default();
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        })}
                        required=true
                    />
                    <div class="tip-type-switch muted-surface border border-token rounded-md p-1 grid grid-cols-3 sm:col-span-2" role="group">
                        <button type="button" onclick={let t = tip_type.clone(); Callback::from(move |_| t.set("casual_tip".to_string()))} class={classes!("rounded-md", "px-3", "py-2", "text-sm", "font-medium", (*tip_type == "casual_tip").then_some("active"))}>{"Casual"}</button>
                        <button type="button" onclick={let t = tip_type.clone(); Callback::from(move |_| t.set("repeatable_tip".to_string()))} class={classes!("rounded-md", "px-3", "py-2", "text-sm", "font-medium", (*tip_type == "repeatable_tip").then_some("active"))}>{"Repeat"}</button>
                        <button type="button" onclick={let t = tip_type.clone(); Callback::from(move |_| t.set("manual_tip".to_string()))} class={classes!("rounded-md", "px-3", "py-2", "text-sm", "font-medium", (*tip_type == "manual_tip").then_some("active"))}>{"Manual"}</button>
                    </div>
                    <button
                        id="tips-submit-btn"
                        type="submit"
                        class={classes!("rounded-md", "bg-primary-solid", "px-4", "py-2", "font-medium", "flex", "items-center", "justify-center", "gap-2", (*pending_count > 0).then_some("opacity-60 cursor-not-allowed"))}
                        disabled={*pending_count > 0}
                    >
                        <iconify-icon icon={if *pending_count > 0 { "radix-icons:update" } else { "radix-icons:magic-wand" }} class={classes!("radix-icon", (*pending_count > 0).then_some("animate-spin"))} aria-hidden="true"></iconify-icon>
                        <span>{ if *pending_count > 0 { "Adding..." } else { "Add" } }</span>
                    </button>
                    if *tip_type == "manual_tip" {
                        <textarea
                            id="manual-card-content"
                            class="rounded-md border px-3 py-2 sm:col-span-2 xl:col-span-5 h-20 resize-y"
                            placeholder="Manual card content"
                            value={(*manual_content).clone()}
                            oninput={Callback::from({ let manual_content = manual_content.clone(); move |e: InputEvent| if let Some(target) = e.target_dyn_into::<HtmlTextAreaElement>() { manual_content.set(target.value()); }})}
                            onkeydown={Callback::from({
                                move |e: KeyboardEvent| {
                                    if e.key() == "Enter" && e.shift_key() {
                                        e.prevent_default();
                                        if let Some(window) = web_sys::window() {
                                            if let Some(document) = window.document() {
                                                if let Some(btn) = document.get_element_by_id("tips-submit-btn") {
                                                    if let Ok(btn_el) = btn.dyn_into::<web_sys::HtmlElement>() {
                                                        btn_el.click();
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            })}
                        ></textarea>
                        <div class="sm:col-span-2 xl:col-span-5 flex flex-wrap items-center gap-3">
                            <label class="inline-flex items-center gap-2 rounded-md border border-token px-3 py-2 text-sm font-medium cursor-pointer">
                                <iconify-icon icon="radix-icons:image" class="radix-icon"></iconify-icon>
                                <span>{"Add images"}</span>
                                <input id="manual-card-images" type="file" multiple=true accept="image/*" class="hidden" onchange={on_manual_images} />
                            </label>
                            <span class="text-sm text-muted">{format!("{} images", manual_images.len())}</span>
                            if !manual_images.is_empty() {
                                <button type="button" class="rounded-md border border-token px-3 py-2 text-sm" onclick={Callback::from({ let manual_images = manual_images.clone(); move |_| manual_images.set(Vec::new()) })}>{"Clear"}</button>
                            }
                        </div>
                    }
                </form>
            </div>

            <div class="flex justify-between items-center gap-3 mb-4">
                <div class="text-sm text-muted">
                    <span id="flow-count">{flow_cards.len()}</span>{"/"}{PAGE_LIMIT}{" loaded cards"}
                </div>
                <div class="flex flex-wrap items-center justify-end gap-2">
                    <div class="flex muted-surface rounded-md p-1 border border-token" role="group" aria-label="Sort cards">
                        <button
                            type="button"
                            class={classes!("rounded", "px-2", "py-1", "text-sm", "font-medium", (*sort_by == "topic").then_some("bg-primary-soft text-primary"))}
                            aria-pressed={(*sort_by == "topic").to_string()}
                            onclick={Callback::from({
                                let sort_by = sort_by.clone();
                                move |_| {
                                    let _ = LocalStorage::set("denpie-flow-sort", "topic");
                                    sort_by.set("topic".to_string());
                                }
                            })}
                        >
                            {"Topic"}
                        </button>
                        <button
                            type="button"
                            class={classes!("rounded", "px-2", "py-1", "text-sm", "font-medium", (*sort_by == "date").then_some("bg-primary-soft text-primary"))}
                            aria-pressed={(*sort_by == "date").to_string()}
                            onclick={Callback::from({
                                let sort_by = sort_by.clone();
                                move |_| {
                                    let _ = LocalStorage::set("denpie-flow-sort", "date");
                                    sort_by.set("date".to_string());
                                }
                            })}
                        >
                            {"Date"}
                        </button>
                        <button
                            type="button"
                            class={classes!("rounded", "px-2", "py-1", "text-sm", "font-medium", (*sort_by == "drag").then_some("bg-primary-soft text-primary"))}
                            aria-pressed={(*sort_by == "drag").to_string()}
                            onclick={Callback::from({
                                let sort_by = sort_by.clone();
                                move |_| {
                                    let _ = LocalStorage::set("denpie-flow-sort", "drag");
                                    sort_by.set("drag".to_string());
                                }
                            })}
                        >
                            {"Drag"}
                        </button>
                    </div>
                    <div class="flex muted-surface rounded-md p-1 border border-token">
                        <button id="flow-grid-btn" type="button" class={classes!("rounded", "px-2", "py-1", (!list_mode).then_some("bg-primary-soft text-primary"))} onclick={Callback::from({ let layout = layout.clone(); move |_| { let _ = LocalStorage::set("denpie-flow-layout", "grid"); layout.set("grid".to_string()); } })}>
                            <iconify-icon icon="radix-icons:grid" class="radix-icon"></iconify-icon>
                        </button>
                        <button id="flow-list-btn" type="button" class={classes!("rounded", "px-2", "py-1", list_mode.then_some("bg-primary-soft text-primary"))} onclick={Callback::from({ let layout = layout.clone(); move |_| { let _ = LocalStorage::set("denpie-flow-layout", "list"); layout.set("list".to_string()); } })}>
                            <iconify-icon icon="radix-icons:list-bullet" class="radix-icon"></iconify-icon>
                        </button>
                    </div>
                </div>
            </div>

            <div
                id="flow-grid"
                class={if list_mode { "grid grid-cols-1 gap-3 items-start w-full max-w-4xl mx-auto" } else { "grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4 gap-3 items-start" }}
                ondragover={on_flow_dragover}
            >
                {
                    for (0..*pending_count).map(|i| html! {
                        <FlowCardSkeleton key={format!("skeleton-{i}")} list_mode={list_mode} />
                    })
                }
                {
                    for flow_cards.iter().map(|card| {
                        let card = card.clone();
                        let id = card.id;
                        html! {
                            <FlowCard
                                card={card}
                                on_review={on_review_cb.clone()}
                                on_toggle_pin={on_toggle_pin_cb.clone()}
                                on_delete={on_delete_cb.clone()}
                                on_reorder={on_reorder_cb.clone()}
                                on_update_images={on_update_images_cb.clone()}
                                on_toggle_fullscreen={on_toggle_fullscreen.clone()}
                                on_request_detail={request_detail.clone()}
                                on_measure={on_measure.clone()}
                                list_mode={list_mode}
                                fullscreen={*fullscreen_card_id == Some(id)}
                                detail_loaded={detail_loaded.get(&id).copied().unwrap_or(false)}
                            />
                        }
                    })
                }
            </div>
            if *loading {
                <div class="flex justify-center py-8 text-sm text-muted">{"Loading cards..."}</div>
            } else if *has_more {
                <div class="flex justify-center py-8">
                    <button type="button" class="rounded-md border border-token px-6 py-2 font-medium" onclick={Callback::from({ let load_cards = load_cards.clone(); move |_| load_cards.emit(false) })}>{"Load More Cards"}</button>
                </div>
            }

            if flow_cards.is_empty() && !*loading {
                <div id="empty-flow" class="surface border rounded-md p-10 text-center text-muted">
                    {"No cards yet."}
                </div>
            }
        </section>
    }
}

const FLOW_GLASS_GRID_THRESHOLD: usize = 8;
const FLOW_GLASS_LIST_VIEWPORT_MULTIPLIER: usize = 3;

fn flow_grid_columns(viewport_width: f64) -> usize {
    if viewport_width >= 1536.0 {
        4
    } else if viewport_width >= 1280.0 {
        3
    } else if viewport_width >= 768.0 {
        2
    } else {
        1
    }
}

fn estimate_visible_card_slots(
    list_mode: bool,
    card_heights: &HashMap<i64, f64>,
    card_ids: &[i64],
) -> usize {
    let Some(window) = web_sys::window() else {
        return if list_mode { 3 } else { 6 };
    };
    let viewport_h = window
        .inner_height()
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(800.0);
    let viewport_w = window
        .inner_width()
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(1024.0);

    let cols = if list_mode {
        1
    } else {
        flow_grid_columns(viewport_w)
    };
    let gap_px = 12.0;
    let measured: Vec<f64> = card_ids
        .iter()
        .filter_map(|id| card_heights.get(id).copied())
        .filter(|height| *height > 0.0)
        .collect();
    let avg_card_h = if measured.is_empty() {
        if list_mode { 360.0 } else { 280.0 }
    } else {
        measured.iter().sum::<f64>() / measured.len() as f64
    };
    let rows = (viewport_h / (avg_card_h + gap_px)).ceil().max(1.0) as usize;
    cols * rows
}

fn should_disable_flow_glass(
    list_mode: bool,
    card_count: usize,
    card_heights: &HashMap<i64, f64>,
    card_ids: &[i64],
) -> bool {
    if card_count == 0 {
        return false;
    }
    if list_mode {
        let visible_slots = estimate_visible_card_slots(true, card_heights, card_ids);
        card_count > visible_slots.saturating_mul(FLOW_GLASS_LIST_VIEWPORT_MULTIPLIER)
    } else {
        card_count > FLOW_GLASS_GRID_THRESHOLD
    }
}

fn normalize_flow_sort(value: &str) -> String {
    match value {
        "manual" | "drag" => "drag".to_string(),
        "date" | "topic" => value.to_string(),
        _ => "topic".to_string(),
    }
}

fn sort_flow_cards(cards: &mut [TipcardInfo], sort_by: &str, drag_order: &[i64]) {
    match sort_by {
        "date" => cards.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
        }),
        "drag" if !drag_order.is_empty() => cards.sort_by_key(|card| {
            drag_order
                .iter()
                .position(|&id| id == card.id)
                .unwrap_or(usize::MAX)
        }),
        _ => cards.sort_by(|a, b| {
            a.topic_name
                .to_lowercase()
                .cmp(&b.topic_name.to_lowercase())
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
                .then_with(|| a.id.cmp(&b.id))
        }),
    }
}

fn normalize_card_order(mut order: Vec<i64>, current_ids: &[i64]) -> Vec<i64> {
    order.retain(|id| current_ids.contains(id));
    for id in current_ids {
        if !order.contains(id) {
            order.push(*id);
        }
    }
    order
}

fn auto_scroll_for_drag(event: &DragEvent) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(inner_height) = window.inner_height() else {
        return;
    };
    let Some(viewport_height) = inner_height.as_f64() else {
        return;
    };

    let pointer_y = event.client_y() as f64;
    let delta = if pointer_y < DRAG_SCROLL_EDGE_PX {
        -scroll_step(DRAG_SCROLL_EDGE_PX - pointer_y)
    } else if viewport_height - pointer_y < DRAG_SCROLL_EDGE_PX {
        scroll_step(DRAG_SCROLL_EDGE_PX - (viewport_height - pointer_y))
    } else {
        return;
    };

    window.scroll_by_with_x_and_y(0.0, delta);
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

fn scroll_step(edge_overlap: f64) -> f64 {
    let intensity = (edge_overlap / DRAG_SCROLL_EDGE_PX).clamp(0.0, 1.0);
    (intensity * DRAG_SCROLL_MAX_STEP_PX).max(4.0)
}
