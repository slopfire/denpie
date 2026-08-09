use std::collections::HashSet;

use gloo_net::http::Request;
use serde::Serialize;
use web_sys::HtmlInputElement;
use yew::{create_portal, prelude::*};

use crate::api_v1;
use crate::image_compress::{collect_files, compress_files_to_data_urls};

const MAX_CARD_IMAGES: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
enum PickerTab {
    Upload,
    Pool,
    Url,
    Suggest,
}

#[derive(Clone, Debug, PartialEq)]
struct PoolImageInfo {
    id: i64,
    name: String,
    description: Option<String>,
    tags: Vec<String>,
}

fn from_pool_image_row(img: api_v1::PoolImageRow) -> PoolImageInfo {
    PoolImageInfo {
        id: img.id,
        name: img.name,
        description: img.description,
        tags: img.tags,
    }
}

#[derive(Serialize)]
struct AppendImagesReq {
    card_id: i64,
    image_data: Vec<String>,
    pool_image_ids: Vec<i64>,
    urls: Vec<String>,
}

#[derive(Properties, PartialEq)]
pub struct CardImagePickerProps {
    pub open: bool,
    pub card_id: i64,
    pub existing_count: usize,
    pub context: String,
    pub on_close: Callback<()>,
    pub on_success: Callback<()>,
    pub on_error: Callback<String>,
}

fn context_score(image: &PoolImageInfo, terms: &[String]) -> usize {
    let searchable = format!(
        "{} {} {}",
        image.name,
        image.description.as_deref().unwrap_or_default(),
        image.tags.join(" ")
    )
    .to_lowercase();
    terms
        .iter()
        .filter(|term| term.len() > 2 && searchable.contains(term.as_str()))
        .count()
}

#[function_component(CardImagePicker)]
pub fn card_image_picker(props: &CardImagePickerProps) -> Html {
    let tab = use_state(|| PickerTab::Upload);
    let uploads = use_state(Vec::<String>::new);
    let selected_pool = use_state(HashSet::<i64>::new);
    let url = use_state(String::new);
    let search = use_state(String::new);
    let pool_images = use_state(Vec::<PoolImageInfo>::new);
    let pool_loading = use_state(|| false);
    let processing_upload = use_state(|| false);
    let saving = use_state(|| false);
    let session = use_mut_ref(|| 0_u64);
    let portal_host = use_state(|| {
        web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.body())
    });

    {
        let tab = tab.clone();
        let uploads = uploads.clone();
        let selected_pool = selected_pool.clone();
        let url = url.clone();
        let search = search.clone();
        let pool_images = pool_images.clone();
        let pool_loading = pool_loading.clone();
        let processing_upload = processing_upload.clone();
        let session = session.clone();
        let on_error = props.on_error.clone();
        use_effect_with((props.open, props.card_id), move |(open, _card_id)| {
            *session.borrow_mut() += 1;
            if *open {
                tab.set(PickerTab::Upload);
                uploads.set(Vec::new());
                selected_pool.set(HashSet::new());
                url.set(String::new());
                search.set(String::new());
                processing_upload.set(false);
                pool_loading.set(true);
                let session_id = *session.borrow();
                wasm_bindgen_futures::spawn_local(async move {
                    let result = api_v1::list_pool_images().await;
                    if *session.borrow() != session_id {
                        return;
                    }
                    match result {
                        Ok(images) => {
                            pool_images.set(images.into_iter().map(from_pool_image_row).collect())
                        }
                        Err(error) => on_error.emit(error.to_string()),
                    }
                    pool_loading.set(false);
                });
            }
            || ()
        });
    }

    if !props.open {
        return Html::default();
    }

    let remaining = MAX_CARD_IMAGES.saturating_sub(props.existing_count);
    let selected_count = uploads.len() + selected_pool.len() + usize::from(!url.trim().is_empty());

    let on_files = {
        let uploads = uploads.clone();
        let selected_pool = selected_pool.clone();
        let url = url.clone();
        let processing_upload = processing_upload.clone();
        let session = session.clone();
        let on_error = props.on_error.clone();
        Callback::from(move |event: Event| {
            let Some(input) = event.target_dyn_into::<HtmlInputElement>() else {
                return;
            };
            let Some(files) = input.files() else {
                return;
            };
            let files = collect_files(&files);
            input.set_value("");
            if files.is_empty() {
                return;
            }
            processing_upload.set(true);
            let session_id = *session.borrow();
            let uploads = uploads.clone();
            let selected_pool = selected_pool.clone();
            let url = url.clone();
            let processing_upload = processing_upload.clone();
            let session = session.clone();
            let on_error = on_error.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let result = compress_files_to_data_urls(files).await;
                if *session.borrow() != session_id {
                    return;
                }
                match result {
                    Ok(mut images) => {
                        let available = remaining.saturating_sub(
                            uploads.len()
                                + selected_pool.len()
                                + usize::from(!url.trim().is_empty()),
                        );
                        if images.len() > available {
                            images.truncate(available);
                            on_error
                                .emit(format!("A card can have at most {MAX_CARD_IMAGES} images"));
                        }
                        let mut next = (*uploads).clone();
                        next.append(&mut images);
                        uploads.set(next);
                    }
                    Err(message) => on_error.emit(message),
                }
                processing_upload.set(false);
            });
        })
    };

    let toggle_pool_image = {
        let selected_pool = selected_pool.clone();
        let uploads = uploads.clone();
        let url = url.clone();
        let on_error = props.on_error.clone();
        Callback::from(move |id: i64| {
            let mut next = (*selected_pool).clone();
            if !next.remove(&id) {
                let count = uploads.len() + next.len() + usize::from(!url.trim().is_empty());
                if count >= remaining {
                    on_error.emit(format!("A card can have at most {MAX_CARD_IMAGES} images"));
                    return;
                }
                next.insert(id);
            }
            selected_pool.set(next);
        })
    };

    let on_submit = {
        let uploads = uploads.clone();
        let selected_pool = selected_pool.clone();
        let url = url.clone();
        let saving = saving.clone();
        let on_success = props.on_success.clone();
        let on_error = props.on_error.clone();
        let session = session.clone();
        let card_id = props.card_id;
        Callback::from(move |_| {
            if *saving {
                return;
            }
            let trimmed_url = url.trim().to_string();
            let request = AppendImagesReq {
                card_id,
                image_data: (*uploads).clone(),
                pool_image_ids: selected_pool.iter().copied().collect(),
                urls: if trimmed_url.is_empty() {
                    Vec::new()
                } else {
                    vec![trimmed_url]
                },
            };
            if request.image_data.is_empty()
                && request.pool_image_ids.is_empty()
                && request.urls.is_empty()
            {
                on_error.emit("Choose at least one image".into());
                return;
            }
            saving.set(true);
            let session_id = *session.borrow();
            let saving = saving.clone();
            let on_success = on_success.clone();
            let on_error = on_error.clone();
            let session = session.clone();
            wasm_bindgen_futures::spawn_local(async move {
                // Session JSON — no v1 op for tipcard-images/append.
                let result = match Request::post("/app/tipcard-images/append").json(&request) {
                    Ok(builder) => match builder.send().await {
                        Ok(response) if response.ok() => Ok(()),
                        Ok(response) => Err(response
                            .text()
                            .await
                            .unwrap_or_else(|_| "Failed to attach images".into())),
                        Err(error) => Err(error.to_string()),
                    },
                    Err(error) => Err(error.to_string()),
                };
                if *session.borrow() != session_id {
                    return;
                }
                match result {
                    Ok(()) => on_success.emit(()),
                    Err(message) => on_error.emit(message),
                }
                saving.set(false);
            });
        })
    };

    let query = search.trim().to_lowercase();
    let mut visible_images: Vec<_> = pool_images
        .iter()
        .filter(|image| {
            query.is_empty()
                || image.name.to_lowercase().contains(&query)
                || image
                    .description
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(&query)
                || image
                    .tags
                    .iter()
                    .any(|tag| tag.to_lowercase().contains(&query))
        })
        .cloned()
        .collect();
    if *tab == PickerTab::Suggest {
        let terms: Vec<String> = props
            .context
            .split(|character: char| !character.is_alphanumeric())
            .map(str::to_lowercase)
            .filter(|term| term.len() > 2)
            .collect();
        visible_images.sort_by_key(|image| std::cmp::Reverse(context_score(image, &terms)));
    }

    let pool_grid = |empty_message: &'static str| {
        if *pool_loading {
            html! { <p class="text-sm text-muted py-8 text-center">{"Loading images..."}</p> }
        } else if visible_images.is_empty() {
            html! { <p class="text-sm text-muted py-8 text-center">{empty_message}</p> }
        } else {
            html! {
                <div id="card-image-pool-grid" class="grid grid-cols-2 sm:grid-cols-3 gap-3 overflow-y-auto max-h-[48vh] pr-1">
                    { for visible_images.iter().map(|image| {
                        let id = image.id;
                        let selected = selected_pool.contains(&id);
                        let toggle = toggle_pool_image.clone();
                        html! {
                            <button
                                type="button"
                                class={classes!("relative", "rounded-md", "border", "p-2", "text-left", selected.then_some("border-primary bg-primary-soft"))}
                                disabled={*processing_upload}
                                aria-pressed={selected.to_string()}
                                onclick={Callback::from(move |_| toggle.emit(id))}
                            >
                                <img src={api_v1::pool_image_url(id)} alt="" class="w-full h-28 object-cover rounded" loading="lazy" />
                                <span class="block mt-2 text-sm font-medium truncate">{&image.name}</span>
                                if selected {
                                    <span class="absolute top-3 right-3 rounded-full bg-primary-solid p-1 inline-flex">
                                        <iconify-icon icon="radix-icons:check" class="radix-icon"></iconify-icon>
                                    </span>
                                }
                            </button>
                        }
                    }) }
                </div>
            }
        }
    };

    let Some(host) = portal_host.as_ref() else {
        return Html::default();
    };
    create_portal(
        html! {
            <div
                id="card-image-picker"
                class="fixed inset-0 z-[90] bg-black/60 p-3 sm:p-6 flex items-end sm:items-center justify-center"
                role="dialog"
                aria-modal="true"
                aria-label="Attach images"
                onclick={Callback::from({
                    let on_close = props.on_close.clone();
                    move |_| on_close.emit(())
                })}
            >
                <div class="surface border rounded-lg w-full max-w-3xl max-h-[92vh] flex flex-col overflow-hidden" onclick={Callback::from(|event: MouseEvent| event.stop_propagation())}>
                    <div class="flex items-start justify-between gap-4 p-4 border-b border-token">
                        <div>
                            <h2 class="text-lg font-semibold">{"Attach images"}</h2>
                            <p class="text-sm text-muted mt-1">{format!("Choose up to {remaining} more image{} for this card.", if remaining == 1 { "" } else { "s" })}</p>
                        </div>
                        <button id="card-image-picker-close" type="button" class="border border-token p-2" onclick={Callback::from({
                            let on_close = props.on_close.clone();
                            move |_| on_close.emit(())
                        })}>
                            <iconify-icon icon="radix-icons:cross-2" class="radix-icon"></iconify-icon>
                        </button>
                    </div>

                    <div class="grid grid-cols-4 border-b border-token p-1" role="tablist">
                        { for [
                            (PickerTab::Upload, "Upload", "radix-icons:upload"),
                            (PickerTab::Pool, "Local Pool", "radix-icons:image"),
                            (PickerTab::Url, "URL", "radix-icons:link-2"),
                            (PickerTab::Suggest, "Suggest", "radix-icons:magic-wand"),
                        ].into_iter().map(|(value, label, icon)| {
                            let tab = tab.clone();
                            html! {
                                <button type="button" role="tab" disabled={*processing_upload} aria-selected={(*tab == value).to_string()} class={classes!("rounded-md", "px-2", "py-2", "text-sm", "font-medium", "flex", "items-center", "justify-center", "gap-2", (*tab == value).then_some("bg-primary-soft text-primary"))} onclick={Callback::from(move |_| tab.set(value))}>
                                    <iconify-icon icon={icon} class="radix-icon hidden sm:inline-flex"></iconify-icon>
                                    <span>{label}</span>
                                </button>
                            }
                        }) }
                    </div>

                    <div class="p-4 overflow-y-auto flex-1 min-h-[15rem]">
                        if remaining == 0 {
                            <p class="text-sm text-muted py-10 text-center">{format!("This card already has the maximum of {MAX_CARD_IMAGES} images.")}</p>
                        } else if *tab == PickerTab::Upload {
                            <label class="block rounded-lg border border-dashed border-token p-8 text-center cursor-pointer">
                                <iconify-icon icon="radix-icons:upload" class="radix-icon text-primary"></iconify-icon>
                                <span class="block mt-3 font-medium">{"Choose images from this device"}</span>
                                <span class="block mt-1 text-sm text-muted">{"PNG, JPEG, WebP, or GIF"}</span>
                                <input id="card-image-upload" type="file" accept="image/*" multiple=true class="hidden" disabled={*processing_upload} onchange={on_files} />
                            </label>
                            if *processing_upload {
                                <p class="text-sm text-muted mt-3 text-center">{"Preparing images..."}</p>
                            }
                            if !uploads.is_empty() {
                                <div class="grid grid-cols-3 sm:grid-cols-4 gap-2 mt-4">
                                    { for uploads.iter().enumerate().map(|(index, image)| {
                                        let uploads = uploads.clone();
                                        html! {
                                            <div class="relative">
                                                <img src={image.clone()} alt="" class="w-full h-24 object-cover rounded-md" />
                                                <button type="button" aria-label="Remove selected image" class="absolute top-1 right-1 bg-black/70 text-white rounded-full p-1" onclick={Callback::from(move |_| {
                                                    let mut next = (*uploads).clone();
                                                    next.remove(index);
                                                    uploads.set(next);
                                                })}>
                                                    <iconify-icon icon="radix-icons:cross-2" class="radix-icon"></iconify-icon>
                                                </button>
                                            </div>
                                        }
                                    }) }
                                </div>
                            }
                        } else if *tab == PickerTab::Url {
                            <div class="max-w-xl mx-auto py-8">
                                <label for="card-image-url" class="block text-sm font-medium mb-2">{"Direct image URL"}</label>
                                <input id="card-image-url" type="url" class="w-full rounded-md border px-3 py-2" placeholder="https://example.com/image.jpg" disabled={*processing_upload} value={(*url).clone()} oninput={Callback::from({
                                    let url = url.clone();
                                    move |event: InputEvent| {
                                        if let Some(input) = event.target_dyn_into::<HtmlInputElement>() {
                                            url.set(input.value());
                                        }
                                    }
                                })} />
                                <p class="text-sm text-muted mt-2">{"The server validates the address, image type, redirects, and download size before attaching it."}</p>
                            </div>
                        } else {
                            <div class="flex items-center gap-2 mb-3">
                                <iconify-icon icon="radix-icons:magnifying-glass" class="radix-icon text-muted"></iconify-icon>
                                <input id="card-image-pool-search" type="search" class="w-full rounded-md border px-3 py-2" placeholder="Search names, descriptions, and tags" value={(*search).clone()} oninput={Callback::from({
                                    let search = search.clone();
                                    move |event: InputEvent| {
                                        if let Some(input) = event.target_dyn_into::<HtmlInputElement>() {
                                            search.set(input.value());
                                        }
                                    }
                                })} />
                            </div>
                            if *tab == PickerTab::Suggest {
                                <p class="text-sm text-muted mb-3">{"Suggestions are ranked from your Local Image Pool using this card's topic and title."}</p>
                                {pool_grid("No matching images are available in the Local Image Pool.")}
                            } else {
                                {pool_grid("No images are available in the Local Image Pool yet.")}
                            }
                        }
                    </div>

                    <div class="p-4 border-t border-token flex items-center justify-between gap-3">
                        <span class="text-sm text-muted">{format!("{selected_count} selected")}</span>
                        <div class="flex gap-2">
                            <button type="button" class="rounded-md border border-token px-4 py-2" onclick={Callback::from({
                                let on_close = props.on_close.clone();
                                move |_| on_close.emit(())
                            })}>{"Cancel"}</button>
                            <button id="card-image-picker-attach" type="button" class="rounded-md bg-primary-solid px-4 py-2 font-medium" disabled={*saving || *processing_upload || selected_count == 0 || remaining == 0} onclick={on_submit}>
                                {if *saving { "Attaching..." } else { "Attach" }}
                            </button>
                        </div>
                    </div>
                </div>
            </div>
        },
        host.clone().into(),
    )
}
