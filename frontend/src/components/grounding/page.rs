//! Main grounding page: topics overview, documents, and image pool.
use super::documents::*;
use super::image_pool::*;
use super::settings::GroundingSettings;
use super::topics::{TopicEditor, tip_type_label};
use super::types::*;
use crate::api::toast;
use crate::app::View;
use crate::components::archive::ArchiveQuery;
use crate::components::button::{ButtonSize, ButtonType, ButtonVariant, ShadcnButton};
use crate::components::image_lightbox::ImageLightbox;
use crate::components::select::{SelectOption, ShadcnSelect};
use crate::components::tooltip::ShadcnTooltip;
use crate::i18n::use_i18n;
use crate::state::AppState;
use crate::topic_visual::display_icon;
use gloo_net::http::Request;
use serde::Deserialize;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{FileReader, HtmlDialogElement, HtmlInputElement, HtmlTextAreaElement};
use yew::prelude::*;
use yew_router::prelude::*;

#[function_component(Grounding)]
pub fn grounding() -> Html {
    let app_state = use_context::<UseReducerHandle<AppState>>().unwrap();
    let i18n = use_i18n();
    let navigator = use_navigator();
    let summary = use_state(|| None::<AppSummary>);
    let token_spend = use_state(|| None::<TokenSpend>);
    let topics = use_state(Vec::<AppTopicInfo>::new);
    let search = use_state(String::new);
    let editing = use_state(|| None::<AppTopicInfo>);
    let confirm_delete = use_state(|| None::<AppTopicInfo>);
    let regenerating_icon = use_state(|| None::<i64>);
    let loading_topic = use_state(|| None::<i64>);
    let dialog_ref = use_node_ref();

    let documents = use_state(Vec::<DocumentInfo>::new);
    let pool_images = use_state(Vec::<PoolImageInfo>::new);
    let sources_loaded = use_state(|| false);

    let doc_title = use_state(String::new);
    let doc_url = use_state(String::new);
    let doc_content = use_state(String::new);
    let doc_source_type = use_state(|| "document".to_string());
    let bulk_urls = use_state(String::new);
    let bulk_progress = use_state(|| None::<(usize, usize)>);
    let viewing_document = use_state(|| None::<DocumentDetail>);
    let doc_viewer_ref = use_node_ref();
    let doc_file_input_ref = use_node_ref();
    let doc_file_data = use_state(String::new);
    let doc_file_name = use_state(String::new);
    let doc_file_uploading = use_state(|| false);

    let img_name = use_state(String::new);
    let img_data = use_state(String::new);
    let file_input_ref = use_node_ref();
    let editing_image = use_state(|| None::<PoolImageInfo>);
    let rename_name = use_state(String::new);
    let rename_desc = use_state(String::new);
    let file_input_ref_for_click = file_input_ref.clone();
    let doc_file_input_ref_for_click = doc_file_input_ref.clone();
    let pool_expanded = use_state(|| false);
    let pool_search = use_state(String::new);
    let pool_lightbox_index = use_state(|| None::<usize>);
    {
        let pool_expanded = pool_expanded.clone();
        use_effect_with(pool_expanded.clone(), move |expanded| {
            set_fullscreen_body_class(**expanded);
            move || set_fullscreen_body_class(false)
        });
    }
    let rename_dialog_ref = use_node_ref();
    {
        let rename_dialog_ref = rename_dialog_ref.clone();
        let editing_image = editing_image.clone();
        use_effect_with(editing_image.clone(), move |ei| {
            if let Some(dialog) = rename_dialog_ref.cast::<HtmlDialogElement>() {
                if ei.is_some() {
                    let _ = dialog.show_modal();
                } else {
                    let _ = dialog.close();
                }
            }
            || ()
        });
    }
    {
        let doc_viewer_ref = doc_viewer_ref.clone();
        let viewing_document = viewing_document.clone();
        use_effect_with(viewing_document.clone(), move |vd| {
            if let Some(dialog) = doc_viewer_ref.cast::<HtmlDialogElement>() {
                if vd.is_some() {
                    let _ = dialog.show_modal();
                } else {
                    let _ = dialog.close();
                }
            }
            || ()
        });
    }

    let filtered_pool_images: Vec<PoolImageInfo> = {
        let search = (*pool_search).to_lowercase();
        if search.is_empty() {
            (*pool_images).clone()
        } else {
            (*pool_images)
                .iter()
                .filter(|img| {
                    img.name.to_lowercase().contains(&search)
                        || img
                            .description
                            .as_ref()
                            .map(|d| d.to_lowercase().contains(&search))
                            .unwrap_or(false)
                        || img.tags.iter().any(|t| t.to_lowercase().contains(&search))
                })
                .cloned()
                .collect()
        }
    };

    {
        let dialog_ref = dialog_ref.clone();
        let confirm_delete = confirm_delete.clone();
        use_effect_with(confirm_delete.clone(), move |cd| {
            if let Some(dialog) = dialog_ref.cast::<HtmlDialogElement>() {
                if cd.is_some() {
                    let _ = dialog.show_modal();
                } else {
                    let _ = dialog.close();
                }
            }
            || ()
        });
    }

    let refresh = {
        let summary = summary.clone();
        let token_spend = token_spend.clone();
        let topics = topics.clone();
        let documents = documents.clone();
        let pool_images = pool_images.clone();
        let sources_loaded = sources_loaded.clone();
        Callback::from(move |_| {
            let summary = summary.clone();
            let token_spend = token_spend.clone();
            let topics = topics.clone();
            let documents = documents.clone();
            let pool_images = pool_images.clone();
            let sources_loaded = sources_loaded.clone();
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
                if let Ok(res) = Request::get("/app/documents").send().await {
                    if let Ok(data) = res.json::<Vec<DocumentInfo>>().await {
                        documents.set(data);
                    }
                }
                if let Ok(res) = Request::get("/app/image-pool").send().await {
                    if let Ok(data) = res.json::<Vec<PoolImageInfo>>().await {
                        pool_images.set(data);
                    }
                }
                sources_loaded.set(true);
            });
        })
    };

    crate::hooks::use_view_refresh(View::Grounding, refresh.clone());

    let refresh_topics = {
        let topics = topics.clone();
        Callback::from(move |_| {
            let topics = topics.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(res) = Request::get("/app/topics").send().await {
                    if let Ok(data) = res.json::<Vec<AppTopicInfo>>().await {
                        topics.set(data);
                    }
                }
            });
        })
    };

    let on_regenerate_icon = {
        let app_state = app_state.clone();
        let topics = topics.clone();
        let regenerating_icon = regenerating_icon.clone();
        Callback::from(move |topic_id: i64| {
            if regenerating_icon.is_some() {
                return;
            }
            regenerating_icon.set(Some(topic_id));
            let app_state = app_state.clone();
            let topics = topics.clone();
            let regenerating_icon = regenerating_icon.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = RegenerateTopicIconReq { id: topic_id };
                let result = Request::post("/app/topics/regenerate-icon")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await;
                match result {
                    Ok(res) if res.ok() => {
                        if let Ok(data) = res.json::<RegenerateTopicIconRes>().await {
                            topics.set(
                                topics
                                    .iter()
                                    .map(|topic| {
                                        if topic.id == topic_id {
                                            AppTopicInfo {
                                                icon_id: data.icon_id.clone(),
                                                topic_color: data.topic_color.clone(),
                                                ..topic.clone()
                                            }
                                        } else {
                                            topic.clone()
                                        }
                                    })
                                    .collect(),
                            );
                            toast(&app_state, "Topic icon and color updated");
                        } else {
                            toast(&app_state, "Failed to read icon response");
                        }
                    }
                    Ok(res) => {
                        toast(
                            &app_state,
                            res.text()
                                .await
                                .unwrap_or_else(|_| "Failed to update topic icon".to_string()),
                        );
                    }
                    Err(err) => toast(&app_state, err.to_string()),
                }
                regenerating_icon.set(None);
            });
        })
    };

    let on_load_topic = {
        let app_state = app_state.clone();
        let loading_topic = loading_topic.clone();
        let navigator = navigator.clone();
        Callback::from(move |topic: AppTopicInfo| {
            if loading_topic.is_some() {
                return;
            }
            loading_topic.set(Some(topic.id));
            let app_state = app_state.clone();
            let loading_topic = loading_topic.clone();
            let navigator = navigator.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = ForceDailyRefreshReq {
                    topics: topic.name,
                    tipcard_type: Some(topic.tipcard_type),
                };
                let result = Request::post("/app/daily-refresh")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await;
                match result {
                    Ok(res) if res.ok() => match res.json::<ForceDailyRefreshRes>().await {
                        Ok(result) if result.refreshed_cards > 0 => {
                            toast(&app_state, "New card loaded");
                            if let Some(nav) = navigator {
                                nav.push(&View::Flow);
                            }
                        }
                        Ok(_) => toast(&app_state, "No new card available"),
                        Err(_) => toast(&app_state, "Failed to read refresh response"),
                    },
                    Ok(res) => toast(
                        &app_state,
                        res.text()
                            .await
                            .unwrap_or_else(|_| "Failed to load card".to_string()),
                    ),
                    Err(err) => toast(&app_state, err.to_string()),
                }
                loading_topic.set(None);
            });
        })
    };

    let on_dialog_close = {
        let confirm_delete = confirm_delete.clone();
        Callback::from(move |_| {
            confirm_delete.set(None);
        })
    };

    let on_cancel_delete = {
        let confirm_delete = confirm_delete.clone();
        Callback::from(move |_| {
            confirm_delete.set(None);
        })
    };

    let on_confirm_delete = {
        let confirm_delete = confirm_delete.clone();
        let app_state = app_state.clone();
        let refresh_topics = refresh_topics.clone();
        Callback::from(move |_| {
            if let Some(topic) = &*confirm_delete {
                let app_state = app_state.clone();
                let refresh_topics = refresh_topics.clone();
                let confirm_delete = confirm_delete.clone();
                let req = DeleteTopicReq { id: topic.id };
                wasm_bindgen_futures::spawn_local(async move {
                    match Request::delete("/app/topics")
                        .json(&req)
                        .unwrap()
                        .send()
                        .await
                    {
                        Ok(res) if res.ok() => {
                            toast(&app_state, "Topic deleted");
                            refresh_topics.emit(());
                            confirm_delete.set(None);
                        }
                        Ok(res) => {
                            toast(
                                &app_state,
                                res.text()
                                    .await
                                    .unwrap_or_else(|_| "Failed to delete topic".to_string()),
                            );
                            confirm_delete.set(None);
                        }
                        Err(err) => {
                            toast(&app_state, err.to_string());
                            confirm_delete.set(None);
                        }
                    }
                });
            }
        })
    };

    let filtered_topics: Vec<_> = topics
        .iter()
        .filter(|topic| {
            let q = search.to_lowercase();
            q.is_empty()
                || topic.name.to_lowercase().contains(&q)
                || topic.tipcard_type.to_lowercase().contains(&q)
        })
        .cloned()
        .collect();

    let on_add_document = {
        let doc_title = doc_title.clone();
        let doc_url = doc_url.clone();
        let doc_content = doc_content.clone();
        let doc_source_type = doc_source_type.clone();
        let refresh = {
            let documents = documents.clone();
            Callback::from(move |_| {
                let documents = documents.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    if let Ok(res) = Request::get("/app/documents").send().await {
                        if let Ok(data) = res.json::<Vec<DocumentInfo>>().await {
                            documents.set(data);
                        }
                    }
                });
            })
        };
        let app_state = app_state.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let title = (*doc_title).clone();
            let url_val = (*doc_url).clone();
            let content = (*doc_content).clone();
            let source_type = (*doc_source_type).clone();
            let refresh = refresh.clone();
            let doc_title = doc_title.clone();
            let doc_url = doc_url.clone();
            let doc_content = doc_content.clone();
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = AddDocumentReq {
                    topic_ids: Vec::new(),
                    source_type: source_type.clone(),
                    title,
                    url: if url_val.is_empty() {
                        None
                    } else {
                        Some(url_val)
                    },
                    content,
                };
                match Request::post("/app/documents")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        toast(&app_state, "Document added");
                        doc_title.set(String::new());
                        doc_url.set(String::new());
                        doc_content.set(String::new());
                        refresh.emit(());
                    }
                    _ => toast(&app_state, "Failed to add document"),
                }
            });
        })
    };

    let on_add_bulk_links = {
        let bulk_urls = bulk_urls.clone();
        let bulk_progress = bulk_progress.clone();
        let app_state = app_state.clone();
        let documents = documents.clone();
        Callback::from(move |_: MouseEvent| {
            let raw = (*bulk_urls).clone();
            let urls: Vec<String> = raw
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(|l| l.to_string())
                .collect();
            if urls.is_empty() {
                toast(&app_state, "No URLs to add");
                return;
            }
            let total = urls.len();
            let bulk_progress = bulk_progress.clone();
            let bulk_urls = bulk_urls.clone();
            let app_state = app_state.clone();
            let documents = documents.clone();
            bulk_progress.set(Some((0, total)));
            wasm_bindgen_futures::spawn_local(async move {
                let mut ok = 0usize;
                let mut fail = 0usize;
                for (i, url) in urls.iter().enumerate() {
                    bulk_progress.set(Some((i, total)));
                    let title = title_from_url(url);
                    let req = AddDocumentReq {
                        topic_ids: Vec::new(),
                        source_type: "link".into(),
                        title,
                        url: Some(url.clone()),
                        content: String::new(),
                    };
                    match Request::post("/app/documents")
                        .json(&req)
                        .unwrap()
                        .send()
                        .await
                    {
                        Ok(res) if res.ok() => ok += 1,
                        _ => fail += 1,
                    }
                }
                bulk_progress.set(Some((total, total)));
                if let Ok(res) = Request::get("/app/documents").send().await {
                    if let Ok(data) = res.json::<Vec<DocumentInfo>>().await {
                        documents.set(data);
                    }
                }
                bulk_progress.set(None);
                bulk_urls.set(String::new());
                toast(
                    &app_state,
                    format!(
                        "Imported {} link(s){}",
                        ok,
                        if fail > 0 {
                            format!(", {} failed", fail)
                        } else {
                            String::new()
                        }
                    ),
                );
            });
        })
    };

    let on_delete_document = {
        let documents = documents.clone();
        let app_state = app_state.clone();
        Callback::from(move |id: i64| {
            let documents = documents.clone();
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = DeleteDocumentReq { id };
                match Request::delete("/app/documents")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        toast(&app_state, "Document deleted");
                        if let Ok(res) = Request::get("/app/documents").send().await {
                            if let Ok(data) = res.json::<Vec<DocumentInfo>>().await {
                                documents.set(data);
                            }
                        }
                    }
                    _ => toast(&app_state, "Failed to delete document"),
                }
            });
        })
    };

    let on_view_document = {
        let viewing_document = viewing_document.clone();
        let app_state = app_state.clone();
        Callback::from(move |id: i64| {
            let viewing_document = viewing_document.clone();
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match Request::get(&format!("/app/documents/{}", id)).send().await {
                    Ok(res) if res.ok() => {
                        if let Ok(doc) = res.json::<DocumentDetail>().await {
                            viewing_document.set(Some(doc));
                        }
                    }
                    _ => toast(&app_state, "Failed to load document"),
                }
            });
        })
    };

    let on_doc_file_selected = {
        let doc_file_data = doc_file_data.clone();
        let doc_file_name = doc_file_name.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            let doc_file_data = doc_file_data.clone();
            let doc_file_name = doc_file_name.clone();
            if let Some(files) = input.files() {
                if let Some(file) = files.get(0) {
                    let name = file.name();
                    let reader = FileReader::new().unwrap();
                    let reader_clone = reader.clone();
                    let closure = Closure::wrap(Box::new(move || {
                        if let Ok(result) = reader_clone.result() {
                            if let Some(data_url) = result.as_string() {
                                doc_file_data.set(data_url);
                            }
                        }
                    }) as Box<dyn FnMut()>);
                    reader.set_onloadend(Some(closure.as_ref().unchecked_ref()));
                    let _ = reader.read_as_data_url(&file);
                    closure.forget();
                    doc_file_name.set(name);
                }
            }
        })
    };

    let on_upload_doc_file = {
        let doc_file_data = doc_file_data.clone();
        let doc_file_name = doc_file_name.clone();
        let doc_file_uploading = doc_file_uploading.clone();
        let app_state = app_state.clone();
        let documents = documents.clone();
        Callback::from(move |_: MouseEvent| {
            let data_url = (*doc_file_data).clone();
            let filename = (*doc_file_name).clone();
            if data_url.is_empty() || filename.is_empty() {
                toast(&app_state, "Choose a file first");
                return;
            }
            let doc_file_data = doc_file_data.clone();
            let doc_file_name = doc_file_name.clone();
            let doc_file_uploading = doc_file_uploading.clone();
            let app_state = app_state.clone();
            let documents = documents.clone();
            doc_file_uploading.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                let req = UploadDocumentReq {
                    filename: filename.clone(),
                    title: None,
                    data_url,
                    topic_ids: Vec::new(),
                };
                match Request::post("/app/documents/upload")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        toast(&app_state, "File uploaded");
                        doc_file_data.set(String::new());
                        doc_file_name.set(String::new());
                        if let Ok(res) = Request::get("/app/documents").send().await {
                            if let Ok(data) = res.json::<Vec<DocumentInfo>>().await {
                                documents.set(data);
                            }
                        }
                    }
                    _ => toast(&app_state, "Failed to upload file"),
                }
                doc_file_uploading.set(false);
            });
        })
    };

    let on_add_image = {
        let img_name = img_name.clone();
        let img_data = img_data.clone();
        let file_input_ref = file_input_ref.clone();
        let refresh = {
            let pool_images = pool_images.clone();
            Callback::from(move |_| {
                let pool_images = pool_images.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    if let Ok(res) = Request::get("/app/image-pool").send().await {
                        if let Ok(data) = res.json::<Vec<PoolImageInfo>>().await {
                            pool_images.set(data);
                        }
                    }
                });
            })
        };
        let app_state = app_state.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let name = (*img_name).clone();
            let image_data = (*img_data).clone();
            let refresh = refresh.clone();
            let img_name = img_name.clone();
            let img_data = img_data.clone();
            let file_input_ref = file_input_ref.clone();
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = AddPoolImageReq { image_data, name };
                match Request::post("/app/image-pool")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        #[derive(Deserialize)]
                        struct AddPoolImageRes {
                            name: String,
                            annotated: bool,
                            fallback_reason: Option<String>,
                            model: Option<String>,
                        }
                        let message = match res.json::<AddPoolImageRes>().await {
                            Ok(data) if data.annotated => {
                                let model = data.model.unwrap_or_else(|| "vision".to_string());
                                format!("Image added · annotated as «{}» via {}", data.name, model)
                            }
                            Ok(data) => {
                                let reason = data
                                    .fallback_reason
                                    .unwrap_or_else(|| "annotation skipped".to_string());
                                let model = data
                                    .model
                                    .map(|m| format!(" · model {m}"))
                                    .unwrap_or_default();
                                format!("Image added · fallback ({reason}){model}")
                            }
                            Err(_) => "Image added".to_string(),
                        };
                        toast(&app_state, message);
                        img_name.set(String::new());
                        img_data.set(String::new());
                        if let Some(input) = file_input_ref.cast::<HtmlInputElement>() {
                            input.set_value("");
                        }
                        refresh.emit(());
                    }
                    Ok(res) => {
                        let message = res
                            .text()
                            .await
                            .unwrap_or_else(|_| "Failed to add image".to_string());
                        toast(&app_state, message);
                    }
                    Err(err) => toast(&app_state, err.to_string()),
                }
            });
        })
    };

    let on_delete_image = {
        let pool_images = pool_images.clone();
        let app_state = app_state.clone();
        Callback::from(move |id: i64| {
            let pool_images = pool_images.clone();
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = DeletePoolImageReq { id };
                match Request::delete("/app/image-pool")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        toast(&app_state, "Image deleted");
                        if let Ok(res) = Request::get("/app/image-pool").send().await {
                            if let Ok(data) = res.json::<Vec<PoolImageInfo>>().await {
                                pool_images.set(data);
                            }
                        }
                    }
                    _ => toast(&app_state, "Failed to delete image"),
                }
            });
        })
    };

    let on_rename_image = {
        let pool_images = pool_images.clone();
        let editing_image = editing_image.clone();
        let rename_name = rename_name.clone();
        let rename_desc = rename_desc.clone();
        let app_state = app_state.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let Some(img) = (*editing_image).clone() else {
                return;
            };
            let name = (*rename_name).clone();
            let description = {
                let d = (*rename_desc).clone();
                if d.is_empty() { None } else { Some(d) }
            };
            let pool_images = pool_images.clone();
            let editing_image = editing_image.clone();
            let rename_name = rename_name.clone();
            let rename_desc = rename_desc.clone();
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = RenamePoolImageReq {
                    id: img.id,
                    name,
                    description,
                };
                match Request::patch("/app/image-pool")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        toast(&app_state, "Image renamed");
                        editing_image.set(None);
                        rename_name.set(String::new());
                        rename_desc.set(String::new());
                        if let Ok(res) = Request::get("/app/image-pool").send().await {
                            if let Ok(data) = res.json::<Vec<PoolImageInfo>>().await {
                                pool_images.set(data);
                            }
                        }
                    }
                    _ => toast(&app_state, "Failed to rename image"),
                }
            });
        })
    };

    let on_remove_tag = {
        let pool_images = pool_images.clone();
        let app_state = app_state.clone();
        Callback::from(move |(id, tag): (i64, String)| {
            let pool_images = pool_images.clone();
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = RemovePoolImageTagReq { id, tag };
                match Request::delete("/app/image-pool/tag")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        if let Ok(res) = Request::get("/app/image-pool").send().await {
                            if let Ok(data) = res.json::<Vec<PoolImageInfo>>().await {
                                pool_images.set(data);
                            }
                        }
                    }
                    _ => toast(&app_state, "Failed to remove tag"),
                }
            });
        })
    };

    let on_start_rename = {
        let editing_image = editing_image.clone();
        let rename_name = rename_name.clone();
        let rename_desc = rename_desc.clone();
        Callback::from(move |img: PoolImageInfo| {
            rename_name.set(img.name.clone());
            rename_desc.set(img.description.clone().unwrap_or_default());
            editing_image.set(Some(img));
        })
    };

    let on_file_selected = {
        let img_data = img_data.clone();
        let img_name = img_name.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            let img_data = img_data.clone();
            let img_name = img_name.clone();
            if let Some(files) = input.files() {
                if let Some(file) = files.get(0) {
                    let name = file.name();
                    let reader = FileReader::new().unwrap();
                    let reader_clone = reader.clone();
                    let closure = Closure::wrap(Box::new(move || {
                        if let Ok(result) = reader_clone.result() {
                            if let Some(data_url) = result.as_string() {
                                img_data.set(data_url);
                            }
                        }
                    }) as Box<dyn FnMut()>);
                    reader.set_onloadend(Some(closure.as_ref().unchecked_ref()));
                    let _ = reader.read_as_data_url(&file);
                    closure.forget();
                    if (*img_name).is_empty() {
                        img_name.set(name);
                    }
                }
            }
        })
    };

    html! {
        <section id="view-grounding">
            <div class="mb-4">
                <h1 class="text-xl font-semibold tracking-tight">{"Grounding"}</h1>
                <p class="text-muted mt-2">{"Cards, topics, and sources."}</p>
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
                    <input
                        id="topic-search"
                        class="rounded-md border px-4 py-2 w-full sm:w-72"
                        placeholder="Find topic"
                        aria-label="Find topic"
                        value={(*search).clone()}
                        oninput={Callback::from({ let search = search.clone(); move |e: InputEvent| {
                            if let Some(target) = e.target_dyn_into::<HtmlInputElement>() {
                                search.set(target.value());
                            }
                        }})}
                    />
                    <ShadcnButton
                        variant={ButtonVariant::Default}
                        size={ButtonSize::Icon}
                        onclick={Callback::from({
                            let navigator = navigator.clone();
                            move |_: MouseEvent| {
                                if let Some(nav) = navigator.clone() {
                                    nav.push(&View::Flow);
                                }
                            }
                        })}
                    >
                        <iconify-icon icon="radix-icons:plus" class="radix-icon" aria-hidden="true"></iconify-icon>
                        <span class="sr-only">{"New topic"}</span>
                    </ShadcnButton>
                </div>
            </div>

            <div id="topics-grid" class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4 gap-3 mb-8">
                {
                    if filtered_topics.is_empty() {
                        html! { <div class="col-span-full surface border rounded-md p-10 text-center text-muted">{"No topics found."}</div> }
                    } else {
                        html! {
                            for filtered_topics.iter().map(|t| {
                                let topic_for_edit = t.clone();
                                let topic_for_load = t.clone();
                                let topic_for_delete = t.clone();
                                let topic_id = t.id;
                                let icon_loading = *regenerating_icon == Some(topic_id);
                                let card_loading = *loading_topic == Some(topic_id);
                                let on_regenerate_icon = on_regenerate_icon.clone();
                                let on_load_topic = on_load_topic.clone();
                                let pending_topic_name = t.name.clone();
                                let pending_count = t.pending_cards;
                                let pending_navigator = navigator.clone();
                                html! {
                                <div class="surface border rounded-md p-4 flex flex-col">
                                    <div class="flex justify-between items-start mb-2 gap-2">
                                        <h3 class="font-semibold text-lg truncate flex items-center gap-2 min-w-0">
                                            <ShadcnTooltip content="Pick new icon with AI">
                                            <button
                                                type="button"
                                                class="topic-icon-btn shrink-0 inline-flex items-center justify-center rounded-sm border border-transparent hover:border-token disabled:opacity-50"
                                                disabled={icon_loading}
                                                onclick={Callback::from(move |_: MouseEvent| on_regenerate_icon.emit(topic_id))}
                                            >
                                                if icon_loading {
                                                    <iconify-icon
                                                        icon="radix-icons:reload"
                                                        class="topic-icon radix-icon animate-spin opacity-70"
                                                        aria-hidden="true"
                                                    ></iconify-icon>
                                                } else {
                                                    <iconify-icon
                                                        icon={display_icon(&t.icon_id).to_string()}
                                                        class="topic-icon radix-icon"
                                                        style={format!("color: {}", t.topic_color)}
                                                        aria-hidden="true"
                                                    ></iconify-icon>
                                                }
                                            </button>
                                            </ShadcnTooltip>
                                            <span class="truncate">{&t.name}</span>
                                        </h3>
                                        <span class="badge shrink-0">{tip_type_label(&i18n, &t.tipcard_type)}</span>
                                    </div>
                                    <div class="text-sm text-muted">
                                        {format!("{} due / {} total", t.due_cards, t.total_cards)}
                                    </div>
                                    <ShadcnButton
                                        variant={ButtonVariant::Outline}
                                        size={ButtonSize::Sm}
                                        class="mt-3 w-full"
                                        disabled={pending_count == 0}
                                        onclick={Callback::from(move |_: MouseEvent| {
                                            if pending_count == 0 {
                                                return;
                                            }
                                            if let Some(nav) = pending_navigator.clone() {
                                                let _ = nav.push_with_query(
                                                    &View::Archive,
                                                    &ArchiveQuery {
                                                        status: Some("pending".to_string()),
                                                        topic: Some(pending_topic_name.clone()),
                                                    },
                                                );
                                            }
                                        })}
                                    >
                                        <iconify-icon icon="radix-icons:eye-open" class="radix-icon" aria-hidden="true"></iconify-icon>
                                        {format!("Show {pending_count} pending cards")}
                                    </ShadcnButton>
                                    <div class="mt-3 grid grid-cols-3 gap-2">
                                        <ShadcnButton
                                            variant={ButtonVariant::Secondary}
                                            size={ButtonSize::Sm}
                                            disabled={loading_topic.is_some()}
                                            onclick={Callback::from(move |_: MouseEvent| on_load_topic.emit(topic_for_load.clone()))}
                                        >
                                            if card_loading {
                                                <iconify-icon icon="radix-icons:reload" class="radix-icon animate-spin" aria-hidden="true"></iconify-icon>
                                                {"Loading..."}
                                            } else {
                                                {"Load"}
                                            }
                                        </ShadcnButton>
                                        <ShadcnButton
                                            variant={ButtonVariant::Outline}
                                            size={ButtonSize::Sm}
                                            onclick={Callback::from({
                                                let editing = editing.clone();
                                                move |_: MouseEvent| editing.set(Some(topic_for_edit.clone()))
                                            })}
                                        >
                                            {"Edit"}
                                        </ShadcnButton>
                                        <ShadcnButton
                                            variant={ButtonVariant::Destructive}
                                            size={ButtonSize::Sm}
                                            onclick={Callback::from({
                                                let confirm_delete = confirm_delete.clone();
                                                let topic = topic_for_delete.clone();
                                                move |_: MouseEvent| {
                                                    confirm_delete.set(Some(topic.clone()));
                                                }
                                            })}
                                        >
                                            {"Delete"}
                                        </ShadcnButton>
                                    </div>
                                </div>
                                }
                            })
                        }
                    }
                }
            </div>

            <div class="space-y-6">
                <div>
                    <h2 class="text-xl font-semibold flex items-center gap-2">
                        <iconify-icon icon="radix-icons:file-text" class="radix-icon text-primary" aria-hidden="true"></iconify-icon>
                        {"Sources"}
                    </h2>
                    <p class="text-sm text-muted">{"Documents and images used for grounding and card illustration."}</p>
                </div>

                <GroundingSettings />

                <div class="space-y-6">
                    <div class="hidden" aria-hidden="true">
                        <div>
                            <h3 class="text-lg font-semibold flex items-center gap-2">
                                <iconify-icon icon="radix-icons:file-text" class="radix-icon text-primary" aria-hidden="true"></iconify-icon>
                                {"Documents"}
                            </h3>
                            <p class="text-sm text-muted">{"Chunked and indexed for retrieval."}</p>
                        </div>

                        <form onsubmit={on_add_document} class="space-y-3">
                            <div class="space-y-1">
                                <label class="text-sm font-medium">{"Source Type"}</label>
                                <ShadcnSelect
                                    value={(*doc_source_type).clone()}
                                    onchange={Callback::from({
                                        let s = doc_source_type.clone();
                                        move |value: String| s.set(value)
                                    })}
                                    options={vec![
                                        SelectOption { value: "document".into(), label: "Document (paste text)".into() },
                                        SelectOption { value: "link".into(), label: "Link (fetch from URL)".into() },
                                    ]}
                                />
                                { if *doc_source_type == "link" {
                                    html! { <p class="text-xs text-muted">{"The server fetches the URL body and indexes it. Leave Content empty to auto-fetch."}</p> }
                                } else {
                                    html! { <p class="text-xs text-muted">{"Paste text directly. URL is optional metadata."}</p> }
                                } }
                            </div>
                            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                                <div class="space-y-1">
                                    <label class="text-sm font-medium">{"Title"}</label>
                                    <input
                                        type="text"
                                        class="w-full rounded-md border px-3 py-2 h-10"
                                        placeholder="My Document"
                                        value={(*doc_title).clone()}
                                        oninput={{
                                            let s = doc_title.clone();
                                            Callback::from(move |e: InputEvent| {
                                                let t: HtmlInputElement = e.target_unchecked_into();
                                                s.set(t.value());
                                            })
                                        }}
                                    />
                                </div>
                                <div class="space-y-1">
                                    <label class="text-sm font-medium">
                                        { if *doc_source_type == "link" { "URL" } else { "URL (optional)" } }
                                    </label>
                                    <input
                                        type="text"
                                        class="w-full rounded-md border px-3 py-2 h-10"
                                        placeholder="https://…"
                                        value={(*doc_url).clone()}
                                        oninput={{
                                            let s = doc_url.clone();
                                            Callback::from(move |e: InputEvent| {
                                                let t: HtmlInputElement = e.target_unchecked_into();
                                                s.set(t.value());
                                            })
                                        }}
                                    />
                                </div>
                            </div>
                            <div class="space-y-1">
                                <label class="text-sm font-medium">{"Content"}</label>
                                <textarea
                                    class="w-full rounded-md border px-3 py-2 min-h-[100px] resize-y"
                                    placeholder={ if *doc_source_type == "link" { "Leave empty to fetch from URL automatically…" } else { "Paste document text here…" } }
                                    value={(*doc_content).clone()}
                                    oninput={{
                                        let s = doc_content.clone();
                                        Callback::from(move |e: InputEvent| {
                                            let t: HtmlTextAreaElement = e.target_unchecked_into();
                                            s.set(t.value());
                                        })
                                    }}
                                />
                            </div>
                            <div class="flex justify-end">
                                <ShadcnButton r#type={ButtonType::Submit}>
                                    <iconify-icon icon="radix-icons:plus" class="radix-icon" aria-hidden="true"></iconify-icon>
                                    { if *doc_source_type == "link" { "Add Link" } else { "Add Document" } }
                                </ShadcnButton>
                            </div>
                        </form>

                        <div class="border-t border-token pt-4 space-y-3">
                            <div>
                                <label class="text-sm font-medium">{"Bulk Import Links"}</label>
                                <p class="text-xs text-muted">{"One URL per line. Titles are auto-derived from the URL path. The server fetches each page."}</p>
                            </div>
                            <textarea
                                class="w-full rounded-md border px-3 py-2 min-h-[80px] resize-y font-mono text-sm"
                                placeholder={"https://helix-editor.vercel.app/start-here/installation/\nhttps://helix-editor.vercel.app/reference/typed-commands/\n…"}
                                value={(*bulk_urls).clone()}
                                oninput={{
                                    let s = bulk_urls.clone();
                                    Callback::from(move |e: InputEvent| {
                                        let t: HtmlTextAreaElement = e.target_unchecked_into();
                                        s.set(t.value());
                                    })
                                }}
                            />
                            <div class="flex items-center justify-between">
                                { if let Some((done, total)) = *bulk_progress {
                                    html! { <span class="text-sm text-muted">{format!("Importing {} / {}…", done, total)}</span> }
                                } else {
                                    html! { <span></span> }
                                } }
                                <ShadcnButton
                                    variant={ButtonVariant::Secondary}
                                    onclick={on_add_bulk_links}
                                    disabled={(*bulk_progress).is_some()}
                                >
                                    <iconify-icon icon="radix-icons:link-2" class="radix-icon" aria-hidden="true"></iconify-icon>
                                    {"Import Links"}
                                </ShadcnButton>
                            </div>
                        </div>

                        <div class="border-t border-token pt-4 space-y-3">
                            <div>
                                <label class="text-sm font-medium">{"Upload File"}</label>
                                <p class="text-xs text-muted">{"PDF, Word, PowerPoint, Excel, OpenDocument, RTF, EPUB, CSV, HTML, or text. Converted to Markdown server-side for retrieval."}</p>
                            </div>
                            <div class="flex items-center gap-3">
                                <input
                                    type="file"
                                    accept=".pdf,.doc,.docx,.docm,.ppt,.pptx,.pptm,.pps,.ppsx,.xls,.xlsx,.xlsm,.xlsb,.odt,.ods,.odp,.rtf,.epub,.csv,.html,.htm,.txt,.md,text/plain,text/html,text/csv,text/markdown,application/pdf,application/msword,application/vnd.openxmlformats-officedocument.wordprocessingml.document,application/vnd.ms-powerpoint,application/vnd.openxmlformats-officedocument.presentationml.presentation,application/vnd.ms-excel,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet,application/rtf,application/epub+zip"
                                    class="hidden"
                                    ref={doc_file_input_ref}
                                    onchange={on_doc_file_selected}
                                />
                                <ShadcnButton
                                    variant={ButtonVariant::Secondary}
                                    onclick={Callback::from(move |_: MouseEvent| {
                                        if let Some(input) = doc_file_input_ref_for_click.cast::<HtmlInputElement>() {
                                            let _ = input.click();
                                        }
                                    })}
                                    disabled={*doc_file_uploading}
                                >
                                    <iconify-icon icon="radix-icons:upload" class="radix-icon" aria-hidden="true"></iconify-icon>
                                    {"Choose File"}
                                </ShadcnButton>
                                <span class="text-sm text-muted truncate">
                                    { if (*doc_file_name).is_empty() { "No file chosen".to_string() } else { (*doc_file_name).clone() } }
                                </span>
                                { if !(*doc_file_data).is_empty() && !(*doc_file_name).is_empty() {
                                    html! {
                                        <ShadcnButton
                                            variant={ButtonVariant::Default}
                                            onclick={on_upload_doc_file}
                                            disabled={*doc_file_uploading}
                                        >
                                            { if *doc_file_uploading { "Uploading…" } else { "Upload" } }
                                        </ShadcnButton>
                                    }
                                } else {
                                    html! {}
                                } }
                            </div>
                        </div>
                        <div class="border-t border-token pt-4 space-y-3">
                            <div>
                                <label class="text-sm font-medium">{"Indexed Documents"}</label>
                                <p class="text-xs text-muted">{"Added documents appear here."}</p>
                            </div>

                        { if !*sources_loaded {
                            html! { <p class="text-sm text-muted">{"Loading documents…"}</p> }
                        } else if documents.is_empty() {
                            html! { <p class="text-sm text-muted italic">{"No documents yet."}</p> }
                        } else {
                            html! {
                                <div class="space-y-2">
                                    { for documents.iter().map(|doc| {
                                        let id = doc.id;
                                        let on_delete = on_delete_document.clone();
                                        let on_view = on_view_document.clone();
                                        html! {
                                            <div class="flex items-start gap-3 p-3 surface border rounded-md">
                                                <iconify-icon
                                                    icon={ if doc.source_type == "link" { "radix-icons:link-2" } else { "radix-icons:file-text" } }
                                                    class="radix-icon text-muted shrink-0 mt-0.5"
                                                    aria-hidden="true"
                                                ></iconify-icon>
                                                <div class="flex-1 min-w-0">
                                                    <div class="font-medium truncate">
                                                        {&doc.title}
                                                        <span class="badge ml-2 text-xs">{ if doc.source_type == "link" { "link" } else { "doc" } }</span>
                                                    </div>
                                                    <div class="text-xs text-muted">
                                                        { doc.url.as_ref().map(|url| html! {
                                                            <a href={url.clone()} target="_blank" class="underline mr-2">{url}</a>
                                                        }) }
                                                        <span>{&doc.created_at}</span>
                                                    </div>
                                                </div>
                                                <ShadcnButton
                                                    variant={ButtonVariant::Ghost}
                                                    size={ButtonSize::Icon}
                                                    onclick={Callback::from(move |_: MouseEvent| on_view.emit(id))}
                                                >
                                                    <iconify-icon icon="radix-icons:eye-open" class="radix-icon" aria-hidden="true"></iconify-icon>
                                                    <span class="sr-only">{"View"}</span>
                                                </ShadcnButton>
                                                <ShadcnButton
                                                    variant={ButtonVariant::Ghost}
                                                    size={ButtonSize::Icon}
                                                    onclick={Callback::from(move |_: MouseEvent| on_delete.emit(id))}
                                                >
                                                    <iconify-icon icon="radix-icons:trash" class="radix-icon text-destructive" aria-hidden="true"></iconify-icon>
                                                    <span class="sr-only">{"Delete"}</span>
                                                </ShadcnButton>
                                            </div>
                                        }
                                    }) }
                                </div>
                            }
                        } }
                        </div>
                    </div>

                    <div class="surface border rounded-md p-4 space-y-4">
                        <div>
                            <h3 class="text-lg font-semibold flex items-center gap-2">
                                <iconify-icon icon="radix-icons:image" class="radix-icon text-primary" aria-hidden="true"></iconify-icon>
                                {"Local Image Pool"}
                            </h3>
                            <p class="text-sm text-muted">{"Images available to the Local Image Pool source."}</p>
                        </div>

                        <form onsubmit={on_add_image} class="space-y-3">
                            <div class="space-y-1">
                                <label class="text-sm font-medium">{"Fallback Name"}</label>
                                <input
                                    type="text"
                                    class="w-full rounded-md border px-3 py-2 h-10"
                                    placeholder="sunset.jpg"
                                    value={(*img_name).clone()}
                                    oninput={{
                                        let s = img_name.clone();
                                        Callback::from(move |e: InputEvent| {
                                            let t: HtmlInputElement = e.target_unchecked_into();
                                            s.set(t.value());
                                        })
                                    }}
                                />
                                <div class="text-xs text-muted">{"Used if vision annotation is unavailable. Name, description, and tags are auto-generated by the vision model."}</div>
                            </div>
                            <div class="space-y-1">
                                <label class="text-sm font-medium">{"Image File"}</label>
                                <div class="flex items-center gap-3">
                                    <input
                                        type="file"
                                        accept="image/*"
                                        class="hidden"
                                        ref={file_input_ref}
                                        onchange={on_file_selected}
                                    />
                                    <ShadcnButton
                                        variant={ButtonVariant::Secondary}
                                        onclick={Callback::from(move |_: MouseEvent| {
                                            if let Some(input) = file_input_ref_for_click.cast::<HtmlInputElement>() {
                                                let _ = input.click();
                                            }
                                        })}
                                    >
                                        <iconify-icon icon="radix-icons:upload" class="radix-icon" aria-hidden="true"></iconify-icon>
                                        {"Choose File"}
                                    </ShadcnButton>
                                    <span class="text-sm text-muted truncate">
                                        { if (*img_data).is_empty() { "No file chosen".to_string() } else { (*img_name).clone() } }
                                    </span>
                                </div>
                            </div>
                            <div class="flex justify-end">
                                <ShadcnButton r#type={ButtonType::Submit}>
                                    <iconify-icon icon="radix-icons:plus" class="radix-icon" aria-hidden="true"></iconify-icon>
                                    {"Add Image"}
                                </ShadcnButton>
                            </div>
                        </form>

                        { if !*sources_loaded {
                            html! { <p class="text-sm text-muted">{"Loading images…"}</p> }
                        } else if pool_images.is_empty() {
                            html! { <p class="text-sm text-muted italic">{"No pool images yet."}</p> }
                        } else {
                            html! {
                                <>
                                    <div class="flex items-center justify-between">
                                        <span class="text-sm text-muted">
                                            {format!("{} image{}", pool_images.len(), if pool_images.len() == 1 { "" } else { "s" })}
                                        </span>
                                        <ShadcnTooltip content="Fullscreen">
                                            <button
                                                type="button"
                                                class="border border-token p-2"
                                                onclick={Callback::from({
                                                    let pool_expanded = pool_expanded.clone();
                                                    move |_: MouseEvent| pool_expanded.set(true)
                                                })}
                                            >
                                                <iconify-icon icon="radix-icons:enter-full-screen" class="radix-icon" aria-hidden="true"></iconify-icon>
                                            </button>
                                        </ShadcnTooltip>
                                    </div>
                                    <div class="max-h-[30rem] overflow-y-auto pr-1">
                                        <div class="grid grid-cols-2 gap-3">
                                            { for pool_images.iter().take(20).enumerate().map(|(idx, img)| {
                                                let id = img.id;
                                                let on_delete = on_delete_image.clone();
                                                let on_start_rename = on_start_rename.clone();
                                                let on_remove_tag = on_remove_tag.clone();
                                                let img_for_rename = img.clone();
                                                let img_url = format!("/app/pool-images/{}", id);
                                                let pool_lightbox_index = pool_lightbox_index.clone();
                                                html! {
                                                    <div class="surface border rounded-md p-3 flex flex-col text-center">
                                                        <button
                                                            type="button"
                                                            class="block w-full mb-2"
                                                            onclick={Callback::from(move |_: MouseEvent| pool_lightbox_index.set(Some(idx)))}
                                                        >
                                                            <img
                                                                src={img_url.clone()}
                                                                alt={img.name.clone()}
                                                                class="w-full h-24 object-cover rounded-md"
                                                                loading="lazy"
                                                            />
                                                        </button>
                                                        <div class="font-medium text-sm truncate w-full">{&img.name}</div>
                                                        { img.description.as_ref().map(|desc| html! {
                                                            <div class="text-xs text-muted truncate w-full">{desc}</div>
                                                        }) }
                                                        { if !img.tags.is_empty() {
                                                            html! {
                                                                <div class="flex flex-wrap gap-1 justify-center mt-1">
                                                                    { for img.tags.iter().map(|tag| {
                                                                        let tag_clone = tag.clone();
                                                                        let on_remove_tag = on_remove_tag.clone();
                                                                        html! {
                                                                            <span class="inline-flex items-center gap-1 rounded-full bg-token/10 text-xs px-2 py-0.5">
                                                                                {tag}
                                                                                <button
                                                                                    type="button"
                                                                                    class="text-muted hover:text-destructive"
                                                                                    onclick={Callback::from(move |_: MouseEvent| on_remove_tag.emit((id, tag_clone.clone())))}
                                                                                >
                                                                                    <iconify-icon icon="radix-icons:cross-2" class="radix-icon text-xs" aria-hidden="true"></iconify-icon>
                                                                                </button>
                                                                            </span>
                                                                        }
                                                                    }) }
                                                                </div>
                                                            }
                                                        } else {
                                                            html! {}
                                                        } }
                                                        <div class="mt-2 flex gap-2 justify-center">
                                                            <ShadcnButton
                                                                variant={ButtonVariant::Outline}
                                                                size={ButtonSize::Sm}
                                                                onclick={Callback::from(move |_: MouseEvent| on_start_rename.emit(img_for_rename.clone()))}
                                                            >
                                                                {"Rename"}
                                                            </ShadcnButton>
                                                            <ShadcnButton
                                                                variant={ButtonVariant::Destructive}
                                                                size={ButtonSize::Sm}
                                                                onclick={Callback::from(move |_: MouseEvent| on_delete.emit(id))}
                                                            >
                                                                {"Delete"}
                                                            </ShadcnButton>
                                                        </div>
                                                    </div>
                                                }
                                            }) }
                                        </div>
                                        { if pool_images.len() > 20 {
                                            html! {
                                                <div class="text-center text-sm text-muted mt-3">
                                                    {format!("Showing 20 of {} — click fullscreen to see all", pool_images.len())}
                                                </div>
                                            }
                                        } else {
                                            html! {}
                                        } }
                                    </div>
                                </>
                            }
                        } }
                    </div>
                </div>
            </div>

            { if *pool_expanded {
                html! {
                    <div class="flow-card is-fullscreen fullscreen-card-enter surface border fixed top-0 right-0 bottom-0 z-[70] overflow-hidden flex flex-col p-6">
                        <div class="flex items-center justify-between mb-4">
                            <h3 class="text-lg font-semibold flex items-center gap-2">
                                <iconify-icon icon="radix-icons:image" class="radix-icon text-primary" aria-hidden="true"></iconify-icon>
                                {"Local Image Pool"}
                                <span class="text-sm text-muted font-normal">
                                    {format!("({} image{})", pool_images.len(), if pool_images.len() == 1 { "" } else { "s" })}
                                </span>
                            </h3>
                            <ShadcnTooltip content="Exit fullscreen">
                                <button
                                    type="button"
                                    class="border border-token p-2"
                                    onclick={Callback::from({
                                        let pool_expanded = pool_expanded.clone();
                                        move |_: MouseEvent| pool_expanded.set(false)
                                    })}
                                >
                                    <iconify-icon icon="radix-icons:exit-full-screen" class="radix-icon" aria-hidden="true"></iconify-icon>
                                </button>
                            </ShadcnTooltip>
                        </div>
                        <input
                            type="text"
                            class="w-full rounded-md border px-3 py-2 h-10 mb-4"
                            placeholder="Search by name, description, or tags…"
                            value={(*pool_search).clone()}
                            oninput={{
                                let s = pool_search.clone();
                                Callback::from(move |e: InputEvent| {
                                    let t: HtmlInputElement = e.target_unchecked_into();
                                    s.set(t.value());
                                })
                            }}
                        />
                        { if filtered_pool_images.is_empty() {
                            html! { <p class="text-sm text-muted italic">{"No images match your search."}</p> }
                        } else {
                            html! {
                                <div class="flex-1 overflow-y-auto pr-1">
                                    <div class="grid grid-cols-3 gap-3">
                                        { for filtered_pool_images.iter().enumerate().map(|(idx, img)| {
                                            let id = img.id;
                                            let on_delete = on_delete_image.clone();
                                            let on_start_rename = on_start_rename.clone();
                                            let on_remove_tag = on_remove_tag.clone();
                                            let img_for_rename = img.clone();
                                            let img_url = format!("/app/pool-images/{}", id);
                                            let pool_lightbox_index = pool_lightbox_index.clone();
                                            html! {
                                                <div class="surface border rounded-md p-3 flex flex-col text-center">
                                                    <button
                                                        type="button"
                                                        class="block w-full mb-2"
                                                        onclick={Callback::from(move |_: MouseEvent| pool_lightbox_index.set(Some(idx)))}
                                                    >
                                                        <img
                                                            src={img_url.clone()}
                                                            alt={img.name.clone()}
                                                            class="w-full h-28 object-cover rounded-md"
                                                            loading="lazy"
                                                        />
                                                    </button>
                                                    <div class="font-medium text-sm truncate w-full">{&img.name}</div>
                                                    { img.description.as_ref().map(|desc| html! {
                                                        <div class="text-xs text-muted truncate w-full">{desc}</div>
                                                    }) }
                                                    { if !img.tags.is_empty() {
                                                        html! {
                                                            <div class="flex flex-wrap gap-1 justify-center mt-1">
                                                                { for img.tags.iter().map(|tag| {
                                                                    let tag_clone = tag.clone();
                                                                    let on_remove_tag = on_remove_tag.clone();
                                                                    html! {
                                                                        <span class="inline-flex items-center gap-1 rounded-full bg-token/10 text-xs px-2 py-0.5">
                                                                            {tag}
                                                                            <button
                                                                                type="button"
                                                                                class="text-muted hover:text-destructive"
                                                                                onclick={Callback::from(move |_: MouseEvent| on_remove_tag.emit((id, tag_clone.clone())))}
                                                                            >
                                                                                <iconify-icon icon="radix-icons:cross-2" class="radix-icon text-xs" aria-hidden="true"></iconify-icon>
                                                                            </button>
                                                                        </span>
                                                                    }
                                                                }) }
                                                            </div>
                                                        }
                                                    } else {
                                                        html! {}
                                                    } }
                                                    <div class="mt-2 flex gap-2 justify-center">
                                                        <ShadcnButton
                                                            variant={ButtonVariant::Outline}
                                                            size={ButtonSize::Sm}
                                                            onclick={Callback::from(move |_: MouseEvent| on_start_rename.emit(img_for_rename.clone()))}
                                                        >
                                                            {"Rename"}
                                                        </ShadcnButton>
                                                        <ShadcnButton
                                                            variant={ButtonVariant::Destructive}
                                                            size={ButtonSize::Sm}
                                                            onclick={Callback::from(move |_: MouseEvent| on_delete.emit(id))}
                                                        >
                                                            {"Delete"}
                                                        </ShadcnButton>
                                                    </div>
                                                </div>
                                            }
                                        }) }
                                    </div>
                                </div>
                            }
                        } }
                    </div>
                }
            } else {
                html! {}
            } }

            { if let Some(index) = *pool_lightbox_index {
                let pool_image_urls: Vec<String> = if *pool_expanded {
                    filtered_pool_images.iter().map(|img| format!("/app/pool-images/{}", img.id)).collect()
                } else {
                    pool_images.iter().take(20).map(|img| format!("/app/pool-images/{}", img.id)).collect()
                };
                let on_close = {
                    let pool_lightbox_index = pool_lightbox_index.clone();
                    Callback::from(move |_| pool_lightbox_index.set(None))
                };
                html! {
                    <ImageLightbox
                        images={pool_image_urls}
                        initial_index={index}
                        on_close={on_close}
                    />
                }
            } else {
                html! {}
            } }

            if let Some(topic) = (*editing).clone() {
                <TopicEditor topic={topic} documents={(*documents).clone()} sources_loaded={*sources_loaded} on_refresh_documents={Callback::from({
                    let refresh = refresh.clone();
                    move |_| refresh.emit(())
                })} on_view_document={on_view_document.clone()} on_close={Callback::from({
                    let editing = editing.clone();
                    move |_| editing.set(None)
                })} on_saved={Callback::from({
                    let editing = editing.clone();
                    let refresh_topics = refresh_topics.clone();
                    move |_| {
                        editing.set(None);
                        refresh_topics.emit(());
                    }
                })} />
            }

            <dialog ref={dialog_ref} onclose={on_dialog_close} class="tailscale-dialog">
                if let Some(topic) = &*confirm_delete {
                    <div class="flex items-start gap-4">
                        <div class="flex-shrink-0 flex items-center justify-center size-10 rounded-full bg-destructive/10 text-destructive">
                            <iconify-icon icon="lucide:alert-triangle" class="text-xl"></iconify-icon>
                        </div>
                        <div class="flex-1">
                            <h3 class="text-lg font-semibold leading-6 text-foreground mb-1">
                                {"Delete topic"}
                            </h3>
                            <p class="text-sm text-muted mb-4">
                                {format!("Are you sure you want to delete topic \"{}\" and all its cards? This action cannot be undone.", topic.name)}
                            </p>
                            <div class="flex justify-end gap-3">
                                <ShadcnButton
                                    variant={ButtonVariant::Outline}
                                    onclick={on_cancel_delete}
                                >
                                    {"Cancel"}
                                </ShadcnButton>
                                <ShadcnButton
                                    variant={ButtonVariant::Destructive}
                                    onclick={on_confirm_delete}
                                >
                                    {"Delete"}
                                </ShadcnButton>
                            </div>
                        </div>
                    </div>
                }
            </dialog>
            <dialog ref={rename_dialog_ref} class="tailscale-dialog" onclose={Callback::from({
                let editing_image = editing_image.clone();
                move |_| editing_image.set(None)
            })}>
                if (*editing_image).is_some() {
                    <form onsubmit={on_rename_image} class="space-y-4">
                        <h3 class="text-lg font-semibold">{"Rename Image"}</h3>
                        <div class="space-y-1">
                            <label class="text-sm font-medium">{"Name"}</label>
                            <input
                                type="text"
                                class="w-full rounded-md border px-3 py-2 h-10"
                                value={(*rename_name).clone()}
                                oninput={{
                                    let s = rename_name.clone();
                                    Callback::from(move |e: InputEvent| {
                                        let t: HtmlInputElement = e.target_unchecked_into();
                                        s.set(t.value());
                                    })
                                }}
                            />
                        </div>
                        <div class="space-y-1">
                            <label class="text-sm font-medium">{"Description"}</label>
                            <input
                                type="text"
                                class="w-full rounded-md border px-3 py-2 h-10"
                                value={(*rename_desc).clone()}
                                oninput={{
                                    let s = rename_desc.clone();
                                    Callback::from(move |e: InputEvent| {
                                        let t: HtmlInputElement = e.target_unchecked_into();
                                        s.set(t.value());
                                    })
                                }}
                            />
                        </div>
                        <div class="flex justify-end gap-3">
                            <ShadcnButton
                                variant={ButtonVariant::Outline}
                                r#type={ButtonType::Button}
                                onclick={Callback::from({
                                    let editing_image = editing_image.clone();
                                    move |_: MouseEvent| editing_image.set(None)
                                })}
                            >
                                {"Cancel"}
                            </ShadcnButton>
                            <ShadcnButton r#type={ButtonType::Submit}>
                                {"Save"}
                            </ShadcnButton>
                        </div>
                    </form>
                }
            </dialog>

            <dialog ref={doc_viewer_ref} class="tailscale-dialog max-w-3xl" onclose={Callback::from({
                let viewing_document = viewing_document.clone();
                move |_| viewing_document.set(None)
            })}>
                { if let Some(doc) = &*viewing_document {
                    let viewing_document = viewing_document.clone();
                    html! {
                        <div class="space-y-4">
                            <div class="flex items-start justify-between gap-4">
                                <div class="min-w-0">
                                    <h3 class="text-lg font-semibold truncate">{&doc.title}</h3>
                                    <div class="text-xs text-muted mt-1">
                                        <span class="badge mr-2">{ if doc.source_type == "link" { "link" } else { "doc" } }</span>
                                        { doc.url.as_ref().map(|url| html! {
                                            <a href={url.clone()} target="_blank" class="underline mr-2">{url}</a>
                                        }) }
                                        <span>{&doc.created_at}</span>
                                    </div>
                                </div>
                                <ShadcnButton
                                    variant={ButtonVariant::Ghost}
                                    size={ButtonSize::Icon}
                                    onclick={Callback::from(move |_: MouseEvent| viewing_document.set(None))}
                                >
                                    <iconify-icon icon="radix-icons:cross-2" class="radix-icon" aria-hidden="true"></iconify-icon>
                                    <span class="sr-only">{"Close"}</span>
                                </ShadcnButton>
                            </div>
                            <div class="border-t pt-3">
                                <pre class="text-sm whitespace-pre-wrap break-words max-h-[60vh] overflow-y-auto font-mono p-3 surface rounded-md">
                                    {&doc.content}
                                </pre>
                            </div>
                        </div>
                    }
                } else {
                    html! {}
                } }
            </dialog>
        </section>
    }
}
