use crate::api::{toast, toast_key};
use crate::i18n::use_i18n;
use crate::passkeys::loginPasskey;
use crate::state::{AppAction, AppState, UserProfile};
use gloo_net::http::Request;
use serde::Serialize;
use web_sys::HtmlInputElement;
use yew::prelude::*;

#[derive(Serialize)]
struct LoginReq {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct SetupReq {
    username: String,
    password: String,
    admin_token: String,
}

#[function_component(LoginPanel)]
pub fn login_panel() -> Html {
    let app_state = use_context::<UseReducerHandle<AppState>>().unwrap();
    let i18n = use_i18n();
    let username = use_state(String::new);
    let password = use_state(String::new);
    let setup_token = use_state(String::new);
    let login_loading = use_state(|| false);
    let passkey_loading = use_state(|| false);

    let form_disabled = *login_loading || *passkey_loading;

    let on_passkey_login = {
        let app_state = app_state.clone();
        let i18n = i18n.clone();
        let passkey_loading = passkey_loading.clone();
        let login_loading = login_loading.clone();
        Callback::from(move |_| {
            if *passkey_loading || *login_loading {
                return;
            }

            let app_state = app_state.clone();
            let i18n = i18n.clone();
            let passkey_loading = passkey_loading.clone();
            passkey_loading.set(true);

            wasm_bindgen_futures::spawn_local(async move {
                let finish = || passkey_loading.set(false);

                // 1. Get challenge
                let challenge_res = match Request::post("/auth/passkeys/login/start").send().await {
                    Ok(r) if r.ok() => r.text().await.unwrap_or_default(),
                    _ => {
                        toast_key(&app_state, &i18n, "toast.failed_passkey_start");
                        finish();
                        return;
                    }
                };

                // 2. Call JS WebAuthn API
                let assertion_json = match loginPasskey(&challenge_res).await {
                    Ok(val) => val.as_string().unwrap_or_default(),
                    Err(e) => {
                        let err_msg = if let Some(err) = e.as_string() {
                            err
                        } else {
                            i18n.t("toast.passkey_error")
                        };
                        toast(&app_state, err_msg);
                        finish();
                        return;
                    }
                };

                // 3. Send response back
                match Request::post("/auth/passkeys/login/finish")
                    .header("Content-Type", "application/json")
                    .body(assertion_json)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        if let Ok(user_res) = Request::get("/auth/me").send().await {
                            if let Ok(user) = user_res.json::<UserProfile>().await {
                                app_state.dispatch(AppAction::SetSession(Some(user)));
                                toast_key(&app_state, &i18n, "toast.logged_in_passkey");
                            }
                        }
                    }
                    Ok(res) => {
                        let err = res
                            .text()
                            .await
                            .unwrap_or_else(|_| i18n.t("toast.failed_passkey_login"));
                        toast(&app_state, err);
                    }
                    Err(err) => {
                        toast(&app_state, err.to_string());
                    }
                }

                finish();
            });
        })
    };

    let on_login = {
        let app_state = app_state.clone();
        let username = username.clone();
        let password = password.clone();
        let setup_token = setup_token.clone();
        let i18n = i18n.clone();
        let login_loading = login_loading.clone();
        let passkey_loading = passkey_loading.clone();

        Callback::from(move |_| {
            if *login_loading || *passkey_loading {
                return;
            }

            let app_state = app_state.clone();
            let user = (*username).clone();
            let pass = (*password).clone();
            let token = (*setup_token).clone();
            let i18n = i18n.clone();
            let login_loading = login_loading.clone();

            login_loading.set(true);

            wasm_bindgen_futures::spawn_local(async move {
                let finish = || login_loading.set(false);

                let res = if !token.is_empty() {
                    let req = SetupReq {
                        username: user,
                        password: pass,
                        admin_token: token,
                    };
                    Request::post("/auth/setup")
                        .json(&req)
                        .unwrap()
                        .send()
                        .await
                } else {
                    let req = LoginReq {
                        username: user,
                        password: pass,
                    };
                    Request::post("/auth/login")
                        .json(&req)
                        .unwrap()
                        .send()
                        .await
                };

                match res {
                    Ok(res) if res.ok() => {
                        if let Ok(user_res) = Request::get("/auth/me").send().await {
                            if let Ok(user) = user_res.json::<UserProfile>().await {
                                app_state.dispatch(AppAction::SetSession(Some(user)));
                                toast_key(&app_state, &i18n, "toast.logged_in");
                            }
                        }
                    }
                    Ok(res) => {
                        let status = res.status();
                        let mut err = res.text().await.unwrap_or_default();
                        if err.is_empty() {
                            err = i18n.tf(
                                "format.http_error",
                                &[
                                    ("status", status.to_string()),
                                    ("status_text", res.status_text()),
                                ],
                            );
                        }
                        toast(&app_state, err);
                    }
                    Err(err) => {
                        toast(&app_state, err.to_string());
                    }
                }

                finish();
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
                        <h1 class="text-xl font-semibold tracking-tight">{i18n.t("app.name")}</h1>
                        <p class="text-sm text-muted">{i18n.t("auth.subtitle")}</p>
                    </div>
                </div>

                <label class="block card-kicker mb-2" for="login-username">{i18n.t("auth.username")}</label>
                <input id="login-username" oninput={on_username_input} value={(*username).clone()} type="text" disabled={form_disabled} class="w-full rounded-md border px-3 py-2 outline-none focus:ring-2 focus:ring-[var(--primary)] disabled:opacity-60" autocomplete="username" />

                <label class="mt-3 block card-kicker mb-2" for="login-password">{i18n.t("auth.password")}</label>
                <input id="login-password" oninput={on_password_input} value={(*password).clone()} type="password" disabled={form_disabled} class="w-full rounded-md border px-3 py-2 outline-none focus:ring-2 focus:ring-[var(--primary)] disabled:opacity-60" autocomplete="current-password" />

                <label class="mt-3 block card-kicker mb-2" for="login-setup-token">{i18n.t("auth.setup_token")}</label>
                <input id="login-setup-token" oninput={on_setup_input} value={(*setup_token).clone()} type="password" disabled={form_disabled} class="w-full rounded-md border px-3 py-2 outline-none focus:ring-2 focus:ring-[var(--primary)] disabled:opacity-60" />

                <button onclick={on_login} id="login-btn" disabled={form_disabled} class="mt-4 w-full rounded-md bg-primary-solid px-3 py-2 font-medium disabled:opacity-60">
                    if *login_loading {
                        {i18n.t("auth.signing_in")}
                    } else {
                        {i18n.t("auth.login_or_setup")}
                    }
                </button>
                <button onclick={on_passkey_login} type="button" disabled={form_disabled} class="mt-2 w-full rounded-md border border-token px-3 py-2 font-medium flex items-center justify-center gap-2 disabled:opacity-60">
                    <iconify-icon icon="radix-icons:lock-closed" class="radix-icon" aria-hidden="true"></iconify-icon>
                    <span>
                        if *passkey_loading {
                            {i18n.t("auth.waiting_passkey")}
                        } else {
                            {i18n.t("auth.passkey_login")}
                        }
                    </span>
                </button>
            </div>
        </section>
    }
}
