//! Opt-in fixture page that renders the production `FlowCard` component.

use serde::Deserialize;
use yew::prelude::*;

use crate::components::flow_card::FlowCard;
use crate::components::unified_flow::TipcardInfo;

const CARD_FIXTURES: &str = include_str!("../../../lab/cases/cards/repeatable-states.json");

#[derive(Clone, Deserialize)]
struct CardFixture {
    id: String,
    topic_name: String,
    title: String,
    full_content: String,
    compressed_content: String,
    tipcard_type: String,
    status: String,
    pinned: bool,
    pending_count: u32,
    #[serde(default)]
    review_message: Option<String>,
    notes: String,
}

#[derive(Clone, PartialEq)]
struct LabCard {
    fixture_id: String,
    notes: String,
    card: TipcardInfo,
}

fn load_cards() -> Vec<LabCard> {
    let fixtures: Vec<CardFixture> =
        serde_json::from_str(CARD_FIXTURES).expect("checked-in card lab fixtures must parse");
    fixtures
        .into_iter()
        .enumerate()
        .map(|(index, fixture)| LabCard {
            fixture_id: fixture.id,
            notes: fixture.notes,
            card: TipcardInfo {
                id: i64::try_from(index + 1).expect("card lab fixture count fits i64"),
                topic_name: fixture.topic_name,
                topic_icon: "radix-icons:bookmark".to_string(),
                topic_color: "hsl(var(--primary-hsl))".to_string(),
                title: fixture.title,
                full_content: fixture.full_content,
                compressed_content: fixture.compressed_content,
                image_data: Vec::new(),
                created_at: String::new(),
                tipcard_type: fixture.tipcard_type,
                status: fixture.status,
                next_review_at: String::new(),
                repeat_count: 0,
                pinned: fixture.pinned,
                pending_count: fixture.pending_count,
                sources: Vec::new(),
                review_message: fixture.review_message,
            },
        })
        .collect()
}

#[function_component(CardLab)]
pub fn card_lab() -> Html {
    let cards = use_state(load_cards);
    let fullscreen_id = use_state(|| None::<i64>);

    {
        let fullscreen_active = fullscreen_id.is_some();
        use_effect_with(fullscreen_active, move |active| {
            if let Some(body) = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| document.body())
            {
                let _ = body
                    .class_list()
                    .toggle_with_force("has-fullscreen-card", *active);
            }
            move || {
                if let Some(body) = web_sys::window()
                    .and_then(|window| window.document())
                    .and_then(|document| document.body())
                {
                    let _ = body.class_list().remove_1("has-fullscreen-card");
                }
            }
        });
    }

    let on_toggle_fullscreen = {
        let fullscreen_id = fullscreen_id.clone();
        Callback::from(move |id: i64| {
            fullscreen_id.set(if *fullscreen_id == Some(id) {
                None
            } else {
                Some(id)
            });
        })
    };
    let on_toggle_pin = {
        let cards = cards.clone();
        Callback::from(move |(id, pinned): (i64, bool)| {
            let mut next = (*cards).clone();
            if let Some(item) = next.iter_mut().find(|item| item.card.id == id) {
                item.card.pinned = pinned;
            }
            cards.set(next);
        })
    };
    let on_delete = {
        let cards = cards.clone();
        Callback::from(move |id: i64| {
            cards.set(
                cards
                    .iter()
                    .filter(|item| item.card.id != id)
                    .cloned()
                    .collect(),
            );
        })
    };
    let on_review = {
        let cards = cards.clone();
        Callback::from(
            move |(id, _grade, action): (i64, Option<u8>, Option<String>)| {
                let mut next = (*cards).clone();
                if let Some(item) = next.iter_mut().find(|item| item.card.id == id) {
                    item.card.status = "reviewed".to_string();
                    item.card.review_message = Some(
                        action
                            .filter(|value| !value.is_empty())
                            .unwrap_or_else(|| "Review saved".to_string()),
                    );
                }
                cards.set(next);
            },
        )
    };
    let on_continue = {
        let cards = cards.clone();
        Callback::from(move |(id, _, _): (i64, String, String)| {
            let mut next = (*cards).clone();
            if let Some(item) = next.iter_mut().find(|item| item.card.id == id) {
                item.card.status = "active".to_string();
                item.card.review_message = None;
            }
            cards.set(next);
        })
    };

    html! {
        <main id="card-lab" class="min-h-screen bg-background px-4 py-8 text-foreground sm:px-6 lg:px-8">
            <div class="mx-auto max-w-7xl space-y-8">
                <header class="space-y-2 border-b border-token pb-5">
                    <p class="text-sm font-semibold text-primary">{"Denpie Lab"}</p>
                    <h1 class="text-3xl font-semibold">{"Production FlowCard fixtures"}</h1>
                    <p class="max-w-3xl text-sm leading-6 text-muted">
                        {"These checked-in states mount the same Yew component used by Transmission and Archive. Expand, review, pin, delete, inspect errors, and enter fullscreen without writing server data. Review and Continue callbacks are simulated locally; queue replacement and refill behavior remain covered by UnifiedFlow tests."}
                    </p>
                </header>

                <section class="grid grid-cols-1 gap-8 xl:grid-cols-2">
                    {for cards.iter().map(|item| {
                        let id = item.card.id;
                        html! {
                            <article class="space-y-3" data-lab-fixture={item.fixture_id.clone()}>
                                <div class="flex items-baseline justify-between gap-4">
                                    <h2 class="font-mono text-sm font-semibold">{&item.fixture_id}</h2>
                                    <span class="text-xs text-muted">{&item.notes}</span>
                                </div>
                                <FlowCard
                                    card={item.card.clone()}
                                    on_review={on_review.clone()}
                                    on_continue={on_continue.clone()}
                                    on_toggle_pin={on_toggle_pin.clone()}
                                    on_delete={on_delete.clone()}
                                    on_update_images={Callback::from(|_: (i64, Vec<String>)| ())}
                                    allow_image_mutation={false}
                                    on_toggle_fullscreen={on_toggle_fullscreen.clone()}
                                    list_mode={false}
                                    fullscreen={*fullscreen_id == Some(id)}
                                    detail_loaded={true}
                                    enable_measure={false}
                                />
                            </article>
                        }
                    })}
                </section>
            </div>
        </main>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_fixtures_map_to_production_cards() {
        let cards = load_cards();
        assert_eq!(cards.len(), 7);
        assert!(
            cards
                .iter()
                .all(|item| item.card.tipcard_type == "repeatable_tip")
        );
        assert!(cards.iter().any(|item| item.card.pending_count == 3));
        assert!(cards.iter().any(|item| item.card.review_message.is_some()));
        assert!(
            cards
                .iter()
                .any(|item| item.card.full_content.starts_with("LLM Error:"))
        );
    }
}
