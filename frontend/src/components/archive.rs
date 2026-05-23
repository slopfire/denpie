use crate::api::toast;
use crate::components::flow_card::FlowCard;
use crate::components::unified_flow::TipcardInfo;
use crate::state::AppState;
use gloo_net::http::Request;
use gloo_storage::{LocalStorage, Storage};
use serde::Serialize;
use web_sys::{HtmlInputElement, HtmlSelectElement};
use yew::prelude::*;

#[derive(Serialize)]
struct PatchTipcardReq {
    id: i64,
    pinned: Option<bool>,
    image_data: Option<Vec<String>>,
}

#[function_component(Archive)]
pub fn archive() -> Html {
    let app_state = use_context::<UseReducerHandle<AppState>>().unwrap();
    let cards = use_state(Vec::<TipcardInfo>::new);
    let search = use_state(String::new);
    let status = use_state(|| "all".to_string());
    let sort_by = use_state(|| {
        LocalStorage::get::<String>("denpie-archive-sort").unwrap_or_else(|_| "topic".to_string())
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

    let mut filtered: Vec<_> = cards
        .iter()
        .filter(|card| {
            let q = search.to_lowercase();
            let status_ok = *status == "all" || card.status == *status;
            let text_ok = q.is_empty()
                || card.title.to_lowercase().contains(&q)
                || card.topic_name.to_lowercase().contains(&q)
                || card.full_content.to_lowercase().contains(&q);
            status_ok && text_ok
        })
        .cloned()
        .collect();
    match sort_by.as_str() {
        "date" => filtered.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
        }),
        _ => filtered.sort_by(|a, b| {
            a.topic_name
                .to_lowercase()
                .cmp(&b.topic_name.to_lowercase())
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
                .then_with(|| a.id.cmp(&b.id))
        }),
    }
    let visible: Vec<_> = filtered.iter().take(*page * 24).cloned().collect();

    let on_review = Callback::from(|_: (i64, Option<u8>, Option<String>)| {});
    let on_reorder = Callback::from(|_: (i64, i64)| {});
    let on_toggle_fullscreen = {
        let fullscreen_card_id = fullscreen_card_id.clone();
        Callback::from(move |id: i64| {
            if *fullscreen_card_id == Some(id) {
                set_fullscreen_body_class(false);
                fullscreen_card_id.set(None);
            } else {
                set_fullscreen_body_class(true);
                fullscreen_card_id.set(Some(id));
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

    let on_toggle_pin = {
        let refresh_cards = refresh_cards.clone();
        Callback::from(move |(id, pinned): (i64, bool)| {
            let refresh_cards = refresh_cards.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = PatchTipcardReq {
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

    let on_delete = {
        let app_state = app_state.clone();
        let refresh_cards = refresh_cards.clone();
        Callback::from(move |id: i64| {
            let app_state = app_state.clone();
            let refresh_cards = refresh_cards.clone();
            if web_sys::window()
                .and_then(|w| w.confirm_with_message("Delete this card?").ok())
                .unwrap_or(false)
            {
                wasm_bindgen_futures::spawn_local(async move {
                    let req = serde_json::json!({ "id": id });
                    match Request::delete("/admin/tipcards")
                        .json(&req)
                        .unwrap()
                        .send()
                        .await
                    {
                        Ok(res) if res.ok() => {
                            toast(&app_state, "Card deleted");
                            refresh_cards.emit(());
                        }
                        Ok(res) => toast(
                            &app_state,
                            res.text()
                                .await
                                .unwrap_or_else(|_| "Failed to delete card".to_string()),
                        ),
                        Err(err) => toast(&app_state, err.to_string()),
                    }
                });
            }
        })
    };

    let on_update_images = {
        let refresh_cards = refresh_cards.clone();
        let app_state = app_state.clone();
        Callback::from(move |(id, image_data): (i64, Vec<String>)| {
            let refresh_cards = refresh_cards.clone();
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = PatchTipcardReq {
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

    html! {
        <section id="view-archive">
            <div class="flex flex-col lg:flex-row lg:items-end justify-between gap-3 mb-4">
                <div>
                    <h1 class="text-xl font-semibold tracking-tight">{"Archive"}</h1>
                    <p class="text-muted mt-2"><span>{filtered.len()}</span>{" of "}<span>{cards.len()}</span>{" cards"}</p>
                </div>
                <div class="surface border rounded-md p-3 flex flex-col sm:flex-row gap-2 w-full lg:w-auto">
                    <input value={(*search).clone()} oninput={Callback::from({ let search = search.clone(); let page = page.clone(); move |e: InputEvent| if let Some(t) = e.target_dyn_into::<HtmlInputElement>() { search.set(t.value()); page.set(1); }})} class="rounded-md border px-3 py-2 flex-1 min-w-0" placeholder="Find card" />
                    <div class="flex flex-wrap sm:flex-nowrap items-center gap-2">
                        <select value={(*status).clone()} onchange={Callback::from({ let status = status.clone(); let page = page.clone(); move |e: Event| if let Some(t) = e.target_dyn_into::<HtmlSelectElement>() { status.set(t.value()); page.set(1); }})} class="rounded-md border px-3 py-2">
                            <option value="all">{"All"}</option>
                            <option value="active">{"Active"}</option>
                            <option value="completed">{"Completed"}</option>
                            <option value="custom">{"Custom"}</option>
                        </select>
                        <div class="flex muted-surface rounded-md p-1 border border-token" role="group" aria-label="Sort cards">
                            <button
                                type="button"
                                class={classes!("rounded", "px-2", "py-1", "text-sm", "font-medium", (*sort_by == "topic").then_some("bg-primary-soft text-primary"))}
                                aria-pressed={(*sort_by == "topic").to_string()}
                                onclick={Callback::from({
                                    let sort_by = sort_by.clone();
                                    let page = page.clone();
                                    move |_| {
                                        let _ = LocalStorage::set("denpie-archive-sort", "topic");
                                        sort_by.set("topic".to_string());
                                        page.set(1);
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
                                    let page = page.clone();
                                    move |_| {
                                        let _ = LocalStorage::set("denpie-archive-sort", "date");
                                        sort_by.set("date".to_string());
                                        page.set(1);
                                    }
                                })}
                            >
                                {"Date"}
                            </button>
                        </div>
                        <button type="button" class="rounded-md border border-token px-3 py-2" onclick={Callback::from({ let search = search.clone(); let status = status.clone(); let page = page.clone(); move |_| { search.set(String::new()); status.set("all".to_string()); page.set(1); } })}>{"Clear"}</button>
                    </div>
                </div>
            </div>

            <div class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4 gap-3">
                {
                    if visible.is_empty() {
                        html! { <div class="col-span-full surface border rounded-md p-10 text-center text-muted">{"No cards found."}</div> }
                    } else {
                        html! {
                            for visible.iter().map(|card| html! {
                                <FlowCard
                                    card={card.clone()}
                                    on_review={on_review.clone()}
                                    on_toggle_pin={on_toggle_pin.clone()}
                                    on_delete={on_delete.clone()}
                                    on_reorder={on_reorder.clone()}
                                    on_update_images={on_update_images.clone()}
                                    on_toggle_fullscreen={on_toggle_fullscreen.clone()}
                                    list_mode={false}
                                    fullscreen={*fullscreen_card_id == Some(card.id)}
                                />
                            })
                        }
                    }
                }
            </div>
            if filtered.len() > visible.len() {
                <div class="flex justify-center py-8">
                    <button type="button" class="rounded-md border border-token px-6 py-2 font-medium" onclick={Callback::from({ let page = page.clone(); move |_| page.set(*page + 1) })}>{"Load more"}</button>
                </div>
            }
        </section>
    }
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
