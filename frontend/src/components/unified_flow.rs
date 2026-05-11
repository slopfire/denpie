use yew::prelude::*;
use gloo_net::http::Request;
use serde::Deserialize;

#[derive(Deserialize, Clone, PartialEq)]
pub struct TipcardInfo {
    pub id: i64,
    pub topic_name: String,
    pub title: String,
    pub full_content: String,
    pub compressed_content: String,
    pub image_data: Vec<String>,
    pub created_at: String,
    pub tipcard_type: String,
    pub status: String,
    pub next_review_at: String,
    pub repeat_count: u32,
    pub pinned: bool,
}

#[function_component(UnifiedFlow)]
pub fn unified_flow() -> Html {
    let cards = use_state(Vec::<TipcardInfo>::new);

    {
        let cards = cards.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(res) = Request::get("/admin/tipcards").send().await {
                    if let Ok(data) = res.json::<Vec<TipcardInfo>>().await {
                        cards.set(data);
                    }
                }
            });
            || ()
        });
    }

    let is_empty = cards.is_empty();
    let pinned_cards: Vec<_> = cards.iter().filter(|c| c.pinned).cloned().collect();
    let unpinned_cards: Vec<_> = cards.iter().filter(|c| !c.pinned).cloned().collect();

    html! {
        <section id="view-flow">
            <div class="flex flex-col xl:flex-row xl:items-end justify-between gap-3 mb-4">
                <div>
                    <h1 class="text-xl font-semibold tracking-tight">
                        {"Unified Flow"}
                    </h1>
                    <p class="text-muted mt-2">
                        {"All cards in one review surface."}
                    </p>
                </div>
                <form id="tips-form" class="surface border rounded-md p-4 grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-5 gap-3 w-full xl:w-auto">
                    <input id="tips-topics" class="rounded-md border px-3 py-2 xl:col-span-2" placeholder="Rust, Python, System Design" aria-label="Topics" required=true />
                    <input id="tips-type" type="hidden" value="casual_tip" />
                    <div class="tip-type-switch muted-surface border border-token rounded-md p-1 grid grid-cols-3 sm:col-span-2" role="group" aria-label="Card class">
                        <button type="button" data-tip-type="casual_tip" class="active rounded-md px-3 py-2 text-sm font-medium">{"Casual"}</button>
                        <button type="button" data-tip-type="repeatable_tip" class="rounded-md px-3 py-2 text-sm font-medium">{"Repeat"}</button>
                        <button type="button" data-tip-type="manual_tip" class="rounded-md px-3 py-2 text-sm font-medium">{"Manual"}</button>
                    </div>
                    <button class="rounded-md bg-primary-solid px-4 py-2 font-medium flex items-center justify-center gap-2">
                        <iconify-icon id="tips-submit-icon" icon="radix-icons:magic-wand" class="radix-icon" aria-hidden="true"></iconify-icon>
                        <span id="tips-submit-label">{"Add"}</span>
                    </button>
                    <textarea id="manual-card-content" class="hidden rounded-md border px-3 py-2 sm:col-span-2 xl:col-span-5 h-20 resize-y" placeholder="Write manual card text"></textarea>
                    <div id="manual-card-images-row" class="hidden sm:col-span-2 xl:col-span-5 flex flex-wrap items-center gap-3">
                        <label class="inline-flex items-center gap-2 rounded-md border border-token px-3 py-2 text-sm font-medium cursor-pointer">
                            <iconify-icon icon="radix-icons:image" class="radix-icon" aria-hidden="true"></iconify-icon>
                            <span>{"Attach images"}</span>
                            <input id="manual-card-images" type="file" accept="image/*" multiple=true class="hidden" />
                        </label>
                        <span id="manual-card-images-count" class="text-sm text-muted">{"No images"}</span>
                    </div>
                </form>
            </div>
            <div class="flex justify-between items-center gap-3 mb-4">
                <div class="text-sm text-muted">
                    <span id="flow-count">{cards.len()}</span>{" cards"}
                </div>
                <div class="flex muted-surface rounded-md p-1 border border-token">
                    <button id="flow-grid-btn" type="button" class="rounded px-2 py-1 bg-primary-soft text-primary" title="Grid view">
                        <iconify-icon icon="radix-icons:grid" class="radix-icon" aria-hidden="true"></iconify-icon>
                    </button>
                    <button id="flow-list-btn" type="button" class="rounded px-2 py-1" title="Column view">
                        <iconify-icon icon="radix-icons:list-bullet" class="radix-icon" aria-hidden="true"></iconify-icon>
                    </button>
                </div>
            </div>
            
            if !pinned_cards.is_empty() {
                <div id="pinned-flow-section" class="mb-4">
                    <div class="flex items-center gap-2 mb-3 text-sm font-medium text-primary">
                        <iconify-icon icon="radix-icons:drawing-pin-filled" class="radix-icon" aria-hidden="true"></iconify-icon>
                        <span>{"Pinned"}</span>
                    </div>
                    <div id="pinned-flow-grid" class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4 gap-3 items-start">
                        {
                            for pinned_cards.iter().map(|c| html! {
                                <div class="surface border rounded-md p-4 flow-card">
                                    <div class="font-semibold">{&c.title}</div>
                                </div>
                            })
                        }
                    </div>
                </div>
            }

            <div id="flow-grid" class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4 gap-3">
                {
                    for unpinned_cards.iter().map(|c| html! {
                        <div class="surface border rounded-md p-4 flow-card">
                            <div class="font-semibold">{&c.title}</div>
                        </div>
                    })
                }
            </div>
            
            if is_empty {
                <div id="empty-flow" class="surface border rounded-md p-10 text-center text-muted">
                    {"No cards yet."}
                </div>
            }
        </section>
    }
}
