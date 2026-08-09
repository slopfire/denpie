//! Topic editor dialog and source helpers.
use super::documents::*;
use super::types::*;
use crate::api::toast;
use crate::api_v1;
use crate::components::button::{ButtonSize, ButtonType, ButtonVariant, ShadcnButton};
use crate::components::select::{SelectOption, ShadcnSelect};
use crate::i18n::{I18n, use_i18n};
use crate::state::AppState;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{FileReader, HtmlInputElement, HtmlTextAreaElement};
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct TopicEditorProps {
    pub topic: AppTopicInfo,
    pub documents: Vec<DocumentInfo>,
    pub sources_loaded: bool,
    pub on_refresh_documents: Callback<()>,
    pub on_view_document: Callback<i64>,
    pub on_close: Callback<()>,
    pub on_saved: Callback<()>,
}

fn source_links(input: &str) -> Vec<String> {
    let values = input.split_whitespace().collect::<Vec<_>>();
    if values.is_empty()
        || values
            .iter()
            .any(|value| !value.starts_with("https://") && !value.starts_with("http://"))
    {
        Vec::new()
    } else {
        values.into_iter().map(str::to_string).collect()
    }
}

fn link_title(url: &str) -> String {
    url.trim_end_matches('/')
        .strip_prefix("https://")
        .or_else(|| url.trim_end_matches('/').strip_prefix("http://"))
        .unwrap_or(url)
        .chars()
        .take(100)
        .collect()
}

fn source_detection_label(input: &str) -> String {
    let links = source_links(input);
    if links.is_empty() {
        "Detected: pasted document".to_string()
    } else if links.len() == 1 {
        "Detected: 1 link".to_string()
    } else {
        format!("Detected: {} links", links.len())
    }
}

fn source_button_label(input: &str, adding: bool) -> &'static str {
    if adding {
        "Adding..."
    } else if source_links(input).len() > 1 {
        "Add Links"
    } else if source_links(input).len() == 1 {
        "Add Link"
    } else {
        "Add Document"
    }
}

#[function_component(TopicEditor)]
pub fn topic_editor(props: &TopicEditorProps) -> Html {
    let app_state = use_context::<UseReducerHandle<AppState>>().unwrap();
    let i18n = use_i18n();
    let prompt_template = use_state(|| props.topic.prompt_template.clone());
    let daily_card_count = use_state(|| props.topic.daily_card_count.to_string());
    let daily_time_zone = use_state(|| props.topic.daily_time_zone.clone());
    let daily_update_time = use_state(|| props.topic.daily_update_time.clone());
    let compression_level = use_state(|| props.topic.compression_level.clone());
    let grounding_strategy = use_state(|| props.topic.grounding_strategy.clone());
    let image_strategy = use_state(|| props.topic.image_strategy.clone());
    let source_input = use_state(String::new);
    let source_adding = use_state(|| false);
    let source_exploring = use_state(|| false);
    let doc_file_data = use_state(String::new);
    let doc_file_name = use_state(String::new);
    let doc_file_uploading = use_state(|| false);
    let doc_file_input_ref = use_node_ref();

    let on_submit = {
        let app_state = app_state.clone();
        let on_saved = props.on_saved.clone();
        let topic_id = props.topic.id;
        let prompt_template = prompt_template.clone();
        let daily_card_count = daily_card_count.clone();
        let daily_time_zone = daily_time_zone.clone();
        let daily_update_time = daily_update_time.clone();
        let compression_level = compression_level.clone();
        let grounding_strategy = grounding_strategy.clone();
        let image_strategy = image_strategy.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let app_state = app_state.clone();
            let on_saved = on_saved.clone();
            let prompt_template = Some((*prompt_template).clone());
            let daily_card_count = Some(daily_card_count.parse().unwrap_or(0));
            let daily_time_zone = Some((*daily_time_zone).clone());
            let daily_update_time = Some((*daily_update_time).clone());
            let compression_level = Some((*compression_level).clone());
            let grounding_strategy = Some((*grounding_strategy).clone());
            let image_strategy = Some((*image_strategy).clone());
            wasm_bindgen_futures::spawn_local(async move {
                match api_v1::update_topic(
                    topic_id,
                    prompt_template,
                    daily_card_count,
                    daily_time_zone,
                    daily_update_time,
                    compression_level,
                    grounding_strategy,
                    image_strategy,
                )
                .await
                {
                    Ok(()) => {
                        toast(&app_state, "Topic saved");
                        on_saved.emit(());
                    }
                    Err(err) => toast(&app_state, err.to_string()),
                }
            });
        })
    };

    let refresh_documents = props.on_refresh_documents.clone();
    let on_add_document = {
        let app_state = app_state.clone();
        let source_input = source_input.clone();
        let source_adding = source_adding.clone();
        let refresh_documents = refresh_documents.clone();
        let topic_id = props.topic.id;
        Callback::from(move |_: MouseEvent| {
            let input = source_input.trim().to_string();
            if input.is_empty() {
                return;
            }
            let links = source_links(&input);
            // (topic_ids, source_type, title, url, content)
            let requests: Vec<(Vec<i64>, String, String, Option<String>, String)> =
                if links.is_empty() {
                    vec![(
                        vec![topic_id],
                        "document".to_string(),
                        String::new(),
                        None,
                        input,
                    )]
                } else {
                    links
                        .into_iter()
                        .map(|url| {
                            (
                                vec![topic_id],
                                "link".to_string(),
                                link_title(&url),
                                Some(url),
                                String::new(),
                            )
                        })
                        .collect()
                };
            let app_state = app_state.clone();
            let source_input = source_input.clone();
            let source_adding = source_adding.clone();
            let refresh_documents = refresh_documents.clone();
            source_adding.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                let count = requests.len();
                for (topic_ids, source_type, title, url, content) in requests {
                    if let Err(err) =
                        api_v1::create_document(topic_ids, source_type, title, url, content).await
                    {
                        toast(&app_state, err.to_string());
                        source_adding.set(false);
                        return;
                    }
                }
                toast(
                    &app_state,
                    if count == 1 {
                        "Source added to topic"
                    } else {
                        "Sources added to topic"
                    },
                );
                source_input.set(String::new());
                source_adding.set(false);
                refresh_documents.emit(());
            });
        })
    };

    let on_explore_link = {
        let app_state = app_state.clone();
        let source_input = source_input.clone();
        let source_exploring = source_exploring.clone();
        Callback::from(move |_: MouseEvent| {
            let links = source_links(source_input.trim());
            if links.len() != 1 {
                return;
            }
            let url = links[0].clone();
            let app_state = app_state.clone();
            let source_input = source_input.clone();
            let source_exploring = source_exploring.clone();
            source_exploring.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                match api_v1::explore_link(url).await {
                    Ok(links) => {
                        let count = links.len();
                        source_input.set(
                            links
                                .into_iter()
                                .map(|link| link.url)
                                .collect::<Vec<_>>()
                                .join("\n"),
                        );
                        toast(&app_state, format!("Found {count} documentation pages"));
                    }
                    Err(err) => toast(&app_state, err.to_string()),
                }
                source_exploring.set(false);
            });
        })
    };

    let on_file_selected = {
        let data = doc_file_data.clone();
        let name = doc_file_name.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            if let Some(file) = input.files().and_then(|files| files.get(0)) {
                name.set(file.name());
                let reader = FileReader::new().unwrap();
                let reader_clone = reader.clone();
                let data = data.clone();
                let closure = Closure::wrap(Box::new(move || {
                    if let Ok(result) = reader_clone.result() {
                        if let Some(value) = result.as_string() {
                            data.set(value);
                        }
                    }
                }) as Box<dyn FnMut()>);
                reader.set_onloadend(Some(closure.as_ref().unchecked_ref()));
                let _ = reader.read_as_data_url(&file);
                closure.forget();
            }
        })
    };

    let on_upload_file = {
        let app_state = app_state.clone();
        let data = doc_file_data.clone();
        let name = doc_file_name.clone();
        let uploading = doc_file_uploading.clone();
        let refresh_documents = refresh_documents.clone();
        let topic_id = props.topic.id;
        Callback::from(move |_: MouseEvent| {
            if (*data).is_empty() || (*name).is_empty() {
                toast(&app_state, "Choose a file first");
                return;
            }
            uploading.set(true);
            let filename = (*name).clone();
            let data_url = (*data).clone();
            let app_state = app_state.clone();
            let data = data.clone();
            let name = name.clone();
            let uploading = uploading.clone();
            let refresh_documents = refresh_documents.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let upload = async {
                    let (mime_type, bytes) = api_v1::decode_data_url(&data_url)?;
                    api_v1::upload_document(vec![topic_id], filename, mime_type, None, bytes).await
                };
                match upload.await {
                    Ok(_) => {
                        toast(&app_state, "File uploaded to topic");
                        data.set(String::new());
                        name.set(String::new());
                        refresh_documents.emit(());
                    }
                    Err(_) => toast(&app_state, "Failed to upload file"),
                }
                uploading.set(false);
            });
        })
    };

    let on_attach = {
        let app_state = app_state.clone();
        let refresh_documents = refresh_documents.clone();
        let topic_id = props.topic.id;
        Callback::from(move |id: i64| {
            let app_state = app_state.clone();
            let refresh_documents = refresh_documents.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match api_v1::attach_document_topic(id, topic_id).await {
                    Ok(()) => refresh_documents.emit(()),
                    Err(_) => toast(&app_state, "Failed to attach source"),
                }
            });
        })
    };
    let on_detach = {
        let app_state = app_state.clone();
        let refresh_documents = refresh_documents.clone();
        let topic_id = props.topic.id;
        Callback::from(move |id: i64| {
            let app_state = app_state.clone();
            let refresh_documents = refresh_documents.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match api_v1::detach_document_topic(id, topic_id).await {
                    Ok(()) => refresh_documents.emit(()),
                    Err(_) => toast(&app_state, "Failed to detach source"),
                }
            });
        })
    };

    html! {
        <div class="fixed inset-0 z-[80] bg-black/60 p-4 flex items-center justify-center">
            <form onsubmit={on_submit} class="surface border rounded-md w-full max-w-2xl p-4 space-y-4 max-h-[90vh] overflow-y-auto">
                <div class="flex items-start justify-between gap-3">
                    <div>
                        <h2 class="text-lg font-semibold">{format!("Topic: {}", props.topic.name)}</h2>
                        <p class="text-sm text-muted">{tip_type_label(&i18n, &props.topic.tipcard_type)}</p>
                    </div>
                    <ShadcnButton
                        variant={ButtonVariant::Outline}
                        size={ButtonSize::Sm}
                        onclick={Callback::from({ let on_close = props.on_close.clone(); move |_: MouseEvent| on_close.emit(()) })}
                    >
                        {"Close"}
                    </ShadcnButton>
                </div>
                <div>
                    <label class="block card-kicker mb-2">{"Prompt Template"}</label>
                    <textarea value={(*prompt_template).clone()} oninput={Callback::from({ let state = prompt_template.clone(); move |e: InputEvent| if let Some(t) = e.target_dyn_into::<HtmlTextAreaElement>() { state.set(t.value()); }})} class="w-full rounded-md border px-3 py-2 h-24 resize-y"></textarea>
                </div>
                <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                    <div>
                        <label class="block card-kicker mb-2">{"Daily Card Count"}</label>
                        <input value={(*daily_card_count).clone()} oninput={Callback::from({ let state = daily_card_count.clone(); move |e: InputEvent| if let Some(t) = e.target_dyn_into::<HtmlInputElement>() { state.set(t.value()); }})} type="number" min="0" class="w-full rounded-md border px-3 py-2" />
                    </div>
                    <div>
                        <label class="block card-kicker mb-2">{"Compression Level"}</label>
                        <ShadcnSelect
                            value={(*compression_level).clone()}
                            onchange={Callback::from({
                                let state = compression_level.clone();
                                move |value: String| state.set(value)
                            })}
                            options={vec![
                                SelectOption { value: "light".into(), label: "Light".into() },
                                SelectOption { value: "balanced".into(), label: "Balanced".into() },
                                SelectOption { value: "strong".into(), label: "Strong".into() },
                                SelectOption { value: "ultra".into(), label: "Ultra".into() },
                            ]}
                        />
                    </div>
                    <div>
                        <label class="block card-kicker mb-2">{"Grounding Strategy"}</label>
                        <ShadcnSelect
                            value={(*grounding_strategy).clone()}
                            onchange={Callback::from({
                                let state = grounding_strategy.clone();
                                move |value: String| state.set(value)
                            })}
                            options={vec![
                                SelectOption { value: "".into(), label: "Inherit from settings".into() },
                                SelectOption { value: "factual".into(), label: "Factual".into() },
                                SelectOption { value: "create_and_ground".into(), label: "Factcheck".into() },
                                SelectOption { value: "agentic".into(), label: "Agentic".into() },
                                SelectOption { value: "rag".into(), label: "From My Data".into() },
                            ]}
                        />
                    </div>
                    if *grounding_strategy == "rag" {
                        <div class="md:col-span-2 surface border border-token rounded-md p-4 space-y-4">
                            <div>
                                <h3 class="font-semibold">{"Topic Sources"}</h3>
                                <p class="text-sm text-muted">{"Add sources for this topic, or attach one you have already added elsewhere. Sources can be detached without deleting them."}</p>
                            </div>
                            <div class="space-y-3">
                                <div class="relative">
                                    <label class="block card-kicker mb-2" for="topic-source-input">{"Paste a document or links"}</label>
                                    <textarea id="topic-source-input" value={(*source_input).clone()} oninput={Callback::from({ let state = source_input.clone(); move |e: InputEvent| if let Some(t) = e.target_dyn_into::<HtmlTextAreaElement>() { state.set(t.value()); }})} class="w-full rounded-md border px-3 py-2 min-h-28 resize-y" placeholder="Paste document text, or one or more https:// links…"></textarea>
                                    if !source_input.trim().is_empty() && props.documents.iter().any(|doc| !doc.topic_ids.contains(&props.topic.id) && doc.title.to_lowercase().contains(&source_input.trim().to_lowercase())) {
                                        <div id="existing-source-matches" class="absolute z-10 mt-1 w-full surface border border-token rounded-md shadow-lg overflow-hidden">
                                            {for props.documents.iter().filter(|doc| !doc.topic_ids.contains(&props.topic.id) && doc.title.to_lowercase().contains(&source_input.trim().to_lowercase())).take(5).map(|doc| {
                                                let id = doc.id; let on_attach = on_attach.clone(); let source_input = source_input.clone();
                                                html! { <button type="button" class="w-full flex items-center gap-2 px-3 py-2 text-left hover:bg-muted/20" onclick={Callback::from(move |_: MouseEvent| { on_attach.emit(id); source_input.set(String::new()); })}><span class="truncate flex-1 text-sm">{&doc.title}</span><span class="text-xs text-muted">{"Attach existing"}</span></button> }
                                            })}
                                        </div>
                                    }
                                </div>
                                <div class="flex items-center justify-between gap-3">
                                    <span id="topic-source-detection" class="text-xs text-muted">{source_detection_label(&source_input)}</span>
                                    <div class="flex items-center gap-2">
                                        if source_links(&source_input).len() == 1 {
                                            <ShadcnButton id="explore-topic-source" variant={ButtonVariant::Secondary} r#type={ButtonType::Button} onclick={on_explore_link} disabled={*source_adding || *source_exploring}><iconify-icon icon="radix-icons:magnifying-glass" class="radix-icon" aria-hidden="true"></iconify-icon>{if *source_exploring { "Exploring..." } else { "Explore Link" }}</ShadcnButton>
                                        }
                                        <ShadcnButton id="add-topic-source" r#type={ButtonType::Button} onclick={on_add_document} disabled={source_input.trim().is_empty() || *source_adding || *source_exploring}><iconify-icon icon="radix-icons:plus" class="radix-icon" aria-hidden="true"></iconify-icon>{source_button_label(&source_input, *source_adding)}</ShadcnButton>
                                    </div>
                                </div>
                            </div>
                            <div class="border-t border-token pt-3 flex items-center gap-3">
                                <input type="file" accept=".pdf,.doc,.docx,.docm,.ppt,.pptx,.pptm,.pps,.ppsx,.xls,.xlsx,.xlsm,.xlsb,.odt,.ods,.odp,.rtf,.epub,.csv,.html,.htm,.txt,.md,text/plain,text/html,text/csv,text/markdown,application/pdf,application/msword,application/vnd.openxmlformats-officedocument.wordprocessingml.document,application/vnd.ms-powerpoint,application/vnd.openxmlformats-officedocument.presentationml.presentation,application/vnd.ms-excel,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet,application/rtf,application/epub+zip" class="hidden" ref={doc_file_input_ref.clone()} onchange={on_file_selected} />
                                <ShadcnButton variant={ButtonVariant::Secondary} r#type={ButtonType::Button} onclick={Callback::from({ let input = doc_file_input_ref.clone(); move |_: MouseEvent| if let Some(input) = input.cast::<HtmlInputElement>() { let _ = input.click(); }})} disabled={*doc_file_uploading}>{"Choose File"}</ShadcnButton>
                                <span class="text-sm text-muted truncate flex-1">{if (*doc_file_name).is_empty() { "No file chosen".to_string() } else { (*doc_file_name).clone() }}</span>
                                if !(*doc_file_data).is_empty() { <ShadcnButton variant={ButtonVariant::Default} r#type={ButtonType::Button} onclick={on_upload_file} disabled={*doc_file_uploading}>{if *doc_file_uploading { "Uploading…" } else { "Upload" }}</ShadcnButton> }
                            </div>
                            <div class="border-t border-token pt-3 space-y-3">
                                <div class="text-sm font-medium">{"Assigned to this topic"}</div>
                                if !props.sources_loaded { <p class="text-sm text-muted">{"Loading sources…"}</p> }
                                else {
                                    <div class="space-y-2">
                                        {for props.documents.iter().filter(|doc| doc.topic_ids.contains(&props.topic.id)).map(|doc| {
                                            let id = doc.id; let on_detach = on_detach.clone(); let on_view = props.on_view_document.clone();
                                            html! { <div class="flex items-center gap-2 p-2 rounded-md border border-token"><iconify-icon icon="radix-icons:file-text" class="radix-icon text-muted" aria-hidden="true"></iconify-icon><span class="truncate flex-1 text-sm">{&doc.title}</span><button type="button" class="text-xs underline" onclick={Callback::from(move |_: MouseEvent| on_view.emit(id))}>{"View"}</button><button type="button" class="text-xs text-muted hover:text-destructive" onclick={Callback::from(move |_: MouseEvent| on_detach.emit(id))}>{"Detach"}</button></div> }
                                        })}
                                        if props.documents.iter().all(|doc| !doc.topic_ids.contains(&props.topic.id)) { <p class="text-sm text-muted italic">{"No sources assigned yet."}</p> }
                                    </div>
                                }
                            </div>
                        </div>
                    }
                    <div>
                        <label class="block card-kicker mb-2">{"Image Strategy"}</label>
                        <ShadcnSelect
                            value={(*image_strategy).clone()}
                            onchange={Callback::from({
                                let state = image_strategy.clone();
                                move |value: String| state.set(value)
                            })}
                            options={vec![
                                SelectOption { value: "".into(), label: "Inherit from settings".into() },
                                SelectOption { value: "none".into(), label: "None".into() },
                                SelectOption { value: "pool".into(), label: "Local Image Pool".into() },
                                SelectOption { value: "programmatic".into(), label: "Tag-based Image APIs".into() },
                                SelectOption { value: "web_search".into(), label: "Web Image Search".into() },
                                SelectOption { value: "agentic".into(), label: "Isolated Image Search".into() },
                            ]}
                        />
                    </div>
                    <div>
                        <label class="block card-kicker mb-2">{"Time Zone"}</label>
                        <input value={(*daily_time_zone).clone()} oninput={Callback::from({ let state = daily_time_zone.clone(); move |e: InputEvent| if let Some(t) = e.target_dyn_into::<HtmlInputElement>() { state.set(t.value()); }})} class="w-full rounded-md border px-3 py-2" />
                    </div>
                    <div>
                        <label class="block card-kicker mb-2">{"Update Time"}</label>
                        <input value={(*daily_update_time).clone()} oninput={Callback::from({ let state = daily_update_time.clone(); move |e: InputEvent| if let Some(t) = e.target_dyn_into::<HtmlInputElement>() { state.set(t.value()); }})} type="time" class="w-full rounded-md border px-3 py-2" />
                    </div>
                </div>
                <ShadcnButton r#type={ButtonType::Submit}>{"Save Topic"}</ShadcnButton>
            </form>
        </div>
    }
}

pub(crate) fn tip_type_label(i18n: &I18n, tipcard_type: &str) -> String {
    match tipcard_type {
        "casual_tip" | "repeatable_tip" | "manual_tip" | "custom_tip" => {
            i18n.t(&format!("tip_type.{tipcard_type}"))
        }
        _ => tipcard_type.to_string(),
    }
}
