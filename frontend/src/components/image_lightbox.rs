use std::cell::RefCell;
use std::rc::Rc;

use gloo_timers::callback::Timeout;
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{
    AddEventListenerOptions, Element, HtmlElement, HtmlImageElement, KeyboardEvent, WheelEvent,
};
use yew::create_portal;
use yew::prelude::*;

const MIN_ZOOM: f64 = 1.0;
const MAX_ZOOM: f64 = 4.0;
const ZOOM_BUTTON_STEP: f64 = 0.25;
const ZOOM_WHEEL_EXP_FACTOR: f64 = 0.0014;
const STATE_SYNC_DEBOUNCE_MS: u32 = 48;

#[derive(Properties, PartialEq)]
pub struct ImageLightboxProps {
    pub images: Vec<String>,
    pub initial_index: usize,
    pub on_close: Callback<()>,
}

fn event_target_is_image(event: &MouseEvent) -> bool {
    event
        .target()
        .and_then(|target| target.dyn_into::<Element>().ok())
        .is_some_and(|element| element.class_list().contains("image-lightbox-image"))
}

fn clamp_zoom(value: f64) -> f64 {
    value.clamp(MIN_ZOOM, MAX_ZOOM)
}

fn scale_wheel_delta(delta: f64, mode: u32) -> f64 {
    match mode {
        1 => delta * 18.0,
        2 => delta * 120.0,
        _ => delta,
    }
}

fn normalize_wheel_delta(e: &WheelEvent) -> f64 {
    scale_wheel_delta(e.delta_y(), e.delta_mode())
}

fn center_carousel_thumb(track_ref: &NodeRef, index: usize) {
    let Some(track) = track_ref.cast::<HtmlElement>() else {
        return;
    };
    let Ok(thumbs) = track.query_selector_all(".image-lightbox-thumb") else {
        return;
    };
    let Some(node) = thumbs.item(index as u32) else {
        return;
    };
    let Ok(thumb) = node.dyn_into::<HtmlElement>() else {
        return;
    };
    let track_rect = track.get_bounding_client_rect();
    let thumb_rect = thumb.get_bounding_client_rect();
    if track_rect.width() < 1.0 || thumb_rect.width() < 1.0 {
        return;
    }
    let track_center = track_rect.left() + track_rect.width() / 2.0;
    let thumb_center = thumb_rect.left() + thumb_rect.width() / 2.0;
    let offset = thumb_center - track_center;
    if !offset.is_finite() {
        return;
    }
    let max_scroll = (track.scroll_width() - track.client_width()).max(0);
    let next_scroll = (f64::from(track.scroll_left()) + offset).clamp(0.0, f64::from(max_scroll));
    track.set_scroll_left(next_scroll as i32);
}

/// Wheel on thumbnail strip: same direction as ArrowRight (next) / ArrowLeft (prev).
fn carousel_wheel_step(e: &WheelEvent) -> i32 {
    let dy = e.delta_y();
    let dx = e.delta_x();
    let primary = if dy.abs() >= dx.abs() { dy } else { dx };
    if primary < 0.0 {
        -1
    } else if primary > 0.0 {
        1
    } else {
        0
    }
}

fn cursor_offset_in_stage(
    stage: &web_sys::HtmlElement,
    client_x: f64,
    client_y: f64,
) -> (f64, f64) {
    let rect = stage.get_bounding_client_rect();
    let center_x = rect.left() + rect.width() / 2.0;
    let center_y = rect.top() + rect.height() / 2.0;
    (client_x - center_x, client_y - center_y)
}

fn pan_for_zoom_at_focal(
    old_zoom: f64,
    new_zoom: f64,
    pan_x: f64,
    pan_y: f64,
    focal_x: f64,
    focal_y: f64,
) -> (f64, f64) {
    let old = old_zoom.max(MIN_ZOOM);
    let ratio = new_zoom / old;
    (
        focal_x - (focal_x - pan_x) * ratio,
        focal_y - (focal_y - pan_y) * ratio,
    )
}

fn apply_transform(img: &HtmlImageElement, pan_x: f64, pan_y: f64, zoom: f64) {
    let style = format!("transform: translate3d({pan_x}px, {pan_y}px, 0) scale({zoom})");
    let _ = img.set_attribute("style", &style);
}

fn sync_img_transform(img_ref: &NodeRef, pan_x: f64, pan_y: f64, zoom: f64) {
    if let Some(img) = img_ref.cast::<HtmlImageElement>() {
        apply_transform(&img, pan_x, pan_y, zoom);
    }
}

fn commit_view_transform(
    img_ref: &NodeRef,
    zoom_live: &Rc<RefCell<f64>>,
    pan_x_live: &Rc<RefCell<f64>>,
    pan_y_live: &Rc<RefCell<f64>>,
    zoom: f64,
    pan_x: f64,
    pan_y: f64,
) {
    *zoom_live.borrow_mut() = zoom;
    *pan_x_live.borrow_mut() = pan_x;
    *pan_y_live.borrow_mut() = pan_y;
    sync_img_transform(img_ref, pan_x, pan_y, zoom);
}

#[allow(clippy::too_many_arguments)]
fn apply_zoom_at_focal(
    img_ref: &NodeRef,
    zoom: &UseStateHandle<f64>,
    zoom_live: &Rc<RefCell<f64>>,
    pan_x: &UseStateHandle<f64>,
    pan_y: &UseStateHandle<f64>,
    pan_x_live: &Rc<RefCell<f64>>,
    pan_y_live: &Rc<RefCell<f64>>,
    value: f64,
    focal_x: f64,
    focal_y: f64,
) {
    let old_zoom = *zoom_live.borrow();
    let next = clamp_zoom(value);
    let (next_px, next_py) = if next <= MIN_ZOOM + f64::EPSILON {
        (0.0, 0.0)
    } else {
        pan_for_zoom_at_focal(
            old_zoom,
            next,
            *pan_x_live.borrow(),
            *pan_y_live.borrow(),
            focal_x,
            focal_y,
        )
    };
    commit_view_transform(
        img_ref, zoom_live, pan_x_live, pan_y_live, next, next_px, next_py,
    );
    zoom.set(next);
    pan_x.set(next_px);
    pan_y.set(next_py);
}

fn reset_view_state(
    img_ref: &NodeRef,
    zoom: &UseStateHandle<f64>,
    zoom_live: &Rc<RefCell<f64>>,
    pan_x: &UseStateHandle<f64>,
    pan_y: &UseStateHandle<f64>,
    pan_x_live: &Rc<RefCell<f64>>,
    pan_y_live: &Rc<RefCell<f64>>,
) {
    *zoom_live.borrow_mut() = MIN_ZOOM;
    *pan_x_live.borrow_mut() = 0.0;
    *pan_y_live.borrow_mut() = 0.0;
    sync_img_transform(img_ref, 0.0, 0.0, MIN_ZOOM);
    zoom.set(MIN_ZOOM);
    pan_x.set(0.0);
    pan_y.set(0.0);
}

#[function_component(ImageLightbox)]
pub fn image_lightbox(props: &ImageLightboxProps) -> Html {
    let current_index = use_state(|| {
        props
            .initial_index
            .min(props.images.len().saturating_sub(1))
    });
    let zoom = use_state(|| MIN_ZOOM);
    let pan_x = use_state(|| 0.0_f64);
    let pan_y = use_state(|| 0.0_f64);
    let dragging = use_state(|| false);
    let root_ref = use_node_ref();
    let stage_ref = use_node_ref();
    let track_ref = use_node_ref();
    let img_ref = use_node_ref();
    let drag_origin = use_mut_ref(|| (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64));
    let state_sync_timer = use_mut_ref(|| None::<Timeout>);
    let wheel_delta_pending = use_mut_ref(|| Rc::new(RefCell::new(0.0_f64)));
    let wheel_focal_pending = use_mut_ref(|| Rc::new(RefCell::new((0.0_f64, 0.0_f64))));
    let wheel_frame_closure = use_mut_ref(|| Rc::new(RefCell::new(None::<Closure<dyn FnMut()>>)));
    let wheel_raf_id = use_mut_ref(|| Rc::new(RefCell::new(None::<i32>)));
    let zoom_live = use_mut_ref(|| Rc::new(RefCell::new(MIN_ZOOM)));
    let pan_x_live = use_mut_ref(|| Rc::new(RefCell::new(0.0_f64)));
    let pan_y_live = use_mut_ref(|| Rc::new(RefCell::new(0.0_f64)));
    let portal_host = use_state(|| {
        web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.body())
    });

    let image_count = props.images.len();
    let has_multiple = image_count > 1;
    let is_zoomed = *zoom > MIN_ZOOM + f64::EPSILON;

    let reset_view = {
        let img_ref = img_ref.clone();
        let zoom = zoom.clone();
        let pan_x = pan_x.clone();
        let pan_y = pan_y.clone();
        let zoom_live = zoom_live.clone();
        let pan_x_live = pan_x_live.clone();
        let pan_y_live = pan_y_live.clone();
        Callback::from(move |_| {
            reset_view_state(
                &img_ref,
                &zoom,
                &zoom_live.borrow(),
                &pan_x,
                &pan_y,
                &pan_x_live.borrow(),
                &pan_y_live.borrow(),
            );
        })
    };

    let go_prev = {
        let current_index = current_index.clone();
        let reset_view = reset_view.clone();
        Callback::from(move |_| {
            if image_count <= 1 || *current_index == 0 {
                return;
            }
            reset_view.emit(());
            current_index.set(*current_index - 1);
        })
    };

    let go_next = {
        let current_index = current_index.clone();
        let reset_view = reset_view.clone();
        Callback::from(move |_| {
            if image_count <= 1 || *current_index + 1 >= image_count {
                return;
            }
            reset_view.emit(());
            current_index.set(*current_index + 1);
        })
    };

    let go_to = {
        let current_index = current_index.clone();
        let reset_view = reset_view.clone();
        Callback::from(move |index: usize| {
            if index >= image_count || index == *current_index {
                return;
            }
            reset_view.emit(());
            current_index.set(index);
        })
    };

    let zoom_in = {
        let img_ref = img_ref.clone();
        let zoom = zoom.clone();
        let pan_x = pan_x.clone();
        let pan_y = pan_y.clone();
        let zoom_live = zoom_live.clone();
        let pan_x_live = pan_x_live.clone();
        let pan_y_live = pan_y_live.clone();
        Callback::from(move |_| {
            apply_zoom_at_focal(
                &img_ref,
                &zoom,
                &zoom_live.borrow(),
                &pan_x,
                &pan_y,
                &pan_x_live.borrow(),
                &pan_y_live.borrow(),
                *zoom_live.borrow().borrow() + ZOOM_BUTTON_STEP,
                0.0,
                0.0,
            );
        })
    };

    let zoom_out = {
        let img_ref = img_ref.clone();
        let zoom = zoom.clone();
        let pan_x = pan_x.clone();
        let pan_y = pan_y.clone();
        let zoom_live = zoom_live.clone();
        let pan_x_live = pan_x_live.clone();
        let pan_y_live = pan_y_live.clone();
        Callback::from(move |_| {
            apply_zoom_at_focal(
                &img_ref,
                &zoom,
                &zoom_live.borrow(),
                &pan_x,
                &pan_y,
                &pan_x_live.borrow(),
                &pan_y_live.borrow(),
                *zoom_live.borrow().borrow() - ZOOM_BUTTON_STEP,
                0.0,
                0.0,
            );
        })
    };

    let on_close = props.on_close.clone();

    {
        let root_ref = root_ref.clone();
        use_effect_with((), move |_| {
            if let Some(root) = root_ref.cast::<web_sys::HtmlElement>() {
                let _ = root.focus();
            }
            || ()
        });
    }

    {
        let root_ref = root_ref.clone();
        let stage_ref = stage_ref.clone();
        let img_ref = img_ref.clone();
        let zoom = zoom.clone();
        let pan_x = pan_x.clone();
        let pan_y = pan_y.clone();
        let zoom_live = zoom_live.clone();
        let pan_x_live = pan_x_live.clone();
        let pan_y_live = pan_y_live.clone();
        let state_sync_timer = state_sync_timer.clone();
        let wheel_delta_pending = wheel_delta_pending.clone();
        let wheel_focal_pending = wheel_focal_pending.clone();
        let wheel_frame_closure = wheel_frame_closure.clone();
        let wheel_raf_id = wheel_raf_id.clone();
        use_effect_with(image_count, move |_| {
            let Some(root) = root_ref.cast::<web_sys::HtmlElement>() else {
                return Box::new(|| ()) as Box<dyn FnOnce()>;
            };

            let schedule_state_sync: Rc<dyn Fn()> = {
                let zoom = zoom.clone();
                let pan_x = pan_x.clone();
                let pan_y = pan_y.clone();
                let zoom_live = zoom_live.borrow().clone();
                let pan_x_live = pan_x_live.borrow().clone();
                let pan_y_live = pan_y_live.borrow().clone();
                let state_sync_timer = state_sync_timer.clone();
                Rc::new(move || {
                    if let Some(timer) = state_sync_timer.borrow_mut().take() {
                        drop(timer);
                    }
                    let zoom = zoom.clone();
                    let pan_x = pan_x.clone();
                    let pan_y = pan_y.clone();
                    let zoom_live = zoom_live.clone();
                    let pan_x_live = pan_x_live.clone();
                    let pan_y_live = pan_y_live.clone();
                    *state_sync_timer.borrow_mut() =
                        Some(Timeout::new(STATE_SYNC_DEBOUNCE_MS, move || {
                            let z = *zoom_live.borrow();
                            let px = *pan_x_live.borrow();
                            let py = *pan_y_live.borrow();
                            zoom.set(z);
                            pan_x.set(px);
                            pan_y.set(py);
                        }));
                })
            };

            let stage_ref = stage_ref.clone();
            let img_ref = img_ref.clone();
            let zoom_live = zoom_live.borrow().clone();
            let pan_x_live = pan_x_live.borrow().clone();
            let pan_y_live = pan_y_live.borrow().clone();
            let schedule_state_sync = schedule_state_sync.clone();
            let wheel_delta_pending = wheel_delta_pending.borrow().clone();
            let wheel_focal_pending = wheel_focal_pending.borrow().clone();
            let wheel_frame_closure = wheel_frame_closure.borrow().clone();
            let wheel_raf_id = wheel_raf_id.borrow().clone();

            let flush_wheel_zoom = {
                let img_ref = img_ref.clone();
                let zoom_live = zoom_live.clone();
                let pan_x_live = pan_x_live.clone();
                let pan_y_live = pan_y_live.clone();
                let wheel_delta_pending = wheel_delta_pending.clone();
                let wheel_focal_pending = wheel_focal_pending.clone();
                let schedule_state_sync = schedule_state_sync.clone();
                Rc::new(move || {
                    let delta = *wheel_delta_pending.borrow_mut();
                    *wheel_delta_pending.borrow_mut() = 0.0;
                    if delta.abs() < f64::EPSILON {
                        return;
                    }
                    let (focal_x, focal_y) = *wheel_focal_pending.borrow();
                    let old_zoom = *zoom_live.borrow();
                    let factor = (-delta * ZOOM_WHEEL_EXP_FACTOR).exp();
                    let next_z = clamp_zoom(old_zoom * factor);
                    let (next_px, next_py) = if next_z <= MIN_ZOOM + f64::EPSILON {
                        (0.0, 0.0)
                    } else {
                        pan_for_zoom_at_focal(
                            old_zoom,
                            next_z,
                            *pan_x_live.borrow(),
                            *pan_y_live.borrow(),
                            focal_x,
                            focal_y,
                        )
                    };
                    commit_view_transform(
                        &img_ref,
                        &zoom_live,
                        &pan_x_live,
                        &pan_y_live,
                        next_z,
                        next_px,
                        next_py,
                    );
                    schedule_state_sync.clone()();
                })
            };

            let schedule_wheel_zoom = {
                let wheel_raf_id = wheel_raf_id.clone();
                let wheel_frame_closure = wheel_frame_closure.clone();
                let flush_wheel_zoom = flush_wheel_zoom.clone();
                Rc::new(move || {
                    if wheel_raf_id.borrow().is_some() {
                        return;
                    }
                    let flush_wheel_zoom = flush_wheel_zoom.clone();
                    let wheel_raf_id_clear = wheel_raf_id.clone();
                    let wheel_raf_id_set = wheel_raf_id.clone();
                    *wheel_frame_closure.borrow_mut() = Some(Closure::new(move || {
                        *wheel_raf_id_clear.borrow_mut() = None;
                        flush_wheel_zoom();
                    }));
                    if let (Some(window), Some(frame)) =
                        (web_sys::window(), wheel_frame_closure.borrow().as_ref())
                    {
                        let id = window
                            .request_animation_frame(frame.as_ref().unchecked_ref())
                            .unwrap_or(0);
                        *wheel_raf_id_set.borrow_mut() = Some(id);
                    }
                })
            };

            let wheel_closure = Closure::<dyn FnMut(WheelEvent)>::new(move |e: WheelEvent| {
                let Some(target) = e.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
                    return;
                };

                let Some(stage) = stage_ref.cast::<web_sys::HtmlElement>() else {
                    return;
                };
                if !stage.contains(Some(&target)) {
                    return;
                }

                e.prevent_default();

                if e.shift_key() && *zoom_live.borrow() > MIN_ZOOM + f64::EPSILON {
                    let z = *zoom_live.borrow();
                    let next_x = *pan_x_live.borrow() - e.delta_x();
                    let next_y = *pan_y_live.borrow() - e.delta_y();
                    commit_view_transform(
                        &img_ref,
                        &zoom_live,
                        &pan_x_live,
                        &pan_y_live,
                        z,
                        next_x,
                        next_y,
                    );
                    schedule_state_sync.clone()();
                    return;
                }

                let delta = normalize_wheel_delta(&e);
                let focal =
                    cursor_offset_in_stage(&stage, e.client_x() as f64, e.client_y() as f64);
                *wheel_delta_pending.borrow_mut() += delta;
                *wheel_focal_pending.borrow_mut() = focal;
                schedule_wheel_zoom.clone()();
            });

            let options = AddEventListenerOptions::new();
            options.set_passive(false);
            let _ = root.add_event_listener_with_callback_and_add_event_listener_options(
                "wheel",
                wheel_closure.as_ref().unchecked_ref(),
                &options,
            );

            let wheel_raf_id = wheel_raf_id.clone();
            let wheel_closure = wheel_closure;
            Box::new(move || {
                if let Some(id) = wheel_raf_id.borrow_mut().take() {
                    if let Some(window) = web_sys::window() {
                        let _ = window.cancel_animation_frame(id);
                    }
                }
                let _ = root.remove_event_listener_with_callback(
                    "wheel",
                    wheel_closure.as_ref().unchecked_ref(),
                );
            }) as Box<dyn FnOnce()>
        });
    }

    {
        let track_ref = track_ref.clone();
        let active = *current_index;
        use_effect_with((image_count, active), move |(count, index)| {
            if *count > 1 {
                let track_ref = track_ref.clone();
                let index = *index;
                Timeout::new(0, move || {
                    center_carousel_thumb(&track_ref, index);
                })
                .forget();
            }
            || ()
        });
    }

    let on_track_wheel = {
        let go_prev = go_prev.clone();
        let go_next = go_next.clone();
        Callback::from(move |e: WheelEvent| {
            e.prevent_default();
            e.stop_propagation();
            match carousel_wheel_step(&e) {
                1 => go_next.emit(()),
                -1 => go_prev.emit(()),
                _ => {}
            }
        })
    };

    {
        let img_ref = img_ref.clone();
        let zoom_live = zoom_live.clone();
        let pan_x_live = pan_x_live.clone();
        let pan_y_live = pan_y_live.clone();
        let current_index = *current_index;
        let current_src = props.images.get(current_index).cloned().unwrap_or_default();
        use_effect_with(current_src, move |_| {
            sync_img_transform(
                &img_ref,
                *pan_x_live.borrow().borrow(),
                *pan_y_live.borrow().borrow(),
                *zoom_live.borrow().borrow(),
            );
            || ()
        });
    }

    {
        let dragging = dragging.clone();
        let pan_x = pan_x.clone();
        let pan_y = pan_y.clone();
        let pan_x_live = pan_x_live.clone();
        let pan_y_live = pan_y_live.clone();
        let drag_origin = drag_origin.clone();
        let img_ref = img_ref.clone();
        let zoom_live = zoom_live.clone();
        use_effect_with(*dragging, move |is_dragging| {
            if !*is_dragging {
                return Box::new(|| ()) as Box<dyn FnOnce()>;
            }

            let dragging = dragging.clone();
            let pan_x = pan_x.clone();
            let pan_y = pan_y.clone();
            let pan_x_live = pan_x_live.borrow().clone();
            let pan_y_live = pan_y_live.borrow().clone();
            let drag_origin = drag_origin.clone();
            let img_ref = img_ref.clone();
            let zoom_live = zoom_live.borrow().clone();
            let move_closure =
                Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |e: MouseEvent| {
                    let (start_x, start_y, origin_x, origin_y) = *drag_origin.borrow();
                    let next_x = origin_x + (e.client_x() as f64 - start_x);
                    let next_y = origin_y + (e.client_y() as f64 - start_y);
                    let z = *zoom_live.borrow();
                    *pan_x_live.borrow_mut() = next_x;
                    *pan_y_live.borrow_mut() = next_y;
                    sync_img_transform(&img_ref, next_x, next_y, z);
                    pan_x.set(next_x);
                    pan_y.set(next_y);
                });
            let up_closure = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |_| {
                dragging.set(false);
            });

            if let Some(window) = web_sys::window() {
                let _ = window.add_event_listener_with_callback(
                    "mousemove",
                    move_closure.as_ref().unchecked_ref(),
                );
                let _ = window.add_event_listener_with_callback(
                    "mouseup",
                    up_closure.as_ref().unchecked_ref(),
                );
            }

            let move_closure = move_closure;
            let up_closure = up_closure;
            Box::new(move || {
                if let Some(window) = web_sys::window() {
                    let _ = window.remove_event_listener_with_callback(
                        "mousemove",
                        move_closure.as_ref().unchecked_ref(),
                    );
                    let _ = window.remove_event_listener_with_callback(
                        "mouseup",
                        up_closure.as_ref().unchecked_ref(),
                    );
                }
            }) as Box<dyn FnOnce()>
        });
    }

    let on_keydown = {
        let on_close = on_close.clone();
        let go_prev = go_prev.clone();
        let go_next = go_next.clone();
        Callback::from(move |e: KeyboardEvent| match e.key().as_str() {
            "Escape" => {
                e.prevent_default();
                on_close.emit(());
            }
            "ArrowLeft" if has_multiple => {
                e.prevent_default();
                go_prev.emit(());
            }
            "ArrowRight" if has_multiple => {
                e.prevent_default();
                go_next.emit(());
            }
            _ => {}
        })
    };

    let on_stage_click = {
        let on_close = props.on_close.clone();
        Callback::from(move |e: MouseEvent| {
            if event_target_is_image(&e) {
                return;
            }
            on_close.emit(());
        })
    };

    let on_image_click = Callback::from(|e: MouseEvent| e.stop_propagation());

    let on_image_double_click = {
        let img_ref = img_ref.clone();
        let zoom = zoom.clone();
        let pan_x = pan_x.clone();
        let pan_y = pan_y.clone();
        let zoom_live = zoom_live.clone();
        let pan_x_live = pan_x_live.clone();
        let pan_y_live = pan_y_live.clone();
        let stage_ref = stage_ref.clone();
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            if *zoom_live.borrow().borrow() > MIN_ZOOM + f64::EPSILON {
                reset_view_state(
                    &img_ref,
                    &zoom,
                    &zoom_live.borrow(),
                    &pan_x,
                    &pan_y,
                    &pan_x_live.borrow(),
                    &pan_y_live.borrow(),
                );
            } else if let Some(stage) = stage_ref.cast::<web_sys::HtmlElement>() {
                let (focal_x, focal_y) =
                    cursor_offset_in_stage(&stage, e.client_x() as f64, e.client_y() as f64);
                apply_zoom_at_focal(
                    &img_ref,
                    &zoom,
                    &zoom_live.borrow(),
                    &pan_x,
                    &pan_y,
                    &pan_x_live.borrow(),
                    &pan_y_live.borrow(),
                    2.0,
                    focal_x,
                    focal_y,
                );
            }
        })
    };

    let on_image_mouse_down = {
        let dragging = dragging.clone();
        let drag_origin = drag_origin.clone();
        let pan_x = pan_x.clone();
        let pan_y = pan_y.clone();
        let zoom_live = zoom_live.clone();
        Callback::from(move |e: MouseEvent| {
            if *zoom_live.borrow().borrow() <= MIN_ZOOM + f64::EPSILON {
                return;
            }
            e.prevent_default();
            *drag_origin.borrow_mut() = (e.client_x() as f64, e.client_y() as f64, *pan_x, *pan_y);
            dragging.set(true);
        })
    };

    let current_src = props
        .images
        .get(*current_index)
        .cloned()
        .unwrap_or_default();

    let at_first = *current_index == 0;
    let at_last = *current_index + 1 >= image_count;

    let portal = portal_host.as_ref().map(|host| {
        create_portal(
            html! {
                <div
                    ref={root_ref}
                    class="image-lightbox"
                    role="dialog"
                    aria-modal="true"
                    aria-label="Image viewer"
                    tabindex="-1"
                    onkeydown={on_keydown}
                >
                    <div
                        class="image-lightbox-header"
                        onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}
                    >
                        <div class="image-lightbox-zoom-controls">
                            <button
                                type="button"
                                class="image-lightbox-nav border border-token"
                                aria-label="Zoom out"
                                disabled={!is_zoomed}
                                onclick={Callback::from({
                                    let zoom_out = zoom_out.clone();
                                    move |e: MouseEvent| {
                                        e.stop_propagation();
                                        zoom_out.emit(());
                                    }
                                })}
                            >
                                <iconify-icon icon="radix-icons:minus" class="radix-icon"></iconify-icon>
                            </button>
                            <span class="image-lightbox-zoom-label">
                                {format!("{}%", (*zoom * 100.0).round() as i32)}
                            </span>
                            <button
                                type="button"
                                class="image-lightbox-nav border border-token"
                                aria-label="Zoom in"
                                disabled={*zoom >= MAX_ZOOM - f64::EPSILON}
                                onclick={Callback::from({
                                    let zoom_in = zoom_in.clone();
                                    move |e: MouseEvent| {
                                        e.stop_propagation();
                                        zoom_in.emit(());
                                    }
                                })}
                            >
                                <iconify-icon icon="radix-icons:plus" class="radix-icon"></iconify-icon>
                            </button>
                            if is_zoomed {
                                <button
                                    type="button"
                                    class="image-lightbox-nav border border-token"
                                    aria-label="Reset zoom"
                                    onclick={Callback::from({
                                        let reset_view = reset_view.clone();
                                        move |e: MouseEvent| {
                                            e.stop_propagation();
                                            reset_view.emit(());
                                        }
                                    })}
                                >
                                    <iconify-icon icon="radix-icons:reset" class="radix-icon"></iconify-icon>
                                </button>
                            }
                        </div>
                        <button
                            type="button"
                            class="image-lightbox-close border border-token"
                            aria-label="Close image viewer"
                            onclick={Callback::from({
                                let on_close = props.on_close.clone();
                                move |e: MouseEvent| {
                                    e.stop_propagation();
                                    on_close.emit(());
                                }
                            })}
                        >
                            <iconify-icon icon="radix-icons:cross-2" class="radix-icon"></iconify-icon>
                        </button>
                    </div>

                    <div
                        ref={stage_ref}
                        class="image-lightbox-stage"
                        onclick={on_stage_click}
                    >
                        <div class="image-lightbox-frame">
                            <img
                                ref={img_ref}
                                src={current_src}
                                alt={format!("Image {} of {}", *current_index + 1, image_count)}
                                class={classes!("image-lightbox-image", is_zoomed.then_some("is-zoomed"))}
                                onclick={on_image_click}
                                ondblclick={on_image_double_click}
                                onmousedown={on_image_mouse_down}
                            />
                        </div>
                    </div>

                    if has_multiple {
                        <div
                            class="image-lightbox-dock"
                            onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}
                        >
                            <div class="image-lightbox-dock-controls">
                                <button
                                    type="button"
                                    class="image-lightbox-nav border border-token"
                                    aria-label="Previous image"
                                    disabled={at_first}
                                    onclick={Callback::from({
                                        let go_prev = go_prev.clone();
                                        move |_| go_prev.emit(())
                                    })}
                                >
                                    <iconify-icon icon="radix-icons:chevron-left" class="radix-icon"></iconify-icon>
                                </button>
                                <span class="image-lightbox-counter">
                                    {format!("{} / {}", *current_index + 1, image_count)}
                                </span>
                                <button
                                    type="button"
                                    class="image-lightbox-nav border border-token"
                                    aria-label="Next image"
                                    disabled={at_last}
                                    onclick={Callback::from({
                                        let go_next = go_next.clone();
                                        move |_| go_next.emit(())
                                    })}
                                >
                                    <iconify-icon icon="radix-icons:chevron-right" class="radix-icon"></iconify-icon>
                                </button>
                            </div>
                            <div
                                ref={track_ref}
                                class="image-lightbox-track"
                                role="tablist"
                                aria-label="Image thumbnails"
                                onwheel={on_track_wheel}
                            >
                                {
                                    for props.images.iter().enumerate().map(|(index, thumb)| {
                                        let is_active = index == *current_index;
                                        let go_to = go_to.clone();
                                        html! {
                                            <button
                                                type="button"
                                                role="tab"
                                                aria-selected={is_active.to_string()}
                                                aria-label={format!("Show image {}", index + 1)}
                                                class={classes!("image-lightbox-thumb", is_active.then_some("is-active"))}
                                                onclick={Callback::from(move |_| go_to.emit(index))}
                                            >
                                                <img src={thumb.clone()} alt="" loading="lazy" />
                                            </button>
                                        }
                                    })
                                }
                            </div>
                        </div>
                    }
                </div>
            },
            host.clone().into(),
        )
    });

    html! {
        <>
            {portal}
        </>
    }
}
