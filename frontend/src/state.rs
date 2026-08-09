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
    pub build_sha: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum AuthStatus {
    #[default]
    Checking,
    Guest,
    Authenticated,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)]
pub enum ToastKind {
    #[default]
    Info,
    Success,
    Error,
}

#[derive(Clone, PartialEq, Default)]
pub struct ToastMessage {
    pub message: String,
    pub detail: Option<String>,
    pub kind: ToastKind,
    pub show: bool,
}

#[derive(Clone, PartialEq, Default)]
pub struct AppState {
    pub user: Option<UserProfile>,
    pub auth_status: AuthStatus,
    pub toast: ToastMessage,
    pub admin_mode: bool,
}
pub enum AppAction {
    SetSession(Option<UserProfile>),
    SetUser(Option<UserProfile>),
    SetAdminMode(bool),
    ShowToast {
        message: String,
        detail: Option<String>,
        kind: ToastKind,
    },
    HideToast,
}

impl Reducible for AppState {
    type Action = AppAction;

    fn reduce(self: Rc<Self>, action: Self::Action) -> Rc<Self> {
        match action {
            AppAction::SetSession(user) => {
                let auth_status = if user.is_some() {
                    AuthStatus::Authenticated
                } else {
                    AuthStatus::Guest
                };
                AppState {
                    user,
                    auth_status,
                    admin_mode: false,
                    ..(*self).clone()
                }
                .into()
            }
            AppAction::SetUser(user) => AppState {
                user,
                ..(*self).clone()
            }
            .into(),
            AppAction::SetAdminMode(enabled) => AppState {
                admin_mode: enabled,
                ..(*self).clone()
            }
            .into(),
            AppAction::ShowToast {
                message,
                detail,
                kind,
            } => AppState {
                toast: ToastMessage {
                    message,
                    detail,
                    kind,
                    show: true,
                },
                ..(*self).clone()
            }
            .into(),
            AppAction::HideToast => AppState {
                toast: ToastMessage {
                    message: self.toast.message.clone(),
                    detail: self.toast.detail.clone(),
                    kind: self.toast.kind,
                    show: false,
                },
                ..(*self).clone()
            }
            .into(),
        }
    }
}
