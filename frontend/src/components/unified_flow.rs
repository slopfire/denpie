use crate::state::{AppAction, AppState};
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};
use web_sys::HtmlInputElement;
use yew::prelude::*;
use crate::components::flow_card::FlowCard;
use gloo_storage::{Storage, LocalStorage};

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
    tipcard_type: String,
    manual_content: Option<String>,
    manual_images: Vec<String>,
}

#[derive(Serialize)]
struct ReviewReq {
    card_id: i64,
    grade: Option<u8>,
    action: Option<String>,
}

#[derive(Serialize)]
struct PinReq {
    card_id: i64,
    pinned: bool,
}

#[function_component(UnifiedFlow)]
pub fn unified_flow() -> Html {
    let app_state = use_context::<UseReducerHandle<AppState>>().unwrap();
    let cards = use_state(Vec::<TipcardInfo>::new);
    let card_order = use_state(|| LocalStorage::get::<Vec<i64>>("denpie_card_order").unwrap_or_default());
    let topics_input = use_state(String::new);
    let tip_type = use_state(|| "casual_tip".to_string());

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
        let refresh_cards = refresh_cards.clone();

        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let app_state = app_state.clone();
            let topics = (*topics_input).clone();
            let ttype = (*tip_type).clone();
            let refresh_cards = refresh_cards.clone();

            wasm_bindgen_futures::spawn_local(async move {
                let req = CreateTipReq {
                    topics,
                    tipcard_type: ttype,
                    manual_content: None,
                    manual_images: Vec::new(),
                };
                match Request::post("/api").header("X-Denpie-Op", "Tips").json(&req).unwrap().send().await {
                    Ok(res) if res.ok() => {
                        app_state.dispatch(AppAction::ShowToast("Cards added".to_string()));
                        let state_clone = app_state.clone();
                        gloo_timers::callback::Timeout::new(2400, move || {
                            state_clone.dispatch(AppAction::HideToast);
                        }).forget();
                        refresh_cards.emit(());
                    }
                    _ => {
                        app_state.dispatch(AppAction::ShowToast("Failed to add cards".to_string()));
                        let state_clone = app_state.clone();
                        gloo_timers::callback::Timeout::new(2400, move || {
                            state_clone.dispatch(AppAction::HideToast);
                        }).forget();
                    }
                }
            });
        })
    };

    let on_review_cb = {
        let refresh_cards = refresh_cards.clone();
        Callback::from(move |(id, grade, action): (i64, Option<u8>, Option<String>)| {
            let refresh_cards = refresh_cards.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = ReviewReq { card_id: id, grade, action };
                if Request::post("/app/review").json(&req).unwrap().send().await.is_ok() {
                    refresh_cards.emit(());
                }
            });
        })
    };

    let on_toggle_pin_cb = {
        let refresh_cards = refresh_cards.clone();
        Callback::from(move |(id, pinned): (i64, bool)| {
            let refresh_cards = refresh_cards.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = PinReq { card_id: id, pinned };
                if Request::post("/admin/tipcards/pin").json(&req).unwrap().send().await.is_ok() {
                    refresh_cards.emit(());
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
                if Request::delete("/admin/tipcards").json(&req).unwrap().send().await.is_ok() {
                    app_state.dispatch(AppAction::ShowToast("Card deleted".to_string()));
                    let state_clone = app_state.clone();
                    gloo_timers::callback::Timeout::new(2400, move || {
                        state_clone.dispatch(AppAction::HideToast);
                    }).forget();
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
            
            if let (Some(from_idx), Some(to_idx)) = (order.iter().position(|&id| id == source_id), order.iter().position(|&id| id == target_id)) {
                let item = order.remove(from_idx);
                order.insert(to_idx, item);
                let _ = LocalStorage::set("denpie_card_order", &order);
                card_order.set(order);
            }
        })
    };

    let mut sorted_cards = (*cards).clone();
    if !card_order.is_empty() {
        sorted_cards.sort_by_key(|c| card_order.iter().position(|&id| id == c.id).unwrap_or(usize::MAX));
    }

    let pinned_cards: Vec<_> = sorted_cards.iter().filter(|c| c.pinned).cloned().collect();
    let unpinned_cards: Vec<_> = sorted_cards.iter().filter(|c| !c.pinned).cloned().collect();

    html! {
        <section id="view-flow">
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
                </form>
            </div>

            <div class="flex justify-between items-center gap-3 mb-4">
                <div class="text-sm text-muted">
                    <span id="flow-count">{cards.len()}</span>{" cards"}
                </div>
            </div>
            
            if !pinned_cards.is_empty() {
                <div id="pinned-flow-section" class="mb-4">
                    <div class="flex items-center gap-2 mb-3 text-sm font-medium text-primary">
                        <iconify-icon icon="radix-icons:drawing-pin-filled" class="radix-icon" aria-hidden="true"></iconify-icon>
                        <span>{"Pinned"}</span>
                    </div>
                    <div id="pinned-flow-grid" class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4 gap-3 items-start">
                        {
                            for pinned_cards.iter().map(|c| {
                                html! {
                                    <FlowCard 
                                        card={c.clone()} 
                                        on_review={on_review_cb.clone()} 
                                        on_toggle_pin={on_toggle_pin_cb.clone()} 
                                        on_delete={on_delete_cb.clone()} 
                                        on_reorder={on_reorder_cb.clone()}
                                    />
                                }
                            })
                        }
                    </div>
                </div>
            }

            <div id="flow-grid" class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4 gap-3">
                {
                    for unpinned_cards.iter().map(|c| {
                        html! {
                            <FlowCard 
                                card={c.clone()} 
                                on_review={on_review_cb.clone()} 
                                on_toggle_pin={on_toggle_pin_cb.clone()} 
                                on_delete={on_delete_cb.clone()} 
                                on_reorder={on_reorder_cb.clone()}
                            />
                        }
                    })
                }
            </div>
            
            if cards.is_empty() {
                <div id="empty-flow" class="surface border rounded-md p-10 text-center text-muted">
                    {"No cards yet."}
                </div>
            }
        </section>
    }
}