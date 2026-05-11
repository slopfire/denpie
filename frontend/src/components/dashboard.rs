use yew::prelude::*;
use gloo_net::http::Request;
use serde::Deserialize;

#[derive(Deserialize, Clone, PartialEq)]
pub struct AppSummary {
    pub topics: i64,
    pub total_cards: i64,
    pub due_cards: i64,
    pub active_cards: i64,
}

#[derive(Deserialize, Clone, PartialEq)]
pub struct TokenSpend {
    pub daily: i64,
    pub monthly: i64,
    pub total: i64,
}

#[derive(Deserialize, Clone, PartialEq)]
pub struct AppTopicInfo {
    pub id: i64,
    pub name: String,
    pub tipcard_type: String,
    pub prompt_template: String,
    pub total_cards: i64,
    pub due_cards: i64,
    pub completed_cards: i64,
    pub daily_card_count: u32,
    pub daily_time_zone: String,
    pub daily_update_time: String,
    pub compression_level: String,
}

#[function_component(Dashboard)]
pub fn dashboard() -> Html {
    let summary = use_state(|| None::<AppSummary>);
    let token_spend = use_state(|| None::<TokenSpend>);
    let topics = use_state(Vec::<AppTopicInfo>::new);

    {
        let summary = summary.clone();
        let token_spend = token_spend.clone();
        let topics = topics.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(res) = Request::get("/app/summary").send().await {
                    if let Ok(data) = res.json::<AppSummary>().await {
                        summary.set(Some(data));
                    }
                }
                if let Ok(res) = Request::get("/admin/token-spend").send().await {
                    if let Ok(data) = res.json::<TokenSpend>().await {
                        token_spend.set(Some(data));
                    }
                }
                if let Ok(res) = Request::get("/app/topics").send().await {
                    if let Ok(data) = res.json::<Vec<AppTopicInfo>>().await {
                        topics.set(data);
                    }
                }
            });
            || ()
        });
    }

    html! {
        <section id="view-dashboard">
            <div class="mb-4">
                <h1 class="text-xl font-semibold tracking-tight">
                    {"Welcome back"}
                </h1>
                <p class="text-muted mt-2">
                    {"Cards, topics, and queue state from local server."}
                </p>
            </div>
            
            <div class="grid grid-cols-2 md:grid-cols-4 gap-2 sm:gap-3 mb-3 sm:mb-4" id="stats-grid">
                { if let Some(s) = &*summary {
                    html! {
                        <>
                            <div class="surface border rounded-md p-4 flex flex-col justify-center">
                                <div class="text-3xl font-bold text-primary">{s.due_cards}</div>
                                <div class="text-sm font-medium text-muted mt-1">{"Due Now"}</div>
                            </div>
                            <div class="surface border rounded-md p-4 flex flex-col justify-center">
                                <div class="text-3xl font-bold">{s.active_cards}</div>
                                <div class="text-sm font-medium text-muted mt-1">{"Active Queue"}</div>
                            </div>
                            <div class="surface border rounded-md p-4 flex flex-col justify-center">
                                <div class="text-3xl font-bold">{s.total_cards}</div>
                                <div class="text-sm font-medium text-muted mt-1">{"Total Cards"}</div>
                            </div>
                            <div class="surface border rounded-md p-4 flex flex-col justify-center">
                                <div class="text-3xl font-bold">{s.topics}</div>
                                <div class="text-sm font-medium text-muted mt-1">{"Topics"}</div>
                            </div>
                        </>
                    }
                } else {
                    html! { <div class="col-span-full surface border rounded-md p-4 text-center text-muted">{"Loading stats..."}</div> }
                }}
            </div>

            <div id="token-spend-row" class="grid grid-cols-1 sm:grid-cols-3 gap-2 sm:gap-3 mb-4">
                { if let Some(t) = &*token_spend {
                    html! {
                        <>
                            <div class="muted-surface border border-token rounded-md p-3 flex justify-between items-center">
                                <span class="text-sm font-medium text-muted">{"Daily Spend"}</span>
                                <span class="font-semibold text-primary">{format!("{} tokens", t.daily)}</span>
                            </div>
                            <div class="muted-surface border border-token rounded-md p-3 flex justify-between items-center">
                                <span class="text-sm font-medium text-muted">{"Monthly"}</span>
                                <span class="font-semibold">{format!("{} tokens", t.monthly)}</span>
                            </div>
                            <div class="muted-surface border border-token rounded-md p-3 flex justify-between items-center">
                                <span class="text-sm font-medium text-muted">{"Total Lifetime"}</span>
                                <span class="font-semibold">{format!("{} tokens", t.total)}</span>
                            </div>
                        </>
                    }
                } else {
                    html! {}
                }}
            </div>

            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-3 mb-4">
                <h2 class="text-xl font-semibold flex items-center gap-2">
                    <iconify-icon icon="radix-icons:layers" class="radix-icon text-primary" aria-hidden="true"></iconify-icon>
                    {"Active Topics"}
                </h2>
                <div class="flex gap-2">
                    <input id="topic-search" class="rounded-md border px-4 py-2 w-full sm:w-72" placeholder="Find topic" aria-label="Find topic" />
                    <button class="nav-shortcut rounded-md bg-primary-solid px-3 py-2 font-semibold">
                        <iconify-icon icon="radix-icons:plus" class="radix-icon" aria-hidden="true"></iconify-icon>
                    </button>
                </div>
            </div>

            <div id="topics-grid" class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4 gap-3">
                {
                    if topics.is_empty() {
                        html! { <div class="col-span-full surface border rounded-md p-10 text-center text-muted">{"No topics found."}</div> }
                    } else {
                        html! {
                            for topics.iter().map(|t| html! {
                                <div class="surface border rounded-md p-4 flex flex-col">
                                    <div class="flex justify-between items-start mb-2">
                                        <h3 class="font-semibold text-lg truncate">{&t.name}</h3>
                                        <span class="badge">{&t.tipcard_type}</span>
                                    </div>
                                    <div class="text-sm text-muted mt-auto">
                                        {format!("{} due / {} total", t.due_cards, t.total_cards)}
                                    </div>
                                </div>
                            })
                        }
                    }
                }
            </div>
        </section>
    }
}
