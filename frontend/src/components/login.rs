use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct LoginProps {
    pub on_login: Callback<()>,
}

#[function_component(LoginPanel)]
pub fn login_panel(props: &LoginProps) -> Html {
    let on_click = {
        let on_login = props.on_login.clone();
        Callback::from(move |_| on_login.emit(()))
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
                <input id="login-username" type="text" class="w-full rounded-md border px-3 py-2 outline-none focus:ring-2 focus:ring-[var(--primary)]" autocomplete="username" />
                
                <label class="mt-3 block card-kicker mb-2" for="login-password">{"Password"}</label>
                <input id="login-password" type="password" class="w-full rounded-md border px-3 py-2 outline-none focus:ring-2 focus:ring-[var(--primary)]" autocomplete="current-password" />
                
                <button onclick={on_click} id="login-btn" class="mt-4 w-full rounded-md bg-primary-solid px-3 py-2 font-medium">{"Login"}</button>
            </div>
        </section>
    }
}
