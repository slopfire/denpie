use serde::{Deserialize, Serialize};
use std::rc::Rc;
use yew::prelude::*;

#[derive(Clone, PartialEq, Default, Deserialize, Serialize)]
pub struct UserProfile {
    pub id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub role: String,
    pub avatar_data: Option<String>,
}

#[derive(Clone, PartialEq, Default)]
pub struct ToastMessage {
    pub message: String,
    pub show: bool,
}

#[derive(Clone, PartialEq, Default)]
pub struct AppState {
    pub user: Option<UserProfile>,
    pub authed: bool,
    pub toast: ToastMessage,
}

pub enum AppAction {
    SetAuthed(bool),
    SetUser(Option<UserProfile>),
    ShowToast(String),
    HideToast,
}

impl Reducible for AppState {
    type Action = AppAction;

    fn reduce(self: Rc<Self>, action: Self::Action) -> Rc<Self> {
        match action {
            AppAction::SetAuthed(authed) => AppState {
                authed,
                ..(*self).clone()
            }
            .into(),
            AppAction::SetUser(user) => AppState {
                user,
                ..(*self).clone()
            }
            .into(),
            AppAction::ShowToast(message) => AppState {
                toast: ToastMessage {
                    message,
                    show: true,
                },
                ..(*self).clone()
            }
            .into(),
            AppAction::HideToast => AppState {
                toast: ToastMessage {
                    message: self.toast.message.clone(),
                    show: false,
                },
                ..(*self).clone()
            }
            .into(),
        }
    }
}
