use crate::state::{AppAction, AppState, UserProfile};
use gloo_net::http::Request;
use web_sys::HtmlInputElement;
use yew::prelude::*;
use serde::Serialize;

#[derive(Serialize)]
struct LoginReq {
    username: String,
    password: String,
    setup_token: Option<String>,
}

#[function_component(LoginPanel)]
pub fn login_panel() -> Html {
    let app_state = use_context::<UseReducerHandle<AppState>>().unwrap();
    let username = use_state(String::new);
    let password = use_state(String::new);
    let setup_token = use_state(String::new);

    let on_login = {
        let app_state = app_state.clone();
        let username = (*username).clone();
        let password = (*password).clone();
        let setup_token = (*setup_token).clone();

        Callback::from(move |_| {
            let app_state = app_state.clone();
            let req = LoginReq {
                username: username.clone(),
                password: password.clone(),
                setup_token: if setup_token.is_empty() { None } else { Some(setup_token.clone()) },
            };

            wasm_bindgen_futures::spawn_local(async move {
                let endpoint = if req.setup_token.is_some() { "/auth/setup" } else { "/auth/login" };
                match Request::post(endpoint)
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        // Fetch user after login
                        if let Ok(user_res) = Request::get("/auth/me").send().await {
                            if let Ok(user) = user_res.json::<UserProfile>().await {
                                app_state.dispatch(AppAction::SetUser(Some(user)));
                                app_state.dispatch(AppAction::SetAuthed(true));
                                app_state.dispatch(AppAction::ShowToast("Logged in".to_string()));
                                
                                let state_clone = app_state.clone();
                                gloo_timers::callback::Timeout::new(2400, move || {
                                    state_clone.dispatch(AppAction::HideToast);
                                }).forget();
                            }
                        }
                    }
                    Ok(res) => {
                        let err = res.text().await.unwrap_or_else(|_| "Login failed".to_string());
                        app_state.dispatch(AppAction::ShowToast(err));
                        let state_clone = app_state.clone();
                        gloo_timers::callback::Timeout::new(2400, move || {
                            state_clone.dispatch(AppAction::HideToast);
                        }).forget();
                    }
                    Err(err) => {
                        app_state.dispatch(AppAction::ShowToast(err.to_string()));
                        let state_clone = app_state.clone();
                        gloo_timers::callback::Timeout::new(2400, move || {
                            state_clone.dispatch(AppAction::HideToast);
                        }).forget();
                    }
                }
            });
        })
    };

    let on_username_input = {
        let username = username.clone();
        Callback::from(move |e: InputEvent| {
            if let Some(target) = e.target_dyn_into::<HtmlInputElement>() {
                username.set(target.value());
            }
        })
    };

    let on_password_input = {
        let password = password.clone();
        Callback::from(move |e: InputEvent| {
            if let Some(target) = e.target_dyn_into::<HtmlInputElement>() {
                password.set(target.value());
            }
        })
    };

    let on_setup_input = {
        let setup_token = setup_token.clone();
        Callback::from(move |e: InputEvent| {
            if let Some(target) = e.target_dyn_into::<HtmlInputElement>() {
                setup_token.set(target.value());
            }
        })
    };

    html! {
        <section id="login-panel" class="min-h-screen flex items-center justify-center p-4">
            <div class="surface border rounded-md w-full max-w-md p-4">
                <div class="flex items-center gap-3 mb-4">
                    <iconify-icon icon="radix-icons:lightning-bolt" class="radix-icon text-primary text-4xl" aria-hidden="true"></iconify-icon>
                    <div>
                        <h1 class="text-xl font-semibold tracking-tight">{"Denpie"}</h1>
                        <p class="text-sm text-muted">{"Username and password"}</p>
                    </div>
                </div>
                
                <label class="block card-kicker mb-2" for="login-username">{"Username"}</label>
                <input id="login-username" oninput={on_username_input} value={(*username).clone()} type="text" class="w-full rounded-md border px-3 py-2 outline-none focus:ring-2 focus:ring-[var(--primary)]" autocomplete="username" />
                
                <label class="mt-3 block card-kicker mb-2" for="login-password">{"Password"}</label>
                <input id="login-password" oninput={on_password_input} value={(*password).clone()} type="password" class="w-full rounded-md border px-3 py-2 outline-none focus:ring-2 focus:ring-[var(--primary)]" autocomplete="current-password" />
                
                <label class="mt-3 block card-kicker mb-2" for="login-setup-token">{"Setup Token (First run only)"}</label>
                <input id="login-setup-token" oninput={on_setup_input} value={(*setup_token).clone()} type="password" class="w-full rounded-md border px-3 py-2 outline-none focus:ring-2 focus:ring-[var(--primary)]" />
                
                <button onclick={on_login} id="login-btn" class="mt-4 w-full rounded-md bg-primary-solid px-3 py-2 font-medium">{"Login / Setup"}</button>
            </div>
        </section>
    }
}
