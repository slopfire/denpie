use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, KeyboardEvent};
use yew::prelude::*;

#[derive(Clone, PartialEq)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

#[derive(Properties, PartialEq)]
pub struct ShadcnSelectProps {
    #[prop_or_default]
    pub id: AttrValue,
    #[prop_or_default]
    pub name: AttrValue,
    pub value: String,
    pub options: Vec<SelectOption>,
    pub onchange: Callback<String>,
    #[prop_or_default]
    pub class: Classes,
}

fn option_label(value: &str, options: &[SelectOption]) -> String {
    options
        .iter()
        .find(|option| option.value == value)
        .map(|option| option.label.clone())
        .unwrap_or_else(|| value.to_string())
}

fn position_menu(menu: &HtmlElement, trigger: &HtmlElement) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(inner_height) = window.inner_height() else {
        return;
    };
    let Some(inner_height) = inner_height.as_f64() else {
        return;
    };
    let trigger_rect = trigger.get_bounding_client_rect();
    let space_below = inner_height - trigger_rect.bottom();
    let space_above = trigger_rect.top();
    let menu_height = (menu.scroll_height() as f64).min(inner_height * 0.52);
    if space_below < menu_height + 12.0 && space_above > space_below {
        let _ = menu.class_list().add_1("opens-up");
    } else {
        let _ = menu.class_list().remove_1("opens-up");
    }
}

#[function_component(ShadcnSelect)]
pub fn shadcn_select(props: &ShadcnSelectProps) -> Html {
    let open = use_state(|| false);
    let wrapper_ref = use_node_ref();
    let trigger_ref = use_node_ref();
    let menu_ref = use_node_ref();

    let close = {
        let open = open.clone();
        Callback::from(move |_| open.set(false))
    };

    let onfocusout = {
        let open = open.clone();
        let wrapper_ref = wrapper_ref.clone();
        Callback::from(move |e: FocusEvent| {
            if let Some(wrapper) = wrapper_ref.cast::<HtmlElement>() {
                if let Some(related_target) = e.related_target() {
                    if let Ok(target_node) = related_target.dyn_into::<web_sys::Node>() {
                        let wrapper_node: web_sys::Node = wrapper.into();
                        if wrapper_node.contains(Some(&target_node)) {
                            return;
                        }
                    }
                }
            }
            open.set(false);
        })
    };

    let toggle = {
        let open = open.clone();
        let trigger_ref = trigger_ref.clone();
        let menu_ref = menu_ref.clone();
        Callback::from(move |_| {
            if *open {
                open.set(false);
                return;
            }
            open.set(true);
            if let (Some(menu), Some(trigger)) = (
                menu_ref.cast::<HtmlElement>(),
                trigger_ref.cast::<HtmlElement>(),
            ) {
                position_menu(&menu, &trigger);
            }
        })
    };

    let choose = {
        let onchange = props.onchange.clone();
        let close = close.clone();
        Callback::from(move |value: String| {
            onchange.emit(value);
            close.emit(());
        })
    };

    let on_trigger_keydown = {
        let open = open.clone();
        let trigger_ref = trigger_ref.clone();
        let menu_ref = menu_ref.clone();
        Callback::from(move |e: KeyboardEvent| {
            let key = e.key();
            if key == "ArrowDown" || key == "Enter" || key == " " {
                e.prevent_default();
                if !*open {
                    open.set(true);
                    if let (Some(menu), Some(trigger)) = (
                        menu_ref.cast::<HtmlElement>(),
                        trigger_ref.cast::<HtmlElement>(),
                    ) {
                        position_menu(&menu, &trigger);
                    }
                }
            }
        })
    };

    let display_label = option_label(&props.value, &props.options);
    let expanded = if *open { "true" } else { "false" };

    html! {
        <div
            ref={wrapper_ref}
            class={classes!("shadcn-select", props.class.clone())}
            {onfocusout}
        >
            <button
                ref={trigger_ref}
                type="button"
                id={props.id.clone()}
                name={props.name.clone()}
                class="shadcn-select-trigger"
                aria-haspopup="listbox"
                aria-expanded={expanded}
                onclick={toggle}
                onkeydown={on_trigger_keydown}
            >
                <span class="shadcn-select-label">{display_label}</span>
                <iconify-icon icon="radix-icons:chevron-down" class="radix-icon" aria-hidden="true"></iconify-icon>
            </button>
            <div
                ref={menu_ref}
                class={classes!("shadcn-select-menu", (!*open).then_some("hidden"))}
                role="listbox"
            >
                {for props.options.iter().map(|option| {
                    let selected = option.value == props.value;
                    let choose = choose.clone();
                    let close = close.clone();
                    let value = option.value.clone();
                    let on_option_keydown = Callback::from(move |e: KeyboardEvent| {
                        let key = e.key();
                        if key == "Escape" {
                            e.prevent_default();
                            close.emit(());
                        }
                    });
                    html! {
                        <button
                            type="button"
                            role="option"
                            class={classes!("shadcn-select-option", selected.then_some("is-selected"))}
                            data-value={option.value.clone()}
                            aria-selected={if selected { "true" } else { "false" }}
                            onclick={Callback::from(move |_| choose.emit(value.clone()))}
                            onkeydown={on_option_keydown}
                        >
                            <span class="shadcn-select-option-check">
                                <iconify-icon icon="radix-icons:check" class="radix-icon" aria-hidden="true"></iconify-icon>
                            </span>
                            <span class="shadcn-select-label">{option.label.clone()}</span>
                        </button>
                    }
                })}
            </div>
        </div>
    }
}
