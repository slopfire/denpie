use crate::state::{AppAction, AppState};
use gloo_timers::callback::Timeout;
use yew::prelude::*;

pub fn toast(app_state: &UseReducerHandle<AppState>, message: impl Into<String>) {
    app_state.dispatch(AppAction::ShowToast(message.into()));
    let state = app_state.clone();
    Timeout::new(2400, move || {
        state.dispatch(AppAction::HideToast);
    })
    .forget();
}
