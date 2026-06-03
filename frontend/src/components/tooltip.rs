use std::cell::Cell;

use gloo_timers::callback::Timeout;
use wasm_bindgen::JsCast;
use web_sys::HtmlElement;
use yew::create_portal;
use yew::prelude::*;

thread_local! {
    static TOOLTIP_ID: Cell<usize> = const { Cell::new(0) };
}

fn next_tooltip_id() -> String {
    TOOLTIP_ID.with(|counter| {
        let id = counter.get();
        counter.set(id + 1);
        format!("shadcn-tooltip-{id}")
    })
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum TooltipSide {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

impl TooltipSide {
    fn data_side(self) -> &'static str {
        match self {
            TooltipSide::Top => "top",
            TooltipSide::Bottom => "bottom",
            TooltipSide::Left => "left",
            TooltipSide::Right => "right",
        }
    }
}

#[derive(Properties, PartialEq)]
pub struct ShadcnTooltipProps {
    pub content: AttrValue,
    #[prop_or_default]
    pub side: TooltipSide,
    #[prop_or(200)]
    pub delay_ms: u32,
    #[prop_or_default]
    pub class: Classes,
    pub children: Children,
}

const TOOLTIP_OFFSET_PX: f64 = 6.0;
const VIEWPORT_PADDING_PX: f64 = 8.0;

fn pick_side(
    trigger: &web_sys::DomRect,
    content: &web_sys::DomRect,
    preferred: TooltipSide,
) -> TooltipSide {
    let window = match web_sys::window() {
        Some(window) => window,
        None => return preferred,
    };
    let inner_width = window
        .inner_width()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let inner_height = window
        .inner_height()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let space_top = trigger.top();
    let space_bottom = inner_height - trigger.bottom();
    let space_left = trigger.left();
    let space_right = inner_width - trigger.right();

    let fits = |side: TooltipSide| match side {
        TooltipSide::Top => space_top >= content.height() + TOOLTIP_OFFSET_PX,
        TooltipSide::Bottom => space_bottom >= content.height() + TOOLTIP_OFFSET_PX,
        TooltipSide::Left => space_left >= content.width() + TOOLTIP_OFFSET_PX,
        TooltipSide::Right => space_right >= content.width() + TOOLTIP_OFFSET_PX,
    };

    if fits(preferred) {
        return preferred;
    }

    let alternates = match preferred {
        TooltipSide::Top => [TooltipSide::Bottom, TooltipSide::Top],
        TooltipSide::Bottom => [TooltipSide::Top, TooltipSide::Bottom],
        TooltipSide::Left => [TooltipSide::Right, TooltipSide::Left],
        TooltipSide::Right => [TooltipSide::Left, TooltipSide::Right],
    };
    alternates
        .into_iter()
        .find(|side| fits(*side))
        .unwrap_or(preferred)
}

fn position_content(content: &HtmlElement, trigger: &HtmlElement, preferred: TooltipSide) {
    let trigger_rect = trigger.get_bounding_client_rect();
    let content_rect = content.get_bounding_client_rect();
    let side = pick_side(&trigger_rect, &content_rect, preferred);
    content.set_attribute("data-side", side.data_side()).ok();

    let (mut left, mut top) = match side {
        TooltipSide::Top => (
            trigger_rect.left() + (trigger_rect.width() - content_rect.width()) / 2.0,
            trigger_rect.top() - content_rect.height() - TOOLTIP_OFFSET_PX,
        ),
        TooltipSide::Bottom => (
            trigger_rect.left() + (trigger_rect.width() - content_rect.width()) / 2.0,
            trigger_rect.bottom() + TOOLTIP_OFFSET_PX,
        ),
        TooltipSide::Left => (
            trigger_rect.left() - content_rect.width() - TOOLTIP_OFFSET_PX,
            trigger_rect.top() + (trigger_rect.height() - content_rect.height()) / 2.0,
        ),
        TooltipSide::Right => (
            trigger_rect.right() + TOOLTIP_OFFSET_PX,
            trigger_rect.top() + (trigger_rect.height() - content_rect.height()) / 2.0,
        ),
    };

    if let Some(window) = web_sys::window() {
        let inner_width = window
            .inner_width()
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let inner_height = window
            .inner_height()
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        left = left.clamp(
            VIEWPORT_PADDING_PX,
            (inner_width - content_rect.width() - VIEWPORT_PADDING_PX).max(VIEWPORT_PADDING_PX),
        );
        top = top.clamp(
            VIEWPORT_PADDING_PX,
            (inner_height - content_rect.height() - VIEWPORT_PADDING_PX).max(VIEWPORT_PADDING_PX),
        );
    }

    let _ = content.set_attribute(
        "style",
        &format!("position:fixed;left:{left}px;top:{top}px"),
    );
}

#[function_component(ShadcnTooltip)]
pub fn shadcn_tooltip(props: &ShadcnTooltipProps) -> Html {
    let side = props.side;
    let visible = use_state(|| false);
    let trigger_ref = use_node_ref();
    let content_ref = use_node_ref();
    let tooltip_id = use_state(next_tooltip_id);
    let show_timeout = use_mut_ref(|| None::<Timeout>);
    let portal_host = use_state(|| {
        web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.body())
    });

    let hide = {
        let visible = visible.clone();
        let show_timeout = show_timeout.clone();
        Callback::from(move |_| {
            if let Some(timeout) = show_timeout.borrow_mut().take() {
                timeout.cancel();
            }
            visible.set(false);
        })
    };

    let show_now = {
        let visible = visible.clone();
        let trigger_ref = trigger_ref.clone();
        let content_ref = content_ref.clone();
        Callback::from(move |_| {
            visible.set(true);
            if let (Some(content), Some(trigger)) = (
                content_ref.cast::<HtmlElement>(),
                trigger_ref.cast::<HtmlElement>(),
            ) {
                position_content(&content, &trigger, side);
            }
        })
    };

    let schedule_show = {
        let show_now = show_now.clone();
        let delay_ms = props.delay_ms;
        let show_timeout = show_timeout.clone();
        move || {
            if let Some(timeout) = show_timeout.borrow_mut().take() {
                timeout.cancel();
            }
            if delay_ms == 0 {
                show_now.emit(());
                return;
            }
            let show_now = show_now.clone();
            *show_timeout.borrow_mut() = Some(Timeout::new(delay_ms, move || {
                show_now.emit(());
            }));
        }
    };
    let on_mouse_enter = {
        let schedule_show = schedule_show.clone();
        Callback::from(move |_e: MouseEvent| schedule_show())
    };
    let on_focus_in = {
        let schedule_show = schedule_show.clone();
        Callback::from(move |_e: FocusEvent| schedule_show())
    };
    let on_mouse_leave = {
        let hide = hide.clone();
        Callback::from(move |_e: MouseEvent| hide.emit(()))
    };

    let on_focus_out = {
        let hide = hide.clone();
        let trigger_ref = trigger_ref.clone();
        Callback::from(move |e: FocusEvent| {
            if let Some(trigger) = trigger_ref.cast::<HtmlElement>() {
                if let Some(related_target) = e.related_target() {
                    if let Ok(target_node) = related_target.dyn_into::<web_sys::Node>() {
                        let trigger_node: web_sys::Node = trigger.into();
                        if trigger_node.contains(Some(&target_node)) {
                            return;
                        }
                    }
                }
            }
            hide.emit(());
        })
    };

    {
        let visible = *visible;
        let trigger_ref = trigger_ref.clone();
        let content_ref = content_ref.clone();
        use_effect_with(visible, move |visible| {
            if *visible {
                if let (Some(content), Some(trigger)) = (
                    content_ref.cast::<HtmlElement>(),
                    trigger_ref.cast::<HtmlElement>(),
                ) {
                    position_content(&content, &trigger, side);
                }
            }
            || ()
        });
    }

    let content_label = props.content.clone();
    let described_by = if *visible {
        Some((*tooltip_id).clone())
    } else {
        None
    };

    let portal = portal_host.as_ref().map(|host| {
        create_portal(
            html! {
                <div
                    ref={content_ref}
                    id={tooltip_id.to_string()}
                    role="tooltip"
                    class={classes!(
                        "shadcn-tooltip-content",
                        (*visible).then_some("is-visible"),
                    )}
                >
                    {content_label}
                </div>
            },
            host.clone().into(),
        )
    });

    html! {
        <>
            <span
                ref={trigger_ref}
                class={classes!("shadcn-tooltip", "shadcn-tooltip-trigger", props.class.clone())}
                aria-describedby={described_by}
                onmouseenter={on_mouse_enter}
                onmouseleave={on_mouse_leave}
                onfocusin={on_focus_in}
                onfocusout={on_focus_out}
            >
                {props.children.clone()}
            </span>
            {portal}
        </>
    }
}
