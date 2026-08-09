use crate::api::toast;
use crate::api_v1;
use crate::app::View;
use crate::components::flow_card::{FlowCard, FlowCardSkeleton};
use crate::i18n::use_i18n;
use crate::image_compress::{collect_files, compress_files_to_data_urls};
use crate::state::AppState;
use gloo_net::http::Request;
use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use wasm_bindgen::JsCast;
use web_sys::{HtmlInputElement, HtmlTextAreaElement, KeyboardEvent};
use yew::prelude::*;
use yew_router::prelude::*;

const PAGE_LIMIT: i64 = 48;
const TRANSMISSION_MAX_PICKS: usize = 9;
const TRANSMISSION_MAX_PICKS_PER_TOPIC: usize = 3;
const REVIEWED_PLACEHOLDERS_KEY: &str = "denpie-reviewed-placeholders";
const FLOW_GRID_COLUMNS_KEY: &str = "denpie-flow-grid-columns";
const PINNED_CARD_ORDER_KEY: &str = "denpie-pinned-card-order";
const REPLACEMENT_SKELETON_DELAY_MS: u32 = 180;

#[derive(Deserialize, Serialize, Clone, PartialEq)]
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
    #[serde(default)]
    pub pending_count: u32,
    /// Client-only placeholder text after a review action; API responses omit this.
    #[serde(default)]
    pub review_message: Option<String>,
}

impl From<api_v1::FlowCardSummary> for TipcardInfo {
    fn from(card: api_v1::FlowCardSummary) -> Self {
        Self {
            id: card.id,
            topic_name: card.topic_name,
            topic_icon: card.topic_icon,
            topic_color: card.topic_color,
            title: card.title,
            full_content: card.full_content,
            compressed_content: card.compressed_content,
            image_data: card.thumbnail_urls,
            created_at: card.created_at,
            tipcard_type: card.tipcard_type,
            status: card.status,
            next_review_at: card.next_review_at,
            repeat_count: card.repeat_count,
            pinned: card.pinned,
            pending_count: card.pending_count,
            review_message: None,
        }
    }
}

impl From<api_v1::FlowCardDetail> for TipcardInfo {
    fn from(card: api_v1::FlowCardDetail) -> Self {
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
            pending_count: card.pending_count,
            review_message: None,
        }
    }
}

impl From<api_v1::InventoryCard> for TipcardInfo {
    fn from(card: api_v1::InventoryCard) -> Self {
        Self {
            id: card.id,
            topic_name: card.topic_name,
            topic_icon: card.topic_icon,
            topic_color: card.topic_color,
            title: card.title,
            full_content: card.full_content,
            compressed_content: card.compressed_content,
            image_data: Vec::new(),
            created_at: card.created_at,
            tipcard_type: card.tipcard_type,
            status: card.status,
            next_review_at: card.next_review_at,
            repeat_count: card.repeat_count,
            pinned: card.pinned,
            pending_count: 0,
            review_message: None,
        }
    }
}

fn topics_csv_to_list(topics: &str) -> Vec<String> {
    topics
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

fn stored_reviewed_placeholders() -> HashMap<i64, TipcardInfo> {
    web_sys::window()
        .and_then(|window| window.local_storage().ok().flatten())
        .and_then(|storage| storage.get_item(REVIEWED_PLACEHOLDERS_KEY).ok().flatten())
        .and_then(|raw| serde_json::from_str::<Vec<TipcardInfo>>(&raw).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|card| (card.id, card))
        .collect()
}

#[derive(Serialize)]
struct PinImagesReq {
    id: i64,
    pinned: Option<bool>,
    image_data: Option<Vec<String>>,
}

#[function_component(UnifiedFlow)]
pub fn unified_flow() -> Html {
    let app_state = use_context::<UseReducerHandle<AppState>>().unwrap();
    let i18n = use_i18n();
    // Render a saved completion card immediately while the first flow request is
    // in flight; the response below replaces it when a real card is due.
    let cards = use_state(|| {
        stored_reviewed_placeholders()
            .into_values()
            .collect::<Vec<_>>()
    });
    let reviewed_placeholders = use_state(stored_reviewed_placeholders);
    let detail_loaded = use_state(HashMap::<i64, bool>::new);
    let card_heights = use_state(HashMap::<i64, f64>::new);
    let next_cursor = use_state(|| None::<String>);
    let has_more = use_state(|| true);
    let loading = use_state(|| false);
    let load_in_flight = use_mut_ref(|| false);
    let detail_in_flight_ids = use_mut_ref(HashSet::<i64>::new);
    let pending_count = use_state(|| 0usize);
    let reviewing_card_ids = use_state(HashSet::<i64>::new);
    let review_in_flight_ids = use_mut_ref(HashSet::<i64>::new);
    let review_idempotency_keys = use_mut_ref(HashMap::<i64, String>::new);
    let replacement_pending_topics = use_mut_ref(HashSet::<String>::new);
    let replacement_loading_topics = use_state(HashSet::<String>::new);
    let pinned_card_order =
        use_state(|| LocalStorage::get::<Vec<i64>>(PINNED_CARD_ORDER_KEY).unwrap_or_default());
    let topics_input =
        use_state(|| LocalStorage::get::<String>("denpie_prefill_topic").unwrap_or_default());
    let tip_type = use_state(|| {
        LocalStorage::get::<String>("denpie_prefill_type")
            .unwrap_or_else(|_| "casual_tip".to_string())
    });
    let manual_content = use_state(String::new);
    let manual_images = use_state(Vec::<String>::new);
    let layout = use_state(|| {
        LocalStorage::get::<String>("denpie-flow-layout").unwrap_or_else(|_| "grid".to_string())
    });
    let grid_columns = use_state(|| {
        LocalStorage::get::<usize>(FLOW_GRID_COLUMNS_KEY)
            .map(normalize_grid_columns)
            .unwrap_or(4)
    });
    let grid_columns_open = use_state(|| false);
    let sort_by = use_state(|| {
        LocalStorage::get::<String>("denpie-flow-sort")
            .map(|value| normalize_flow_sort(&value))
            .unwrap_or_else(|_| "topic".to_string())
    });
    let fullscreen_card_key = use_state(|| None::<String>);

    {
        let placeholders = (*reviewed_placeholders).clone();
        use_effect_with(placeholders, move |placeholders| {
            if placeholders.is_empty() {
                LocalStorage::delete(REVIEWED_PLACEHOLDERS_KEY);
            } else {
                let cards = placeholders.values().cloned().collect::<Vec<_>>();
                let _ = LocalStorage::set(REVIEWED_PLACEHOLDERS_KEY, cards);
            }
            || ()
        });
    }

    let load_cards = {
        let cards = cards.clone();
        let reviewed_placeholders = reviewed_placeholders.clone();
        let detail_loaded = detail_loaded.clone();
        let next_cursor = next_cursor.clone();
        let has_more = has_more.clone();
        let loading = loading.clone();
        let load_in_flight = load_in_flight.clone();
        let app_state = app_state.clone();
        let replacement_pending_topics = replacement_pending_topics.clone();
        let replacement_loading_topics = replacement_loading_topics.clone();
        Callback::from(move |reset: bool| {
            if *load_in_flight.borrow() {
                return;
            }
            let cards = cards.clone();
            let reviewed_placeholders = reviewed_placeholders.clone();
            let detail_loaded = detail_loaded.clone();
            let next_cursor = next_cursor.clone();
            let has_more = has_more.clone();
            let loading = loading.clone();
            let load_in_flight = load_in_flight.clone();
            let app_state = app_state.clone();
            let replacement_pending_topics = replacement_pending_topics.clone();
            let replacement_loading_topics = replacement_loading_topics.clone();
            let cursor = if reset { None } else { (*next_cursor).clone() };
            if !reset && cursor.is_none() && !*has_more {
                return;
            }
            *load_in_flight.borrow_mut() = true;
            loading.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                match api_v1::list_flow_cards(PAGE_LIMIT as u32, cursor).await {
                    Ok(page) => {
                        let mut new_cards: Vec<TipcardInfo> =
                            page.cards.into_iter().map(Into::into).collect();
                        if reset {
                            let loaded_ids =
                                new_cards.iter().map(|card| card.id).collect::<HashSet<_>>();
                            let active_repeatable_topics = new_cards
                                .iter()
                                .filter(|card| {
                                    card.tipcard_type == "repeatable_tip" && card.status == "active"
                                })
                                .map(|card| card.topic_name.as_str())
                                .collect::<HashSet<_>>();
                            let mut loading_topics = (*replacement_loading_topics).clone();
                            loading_topics
                                .retain(|topic| !active_repeatable_topics.contains(topic.as_str()));
                            replacement_loading_topics.set(loading_topics);
                            replacement_pending_topics
                                .borrow_mut()
                                .retain(|topic| !active_repeatable_topics.contains(topic.as_str()));
                            // Read storage again on the first page response. It prevents a
                            // reload from dropping a completion card while the flow fetch is
                            // resolving during component initialization.
                            let mut placeholder_map = (*reviewed_placeholders).clone();
                            placeholder_map.extend(stored_reviewed_placeholders());
                            placeholder_map.retain(|id, card| {
                                !loaded_ids.contains(id)
                                    && !active_repeatable_topics.contains(card.topic_name.as_str())
                            });
                            let placeholders =
                                placeholder_map.values().cloned().collect::<Vec<_>>();
                            reviewed_placeholders.set(placeholder_map);
                            new_cards.extend(placeholders);
                            let loaded = new_cards
                                .iter()
                                .map(|card| (card.id, false))
                                .collect::<HashMap<_, _>>();
                            detail_loaded.set(loaded);
                            cards.set(new_cards);
                        } else {
                            let mut merged = (*cards).clone();
                            let mut loaded = (*detail_loaded).clone();
                            let mut placeholders = (*reviewed_placeholders).clone();
                            for card in new_cards {
                                if let Some(existing) =
                                    merged.iter_mut().find(|existing| existing.id == card.id)
                                {
                                    if existing.review_message.is_some() {
                                        placeholders.remove(&card.id);
                                        loaded.insert(card.id, false);
                                        *existing = card;
                                    }
                                } else {
                                    loaded.entry(card.id).or_insert(false);
                                    merged.push(card);
                                }
                            }
                            reviewed_placeholders.set(placeholders);
                            detail_loaded.set(loaded);
                            cards.set(merged);
                        }
                        next_cursor.set(page.next_cursor);
                        has_more.set(page.has_more);
                    }
                    Err(err) => toast(&app_state, err.to_string()),
                }
                *load_in_flight.borrow_mut() = false;
                loading.set(false);
            });
        })
    };

    {
        let load_cards = load_cards.clone();
        let refresh = Callback::from(move |_| load_cards.emit(true));
        crate::hooks::use_view_refresh(crate::app::View::Flow, refresh);
    }

    let request_detail = {
        let cards = cards.clone();
        let detail_loaded = detail_loaded.clone();
        let detail_in_flight_ids = detail_in_flight_ids.clone();
        let app_state = app_state.clone();
        Callback::from(move |id: i64| {
            if !detail_in_flight_ids.borrow_mut().insert(id) {
                return;
            }
            let cards = cards.clone();
            let detail_loaded = detail_loaded.clone();
            let detail_in_flight_ids = detail_in_flight_ids.clone();
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match api_v1::get_tipcard(id).await {
                    Ok(detail) => {
                        let mut updated_card: TipcardInfo = detail.into();
                        let mut next = (*cards).clone();
                        if let Some(card) = next.iter_mut().find(|card| card.id == id) {
                            if updated_card.pending_count == 0 {
                                updated_card.pending_count = card.pending_count;
                            }
                            *card = updated_card;
                        }
                        let mut loaded = (*detail_loaded).clone();
                        loaded.insert(id, true);
                        detail_loaded.set(loaded);
                        cards.set(next);
                    }
                    Err(err) => toast(&app_state, err.to_string()),
                }
                detail_in_flight_ids.borrow_mut().remove(&id);
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
                let topic_list = topics_csv_to_list(&topics);
                match api_v1::tips_v1(
                    (ttype == "repeatable_tip").then_some(5),
                    topic_list,
                    &ttype,
                    None,
                    if ttype == "manual_tip" {
                        Some(content)
                    } else {
                        None
                    },
                    if ttype == "manual_tip" {
                        Some(images)
                    } else {
                        None
                    },
                )
                .await
                {
                    Ok(_) => {
                        toast(&app_state, "Cards added");
                        LocalStorage::delete("denpie_prefill_topic");
                        LocalStorage::delete("denpie_prefill_type");
                        load_cards.emit(true);
                    }
                    Err(err) => toast(&app_state, err.to_string()),
                }
                pending_count.set(0);
            });
        })
    };

    let on_review_cb = {
        let cards = cards.clone();
        let reviewed_placeholders = reviewed_placeholders.clone();
        let app_state = app_state.clone();
        let load_cards = load_cards.clone();
        let replacement_pending_topics = replacement_pending_topics.clone();
        let replacement_loading_topics = replacement_loading_topics.clone();
        let reviewing_card_ids = reviewing_card_ids.clone();
        let review_in_flight_ids = review_in_flight_ids.clone();
        let review_idempotency_keys = review_idempotency_keys.clone();
        Callback::from(
            move |(id, grade, action): (i64, Option<u8>, Option<String>)| {
                if !review_in_flight_ids.borrow_mut().insert(id) {
                    return;
                }
                let mut reviewing = (*reviewing_card_ids).clone();
                reviewing.insert(id);
                reviewing_card_ids.set(reviewing);
                let idempotency_key = review_idempotency_keys
                    .borrow_mut()
                    .entry(id)
                    .or_insert_with(api_v1::new_idempotency_key)
                    .clone();
                let cards = cards.clone();
                let reviewed_placeholders = reviewed_placeholders.clone();
                let app_state = app_state.clone();
                let load_cards = load_cards.clone();
                let replacement_pending_topics = replacement_pending_topics.clone();
                let replacement_loading_topics = replacement_loading_topics.clone();
                let reviewing_card_ids = reviewing_card_ids.clone();
                let review_in_flight_ids = review_in_flight_ids.clone();
                let review_idempotency_keys = review_idempotency_keys.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let reviewed_card = cards.iter().find(|card| card.id == id).cloned();
                    let reviewed_repeatable_topic = reviewed_card.as_ref().and_then(|card| {
                        (card.tipcard_type == "repeatable_tip").then(|| card.topic_name.clone())
                    });
                    if let Some(topic) = &reviewed_repeatable_topic {
                        replacement_pending_topics
                            .borrow_mut()
                            .insert(topic.clone());

                        let pending_topics = replacement_pending_topics.clone();
                        let loading_topics = replacement_loading_topics.clone();
                        let topic = topic.clone();
                        gloo_timers::callback::Timeout::new(
                            REPLACEMENT_SKELETON_DELAY_MS,
                            move || {
                                if pending_topics.borrow().contains(&topic) {
                                    let mut loading = (*loading_topics).clone();
                                    loading.insert(topic);
                                    loading_topics.set(loading);
                                }
                            },
                        )
                        .forget();
                    }
                    let action_name = action.clone().unwrap_or_default();
                    match api_v1::review_v1_with_key(id, grade, action, idempotency_key).await {
                        Ok(()) => {
                            review_idempotency_keys.borrow_mut().remove(&id);
                            let mut reload_flow = true;
                            if let Some(mut placeholder) = reviewed_card {
                                if placeholder.tipcard_type == "repeatable_tip" {
                                    let (next_ready, next_error) = match api_v1::tips_v1(
                                        Some(1),
                                        vec![placeholder.topic_name.clone()],
                                        "repeatable_tip",
                                        Some(vec![id]),
                                        None,
                                        None,
                                    )
                                    .await
                                    {
                                        Ok(cards) => (!cards.is_empty(), None),
                                        Err(err) => (false, Some(err.to_string())),
                                    };
                                    if !next_ready {
                                        // Keep the completion card mounted. Reloading the flow
                                        // here races the state update above and can replace the
                                        // newly inserted placeholder with an empty page.
                                        reload_flow = false;
                                        replacement_pending_topics
                                            .borrow_mut()
                                            .remove(&placeholder.topic_name);
                                        let mut loading_topics =
                                            (*replacement_loading_topics).clone();
                                        loading_topics.remove(&placeholder.topic_name);
                                        replacement_loading_topics.set(loading_topics);
                                        placeholder.status = "reviewed".to_string();
                                        placeholder.review_message =
                                            Some(review_placeholder_message(
                                                &action_name,
                                                &placeholder.next_review_at,
                                            ));
                                        let mut placeholders = (*reviewed_placeholders).clone();
                                        placeholders.retain(|_, card| {
                                            card.topic_name != placeholder.topic_name
                                        });
                                        placeholders.insert(id, placeholder.clone());
                                        reviewed_placeholders.set(placeholders);

                                        let mut next = (*cards).clone();
                                        next.retain(|card| {
                                            !(card.status == "reviewed"
                                                && card.topic_name == placeholder.topic_name)
                                        });
                                        if let Some(card) =
                                            next.iter_mut().find(|card| card.id == id)
                                        {
                                            *card = placeholder.clone();
                                        } else {
                                            next.push(placeholder.clone());
                                        }
                                        cards.set(next);
                                        let message = next_error.unwrap_or_else(|| {
                                            "Review saved, but the next card is unavailable"
                                                .to_string()
                                        });
                                        toast(
                                            &app_state,
                                            format!(
                                                "Review saved, but the next card could not be loaded: {message}"
                                            ),
                                        );
                                    }
                                } else {
                                    cards.set(
                                        cards
                                            .iter()
                                            .filter(|card| card.id != id)
                                            .cloned()
                                            .collect(),
                                    );
                                }
                            }
                            if reload_flow {
                                load_cards.emit(true);
                            }
                        }
                        Err(err) => {
                            if !err.mutation_outcome_indeterminate {
                                review_idempotency_keys.borrow_mut().remove(&id);
                            }
                            if let Some(topic) = reviewed_repeatable_topic {
                                replacement_pending_topics.borrow_mut().remove(&topic);
                                let mut loading_topics = (*replacement_loading_topics).clone();
                                loading_topics.remove(&topic);
                                replacement_loading_topics.set(loading_topics);
                            }
                            toast(&app_state, err.to_string());
                        }
                    }
                    review_in_flight_ids.borrow_mut().remove(&id);
                    let mut reviewing = (*reviewing_card_ids).clone();
                    reviewing.remove(&id);
                    reviewing_card_ids.set(reviewing);
                });
            },
        )
    };

    let on_continue_cb = {
        let app_state = app_state.clone();
        let load_cards = load_cards.clone();
        let pending_count = pending_count.clone();
        Callback::from(move |(topic, tipcard_type): (String, String)| {
            let app_state = app_state.clone();
            let load_cards = load_cards.clone();
            let pending_count = pending_count.clone();
            pending_count.set(1);
            wasm_bindgen_futures::spawn_local(async move {
                match api_v1::continue_daily_review(vec![topic], Some(tipcard_type)).await {
                    Ok(_) => {
                        toast(&app_state, "Continuing today's review");
                        load_cards.emit(true);
                    }
                    Err(err) => toast(&app_state, err.to_string()),
                }
                pending_count.set(0);
            });
        })
    };

    let on_toggle_pin_cb = {
        let cards = cards.clone();
        let pinned_card_order = pinned_card_order.clone();
        Callback::from(move |(id, pinned): (i64, bool)| {
            let cards = cards.clone();
            let pinned_card_order = pinned_card_order.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if api_v1::pin_tipcard(id, pinned).await.is_ok() {
                    let mut next = (*cards).clone();
                    if let Some(card) = next.iter_mut().find(|card| card.id == id) {
                        card.pinned = pinned;
                    }
                    let pinned_ids = next
                        .iter()
                        .filter(|card| card.pinned)
                        .map(|card| card.id)
                        .collect::<Vec<_>>();
                    let order = normalize_card_order((*pinned_card_order).clone(), &pinned_ids);
                    let _ = LocalStorage::set(PINNED_CARD_ORDER_KEY, &order);
                    pinned_card_order.set(order);
                    cards.set(next);
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
                // Tipcard image replace has no v1 op yet — keep session dashboard route.
                let req = PinImagesReq {
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
        let reviewed_placeholders = reviewed_placeholders.clone();
        let fullscreen_card_key = fullscreen_card_key.clone();
        Callback::from(move |id: i64| {
            let app_state = app_state.clone();
            let cards = cards.clone();
            let reviewed_placeholders = reviewed_placeholders.clone();
            let fullscreen_card_key = fullscreen_card_key.clone();
            let deleted_card_key = cards.iter().find(|card| card.id == id).map(flow_card_key);
            wasm_bindgen_futures::spawn_local(async move {
                match api_v1::delete_tipcard(id).await {
                    Ok(()) => {
                        toast(&app_state, "Card deleted");
                        if deleted_card_key == *fullscreen_card_key {
                            set_fullscreen_body_class(false);
                            fullscreen_card_key.set(None);
                        }
                        cards.set(cards.iter().filter(|card| card.id != id).cloned().collect());
                        let mut placeholders = (*reviewed_placeholders).clone();
                        placeholders.remove(&id);
                        reviewed_placeholders.set(placeholders);
                    }
                    Err(err) => toast(&app_state, err.to_string()),
                }
            });
        })
    };

    let on_toggle_fullscreen = {
        let fullscreen_card_key = fullscreen_card_key.clone();
        let request_detail = request_detail.clone();
        let cards = cards.clone();
        Callback::from(move |id: i64| {
            let Some(card_key) = cards.iter().find(|card| card.id == id).map(flow_card_key) else {
                return;
            };
            if *fullscreen_card_key == Some(card_key.clone()) {
                set_fullscreen_body_class(false);
                fullscreen_card_key.set(None);
            } else {
                set_fullscreen_body_class(true);
                request_detail.emit(id);
                fullscreen_card_key.set(Some(card_key));
            }
        })
    };

    let on_reorder_pinned = {
        let cards = cards.clone();
        let pinned_card_order = pinned_card_order.clone();
        Callback::from(move |(source_id, target_id): (i64, i64)| {
            let pinned_ids = cards
                .iter()
                .filter(|card| card.pinned)
                .map(|card| card.id)
                .collect::<Vec<_>>();
            if !pinned_ids.contains(&source_id) || !pinned_ids.contains(&target_id) {
                return;
            }
            let mut order = normalize_card_order((*pinned_card_order).clone(), &pinned_ids);
            let (Some(source_index), Some(target_index)) = (
                order.iter().position(|&id| id == source_id),
                order.iter().position(|&id| id == target_id),
            ) else {
                return;
            };
            let card_id = order.remove(source_index);
            order.insert(target_index, card_id);
            let _ = LocalStorage::set(PINNED_CARD_ORDER_KEY, &order);
            pinned_card_order.set(order);
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

    let mut pinned_cards: Vec<TipcardInfo> =
        cards.iter().filter(|card| card.pinned).cloned().collect();
    let mut unpinned_cards: Vec<TipcardInfo> =
        cards.iter().filter(|card| !card.pinned).cloned().collect();
    let pinned_ids = pinned_cards.iter().map(|card| card.id).collect::<Vec<_>>();
    let normalized_pinned_order = normalize_card_order((*pinned_card_order).clone(), &pinned_ids);
    pinned_cards.sort_by_key(|card| {
        normalized_pinned_order
            .iter()
            .position(|&id| id == card.id)
            .unwrap_or(usize::MAX)
    });
    sort_flow_cards(&mut unpinned_cards, sort_by.as_str());

    let (transmission_picks, remaining_cards) = split_topic_picks(&unpinned_cards);
    let mut current_ids: Vec<i64> = pinned_cards
        .iter()
        .chain(transmission_picks.iter())
        .chain(remaining_cards.iter())
        .map(|card| card.id)
        .collect();

    // Fullscreen can target a card that topic-pick filtering would otherwise hide
    // (e.g. stacked active repeatables). Keep that card mounted so the
    // `is-fullscreen` node remains while `body.has-fullscreen-card` is set.
    let current_keys = pinned_cards
        .iter()
        .chain(transmission_picks.iter())
        .chain(remaining_cards.iter())
        .map(flow_card_key)
        .collect::<HashSet<_>>();
    let fullscreen_orphan = (*fullscreen_card_key).as_ref().and_then(|key| {
        if current_keys.contains(key) {
            None
        } else {
            cards
                .iter()
                .find(|card| flow_card_key(card) == *key)
                .cloned()
        }
    });
    if let Some(card) = fullscreen_orphan.as_ref() {
        current_ids.push(card.id);
    }

    {
        let fullscreen_card_key = fullscreen_card_key.clone();
        let card_keys = cards.iter().map(flow_card_key).collect::<HashSet<_>>();
        let route = use_route::<View>();
        let flow_active = route.as_ref() == Some(&View::Flow);
        use_effect_with(
            ((*fullscreen_card_key).clone(), card_keys, flow_active),
            move |(fullscreen, keys, active)| {
                // Keep-alive routes share `body.has-fullscreen-card`. Only the active
                // Flow view may own it, and only while its target card still exists.
                let should_lock = *active
                    && match fullscreen {
                        Some(key) if keys.contains(key) => true,
                        Some(_) => {
                            fullscreen_card_key.set(None);
                            false
                        }
                        None => false,
                    };
                set_fullscreen_body_class(should_lock);
                move || {
                    set_fullscreen_body_class(false);
                }
            },
        );
    }

    let list_mode = *layout == "list";
    let visible_card_count = current_ids.len();
    let disable_flow_glass =
        should_disable_flow_glass(list_mode, visible_card_count, &card_heights, &current_ids);

    {
        let load_cards = load_cards.clone();
        let has_more = *has_more;
        let loading = *loading;
        let loaded_count = cards.len();
        use_effect_with((has_more, loading, loaded_count), move |_| {
            if has_more && !loading && loaded_count == 0 {
                load_cards.emit(false);
            }
            || ()
        });
    }

    let on_manual_images = {
        let manual_images = manual_images.clone();
        let app_state = app_state.clone();
        Callback::from(move |e: Event| {
            let Some(input) = e.target_dyn_into::<HtmlInputElement>() else {
                return;
            };
            let Some(files) = input.files() else {
                return;
            };
            if files.length() == 0 {
                return;
            }
            let selected = collect_files(&files);
            input.set_value("");
            let manual_images = manual_images.clone();
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match compress_files_to_data_urls(selected).await {
                    Ok(mut compressed) => {
                        let mut next = (*manual_images).clone();
                        next.append(&mut compressed);
                        manual_images.set(next);
                    }
                    Err(message) => toast(&app_state, message),
                }
            });
        })
    };
    let on_upload_error = {
        let app_state = app_state.clone();
        Callback::from(move |message: String| toast(&app_state, message))
    };
    let render_flow_card = |card: &TipcardInfo, enable_drag: bool| {
        let card = card.clone();
        let id = card.id;
        let card_key = flow_card_key(&card);
        html! {
            <FlowCard
                key={card_key.clone()}
                card={card}
                on_review={on_review_cb.clone()}
                on_continue={on_continue_cb.clone()}
                on_toggle_pin={on_toggle_pin_cb.clone()}
                on_delete={on_delete_cb.clone()}
                on_reorder={on_reorder_pinned.clone()}
                on_update_images={on_update_images_cb.clone()}
                on_upload_error={on_upload_error.clone()}
                on_toggle_fullscreen={on_toggle_fullscreen.clone()}
                on_request_detail={request_detail.clone()}
                on_measure={on_measure.clone()}
                list_mode={list_mode}
                fullscreen={*fullscreen_card_key == Some(card_key.clone())}
                detail_loaded={detail_loaded.get(&id).copied().unwrap_or(false)}
                enable_drag={enable_drag}
                review_pending={review_in_flight_ids.borrow().contains(&id)}
            />
        }
    };
    let grid_classes = if list_mode {
        "grid grid-cols-1 gap-3 items-start w-full max-w-4xl mx-auto"
    } else {
        grid_classes_for_columns(*grid_columns)
    };

    html! {
        <section
            id="view-flow"
            class={classes!(disable_flow_glass.then_some("flow-many-cards"))}
        >
            <div class="mb-4">
                <h1 class="text-xl font-semibold tracking-tight">{"Transmission"}</h1>
                <p class="text-muted mt-2">{"All cards in one review surface."}</p>
            </div>
            <div class="flow-toolbar mb-4 flex flex-col items-end gap-3">
                <form id="tips-form" onsubmit={on_submit} class="surface border rounded-md p-4 grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-5 gap-3 w-full sm:w-auto sm:max-w-fit">
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
                        <button type="button" onclick={let t = tip_type.clone(); Callback::from(move |_| t.set("manual_tip".to_string()))} class={classes!("rounded-md", "px-3", "py-2", "text-sm", "font-medium", "inline-flex", "items-center", "justify-center", "gap-1.5", (*tip_type == "manual_tip").then_some("active"))}>
                            {"Manual"}
                        </button>
                    </div>
                    <button
                        id="tips-submit-btn"
                        type="submit"
                        class={classes!("rounded-md", "bg-primary-solid", "px-4", "py-2", "font-medium", "flex", "items-center", "justify-center", "gap-2", (*pending_count > 0).then_some("opacity-60 cursor-not-allowed"))}
                        disabled={*pending_count > 0}
                    >
                        <iconify-icon icon={if *pending_count > 0 { "radix-icons:update" } else if *tip_type == "manual_tip" { "material-symbols:accessible-forward" } else { "radix-icons:magic-wand" }} class={classes!("radix-icon", (*pending_count > 0).then_some("animate-spin"))} aria-hidden="true"></iconify-icon>
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
                    </div>
                    <div class="relative z-40 flex muted-surface rounded-md p-1 border border-token">
                        <div class="relative flex">
                            <button
                                id="flow-grid-btn"
                                type="button"
                                class={classes!("rounded", "px-2", "py-1", (!list_mode).then_some("bg-primary-soft text-primary"))}
                                aria-label={i18n.t("flow.grid_layout")}
                                aria-haspopup="menu"
                                aria-expanded={grid_columns_open.to_string()}
                                onclick={Callback::from({
                                    let layout = layout.clone();
                                    let grid_columns_open = grid_columns_open.clone();
                                    move |_| {
                                        let _ = LocalStorage::set("denpie-flow-layout", "grid");
                                        layout.set("grid".to_string());
                                        grid_columns_open.set(!*grid_columns_open);
                                    }
                                })}
                            >
                                <iconify-icon icon="radix-icons:grid" class="radix-icon"></iconify-icon>
                            </button>
                            if *grid_columns_open {
                                <div
                                    role="menu"
                                    aria-label={i18n.t("flow.grid_columns")}
                                    class="shadcn-dropdown-menu opens-down grid grid-cols-4 gap-1"
                                    style="width: auto; min-width: max-content; padding: 0.375rem; right: 0; left: auto;"
                                >
                                    {
                                        for (1..=4).map(|columns| {
                                            let grid_columns = grid_columns.clone();
                                            let grid_columns_open = grid_columns_open.clone();
                                            html! {
                                                <button
                                                    type="button"
                                                    role="menuitemradio"
                                                    aria-checked={(*grid_columns == columns).to_string()}
                                                    aria-label={i18n.tf("flow.column_count", &[("count", columns.to_string())])}
                                                    class={classes!(
                                                        "size-8",
                                                        "rounded",
                                                        "text-sm",
                                                        "font-medium",
                                                        (*grid_columns == columns).then_some("bg-primary-soft text-primary"),
                                                    )}
                                                    onclick={Callback::from(move |_| {
                                                        let _ = LocalStorage::set(FLOW_GRID_COLUMNS_KEY, columns);
                                                        grid_columns.set(columns);
                                                        grid_columns_open.set(false);
                                                    })}
                                                >
                                                    {columns}
                                                </button>
                                            }
                                        })
                                    }
                                </div>
                            }
                        </div>
                        <button id="flow-list-btn" type="button" class={classes!("rounded", "px-2", "py-1", list_mode.then_some("bg-primary-soft text-primary"))} onclick={Callback::from({ let layout = layout.clone(); let grid_columns_open = grid_columns_open.clone(); move |_| { let _ = LocalStorage::set("denpie-flow-layout", "list"); layout.set("list".to_string()); grid_columns_open.set(false); } })}>
                            <iconify-icon icon="radix-icons:list-bullet" class="radix-icon"></iconify-icon>
                        </button>
                    </div>
                </div>
            </div>

            if !pinned_cards.is_empty() {
                <section id="flow-pins" class="mb-8" aria-labelledby="flow-pins-heading">
                    <div class="flex items-baseline gap-2 mb-4">
                        <h2 id="flow-pins-heading" class="text-lg font-semibold tracking-tight">
                            {i18n.t("flow.pins")}
                        </h2>
                        <span id="flow-pinned-count" class="text-sm text-muted">
                            {pinned_cards.len()}
                        </span>
                    </div>
                    <div
                        id="flow-pinned-grid"
                        class={grid_classes}
                    >
                        {for pinned_cards.iter().map(|card| render_flow_card(card, true))}
                    </div>
                </section>
            }

            <div class="mb-4">
                <h2 id="flow-picks-heading" class="text-lg font-semibold tracking-tight">
                    {i18n.t("flow.picks")}
                </h2>
                <div class="text-sm text-muted mt-1">
                    <span id="flow-count">{transmission_picks.len()}</span>
                    {format!("/{TRANSMISSION_MAX_PICKS} {}", i18n.t("flow.picks_count_suffix"))}
                </div>
            </div>

            <div
                id="flow-grid"
                class={grid_classes}
                aria-labelledby="flow-picks-heading"
            >
                {
                    for (0..*pending_count).map(|i| html! {
                        <FlowCardSkeleton
                            key={format!("skeleton-{i}")}
                            list_mode={list_mode}
                            label="Generating card"
                        />
                    })
                }
                {
                    for transmission_picks.iter().map(|card| {
                        if card.tipcard_type == "repeatable_tip"
                            && replacement_loading_topics.contains(&card.topic_name)
                        {
                            html! {
                                <FlowCardSkeleton
                                    key={format!("replacement-skeleton:{}", card.topic_name)}
                                    list_mode={list_mode}
                                    label="Loading next card"
                                />
                            }
                        } else {
                            render_flow_card(card, false)
                        }
                    })
                }
            </div>

            if !remaining_cards.is_empty() {
                <section id="flow-other-cards" class="mt-8" aria-labelledby="flow-other-cards-heading">
                    <div class="flex items-center justify-between gap-3 mb-4">
                        <h2 id="flow-other-cards-heading" class="text-lg font-semibold tracking-tight">
                            {i18n.t("flow.other_cards")}
                        </h2>
                        <span id="flow-other-count" class="text-sm text-muted">
                            {remaining_cards.len()}
                        </span>
                    </div>
                    <div
                        id="flow-other-grid"
                        class={grid_classes}
                    >
                        {for remaining_cards.iter().map(|card| render_flow_card(card, false))}
                    </div>
                </section>
            }

            if *loading {
                <div class="flex justify-center py-8 text-sm text-muted">{"Loading cards..."}</div>
            } else if *has_more {
                <div class="flex justify-center py-8">
                    <button type="button" class="rounded-md border border-token px-6 py-2 font-medium" onclick={Callback::from({ let load_cards = load_cards.clone(); move |_| load_cards.emit(false) })}>{"Load More Cards"}</button>
                </div>
            }

            if visible_card_count == 0 && !*loading {
                <div id="empty-flow" class="surface border rounded-md p-10 text-center text-muted">
                    {"No cards yet."}
                </div>
            }

            if let Some(card) = fullscreen_orphan.as_ref() {
                { render_flow_card(card, false) }
            }
        </section>
    }
}

const FLOW_GLASS_GRID_THRESHOLD: usize = 8;
const FLOW_GLASS_LIST_VIEWPORT_MULTIPLIER: usize = 3;

fn normalize_grid_columns(columns: usize) -> usize {
    columns.clamp(1, 4)
}

fn grid_classes_for_columns(columns: usize) -> &'static str {
    match normalize_grid_columns(columns) {
        1 => "grid grid-cols-1 gap-3 items-start",
        2 => "grid grid-cols-1 md:grid-cols-2 gap-3 items-start",
        3 => "grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3 items-start",
        _ => "grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4 gap-3 items-start",
    }
}

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
        "date" | "topic" => value.to_string(),
        _ => "topic".to_string(),
    }
}

fn sort_flow_cards(cards: &mut [TipcardInfo], sort_by: &str) {
    match sort_by {
        "date" => cards.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
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

fn flow_card_key(card: &TipcardInfo) -> String {
    if card.tipcard_type == "repeatable_tip" {
        format!("repeatable:{}", card.topic_name)
    } else {
        format!("card:{}", card.id)
    }
}

fn select_topic_picks(cards: &[TipcardInfo]) -> Vec<TipcardInfo> {
    let active_repeatable_topics = cards
        .iter()
        .filter(|card| card.tipcard_type == "repeatable_tip" && card.status == "active")
        .map(|card| card.topic_name.as_str())
        .collect::<HashSet<_>>();
    let topic_count = cards
        .iter()
        .filter(|card| {
            card.status == "active"
                || card.tipcard_type == "repeatable_tip"
                    && card.status == "reviewed"
                    && !active_repeatable_topics.contains(card.topic_name.as_str())
        })
        .map(|card| card.topic_name.as_str())
        .collect::<HashSet<_>>()
        .len();
    if topic_count == 0 {
        return Vec::new();
    }

    let per_topic_limit =
        TRANSMISSION_MAX_PICKS_PER_TOPIC.min((TRANSMISSION_MAX_PICKS / topic_count).max(1));
    let mut topic_counts = HashMap::<&str, usize>::new();
    let mut repeatable_topics = HashSet::<&str>::new();
    let mut picks = Vec::with_capacity(TRANSMISSION_MAX_PICKS.min(cards.len()));

    for card in cards {
        let repeatable_placeholder = card.tipcard_type == "repeatable_tip"
            && card.status == "reviewed"
            && !active_repeatable_topics.contains(card.topic_name.as_str());
        if card.status != "active" && !repeatable_placeholder {
            continue;
        }
        if card.tipcard_type == "repeatable_tip"
            && !repeatable_topics.insert(card.topic_name.as_str())
        {
            continue;
        }
        let count = topic_counts.entry(card.topic_name.as_str()).or_default();
        let card_limit = if card.tipcard_type == "repeatable_tip" {
            1
        } else {
            per_topic_limit
        };
        if *count >= card_limit {
            continue;
        }
        picks.push(card.clone());
        *count += 1;
        if picks.len() == TRANSMISSION_MAX_PICKS {
            break;
        }
    }

    picks
}

fn split_topic_picks(cards: &[TipcardInfo]) -> (Vec<TipcardInfo>, Vec<TipcardInfo>) {
    let picks = select_topic_picks(cards);
    let pick_ids = picks.iter().map(|card| card.id).collect::<HashSet<_>>();
    let remaining = cards
        .iter()
        .filter(|card| !(pick_ids.contains(&card.id) || card.tipcard_type == "repeatable_tip"))
        .cloned()
        .collect();
    (picks, remaining)
}

fn review_placeholder_message(action: &str, _previous_review_at: &str) -> String {
    match action {
        "again" | "repeat" => {
            "Saved for another review on its SM-2 schedule. Come back tomorrow, or continue with another card now.".to_string()
        }
        "learned" | "memorize" => {
            "Marked as learned on its SM-2 schedule. Come back tomorrow, or continue with another card now.".to_string()
        }
        "skip_known" => "Skipped as already known. Come back tomorrow, or continue with another card now.".to_string(),
        "skip_too_difficult" => {
            "Skipped as too difficult. Come back tomorrow, or continue with another card now.".to_string()
        }
        "skip_not_interested" | "dismiss" => {
            "Skipped as not interesting. Come back tomorrow, or continue with another card now.".to_string()
        }
        _ => "Review saved on its schedule. Come back tomorrow, or continue with another card now.".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: i64, topic_name: &str) -> TipcardInfo {
        TipcardInfo {
            id,
            topic_name: topic_name.to_string(),
            topic_icon: String::new(),
            topic_color: String::new(),
            title: format!("Card {id}"),
            full_content: String::new(),
            compressed_content: String::new(),
            image_data: Vec::new(),
            created_at: String::new(),
            tipcard_type: "casual_tip".to_string(),
            status: "active".to_string(),
            next_review_at: String::new(),
            repeat_count: 0,
            pinned: false,
            pending_count: 0,
            review_message: None,
        }
    }

    #[test]
    fn transmission_keeps_three_picks_for_non_repeatable_topics() {
        let cards = ["A", "B", "C"]
            .into_iter()
            .flat_map(|topic| (0..4).map(move |index| card(index, topic)))
            .collect::<Vec<_>>();

        let picks = select_topic_picks(&cards);

        assert_eq!(picks.len(), 9);
        for topic in ["A", "B", "C"] {
            assert_eq!(
                picks.iter().filter(|card| card.topic_name == topic).count(),
                3
            );
        }
    }

    #[test]
    fn repeatable_cards_stack_behind_one_topic_pick() {
        let cards = (0..3)
            .map(|index| {
                let mut card = card(index, "A");
                card.tipcard_type = "repeatable_tip".to_string();
                card
            })
            .collect::<Vec<_>>();

        let (picks, remaining) = split_topic_picks(&cards);

        assert_eq!(picks.len(), 1);
        assert!(remaining.is_empty());
    }

    #[test]
    fn reviewed_repeatable_holds_topic_pick_until_next_card_is_active() {
        let mut reviewed = card(1, "A");
        reviewed.tipcard_type = "repeatable_tip".to_string();
        reviewed.status = "reviewed".to_string();
        reviewed.review_message = Some("Review saved".to_string());

        let (picks, remaining) = split_topic_picks(&[reviewed]);

        assert_eq!(
            picks.iter().map(|card| card.id).collect::<Vec<_>>(),
            vec![1]
        );
        assert!(remaining.is_empty());
    }

    #[test]
    fn active_repeatable_replaces_placeholder_in_the_same_topic_slot() {
        let mut reviewed = card(1, "A");
        reviewed.tipcard_type = "repeatable_tip".to_string();
        reviewed.status = "reviewed".to_string();
        let mut active = card(2, "A");
        active.tipcard_type = "repeatable_tip".to_string();

        let (picks, remaining) = split_topic_picks(&[reviewed.clone(), active]);

        assert_eq!(
            picks.iter().map(|card| card.id).collect::<Vec<_>>(),
            vec![2]
        );
        assert!(remaining.is_empty());
        assert_eq!(flow_card_key(&reviewed), flow_card_key(&picks[0]));
    }

    #[test]
    fn legacy_drag_sort_preference_falls_back_to_topic() {
        assert_eq!(normalize_flow_sort("drag"), "topic");
        assert_eq!(normalize_flow_sort("manual"), "topic");
    }

    #[test]
    fn pinned_order_keeps_saved_positions_and_appends_new_cards() {
        assert_eq!(
            normalize_card_order(vec![3, 1, 9], &[1, 2, 3]),
            vec![3, 1, 2]
        );
    }

    #[test]
    fn grid_column_preference_stays_within_supported_range() {
        assert_eq!(normalize_grid_columns(0), 1);
        assert_eq!(normalize_grid_columns(3), 3);
        assert_eq!(normalize_grid_columns(12), 4);
    }

    #[test]
    fn grid_column_preference_keeps_responsive_page_constraints() {
        assert_eq!(
            grid_classes_for_columns(1),
            "grid grid-cols-1 gap-3 items-start"
        );
        assert!(grid_classes_for_columns(2).contains("md:grid-cols-2"));
        assert!(!grid_classes_for_columns(2).contains("xl:grid-cols-3"));
        assert!(grid_classes_for_columns(4).contains("2xl:grid-cols-4"));
    }

    #[test]
    fn transmission_never_shows_more_than_nine_topics() {
        let cards = (0..12)
            .map(|index| card(index, &format!("Topic {index}")))
            .collect::<Vec<_>>();

        let picks = select_topic_picks(&cards);

        assert_eq!(picks.len(), 9);
        assert!(picks.iter().all(|pick| {
            picks
                .iter()
                .filter(|other| other.topic_name == pick.topic_name)
                .count()
                == 1
        }));
    }

    #[test]
    fn reviewed_repeatable_is_hidden_when_its_topic_has_an_active_card() {
        let active = card(1, "A");
        let mut reviewed = card(2, "A");
        reviewed.tipcard_type = "repeatable_tip".to_string();
        reviewed.status = "reviewed".to_string();
        reviewed.review_message = Some("Review saved".to_string());
        let cards = vec![active, reviewed];

        let (picks, remaining) = split_topic_picks(&cards);

        assert_eq!(
            picks.iter().map(|card| card.id).collect::<Vec<_>>(),
            vec![1]
        );
        assert!(remaining.is_empty());
    }
}
