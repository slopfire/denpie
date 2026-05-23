use crate::components::unified_flow::TipcardInfo;
use crate::markdown::render_markdown;
use gloo_file::{callbacks::FileReader, File};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{HtmlInputElement, ResizeObserver, ResizeObserverEntry};
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct FlowCardProps {
    pub card: TipcardInfo,
    pub on_review: Callback<(i64, Option<u8>, Option<String>)>,
    pub on_toggle_pin: Callback<(i64, bool)>,
    pub on_delete: Callback<i64>,
    pub on_reorder: Callback<(i64, i64)>,
    pub on_update_images: Callback<(i64, Vec<String>)>,
    pub on_toggle_fullscreen: Callback<i64>,
    #[prop_or_default]
    pub on_request_detail: Callback<i64>,
    #[prop_or_default]
    pub on_measure: Callback<(i64, f64)>,
    pub list_mode: bool,
    pub fullscreen: bool,
    #[prop_or(true)]
    pub detail_loaded: bool,
}

fn highlight_card_code_blocks(root: &web_sys::Element) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(integration) =
        js_sys::Reflect::get(window.as_ref(), &JsValue::from_str("DenpieHighlight"))
    else {
        return;
    };
    if integration.is_null() || integration.is_undefined() {
        return;
    }
    let Ok(highlight_card) =
        js_sys::Reflect::get(&integration, &JsValue::from_str("highlightCard"))
    else {
        return;
    };
    let Ok(highlight_card) = highlight_card.dyn_into::<js_sys::Function>() else {
        return;
    };
    let _ = highlight_card.call1(&JsValue::NULL, root.as_ref());
}

#[function_component(FlowCard)]
pub fn flow_card(props: &FlowCardProps) -> Html {
    let expanded = use_state(|| false);
    let copied = use_state(|| false);
    let image_readers = use_state(Vec::<FileReader>::new);
    let card = &props.card;
    let id = card.id;
    let pinned = card.pinned;
    let root_ref = use_node_ref();

    let toggle_expand = {
        let expanded = expanded.clone();
        let on_request_detail = props.on_request_detail.clone();
        let detail_loaded = props.detail_loaded;
        Callback::from(move |_| {
            if !detail_loaded {
                on_request_detail.emit(id);
            }
            expanded.set(!*expanded)
        })
    };

    let has_compact =
        !card.compressed_content.is_empty() && card.compressed_content != card.full_content;
    let displayed_text = if !*expanded && has_compact && !props.fullscreen {
        &card.compressed_content
    } else {
        &card.full_content
    };

    let html_content = render_markdown(displayed_text);

    let on_review = props.on_review.clone();
    let on_toggle_pin = props.on_toggle_pin.clone();
    let on_delete = props.on_delete.clone();
    let on_reorder = props.on_reorder.clone();
    let on_update_images = props.on_update_images.clone();
    let on_toggle_fullscreen = props.on_toggle_fullscreen.clone();
    let fullscreen = props.fullscreen;
    {
        let root_ref = root_ref.clone();
        let highlight_key = (id, props.fullscreen, *expanded, html_content.clone());
        use_effect_with(highlight_key, move |_| {
            if let Some(element) = root_ref.cast::<web_sys::Element>() {
                highlight_card_code_blocks(&element);
            }
            || ()
        });
    }
    {
        let root_ref = root_ref.clone();
        let on_measure = props.on_measure.clone();
        let measure_key = (
            card.id,
            props.fullscreen,
            props.list_mode,
            *expanded,
            card.full_content.len(),
            card.image_data.len(),
        );
        use_effect_with(measure_key, move |_| {
            if let Some(element) = root_ref.cast::<web_sys::Element>() {
                on_measure.emit((id, element.get_bounding_client_rect().height()));
            }
            || ()
        });
    }
    {
        let root_ref = root_ref.clone();
        let on_measure = props.on_measure.clone();
        use_effect_with(id, move |_| {
            let element = root_ref
                .cast::<web_sys::Element>()
                .expect("card root exists");
            let callback = Closure::<dyn FnMut(js_sys::Array, ResizeObserver)>::wrap(Box::new({
                let on_measure = on_measure.clone();
                move |entries: js_sys::Array, _observer: ResizeObserver| {
                    let Some(entry) = entries.get(0).dyn_into::<ResizeObserverEntry>().ok() else {
                        return;
                    };
                    on_measure.emit((id, entry.content_rect().height()));
                }
            }));
            let observer = ResizeObserver::new(callback.as_ref().unchecked_ref()).ok();
            if let Some(observer) = observer.as_ref() {
                observer.observe(&element);
            }
            move || {
                if let Some(observer) = observer {
                    observer.disconnect();
                }
                drop(callback);
            }
        });
    }

    let ondragstart = {
        let root_ref = root_ref.clone();
        Callback::from(move |e: DragEvent| {
            if fullscreen {
                e.prevent_default();
                return;
            }
            if let Some(dt) = e.data_transfer() {
                let _ = dt.set_data("text/plain", &id.to_string());
                dt.set_drop_effect("move");
                if let Some(element) = root_ref.cast::<web_sys::Element>() {
                    let rect = element.get_bounding_client_rect();
                    let offset_x =
                        ((e.client_x() as f64) - rect.left()).clamp(0.0, rect.width()) as i32;
                    let offset_y =
                        ((e.client_y() as f64) - rect.top()).clamp(0.0, rect.height()) as i32;
                    dt.set_drag_image(&element, offset_x, offset_y);
                }
            }
        })
    };

    let ondragover = Callback::from(|e: DragEvent| {
        e.prevent_default();
        if let Some(dt) = e.data_transfer() {
            dt.set_drop_effect("move");
        }
    });

    let ondrop = {
        Callback::from(move |e: DragEvent| {
            e.prevent_default();
            if let Some(dt) = e.data_transfer() {
                if let Ok(source_id_str) = dt.get_data("text/plain") {
                    if let Ok(source_id) = source_id_str.parse::<i64>() {
                        if source_id != id {
                            on_reorder.emit((source_id, id));
                        }
                    }
                }
            }
        })
    };

    let on_copy = {
        let text = card.full_content.clone();
        let copied = copied.clone();
        Callback::from(move |_| {
            if let Some(window) = web_sys::window() {
                let clipboard = window.navigator().clipboard();
                let _ = clipboard.write_text(&text);
                copied.set(true);
                let copied = copied.clone();
                gloo_timers::callback::Timeout::new(1200, move || copied.set(false)).forget();
            }
        })
    };

    let on_image_upload = {
        let current_images = card.image_data.clone();
        let image_readers = image_readers.clone();
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
            let mut readers = Vec::new();
            let next_images = Rc::new(RefCell::new(current_images.clone()));
            let remaining = Rc::new(Cell::new(files.length()));
            for index in 0..files.length() {
                let Some(file) = files.get(index) else {
                    continue;
                };
                let on_update_images = on_update_images.clone();
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
                            on_update_images.emit((id, next_images.borrow().clone()));
                        }
                    });
                readers.push(reader);
            }
            input.set_value("");
            image_readers.set(readers);
        })
    };

    let line_class = match card.tipcard_type.as_str() {
        "casual_tip" => "card-line-casual",
        "manual_tip" => "card-line-manual",
        "custom_tip" => "card-line-custom",
        _ => "card-line-repeatable",
    };
    let badge_label = if card.tipcard_type == "repeatable_tip" {
        if card.repeat_count > 0 {
            "Known Repeatable".to_string()
        } else {
            "New Repeatable".to_string()
        }
    } else if card.tipcard_type == "manual_tip" {
        "Manual".to_string()
    } else if card.tipcard_type == "custom_tip" {
        "Custom".to_string()
    } else if card.tipcard_type == "casual_tip" {
        "Casual".to_string()
    } else {
        "Repeatable".to_string()
    };

    let article_classes = if fullscreen {
        "flow-card is-fullscreen fullscreen-card-enter surface border fixed top-0 right-0 bottom-0 z-[70] overflow-hidden flex flex-col"
    } else if props.list_mode {
        "flow-card flow-card-list surface border relative overflow-hidden flex flex-col lg:flex-row"
    } else if !*expanded && has_compact {
        "flow-card surface border relative overflow-hidden min-h-[240px] flex flex-col"
    } else {
        "flow-card surface border relative overflow-hidden flex flex-col"
    };
    let content_class = if props.list_mode {
        "text-base leading-7"
    } else {
        "text-base leading-7 flex-1"
    };

    html! {
        <article
            ref={root_ref}
            class={article_classes}
            data-card-id={id.to_string()}
            {ondragover}
            {ondrop}
        >
            <div class={classes!("absolute", "top-0", "left-0", "w-1", "h-full", line_class)}></div>
            <div class="p-4 flex flex-col flex-1">
                <div class="flex justify-between items-start gap-3 border-b border-token pb-3 mb-4">
                    <div class="flex items-center gap-2 font-medium min-w-0">
                        <button type="button" class="card-drag-handle border border-token p-1" title="Drag to reorder" draggable={if fullscreen { "false" } else { "true" }} ondragstart={ondragstart.clone()}>
                            <iconify-icon icon="radix-icons:drag-handle-dots-2" class="radix-icon"></iconify-icon>
                        </button>
                        if pinned {
                            <iconify-icon icon="radix-icons:drawing-pin-filled" class="radix-icon text-primary" title="Pinned"></iconify-icon>
                        }
                        <iconify-icon icon="radix-icons:code" class="radix-icon text-primary"></iconify-icon>
                        <span class="truncate">{&card.topic_name}</span>
                    </div>
                    <div class="card-title-controls flex items-center gap-2 shrink-0">
                        <span class="badge">{badge_label}</span>
                        <button type="button" onclick={Callback::from(move |_| on_toggle_fullscreen.emit(id))} class="border border-token p-2" title={if fullscreen { "Exit fullscreen" } else { "Fullscreen" }}>
                            <iconify-icon icon={if fullscreen { "radix-icons:exit-full-screen" } else { "radix-icons:enter-full-screen" }} class="radix-icon"></iconify-icon>
                        </button>
                    </div>
                </div>

                if !card.image_data.is_empty() {
                    <div class="card-images mb-4">
                        {
                            for card.image_data.iter().enumerate().map(|(index, img)| html! {
                                <img src={img.clone()} alt={format!("Attached image {}", index + 1)} loading="lazy" />
                            })
                        }
                    </div>
                }

                <div class={classes!(content_class, "card-text", if *expanded && has_compact { "is-expanded" } else { "is-compact" })}>
                    <div class="card-text-body markdown-content">
                        { Html::from_html_unchecked(AttrValue::from(html_content)) }
                        if has_compact && !fullscreen {
                            <button onclick={toggle_expand} class="card-inline-expand rounded-md border border-token" title={if *expanded { "Show compact text" } else { "Expand text" }}>
                                <iconify-icon icon={if *expanded { "radix-icons:double-arrow-up" } else { "radix-icons:double-arrow-down" }} class="radix-icon"></iconify-icon>
                            </button>
                        }
                    </div>
                </div>

                <div class="mt-5 pt-4 border-t border-token flex gap-2">
                    if card.status != "active" {
                        <div class="muted-surface border border-token p-2 flex-1 text-center text-sm font-medium text-muted">{&card.status}</div>
                    } else if card.tipcard_type == "casual_tip" {
                        <button onclick={let on_review = on_review.clone(); Callback::from(move |_| on_review.emit((id, Some(3), Some("dismiss".to_string()))))} class="border border-token p-2" title="Dismiss"><iconify-icon icon="radix-icons:cross-2" class="radix-icon"></iconify-icon></button>
                        <button onclick={let on_review = on_review.clone(); Callback::from(move |_| on_review.emit((id, Some(3), Some("acknowledge".to_string()))))} class="bg-primary-solid p-2 flex-1" title="Acknowledge"><iconify-icon icon="radix-icons:check" class="radix-icon"></iconify-icon></button>
                    } else if card.tipcard_type == "repeatable_tip" {
                        <button onclick={let on_review = on_review.clone(); Callback::from(move |_| on_review.emit((id, Some(3), Some("dismiss".to_string()))))} class="border border-token p-2" title="Dismiss"><iconify-icon icon="radix-icons:cross-2" class="radix-icon"></iconify-icon></button>
                        <button onclick={let on_review = on_review.clone(); Callback::from(move |_| on_review.emit((id, Some(3), Some("repeat".to_string()))))} class="border border-token p-2" title="Repeat"><iconify-icon icon="radix-icons:reset" class="radix-icon"></iconify-icon></button>
                        <button onclick={let on_review = on_review.clone(); Callback::from(move |_| on_review.emit((id, Some(5), Some("memorize".to_string()))))} class="bg-primary-solid p-2 flex-1" title="Memorize"><iconify-icon icon="radix-icons:lightning-bolt" class="radix-icon"></iconify-icon></button>
                    } else if card.tipcard_type == "manual_tip" {
                        <button onclick={let on_review = on_review.clone(); Callback::from(move |_| on_review.emit((id, Some(3), Some("dismiss".to_string()))))} class="border border-token p-2" title="Dismiss"><iconify-icon icon="radix-icons:cross-2" class="radix-icon"></iconify-icon></button>
                        <button onclick={let on_review = on_review.clone(); Callback::from(move |_| on_review.emit((id, Some(3), Some("acknowledge".to_string()))))} class="bg-primary-solid p-2 flex-1" title="Acknowledge"><iconify-icon icon="radix-icons:check" class="radix-icon"></iconify-icon></button>
                    } else {
                        <button onclick={let on_review = on_review.clone(); Callback::from(move |_| on_review.emit((id, Some(1), Some(String::new()))))} class="border border-token p-2 flex-1" title="Again">{"Again"}</button>
                        <button onclick={let on_review = on_review.clone(); Callback::from(move |_| on_review.emit((id, Some(3), Some(String::new()))))} class="border border-token p-2 flex-1" title="Good">{"Good"}</button>
                        <button onclick={let on_review = on_review.clone(); Callback::from(move |_| on_review.emit((id, Some(5), Some(String::new()))))} class="bg-primary-solid p-2 flex-1" title="Easy">{"Easy"}</button>
                    }
                    <button onclick={Callback::from(move |_| on_delete.emit(id))} class="border border-token p-2" title="Delete card">
                        <iconify-icon icon="radix-icons:trash" class="radix-icon" style="color:var(--danger)"></iconify-icon>
                    </button>
                    <button onclick={Callback::from(move |_| on_toggle_pin.emit((id, !pinned)))} class={classes!("border", "border-token", "p-2", (pinned).then_some("bg-primary-soft text-primary"))} title={if pinned { "Unpin card" } else { "Pin card" }}>
                        <iconify-icon icon={if pinned { "radix-icons:drawing-pin-filled" } else { "radix-icons:drawing-pin" }} class="radix-icon"></iconify-icon>
                    </button>
                    <label class="border border-token p-2 cursor-pointer" title="Attach images">
                        <iconify-icon icon="radix-icons:image" class="radix-icon"></iconify-icon>
                        <input type="file" accept="image/*" multiple=true class="hidden" onchange={on_image_upload} />
                    </label>
                    if !card.image_data.is_empty() {
                        <button onclick={Callback::from({
                            let on_update_images = props.on_update_images.clone();
                            move |_| on_update_images.emit((id, Vec::new()))
                        })} class="border border-token p-2" title="Clear images">
                            <iconify-icon icon="radix-icons:eye-closed" class="radix-icon"></iconify-icon>
                        </button>
                    }
                    <button onclick={on_copy} data-copy-card-id={id.to_string()} class={classes!("card-copy-btn", "border", "border-token", "p-2", (*copied).then_some("copied"))} title="Copy text">
                        <iconify-icon icon="radix-icons:clipboard-copy" class="radix-icon"></iconify-icon>
                    </button>
                </div>
            </div>
        </article>
    }
}
