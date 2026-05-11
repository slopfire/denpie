use crate::markdown::render_markdown;
use crate::components::unified_flow::TipcardInfo;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct FlowCardProps {
    pub card: TipcardInfo,
    pub on_review: Callback<(i64, Option<u8>, Option<String>)>,
    pub on_toggle_pin: Callback<(i64, bool)>,
    pub on_delete: Callback<i64>,
    pub on_reorder: Callback<(i64, i64)>,
}

#[function_component(FlowCard)]
pub fn flow_card(props: &FlowCardProps) -> Html {
    let expanded = use_state(|| false);
    let card = &props.card;
    let id = card.id;
    let pinned = card.pinned;

    let toggle_expand = {
        let expanded = expanded.clone();
        Callback::from(move |_| expanded.set(!*expanded))
    };

    let has_compact = !card.compressed_content.is_empty() && card.compressed_content != card.full_content;
    let displayed_text = if !*expanded && has_compact {
        &card.compressed_content
    } else {
        &card.full_content
    };

    let html_content = render_markdown(displayed_text);
    
    let on_review = props.on_review.clone();
    let on_toggle_pin = props.on_toggle_pin.clone();
    let on_delete = props.on_delete.clone();
    let on_reorder = props.on_reorder.clone();

    let ondragstart = {
        let id = id;
        Callback::from(move |e: DragEvent| {
            if let Some(dt) = e.data_transfer() {
                let _ = dt.set_data("text/plain", &id.to_string());
                dt.set_drop_effect("move");
            }
        })
    };

    let ondragover = Callback::from(|e: DragEvent| {
        e.prevent_default();
        if let Some(dt) = e.data_transfer() {
            dt.set_drop_effect("move");
        }
    });

    let ondrop = {
        let id = id;
        Callback::from(move |e: DragEvent| {
            e.prevent_default();
            if let Some(dt) = e.data_transfer() {
                if let Ok(source_id_str) = dt.get_data("text/plain") {
                    if let Ok(source_id) = source_id_str.parse::<i64>() {
                        if source_id != id {
                            on_reorder.emit((source_id, id));
                        }
                    }
                }
            }
        })
    };

    html! {
        <div 
            class="surface border rounded-md p-4 flow-card relative flex flex-col h-full cursor-default"
            draggable="true"
            {ondragstart}
            {ondragover}
            {ondrop}
        >
            <div class={classes!("absolute", "top-0", "left-0", "w-1", "h-full", if pinned { "bg-primary" } else { "bg-muted" })}></div>
            <div class="flex justify-between items-start mb-2 gap-2">
                <div class="font-semibold text-sm line-clamp-2 flex-1 flex items-center gap-2">
                    <iconify-icon icon="radix-icons:drag-handle-dots-2" class="text-muted/50 cursor-grab active:cursor-grabbing"></iconify-icon>
                    {&card.title}
                </div>
                <button onclick={Callback::from(move |_| on_toggle_pin.emit((id, !pinned)))} class={classes!("shrink-0", if pinned { "text-primary" } else { "text-muted" })}>
                    <iconify-icon icon={if pinned { "radix-icons:drawing-pin-filled" } else { "radix-icons:drawing-pin" }}></iconify-icon>
                </button>
            </div>
            
            <div class="text-[10px] uppercase tracking-wider font-bold text-muted mb-3 flex items-center gap-2">
                <span>{&card.topic_name}</span>
                <span class="w-1 h-1 rounded-full bg-border"></span>
                <span>{&card.tipcard_type.replace("_", " ")}</span>
            </div>

            <div class="card-text-body markdown-content text-sm leading-6 flex-1 mb-4">
                { Html::from_html_unchecked(AttrValue::from(html_content)) }
                if has_compact {
                    <button onclick={toggle_expand} class="text-primary hover:underline font-medium ml-1 inline-flex items-center gap-0.5">
                        {if *expanded { "show less" } else { "...read more" }}
                    </button>
                }
            </div>

            if !card.image_data.is_empty() {
                <div class="flex gap-2 overflow-x-auto pb-2 mb-4 scrollbar-hide">
                    {
                        for card.image_data.iter().map(|img| html! {
                            <img src={img.clone()} class="h-16 w-16 rounded object-cover border border-token shrink-0" />
                        })
                    }
                </div>
            }

            <div class="mt-auto flex gap-2 flex-wrap pt-2 border-t border-token/50">
                if card.tipcard_type == "casual_tip" {
                    <button onclick={let on_review = on_review.clone(); Callback::from(move |_| on_review.emit((id, None, Some("acknowledge".to_string()))))} class="bg-primary-solid px-3 py-1.5 rounded text-xs font-medium flex-1">{"Got it"}</button>
                } else {
                    <button onclick={let on_review = on_review.clone(); Callback::from(move |_| on_review.emit((id, Some(1), None)))} class="border border-token px-2 py-1.5 rounded text-xs font-medium hover:bg-muted flex-1">{"Again"}</button>
                    <button onclick={let on_review = on_review.clone(); Callback::from(move |_| on_review.emit((id, Some(3), None)))} class="border border-token px-2 py-1.5 rounded text-xs font-medium hover:bg-muted flex-1">{"Good"}</button>
                    <button onclick={let on_review = on_review.clone(); Callback::from(move |_| on_review.emit((id, Some(5), None)))} class="bg-primary-solid px-2 py-1.5 rounded text-xs font-medium flex-1">{"Easy"}</button>
                }
                <button onclick={Callback::from(move |_| on_delete.emit(id))} class="text-danger hover:underline p-1 shrink-0" title="Delete card">
                    <iconify-icon icon="radix-icons:trash"></iconify-icon>
                </button>
            </div>
        </div>
    }
}
