use std::rc::Rc;
use yew::prelude::*;

#[derive(Clone, PartialEq, Default)]
pub struct AppState {
    pub username: String,
    // Add other global state fields as needed
}

pub enum AppAction {
    SetUsername(String),
}

impl Reducible for AppState {
    type Action = AppAction;

    fn reduce(self: Rc<Self>, action: Self::Action) -> Rc<Self> {
        match action {
            AppAction::SetUsername(username) => AppState {
                username,
                ..(*self).clone()
            }
            .into(),
        }
    }
}
