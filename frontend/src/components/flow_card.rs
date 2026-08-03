use crate::components::button::{ButtonSize, ButtonVariant, ShadcnButton};
use crate::components::card_image_picker::CardImagePicker;
use crate::components::image_lightbox::ImageLightbox;
use crate::components::tooltip::ShadcnTooltip;
use crate::components::unified_flow::TipcardInfo;
use crate::markdown::render_markdown;
use crate::topic_visual::display_icon;
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::{HtmlElement, ResizeObserver, ResizeObserverEntry};
use yew::prelude::*;

fn repeatable_stack_layers(tipcard_type: &str, pending_count: u32, fullscreen: bool) -> usize {
    if tipcard_type == "repeatable_tip" && !fullscreen {
        pending_count.min(3) as usize
    } else {
        0
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ReviewSway {
    Left,
    Center,
    Right,
}

impl ReviewSway {
    fn class_name(self) -> &'static str {
        match self {
            Self::Left => "leaves-left",
            Self::Center => "leaves-center",
            Self::Right => "leaves-right",
        }
    }
}

#[derive(Properties, PartialEq)]
pub struct FlowCardProps {
    pub card: TipcardInfo,
    pub on_review: Callback<(i64, Option<u8>, Option<String>)>,
    #[prop_or_default]
    pub on_learn_more: Callback<(String, String)>,
    pub on_toggle_pin: Callback<(i64, bool)>,
    pub on_delete: Callback<i64>,
    pub on_update_images: Callback<(i64, Vec<String>)>,
    #[prop_or_default]
    pub on_upload_error: Callback<String>,
    #[prop_or_default]
    pub on_images_attached: Callback<i64>,
    pub on_toggle_fullscreen: Callback<i64>,
    #[prop_or_default]
    pub on_request_detail: Callback<i64>,
    #[prop_or_default]
    pub on_measure: Callback<(i64, f64)>,
    pub list_mode: bool,
    pub fullscreen: bool,
    #[prop_or(true)]
    pub detail_loaded: bool,
    #[prop_or(true)]
    pub enable_measure: bool,
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

fn human_datetime(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return String::new();
    }

    let normalized = if value.len() == 19 && value.as_bytes().get(10) == Some(&b' ') {
        format!("{}T{}Z", &value[..10], &value[11..])
    } else {
        value.to_string()
    };
    let date = js_sys::Date::new(&JsValue::from_str(&normalized));
    if date.get_time().is_nan() {
        return value.to_string();
    }

    let delta_seconds = ((date.get_time() - js_sys::Date::now()) / 1_000.0).round() as i64;
    let absolute_seconds = delta_seconds.unsigned_abs();
    let (amount, unit) = if absolute_seconds < 60 {
        return "just now".to_string();
    } else if absolute_seconds < 3_600 {
        ((absolute_seconds / 60).max(1), "minute")
    } else if absolute_seconds < 86_400 {
        ((absolute_seconds / 3_600).max(1), "hour")
    } else if absolute_seconds < 604_800 {
        ((absolute_seconds / 86_400).max(1), "day")
    } else {
        return date
            .to_locale_date_string("en-US", &JsValue::UNDEFINED)
            .as_string()
            .unwrap_or_else(|| value.to_string());
    };
    let unit = if amount == 1 {
        unit.to_string()
    } else {
        format!("{unit}s")
    };
    if delta_seconds < 0 {
        format!("{amount} {unit} ago")
    } else {
        format!("in {amount} {unit}")
    }
}

#[function_component(FlowCard)]
pub fn flow_card(props: &FlowCardProps) -> Html {
    let expanded = use_state(|| false);
    let copied = use_state(|| false);
    let lightbox_index = use_state(|| None::<usize>);
    let image_picker_open = use_state(|| false);
    let skip_open = use_state(|| false);
    let more_open = use_state(|| false);
    let metadata_open = use_state(|| false);
    let leaving = use_state(|| None::<ReviewSway>);
    let grid_min_height = use_state(|| None::<f64>);
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

    let has_compact = card.review_message.is_none()
        && !card.compressed_content.is_empty()
        && card.compressed_content != card.full_content;
    let displayed_text = if let Some(message) = card.review_message.as_deref() {
        message
    } else if !*expanded && has_compact && !props.fullscreen {
        &card.compressed_content
    } else {
        &card.full_content
    };

    let html_content = render_markdown(displayed_text);

    let on_review = props.on_review.clone();
    let on_learn_more = props.on_learn_more.clone();
    let on_toggle_pin = props.on_toggle_pin.clone();
    let on_delete = props.on_delete.clone();
    let on_toggle_fullscreen = props.on_toggle_fullscreen.clone();
    let fullscreen = props.fullscreen;
    let lock_grid_height = card.tipcard_type == "repeatable_tip" && !fullscreen;
    let review_with_animation: Callback<(Option<u8>, Option<String>, ReviewSway)> = {
        let on_review = on_review.clone();
        let leaving = leaving.clone();
        let root_ref = root_ref.clone();
        let grid_min_height = grid_min_height.clone();
        Callback::from(move |(grade, action, sway)| {
            if leaving.is_some() {
                return;
            }
            if lock_grid_height {
                if let Some(element) = root_ref.cast::<web_sys::Element>() {
                    let height = element.get_bounding_client_rect().height();
                    if height.is_finite() && height > 0.0 {
                        grid_min_height.set(Some(height));
                    }
                }
            }
            leaving.set(Some(sway));
            let on_review = on_review.clone();
            let leaving = leaving.clone();
            gloo_timers::callback::Timeout::new(200, move || {
                on_review.emit((id, grade, action));
                // Keep the reviewed card out of view while the request is in flight. A
                // status/id change clears this sooner; this is only the failure fallback.
                gloo_timers::callback::Timeout::new(1_500, move || leaving.set(None)).forget();
            })
            .forget();
        })
    };
    {
        let leaving = leaving.clone();
        use_effect_with((id, card.status.clone()), move |_| {
            leaving.set(None);
            || ()
        });
    }
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
        let enable_measure = props.enable_measure;
        let measure_key = (
            card.id,
            props.fullscreen,
            props.list_mode,
            *expanded,
            card.full_content.len(),
            card.image_data.len(),
            enable_measure,
        );
        use_effect_with(measure_key, move |_| {
            if enable_measure {
                if let Some(element) = root_ref.cast::<web_sys::Element>() {
                    on_measure.emit((id, element.get_bounding_client_rect().height()));
                }
            }
            || ()
        });
    }
    {
        let root_ref = root_ref.clone();
        let on_measure = props.on_measure.clone();
        let enable_measure = props.enable_measure;
        use_effect_with((id, enable_measure), move |_| {
            let observer = if enable_measure {
                root_ref.cast::<web_sys::Element>().and_then(|element| {
                    let callback =
                        Closure::<dyn FnMut(js_sys::Array, ResizeObserver)>::wrap(Box::new({
                            let on_measure = on_measure.clone();
                            move |entries: js_sys::Array, _observer: ResizeObserver| {
                                let Some(entry) =
                                    entries.get(0).dyn_into::<ResizeObserverEntry>().ok()
                                else {
                                    return;
                                };
                                on_measure.emit((id, entry.content_rect().height()));
                            }
                        }));
                    ResizeObserver::new(callback.as_ref().unchecked_ref())
                        .ok()
                        .map(|observer| {
                            observer.observe(&element);
                            (observer, callback)
                        })
                })
            } else {
                None
            };
            move || {
                if let Some((observer, callback)) = observer {
                    observer.disconnect();
                    drop(callback);
                }
            }
        });
    }

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

    let is_known = card.repeat_count > 0 || card.status != "active";
    let badge_label = match card.tipcard_type.as_str() {
        "casual_tip" | "repeatable_tip" => {
            if is_known {
                "Known"
            } else {
                "New"
            }
        }
        "manual_tip" => "Manual",
        "custom_tip" => "Custom",
        _ => {
            if is_known {
                "Known"
            } else {
                "New"
            }
        }
    };
    let type_label = match card.tipcard_type.as_str() {
        "casual_tip" => "Casual",
        "repeatable_tip" => "Repeatable",
        "manual_tip" => "Manual",
        "custom_tip" => "Custom",
        _ => "Card",
    };
    let metadata_id = format!("card-metadata-{id}");
    let created_label = human_datetime(&card.created_at);
    let next_review_label = human_datetime(&card.next_review_at);

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
    let topic_icon = display_icon(&card.topic_icon).to_string();
    let topic_color_style = format!("color: {}", card.topic_color);

    let stack_layers = repeatable_stack_layers(&card.tipcard_type, card.pending_count, fullscreen);
    let card_style = if card.tipcard_type == "repeatable_tip" && !fullscreen {
        (*grid_min_height).map(|height| format!("min-height: {height:.1}px;"))
    } else {
        None
    };

    html! {
        <article
            ref={root_ref}
            class={classes!(
                article_classes,
                (stack_layers > 0).then_some("repeatable-card-stack"),
                (leaving.is_some() && !fullscreen).then_some("is-leaving"),
                (leaving.is_some() && fullscreen).then_some("is-reviewing-fullscreen"),
                (fullscreen && card.tipcard_type == "repeatable_tip").then_some("repeatable-fullscreen"),
                (*leaving).map(ReviewSway::class_name),
            )}
            data-card-id={id.to_string()}
            style={card_style}
        >
            {
                for (1..=stack_layers).map(|layer| html! {
                    <div
                        class={classes!("repeatable-card-stack-back", format!("repeatable-card-stack-back-{layer}"))}
                        data-stack-layer={layer.to_string()}
                        aria-hidden="true"
                    ></div>
                })
            }
            <div class="flow-card-front p-4 flex flex-col flex-1">
                <div class="card-title-bar border-b border-token pb-3 mb-4">
                    <div class="card-title-leading flex items-center justify-self-start">
                        <span class="border border-token p-1 inline-flex">
                            <iconify-icon icon={topic_icon} class="topic-icon radix-icon shrink-0" style={topic_color_style}></iconify-icon>
                        </span>
                    </div>
                    <div class="card-title-center flex items-center justify-center gap-1.5 min-w-0 px-1">
                        if pinned {
                            <ShadcnTooltip content="Pinned">
                                <span class="inline-flex shrink-0">
                                    <iconify-icon icon="radix-icons:drawing-pin-filled" class="radix-icon text-primary shrink-0"></iconify-icon>
                                </span>
                            </ShadcnTooltip>
                        }
                        <span class="card-topic-title truncate text-center">{&card.topic_name}</span>
                    </div>
                    <div class="card-title-controls flex items-center gap-2 justify-self-end shrink-0">
                        <div class="relative">
                            <button
                                type="button"
                                class="badge h-5 cursor-pointer px-2 py-0 text-xs leading-none transition-colors hover:bg-secondary hover:text-secondary-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                                aria-expanded={metadata_open.to_string()}
                                aria-controls={metadata_id.clone()}
                                title="Show card information"
                                onclick={Callback::from({
                                    let metadata_open = metadata_open.clone();
                                    move |_| metadata_open.set(!*metadata_open)
                                })}
                            >
                                {badge_label}
                            </button>
                            if *metadata_open {
                                <dl
                                    id={metadata_id.clone()}
                                    aria-label="Card information"
                                    class="shadcn-dropdown-menu opens-down grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1.5 text-xs"
                                    style="width: 15rem; min-width: 15rem; padding: 0.625rem;"
                                >
                                    <dt class="text-muted">{"Type"}</dt>
                                    <dd class="font-medium text-right">{type_label}</dd>
                                    <dt class="text-muted">{"Created"}</dt>
                                    <dd class="font-medium text-right break-words">
                                        {if created_label.is_empty() { "Unknown" } else { created_label.as_str() }}
                                    </dd>
                                    <dt class="text-muted">{"Scheduled repeat"}</dt>
                                    <dd class="font-medium text-right break-words">
                                        {if next_review_label.is_empty() { "Not scheduled" } else { next_review_label.as_str() }}
                                    </dd>
                                    <dt class="text-muted">{"Reviews"}</dt>
                                    <dd class="font-medium text-right">{card.repeat_count}</dd>
                                    <dt class="text-muted">{"State"}</dt>
                                    <dd class="font-medium text-right">{if is_known { "Known" } else { "New" }}</dd>
                                </dl>
                            }
                        </div>
                        <ShadcnTooltip content={if fullscreen { "Exit fullscreen" } else { "Fullscreen" }}>
                            <button type="button" onclick={Callback::from(move |_| on_toggle_fullscreen.emit(id))} class="border border-token p-2">
                                <iconify-icon icon={if fullscreen { "radix-icons:exit-full-screen" } else { "radix-icons:enter-full-screen" }} class="radix-icon"></iconify-icon>
                            </button>
                        </ShadcnTooltip>
                    </div>
                </div>

                if !card.image_data.is_empty() {
                    <div class="card-images mb-4">
                        {
                            for card.image_data.iter().enumerate().map(|(index, img)| {
                                let lightbox_index = lightbox_index.clone();
                                html! {
                                    <button
                                        type="button"
                                        class="card-image-trigger"
                                        aria-label={format!("View image {} of {}", index + 1, card.image_data.len())}
                                        onclick={Callback::from(move |e: MouseEvent| {
                                            e.stop_propagation();
                                            lightbox_index.set(Some(index));
                                        })}
                                    >
                                        <img src={img.clone()} alt="" loading="lazy"
                                            onload={Callback::from(|e: Event| {
                                                if let Some(el) = e.target().and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok()) {
                                                    let _ = el.class_list().add_1("is-loaded");
                                                }
                                            })}
                                            onerror={Callback::from(|e: Event| {
                                                if let Some(el) = e.target().and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok()) {
                                                    let _ = el.class_list().add_1("is-loaded");
                                                }
                                            })}
                                        />
                                    </button>
                                }
                            })
                        }
                    </div>
                }

                <div class={classes!(content_class, "card-text", card.review_message.is_some().then_some("muted-surface border border-token p-4"), if *expanded && has_compact { "is-expanded" } else { "is-compact" })}>
                    <div class="card-text-body markdown-content">
                        { Html::from_html_unchecked(AttrValue::from(html_content)) }
                        if has_compact && !fullscreen {
                            <ShadcnTooltip content={if *expanded { "Show compact text" } else { "Expand text" }}>
                                <button onclick={toggle_expand} class="card-inline-expand rounded-md border border-token">
                                    <iconify-icon icon={if *expanded { "radix-icons:double-arrow-up" } else { "radix-icons:double-arrow-down" }} class="radix-icon"></iconify-icon>
                                </button>
                            </ShadcnTooltip>
                        }
                    </div>
                </div>

                <div class="card-actions mt-5 pt-4 border-t border-token flex items-center gap-2">
                    if card.status != "active" {
                        <div class="muted-surface border border-token p-2 flex-1 text-center text-sm font-medium text-muted">{"Review saved"}</div>
                        if card.tipcard_type == "repeatable_tip" && card.review_message.is_some() {
                            <ShadcnButton
                                variant={ButtonVariant::Default}
                                onclick={Callback::from({
                                    let topic_name = card.topic_name.clone();
                                    let tipcard_type = card.tipcard_type.clone();
                                    move |_| on_learn_more.emit((topic_name.clone(), tipcard_type.clone()))
                                })}
                            >{"Learn more"}</ShadcnButton>
                        }
                    } else if card.tipcard_type == "casual_tip" || card.tipcard_type == "manual_tip" {
                        <ShadcnTooltip content="Dismiss" class={classes!("flex-1")}>
                            <ShadcnButton
                                variant={ButtonVariant::Outline}
                                class={classes!("w-full")}
                                onclick={let review = review_with_animation.clone(); Callback::from(move |_| review.emit((Some(3), Some("dismiss".to_string()), ReviewSway::Left)))}
                            >
                                <iconify-icon icon="radix-icons:cross-2" class="radix-icon" aria-hidden="true"></iconify-icon>
                            </ShadcnButton>
                        </ShadcnTooltip>
                        <ShadcnTooltip content="Acknowledge" class={classes!("flex-1")}>
                            <ShadcnButton
                                variant={ButtonVariant::Default}
                                class={classes!("w-full")}
                                onclick={let review = review_with_animation.clone(); Callback::from(move |_| review.emit((Some(3), Some("acknowledge".to_string()), ReviewSway::Right)))}
                            >
                                <iconify-icon icon="radix-icons:check" class="radix-icon" aria-hidden="true"></iconify-icon>
                            </ShadcnButton>
                        </ShadcnTooltip>
                    } else if card.tipcard_type == "repeatable_tip" {
                        <ShadcnTooltip content="Show this card again sooner" class={classes!("flex-1")}>
                            <ShadcnButton
                                variant={ButtonVariant::Outline}
                                class={classes!("w-full")}
                                onclick={let review = review_with_animation.clone(); Callback::from(move |_| review.emit((Some(1), Some("again".to_string()), ReviewSway::Left)))}
                            >{"Again"}</ShadcnButton>
                        </ShadcnTooltip>
                        <ShadcnTooltip content="I learned this" class={classes!("flex-1")}>
                            <ShadcnButton
                                variant={ButtonVariant::Default}
                                class={classes!("w-full")}
                                onclick={let review = review_with_animation.clone(); Callback::from(move |_| review.emit((Some(5), Some("learned".to_string()), ReviewSway::Right)))}
                            >{"Learned"}</ShadcnButton>
                        </ShadcnTooltip>
                        <div
                            class="shadcn-dropdown shrink-0"
                            onmouseenter={Callback::from({
                                let skip_open = skip_open.clone();
                                let more_open = more_open.clone();
                                move |_| {
                                    more_open.set(false);
                                    skip_open.set(true);
                                }
                            })}
                            onmouseleave={Callback::from({
                                let skip_open = skip_open.clone();
                                move |_| skip_open.set(false)
                            })}
                            onfocusin={Callback::from({
                                let skip_open = skip_open.clone();
                                let more_open = more_open.clone();
                                move |_| {
                                    more_open.set(false);
                                    skip_open.set(true);
                                }
                            })}
                            onfocusout={Callback::from({
                                let skip_open = skip_open.clone();
                                move |e: FocusEvent| {
                                    if let Some(related) = e.related_target() {
                                        if let Ok(node) = related.dyn_into::<web_sys::Node>() {
                                            if let Some(current) = e.current_target().and_then(|t| t.dyn_into::<HtmlElement>().ok()) {
                                                if current.contains(Some(&node)) {
                                                    return;
                                                }
                                            }
                                        }
                                    }
                                    skip_open.set(false);
                                }
                            })}
                            onkeydown={Callback::from({
                                let skip_open = skip_open.clone();
                                move |event: KeyboardEvent| {
                                    if event.key() == "Escape" {
                                        event.prevent_default();
                                        skip_open.set(false);
                                    }
                                }
                            })}
                        >
                            <ShadcnTooltip content="Skip">
                                <ShadcnButton
                                    variant={ButtonVariant::Outline}
                                    size={ButtonSize::Icon}
                                    class={classes!(
                                        "skip-trigger",
                                        (*skip_open).then_some("is-open bg-accent text-accent-foreground")
                                    )}
                                >
                                    <iconify-icon icon="radix-icons:chevron-up" class="radix-icon skip-trigger-icon" aria-hidden="true"></iconify-icon>
                                </ShadcnButton>
                            </ShadcnTooltip>
                            if *skip_open {
                                <div role="menu" aria-label="Skip reasons" class="shadcn-dropdown-menu">
                                    <button
                                        type="button"
                                        role="menuitem"
                                        class="shadcn-dropdown-item"
                                        onclick={Callback::from({
                                            let review = review_with_animation.clone();
                                            let skip_open = skip_open.clone();
                                            move |_| {
                                                skip_open.set(false);
                                                review.emit((Some(5), Some("skip_known".to_string()), ReviewSway::Right));
                                            }
                                        })}
                                    >
                                        <iconify-icon icon="radix-icons:check-circled" class="radix-icon" aria-hidden="true"></iconify-icon>
                                        <span class="shadcn-dropdown-item-copy">
                                            <span class="shadcn-dropdown-item-title">{"Known"}</span>
                                            <span class="shadcn-dropdown-item-desc">{"Already know this"}</span>
                                        </span>
                                    </button>
                                    <button
                                        type="button"
                                        role="menuitem"
                                        class="shadcn-dropdown-item"
                                        onclick={Callback::from({
                                            let review = review_with_animation.clone();
                                            let skip_open = skip_open.clone();
                                            move |_| {
                                                skip_open.set(false);
                                                review.emit((Some(3), Some("skip_not_interested".to_string()), ReviewSway::Right));
                                            }
                                        })}
                                    >
                                        <iconify-icon icon="radix-icons:cross-circled" class="radix-icon" aria-hidden="true"></iconify-icon>
                                        <span class="shadcn-dropdown-item-copy">
                                            <span class="shadcn-dropdown-item-title">{"Not interested"}</span>
                                            <span class="shadcn-dropdown-item-desc">{"Change direction"}</span>
                                        </span>
                                    </button>
                                    <button
                                        type="button"
                                        role="menuitem"
                                        class="shadcn-dropdown-item"
                                        onclick={Callback::from({
                                            let review = review_with_animation.clone();
                                            let skip_open = skip_open.clone();
                                            move |_| {
                                                skip_open.set(false);
                                                review.emit((Some(1), Some("skip_too_difficult".to_string()), ReviewSway::Right));
                                            }
                                        })}
                                    >
                                        <iconify-icon icon="radix-icons:exclamation-triangle" class="radix-icon" aria-hidden="true"></iconify-icon>
                                        <span class="shadcn-dropdown-item-copy">
                                            <span class="shadcn-dropdown-item-title">{"Too difficult"}</span>
                                            <span class="shadcn-dropdown-item-desc">{"Prefer an easier step"}</span>
                                        </span>
                                    </button>
                                </div>
                            }
                        </div>
                    } else {
                        <ShadcnTooltip content="Again" class={classes!("flex-1")}>
                            <ShadcnButton
                                variant={ButtonVariant::Outline}
                                class={classes!("w-full")}
                                onclick={let review = review_with_animation.clone(); Callback::from(move |_| review.emit((Some(1), Some(String::new()), ReviewSway::Left)))}
                            >{"Again"}</ShadcnButton>
                        </ShadcnTooltip>
                        <ShadcnTooltip content="Good" class={classes!("flex-1")}>
                            <ShadcnButton
                                variant={ButtonVariant::Outline}
                                class={classes!("w-full")}
                                onclick={let review = review_with_animation.clone(); Callback::from(move |_| review.emit((Some(3), Some(String::new()), ReviewSway::Center)))}
                            >{"Good"}</ShadcnButton>
                        </ShadcnTooltip>
                        <ShadcnTooltip content="Easy" class={classes!("flex-1")}>
                            <ShadcnButton
                                variant={ButtonVariant::Default}
                                class={classes!("w-full")}
                                onclick={let review = review_with_animation.clone(); Callback::from(move |_| review.emit((Some(5), Some(String::new()), ReviewSway::Right)))}
                            >{"Easy"}</ShadcnButton>
                        </ShadcnTooltip>
                    }
                    <ShadcnTooltip content={if pinned { "Unpin card" } else { "Pin card" }}>
                        <ShadcnButton
                            variant={ButtonVariant::Outline}
                            size={ButtonSize::Icon}
                            class={classes!(pinned.then_some("bg-primary-soft text-primary"))}
                            onclick={Callback::from(move |_| on_toggle_pin.emit((id, !pinned)))}
                        >
                            <iconify-icon icon={if pinned { "radix-icons:drawing-pin-filled" } else { "radix-icons:drawing-pin" }} class="radix-icon" aria-hidden="true"></iconify-icon>
                        </ShadcnButton>
                    </ShadcnTooltip>
                    <div
                        class="shadcn-dropdown shrink-0"
                        onmouseenter={Callback::from({
                            let more_open = more_open.clone();
                            let skip_open = skip_open.clone();
                            move |_| {
                                skip_open.set(false);
                                more_open.set(true);
                            }
                        })}
                        onmouseleave={Callback::from({
                            let more_open = more_open.clone();
                            move |_| more_open.set(false)
                        })}
                        onfocusin={Callback::from({
                            let more_open = more_open.clone();
                            let skip_open = skip_open.clone();
                            move |_| {
                                skip_open.set(false);
                                more_open.set(true);
                            }
                        })}
                        onfocusout={Callback::from({
                            let more_open = more_open.clone();
                            move |e: FocusEvent| {
                                if let Some(related) = e.related_target() {
                                    if let Ok(node) = related.dyn_into::<web_sys::Node>() {
                                        if let Some(current) = e.current_target().and_then(|t| t.dyn_into::<HtmlElement>().ok()) {
                                            if current.contains(Some(&node)) {
                                                return;
                                            }
                                        }
                                    }
                                }
                                more_open.set(false);
                            }
                        })}
                        onkeydown={Callback::from({
                            let more_open = more_open.clone();
                            move |event: KeyboardEvent| {
                                if event.key() == "Escape" {
                                    event.prevent_default();
                                    more_open.set(false);
                                }
                            }
                        })}
                    >
                        <ShadcnButton
                            variant={ButtonVariant::Outline}
                            size={ButtonSize::Icon}
                            class={classes!((*more_open).then_some("bg-accent text-accent-foreground"))}
                        >
                            <iconify-icon icon="radix-icons:dots-horizontal" class="radix-icon" aria-hidden="true"></iconify-icon>
                        </ShadcnButton>
                        if *more_open {
                            <div role="menu" aria-label="Card actions" class="shadcn-dropdown-menu">
                                <button
                                    type="button"
                                    role="menuitem"
                                    class="shadcn-dropdown-item"
                                    onclick={Callback::from({
                                        let image_picker_open = image_picker_open.clone();
                                        let more_open = more_open.clone();
                                        move |_| {
                                            more_open.set(false);
                                            image_picker_open.set(true);
                                        }
                                    })}
                                >
                                    <iconify-icon icon="radix-icons:image" class="radix-icon" aria-hidden="true"></iconify-icon>
                                    <span>{"Attach images"}</span>
                                </button>
                                if !card.image_data.is_empty() {
                                    <button
                                        type="button"
                                        role="menuitem"
                                        class="shadcn-dropdown-item"
                                        onclick={Callback::from({
                                            let on_update_images = props.on_update_images.clone();
                                            let more_open = more_open.clone();
                                            move |_| {
                                                more_open.set(false);
                                                on_update_images.emit((id, Vec::new()));
                                            }
                                        })}
                                    >
                                        <iconify-icon icon="radix-icons:eye-closed" class="radix-icon" aria-hidden="true"></iconify-icon>
                                        <span>{"Clear images"}</span>
                                    </button>
                                }
                                <button
                                    type="button"
                                    role="menuitem"
                                    class={classes!("shadcn-dropdown-item", "card-copy-btn", (*copied).then_some("copied"))}
                                    data-copy-card-id={id.to_string()}
                                    onclick={Callback::from({
                                        let on_copy = on_copy.clone();
                                        let more_open = more_open.clone();
                                        move |e: MouseEvent| {
                                            more_open.set(false);
                                            on_copy.emit(e);
                                        }
                                    })}
                                >
                                    <iconify-icon icon="radix-icons:clipboard-copy" class="radix-icon" aria-hidden="true"></iconify-icon>
                                    <span>{if *copied { "Copied" } else { "Copy text" }}</span>
                                </button>
                                <div class="shadcn-dropdown-separator" role="separator"></div>
                                <button
                                    type="button"
                                    role="menuitem"
                                    class="shadcn-dropdown-item shadcn-dropdown-item--danger"
                                    onclick={Callback::from({
                                        let on_delete = on_delete.clone();
                                        let more_open = more_open.clone();
                                        move |_| {
                                            more_open.set(false);
                                            on_delete.emit(id);
                                        }
                                    })}
                                >
                                    <iconify-icon icon="radix-icons:trash" class="radix-icon" aria-hidden="true"></iconify-icon>
                                    <span>{"Delete card"}</span>
                                </button>
                            </div>
                        }
                    </div>
                </div>
            </div>
            if let Some(index) = *lightbox_index {
                <ImageLightbox
                    images={card.image_data.clone()}
                    initial_index={index}
                    on_close={Callback::from({
                        let lightbox_index = lightbox_index.clone();
                        move |_| lightbox_index.set(None)
                    })}
                />
            }
            <CardImagePicker
                open={*image_picker_open}
                card_id={id}
                existing_count={card.image_data.len()}
                context={format!("{} {}", card.topic_name, card.title)}
                on_close={Callback::from({
                    let image_picker_open = image_picker_open.clone();
                    move |_| image_picker_open.set(false)
                })}
                on_success={Callback::from({
                    let image_picker_open = image_picker_open.clone();
                    let on_request_detail = props.on_request_detail.clone();
                    let on_images_attached = props.on_images_attached.clone();
                    move |_| {
                        image_picker_open.set(false);
                        on_request_detail.emit(id);
                        on_images_attached.emit(id);
                    }
                })}
                on_error={props.on_upload_error.clone()}
            />
        </article>
    }
}

#[derive(Properties, PartialEq)]
pub struct FlowCardSkeletonProps {
    pub list_mode: bool,
}

#[function_component(FlowCardSkeleton)]
pub fn flow_card_skeleton(props: &FlowCardSkeletonProps) -> Html {
    let article_classes = if props.list_mode {
        "flow-card flow-card-list surface border relative overflow-hidden flex flex-col lg:flex-row"
    } else {
        "flow-card surface border relative overflow-hidden min-h-[240px] flex flex-col"
    };

    html! {
        <article class={article_classes} aria-busy="true" aria-label="Generating card">
            <div class="p-4 flex flex-col flex-1">
                <div class="flex justify-between items-start gap-3 border-b border-token pb-3 mb-4">
                    <div class="flex items-center gap-2 min-w-0 flex-1">
                        <div class="skeleton-block shrink-0" style="width: 22px; height: 22px; border-radius: 6px"></div>
                        <div class="skeleton-block" style="height: 14px; width: 40%"></div>
                    </div>
                    <div class="skeleton-block shrink-0" style="height: 18px; width: 56px"></div>
                </div>

                <div class="skeleton-block mb-3" style="height: 18px; width: 70%"></div>
                <div class="flex-1 space-y-2">
                    <div class="skeleton-block" style="height: 12px; width: 100%"></div>
                    <div class="skeleton-block" style="height: 12px; width: 92%"></div>
                    <div class="skeleton-block" style="height: 12px; width: 78%"></div>
                </div>

                <div class="mt-5 pt-4 border-t border-token flex items-center gap-2">
                    <div class="skeleton-block flex-1" style="height: 40px"></div>
                    <div class="skeleton-block flex-1" style="height: 40px"></div>
                    <div class="skeleton-block flex-1" style="height: 40px"></div>
                    <div class="skeleton-block shrink-0" style="height: 40px; width: 40px"></div>
                </div>
            </div>
        </article>
    }
}

#[cfg(test)]
mod tests {
    use super::repeatable_stack_layers;

    #[test]
    fn repeatable_stack_matches_pending_card_count() {
        assert_eq!(repeatable_stack_layers("repeatable_tip", 0, false), 0);
        assert_eq!(repeatable_stack_layers("repeatable_tip", 1, false), 1);
        assert_eq!(repeatable_stack_layers("repeatable_tip", 2, false), 2);
        assert_eq!(repeatable_stack_layers("repeatable_tip", 8, false), 3);
    }

    #[test]
    fn repeatable_stack_is_hidden_for_fullscreen_and_other_types() {
        assert_eq!(repeatable_stack_layers("repeatable_tip", 3, true), 0);
        assert_eq!(repeatable_stack_layers("casual_tip", 3, false), 0);
    }
}
