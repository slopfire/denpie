use yew::prelude::*;

/// Button visual style. Mirrors shadcn/ui variants.
#[derive(Clone, Copy, PartialEq, Default)]
#[allow(dead_code)]
pub enum ButtonVariant {
    #[default]
    Default,
    Destructive,
    Outline,
    Secondary,
    Ghost,
    Link,
}

/// Button size. Mirrors shadcn/ui sizes.
#[derive(Clone, Copy, PartialEq, Default)]
#[allow(dead_code)]
pub enum ButtonSize {
    #[default]
    Default,
    Sm,
    Lg,
    Icon,
}

impl ButtonVariant {
    fn classes(self) -> &'static str {
        match self {
            Self::Default => "bg-primary text-primary-foreground hover:bg-primary/90",
            Self::Destructive => {
                "bg-destructive text-destructive-foreground hover:bg-destructive/90"
            }
            Self::Outline => {
                "border border-input bg-background hover:bg-accent hover:text-accent-foreground"
            }
            Self::Secondary => "bg-secondary text-secondary-foreground hover:bg-secondary/80",
            Self::Ghost => "hover:bg-accent hover:text-accent-foreground",
            Self::Link => "text-primary underline-offset-4 hover:underline",
        }
    }
}

impl ButtonSize {
    fn classes(self) -> &'static str {
        match self {
            Self::Default => "h-10 px-4 py-2",
            Self::Sm => "h-9 px-3",
            Self::Lg => "h-11 px-8",
            Self::Icon => "size-10",
        }
    }
}

#[derive(Properties, PartialEq)]
pub struct ShadcnButtonProps {
    #[prop_or_default]
    pub id: AttrValue,
    #[prop_or_default]
    pub name: AttrValue,
    #[prop_or(ButtonType::Button)]
    pub r#type: ButtonType,
    #[prop_or_default]
    pub variant: ButtonVariant,
    #[prop_or_default]
    pub size: ButtonSize,
    #[prop_or_default]
    pub disabled: bool,
    #[prop_or_default]
    pub onclick: Callback<MouseEvent>,
    #[prop_or_default]
    pub class: Classes,
    #[prop_or_default]
    pub children: Children,
}

#[derive(Clone, Copy, PartialEq, Default)]
#[allow(dead_code)]
pub enum ButtonType {
    #[default]
    Button,
    Submit,
    Reset,
}

impl ButtonType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Button => "button",
            Self::Submit => "submit",
            Self::Reset => "reset",
        }
    }
}

/// shadcn/ui Button port for Yew.
///
/// Renders a `<button>` with shadcn variant/size classes. Pass extra layout
/// classes via `class`; they merge with the variant base. Semantic color tokens
/// (`bg-primary`, `text-destructive-foreground`, …) come from the `@theme inline`
/// block in `frontend/index.html`.
#[function_component(ShadcnButton)]
pub fn shadcn_button(props: &ShadcnButtonProps) -> Html {
    let base = "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50";

    html! {
        <button
            id={props.id.clone()}
            name={props.name.clone()}
            type={props.r#type.as_str()}
            disabled={props.disabled}
            onclick={props.onclick.clone()}
            class={classes!(
                base,
                props.variant.classes(),
                props.size.classes(),
                props.class.clone(),
            )}
        >
            {props.children.clone()}
        </button>
    }
}
