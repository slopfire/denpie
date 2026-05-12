use crate::api::toast;
use crate::components::flow_card::FlowCard;
use crate::state::AppState;
use gloo_file::{callbacks::FileReader, File};
use gloo_net::http::Request;
use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use web_sys::{HtmlInputElement, HtmlTextAreaElement};
use yew::prelude::*;

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

#[derive(Deserialize)]
struct TipcardIdOnly {
    id: i64,
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
    let page = use_state(|| 1usize);
    let fullscreen_card_id = use_state(|| None::<i64>);

    let refresh_cards = {
        let cards = cards.clone();
        Callback::from(move |_| {
            let cards = cards.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(res) = Request::get("/admin/tipcards").send().await {
                    if let Ok(data) = res.json::<Vec<TipcardInfo>>().await {
                        cards.set(data);
                    }
                }
            });
        })
    };

    {
        let refresh_cards = refresh_cards.clone();
        use_effect_with((), move |_| {
            refresh_cards.emit(());
            || ()
        });
    }

    let on_submit = {
        let app_state = app_state.clone();
        let topics_input = topics_input.clone();
        let tip_type = tip_type.clone();
        let manual_content = manual_content.clone();
        let manual_images = manual_images.clone();
        let refresh_cards = refresh_cards.clone();

        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let app_state = app_state.clone();
            let topics = (*topics_input).clone();
            let ttype = (*tip_type).clone();
            let content = (*manual_content).clone();
            let images = (*manual_images).clone();
            let refresh_cards = refresh_cards.clone();

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
                        let _ = LocalStorage::delete("denpie_prefill_topic");
                        let _ = LocalStorage::delete("denpie_prefill_type");
                        refresh_cards.emit(());
                    }
                    _ => {
                        toast(&app_state, "Failed to add cards");
                    }
                }
            });
        })
    };

    let on_review_cb = {
        let refresh_cards = refresh_cards.clone();
        let cards = cards.clone();
        let app_state = app_state.clone();
        Callback::from(
            move |(id, grade, action): (i64, Option<u8>, Option<String>)| {
                let refresh_cards = refresh_cards.clone();
                let cards = cards.clone();
                let app_state = app_state.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let action_for_replace = action.clone();
                    let req = ReviewReq {
                        card_id: id,
                        grade,
                        action,
                    };
                    if Request::post("/app/review")
                        .json(&req)
                        .unwrap()
                        .send()
                        .await
                        .is_ok()
                    {
                        if let Some(card) = cards.iter().find(|card| card.id == id).cloned() {
                            let action_name = action_for_replace.as_deref().unwrap_or_default();
                            let should_replace = (card.tipcard_type == "casual_tip"
                                && action_name == "dismiss")
                                || (card.tipcard_type == "repeatable_tip"
                                    && matches!(action_name, "dismiss" | "repeat" | "memorize"));
                            if should_replace {
                                let pull = CreateTipReq {
                                    topics: card.topic_name,
                                    tipcard_type: Some(card.tipcard_type),
                                    manual_content: None,
                                    manual_image_data: None,
                                    exclude_card_ids: Some(
                                        cards.iter().map(|card| card.id).collect(),
                                    ),
                                };
                                if let Ok(res) =
                                    Request::post("/app/tips").json(&pull).unwrap().send().await
                                {
                                    if res.ok() {
                                        if let Ok(new_cards) =
                                            res.json::<Vec<TipcardIdOnly>>().await
                                        {
                                            let mut order =
                                                LocalStorage::get::<Vec<i64>>("denpie-card-order")
                                                    .unwrap_or_default();
                                            let new_ids: Vec<i64> =
                                                new_cards.iter().map(|card| card.id).collect();
                                            if let Some(position) =
                                                order.iter().position(|existing| *existing == id)
                                            {
                                                order.retain(|existing| {
                                                    *existing != id && !new_ids.contains(existing)
                                                });
                                                for (offset, new_id) in new_ids.iter().enumerate() {
                                                    order.insert(position + offset, *new_id);
                                                }
                                            } else {
                                                order
                                                    .retain(|existing| !new_ids.contains(existing));
                                                order.extend(new_ids);
                                            }
                                            let _ = LocalStorage::set("denpie-card-order", &order);
                                        }
                                    }
                                }
                            }
                        }
                        refresh_cards.emit(());
                    } else {
                        toast(&app_state, "Review failed");
                    }
                });
            },
        )
    };

    let on_toggle_pin_cb = {
        let refresh_cards = refresh_cards.clone();
        Callback::from(move |(id, pinned): (i64, bool)| {
            let refresh_cards = refresh_cards.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = PinReq {
                    id,
                    pinned: Some(pinned),
                    image_data: None,
                };
                if Request::patch("/admin/tipcards")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                    .is_ok()
                {
                    refresh_cards.emit(());
                }
            });
        })
    };

    let on_update_images_cb = {
        let refresh_cards = refresh_cards.clone();
        let app_state = app_state.clone();
        Callback::from(move |(id, image_data): (i64, Vec<String>)| {
            let refresh_cards = refresh_cards.clone();
            let app_state = app_state.clone();
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
                        refresh_cards.emit(());
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
        let refresh_cards = refresh_cards.clone();
        Callback::from(move |id: i64| {
            let app_state = app_state.clone();
            let refresh_cards = refresh_cards.clone();
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
                    refresh_cards.emit(());
                }
            });
        })
    };

    let on_reorder_cb = {
        let card_order = card_order.clone();
        let cards = cards.clone();
        Callback::from(move |(source_id, target_id): (i64, i64)| {
            let mut order = (*card_order).clone();
            if order.is_empty() {
                order = cards.iter().map(|c| c.id).collect();
            }

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
        Callback::from(move |id: i64| {
            if *fullscreen_card_id == Some(id) {
                fullscreen_card_id.set(None);
            } else {
                fullscreen_card_id.set(Some(id));
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

    let mut sorted_cards = (*cards).clone();
    sorted_cards.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    if !card_order.is_empty() {
        sorted_cards.sort_by_key(|c| {
            card_order
                .iter()
                .position(|&id| id == c.id)
                .unwrap_or(usize::MAX)
        });
    }

    let flow_cards: Vec<_> = sorted_cards
        .iter()
        .filter(|card| {
            (card.status.is_empty() || card.status == "active") && (card.pinned || is_due(card))
        })
        .cloned()
        .collect();
    let pinned_cards: Vec<_> = flow_cards.iter().filter(|c| c.pinned).cloned().collect();
    let unpinned_cards_all: Vec<_> = flow_cards.iter().filter(|c| !c.pinned).cloned().collect();
    let visible_unpinned: Vec<_> = unpinned_cards_all
        .iter()
        .take(*page * 24)
        .cloned()
        .collect();
    let list_mode = *layout == "list";
    let visible_card_count = pinned_cards.len() + visible_unpinned.len();

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
            }
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
            class={classes!((visible_card_count > 8).then_some("flow-many-cards"))}
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
                    <span id="flow-count">{flow_cards.len()}</span>{" cards"}
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

            if !pinned_cards.is_empty() {
                <div id="pinned-flow-section" class="mb-4">
                    <div class="flex items-center gap-2 mb-3 text-sm font-medium text-primary">
                        <iconify-icon icon="radix-icons:drawing-pin-filled" class="radix-icon" aria-hidden="true"></iconify-icon>
                        <span>{"Pinned"}</span>
                    </div>
                    <div id="pinned-flow-grid" class={if list_mode { "grid grid-cols-1 gap-3 items-start w-full max-w-4xl mx-auto" } else { "grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4 gap-3 items-start" }}>
                        {
                            for pinned_cards.iter().map(|c| {
                                html! {
                                    <FlowCard
                                        card={c.clone()}
                                        on_review={on_review_cb.clone()}
                                        on_toggle_pin={on_toggle_pin_cb.clone()}
                                        on_delete={on_delete_cb.clone()}
                                        on_reorder={on_reorder_cb.clone()}
                                        on_update_images={on_update_images_cb.clone()}
                                        on_toggle_fullscreen={on_toggle_fullscreen.clone()}
                                        list_mode={list_mode}
                                        fullscreen={*fullscreen_card_id == Some(c.id)}
                                    />
                                }
                            })
                        }
                    </div>
                </div>
            }

            <div id="flow-grid" class={if list_mode { "grid grid-cols-1 gap-3 items-start w-full max-w-4xl mx-auto" } else { "grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4 gap-3 items-start" }}>
                {
                    for visible_unpinned.iter().map(|c| {
                        html! {
                            <FlowCard
                                card={c.clone()}
                                on_review={on_review_cb.clone()}
                                on_toggle_pin={on_toggle_pin_cb.clone()}
                                on_delete={on_delete_cb.clone()}
                                on_reorder={on_reorder_cb.clone()}
                                on_update_images={on_update_images_cb.clone()}
                                on_toggle_fullscreen={on_toggle_fullscreen.clone()}
                                list_mode={list_mode}
                                fullscreen={*fullscreen_card_id == Some(c.id)}
                            />
                        }
                    })
                }
            </div>
            if unpinned_cards_all.len() > visible_unpinned.len() {
                <div class="flex justify-center py-8">
                    <button type="button" class="rounded-md border border-token px-6 py-2 font-medium" onclick={Callback::from({ let page = page.clone(); move |_| page.set(*page + 1) })}>{"Load More Cards"}</button>
                </div>
            }

            if flow_cards.is_empty() {
                <div id="empty-flow" class="surface border rounded-md p-10 text-center text-muted">
                    {"No cards yet."}
                </div>
            }
        </section>
    }
}

fn is_due(card: &TipcardInfo) -> bool {
    let raw = card.next_review_at.trim();
    if raw.is_empty() {
        return true;
    }
    let isoish = if raw.contains('T') {
        raw.to_string()
    } else {
        raw.replace(' ', "T")
    };
    let has_offset = isoish.len() >= 6
        && matches!(
            isoish.as_bytes().get(isoish.len() - 6),
            Some(b'+') | Some(b'-')
        );
    let normalized = if isoish.ends_with('Z') || has_offset {
        isoish
    } else {
        format!("{isoish}Z")
    };
    let parsed = js_sys::Date::parse(&normalized);
    parsed.is_nan() || parsed <= js_sys::Date::now()
}
