use crate::app::View;
use gloo_events::EventListener;
use gloo_timers::callback::Interval;
use yew::prelude::*;
use yew_router::prelude::*;

/// Interval in milliseconds between automatic refreshes while a view is active.
const REFRESH_INTERVAL_MS: u32 = 60_000;

/// Re-fetches `view`'s data when the view becomes the active route, when the
/// browser tab becomes visible again, and every 60 seconds while it stays
/// active. All listeners and intervals are cleaned up on route change or
/// unmount.
///
/// This compensates for the `RouteView` keep-alive mounting strategy in
/// `app.rs`, where components are never unmounted and their one-shot
/// `use_effect_with((), ...)` loads only fire once.
#[hook]
pub fn use_view_refresh(view: View, refresh: Callback<()>) {
    let route = use_route::<View>();
    let is_active = route.as_ref() == Some(&view);

    // Emit a refresh whenever this view becomes the active route (including
    // the initial mount when the view is already active).
    {
        let refresh = refresh.clone();
        use_effect_with(is_active, move |active| {
            if *active {
                refresh.emit(());
            }
            || ()
        });
    }

    // While the view is active, watch for tab visibility changes and run a
    // periodic interval. Both are torn down when the view is no longer active
    // or the component unmounts.
    {
        let refresh = refresh.clone();
        use_effect_with(is_active, move |active| {
            let document = if *active {
                web_sys::window().and_then(|window| window.document())
            } else {
                None
            };

            let visibility_listener = document.as_ref().map(|document| {
                let refresh = refresh.clone();
                let target: &web_sys::EventTarget = document.as_ref();
                EventListener::new(target, "visibilitychange", move |_| {
                    let visible = web_sys::window()
                        .and_then(|window| window.document())
                        .map(|document| document.visibility_state())
                        == Some(web_sys::VisibilityState::Visible);
                    if visible {
                        refresh.emit(());
                    }
                })
            });

            let interval = if *active {
                let refresh = refresh.clone();
                Some(Interval::new(REFRESH_INTERVAL_MS, move || refresh.emit(())))
            } else {
                None
            };

            move || {
                drop(visibility_listener);
                drop(interval);
            }
        });
    }
}
