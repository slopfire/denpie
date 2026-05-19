use crate::api::toast;
use crate::components::flow_card::FlowCard;
use crate::state::AppState;
use gloo_file::{callbacks::FileReader, File};
use gloo_net::http::Request;
use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};
use web_sys::{HtmlInputElement, HtmlTextAreaElement};
use yew::prelude::*;

const PAGE_LIMIT: i64 = 48;

#[derive(Deserialize, Clone, PartialEq)]
pub struct TipcardInfo {
    pub id: i64,
    pub topic_name: String,
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
    let card_order =
        use_state(|| LocalStorage::get::<Vec<i64>>("denpie-card-order").unwrap_or_default());
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

        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let app_state = app_state.clone();
            let topics = (*topics_input).clone();
            let ttype = (*tip_type).clone();
            let content = (*manual_content).clone();
            let images = (*manual_images).clone();
            let load_cards = load_cards.clone();

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
                    match Request::post("/app/review").json(&req).unwrap().send().await {
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
        Callback::from(move |(id, pinned): (i64, bool)| {
            let cards = cards.clone();
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
                        cards.set(next);
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
        let cards = cards.clone();
        Callback::from(move |(source_id, target_id): (i64, i64)| {
            let current_ids: Vec<i64> = cards.iter().map(|c| c.id).collect();
            let mut order = normalize_card_order((*card_order).clone(), &current_ids);

            if let (Some(from_idx), Some(to_idx)) = (
                order.iter().position(|&id| id == source_id),
                order.iter().position(|&id| id == target_id),
            ) {
                let item = order.remove(from_idx);
                order.insert(to_idx, item);
                let _ = LocalStorage::set("denpie-card-order", &order);
                card_order.set(order);
            }
        })
    };

    let on_toggle_fullscreen = {
        let fullscreen_card_id = fullscreen_card_id.clone();
        let request_detail = request_detail.clone();
        Callback::from(move |id: i64| {
            if *fullscreen_card_id == Some(id) {
                fullscreen_card_id.set(None);
            } else {
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
            let body = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| document.body());
            if let Some(body) = body.as_ref() {
                let _ = body
                    .class_list()
                    .toggle_with_force("has-fullscreen-card", fullscreen.is_some());
            }
            move || {
                if let Some(body) = body.as_ref() {
                    let _ = body.class_list().remove_1("has-fullscreen-card");
                }
            }
        });
    }

    let mut flow_cards = (*cards).clone();
    flow_cards.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    let current_ids: Vec<i64> = flow_cards.iter().map(|card| card.id).collect();
    let normalized_order = normalize_card_order((*card_order).clone(), &current_ids);
    if !normalized_order.is_empty() {
        flow_cards.sort_by_key(|c| {
            normalized_order
                .iter()
                .position(|&id| id == c.id)
                .unwrap_or(usize::MAX)
        });
    }

    let list_mode = *layout == "list";

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

    html! {
        <section
            id="view-flow"
            class={classes!((flow_cards.len() > 8).then_some("flow-many-cards"))}
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
                        required=true
                    />
                    <div class="tip-type-switch muted-surface border border-token rounded-md p-1 grid grid-cols-3 sm:col-span-2" role="group">
                        <button type="button" onclick={let t = tip_type.clone(); Callback::from(move |_| t.set("casual_tip".to_string()))} class={classes!("rounded-md", "px-3", "py-2", "text-sm", "font-medium", (*tip_type == "casual_tip").then_some("active"))}>{"Casual"}</button>
                        <button type="button" onclick={let t = tip_type.clone(); Callback::from(move |_| t.set("repeatable_tip".to_string()))} class={classes!("rounded-md", "px-3", "py-2", "text-sm", "font-medium", (*tip_type == "repeatable_tip").then_some("active"))}>{"Repeat"}</button>
                        <button type="button" onclick={let t = tip_type.clone(); Callback::from(move |_| t.set("manual_tip".to_string()))} class={classes!("rounded-md", "px-3", "py-2", "text-sm", "font-medium", (*tip_type == "manual_tip").then_some("active"))}>{"Manual"}</button>
                    </div>
                    <button type="submit" class="rounded-md bg-primary-solid px-4 py-2 font-medium flex items-center justify-center gap-2">
                        <iconify-icon icon="radix-icons:magic-wand" class="radix-icon" aria-hidden="true"></iconify-icon>
                        <span>{"Add"}</span>
                    </button>
                    if *tip_type == "manual_tip" {
                        <textarea
                            id="manual-card-content"
                            class="rounded-md border px-3 py-2 sm:col-span-2 xl:col-span-5 h-20 resize-y"
                            placeholder="Manual card content"
                            value={(*manual_content).clone()}
                            oninput={Callback::from({ let manual_content = manual_content.clone(); move |e: InputEvent| if let Some(target) = e.target_dyn_into::<HtmlTextAreaElement>() { manual_content.set(target.value()); }})}
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
                    <span id="flow-count">{flow_cards.len()}</span>{" loaded cards"}
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

            <div
                id="flow-grid"
                class={if list_mode { "grid grid-cols-1 gap-3 items-start w-full max-w-4xl mx-auto" } else { "grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4 gap-3 items-start" }}
            >
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

fn normalize_card_order(mut order: Vec<i64>, current_ids: &[i64]) -> Vec<i64> {
    order.retain(|id| current_ids.contains(id));
    for id in current_ids {
        if !order.contains(id) {
            order.push(*id);
        }
    }
    order
}
