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
    AutoHideToast {
        message: String,
        detail: Option<String>,
        kind: ToastKind,
    },
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
            // Error toasts stay visible until dismissed. `self.toast.kind` is
            // checked as well as the timer's recorded kind so a stale timer
            // fired over an error toast can never hide it.
            AppAction::AutoHideToast {
                message: _,
                detail: _,
                kind,
            } if kind == ToastKind::Error || self.toast.kind == ToastKind::Error => self,
            // Only hide the exact toast the timer was created for; a timer from
            // a replaced toast must not hide a newer one.
            AppAction::AutoHideToast {
                message,
                detail,
                kind,
            } if self.toast.message == message
                && self.toast.detail == detail
                && self.toast.kind == kind =>
            {
                AppState {
                    toast: ToastMessage {
                        message: self.toast.message.clone(),
                        detail: self.toast.detail.clone(),
                        kind: self.toast.kind,
                        show: false,
                    },
                    ..(*self).clone()
                }
                .into()
            }
            AppAction::AutoHideToast { .. } => self,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AppAction, AppState, ToastKind, ToastMessage};
    use std::rc::Rc;
    use yew::Reducible;

    #[test]
    fn error_toasts_ignore_auto_hide_but_can_be_dismissed() {
        let state = Rc::new(AppState {
            toast: ToastMessage {
                message: "Failed to save".to_string(),
                kind: ToastKind::Error,
                show: true,
                ..ToastMessage::default()
            },
            ..AppState::default()
        });

        let state = state.reduce(AppAction::AutoHideToast {
            message: "Failed to save".to_string(),
            detail: None,
            kind: ToastKind::Error,
        });
        assert!(state.toast.show);

        let state = state.reduce(AppAction::HideToast);
        assert!(!state.toast.show);
    }

    #[test]
    fn auto_hide_timer_cannot_hide_an_error_toast_that_replaced_it() {
        // A stale Info timer must not hide an error toast that replaced the
        // toast the timer was created for, even when messages match.
        let state = Rc::new(AppState {
            toast: ToastMessage {
                message: "The model request failed".to_string(),
                kind: ToastKind::Error,
                show: true,
                ..ToastMessage::default()
            },
            ..AppState::default()
        });

        let state = state.reduce(AppAction::AutoHideToast {
            message: "The model request failed".to_string(),
            detail: None,
            kind: ToastKind::Info,
        });
        assert!(state.toast.show);
    }

    #[test]
    fn stale_timer_does_not_hide_a_newer_toast() {
        let state = Rc::new(AppState {
            toast: ToastMessage {
                message: "Newer toast".to_string(),
                kind: ToastKind::Info,
                show: true,
                ..ToastMessage::default()
            },
            ..AppState::default()
        });

        let state = state.reduce(AppAction::AutoHideToast {
            message: "Older toast".to_string(),
            detail: None,
            kind: ToastKind::Info,
        });
        assert!(state.toast.show);
    }

    #[test]
    fn matching_info_timer_hides_its_own_toast() {
        let state = Rc::new(AppState {
            toast: ToastMessage {
                message: "Profile refreshed".to_string(),
                kind: ToastKind::Info,
                show: true,
                ..ToastMessage::default()
            },
            ..AppState::default()
        });

        let state = state.reduce(AppAction::AutoHideToast {
            message: "Profile refreshed".to_string(),
            detail: None,
            kind: ToastKind::Info,
        });
        assert!(!state.toast.show);
    }
}
