use crate::domain::grounding::SearchProvider;
use crate::{
    AppState,
    db::repositories::{documents, image_pool, topics},
    error::{AppError, AppResult},
    http_client,
    image_compress::prepare_image_bytes,
    image_store,
    llm::{ReasoningConfig, annotate_image, remove_tag_json, tags_to_json},
    services::settings::SettingsService,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use rand::{Rng, distributions::Alphanumeric};
use std::collections::HashSet;
use url::Url;

#[derive(Clone, Copy, Debug, Default)]
pub struct DocumentService;

/// Structured result from adding a pool image (includes vision annotation diagnostics).
#[derive(Clone, Debug, serde::Serialize)]
pub struct PoolImageAddResult {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    /// True when the vision model produced a usable annotation.
    pub annotated: bool,
    /// Why annotation was skipped or failed (present when `annotated` is false).
    pub fallback_reason: Option<String>,
    /// Vision/default model that was considered for annotation.
    pub model: Option<String>,
}

/// Result of a non-destructive vision-model connectivity check.
#[derive(Clone, Debug, serde::Serialize)]
pub struct VisionModelTestResult {
    pub ok: bool,
    pub model: String,
    pub message: String,
}

const LINK_FETCH_BYTE_CAP: usize = 2 * 1024 * 1024;
const TITLE_SOURCE_CHAR_CAP: usize = 4_000;
const TITLE_MAX_CHARS: usize = 100;
const TITLE_COMPLETION_TOKEN_CAP: u32 = 32;
const EXPLORE_LINK_CAP: usize = 100;

#[derive(Clone, Debug, serde::Serialize)]
pub struct ExploredLink {
    pub title: String,
    pub url: String,
}

impl DocumentService {
    pub async fn explore_link(url: &str) -> AppResult<Vec<ExploredLink>> {
        let root = Url::parse(url.trim())
            .map_err(|_| AppError::Validation("Enter a valid http(s) URL".to_string()))?;
        if !matches!(root.scheme(), "http" | "https") || root.host_str().is_none() {
            return Err(AppError::Validation(
                "Enter a valid http(s) URL".to_string(),
            ));
        }

        let root_html = fetch_link_html(root.as_str()).await?;
        let (navigation_url, navigation_html) = if root_html.contains("toc.js")
            || root_html.contains("mdbook-sidebar")
            || root_html.contains("mdBook")
        {
            let toc_url = root
                .join("toc.html")
                .map_err(|_| AppError::Validation("Could not resolve site contents".to_string()))?;
            match fetch_link_html(toc_url.as_str()).await {
                Ok(html) => (toc_url, html),
                Err(_) => (root.clone(), root_html),
            }
        } else {
            (root.clone(), root_html)
        };

        let links = extract_navigation_links(&navigation_html, &navigation_url, &root);
        if links.is_empty() {
            return Err(AppError::Validation(
                "No documentation subpages were found".to_string(),
            ));
        }
        Ok(links)
    }

    /// Add a document or link. For links with an empty `content`, fetch the URL
    /// body (capped) and use it as the document content.
    pub async fn add_document(
        state: &AppState,
        user_id: &str,
        topic_ids: &[i64],
        source_type: &str,
        title: &str,
        url: Option<&str>,
        content: &str,
    ) -> AppResult<i64> {
        let source_type = source_type.trim();
        if !matches!(source_type, "document" | "link") {
            return Err(AppError::Validation(
                "source_type must be 'document' or 'link'".to_string(),
            ));
        }
        if source_type == "link" && title.trim().is_empty() {
            return Err(AppError::Validation("title is required".to_string()));
        }
        validate_topic_ids(state, user_id, topic_ids).await?;

        let mut content = content.to_string();
        if source_type == "link" {
            if let Some(link) = url.map(str::trim).filter(|value| !value.is_empty()) {
                if content.trim().is_empty() {
                    let settings = SettingsService::user_settings_get(state, user_id).await?;
                    content = fetch_link_body(link, &settings).await?;
                }
            } else if content.trim().is_empty() {
                return Err(AppError::Validation(
                    "link documents require a url or content".to_string(),
                ));
            }
        }
        if content.trim().is_empty() {
            return Err(AppError::Validation("content is required".to_string()));
        }

        let title = if title.trim().is_empty() {
            generate_document_title(state, user_id, &content).await?
        } else {
            title.trim().to_string()
        };

        documents::insert_document(
            &state.db,
            user_id,
            topic_ids,
            source_type,
            &title,
            url,
            &content,
        )
        .await
    }

    /// Add a document from an uploaded file. Detects the type from the MIME
    /// type / filename and extracts text accordingly:
    /// - PDF: decode + `pdf_extract::extract_text_from_mem`
    /// - HTML: decode + `strip_html`
    /// - Plain text: decode as UTF-8
    pub async fn upload_document(
        state: &AppState,
        user_id: &str,
        topic_ids: &[i64],
        filename: &str,
        mime_type: &str,
        title: Option<&str>,
        data: &[u8],
    ) -> AppResult<i64> {
        let source_type = "document";
        validate_topic_ids(state, user_id, topic_ids).await?;
        let title = title
            .filter(|t| !t.trim().is_empty())
            .map(|t| t.trim().to_string())
            .unwrap_or_else(|| title_from_filename(filename));

        let content = extract_text_from_file(mime_type, filename, data)?;

        if content.trim().is_empty() {
            return Err(AppError::Validation(
                "No text could be extracted from the file".to_string(),
            ));
        }

        documents::insert_document(
            &state.db,
            user_id,
            topic_ids,
            source_type,
            &title,
            None,
            &content,
        )
        .await
    }

    pub async fn list_documents(
        state: &AppState,
        user_id: &str,
        topic_id: Option<i64>,
    ) -> AppResult<Vec<documents::DocumentRecord>> {
        if let Some(topic_id) = topic_id {
            if topics::find_by_id(&state.db, user_id, topic_id)
                .await?
                .is_none()
            {
                return Err(AppError::NotFound("Topic not found".to_string()));
            }
        }
        documents::list_documents(&state.db, user_id, topic_id).await
    }

    pub async fn get_document(
        state: &AppState,
        user_id: &str,
        id: i64,
    ) -> AppResult<Option<documents::DocumentRecord>> {
        documents::get_document_by_id(&state.db, user_id, id).await
    }

    pub async fn delete_document(state: &AppState, user_id: &str, id: i64) -> AppResult<()> {
        documents::delete_document(&state.db, user_id, id).await
    }

    pub async fn attach_document_topic(
        state: &AppState,
        user_id: &str,
        document_id: i64,
        topic_id: i64,
    ) -> AppResult<()> {
        documents::attach_document_topic(&state.db, user_id, document_id, topic_id).await
    }

    pub async fn detach_document_topic(
        state: &AppState,
        user_id: &str,
        document_id: i64,
        topic_id: i64,
    ) -> AppResult<()> {
        documents::detach_document_topic(&state.db, user_id, document_id, topic_id).await
    }

    /// Add a pool image from a data-URL: decode, recompress, persist bytes under
    /// the image dir, then record the row. If a vision model is configured, the
    /// image is automatically annotated (name, description, tags) via the LLM;
    /// on any failure the original `name` is used as fallback.
    ///
    /// Returns structured diagnostics so the UI can show whether annotation ran,
    /// which model was used, and why a fallback occurred.
    pub async fn add_pool_image(
        state: &AppState,
        user_id: &str,
        image_data: &str,
        fallback_name: &str,
        _user_description: Option<&str>,
    ) -> AppResult<PoolImageAddResult> {
        if fallback_name.trim().is_empty() {
            return Err(AppError::Validation("name is required".to_string()));
        }
        let parsed = parse_data_url(image_data)?;
        let prepared = prepare_image_bytes(parsed.bytes, &parsed.mime_type, &parsed.extension)
            .map_err(AppError::Validation)?;
        let settings = SettingsService::user_settings_get(state, user_id).await?;
        tokio::fs::create_dir_all(&state.image_dir)
            .await
            .map_err(AppError::Io)?;
        let storage_path = random_pool_image_name(&prepared.extension);
        let byte_size = prepared.bytes.len() as i64;
        tokio::fs::write(state.image_dir.join(&storage_path), &prepared.bytes)
            .await
            .map_err(AppError::Io)?;

        // Try vision-based auto-annotation. On any failure, fall back to the
        // user-supplied name with empty description and tags. The settings UI
        // promises that an empty vision model inherits the default LLM model.
        let annotation_model =
            preferred_vision_model(&settings.llm_vision_model, &settings.llm_model);
        let annotation_data = format!(
            "data:{};base64,{}",
            prepared.mime_type,
            STANDARD.encode(&prepared.bytes)
        );

        let (annotation, fallback_reason, model_used) = if annotation_model.is_empty() {
            (
                None,
                Some("no vision or default model configured".to_string()),
                None,
            )
        } else if settings.llm_api_key.trim().is_empty() {
            (
                None,
                Some("no API key configured".to_string()),
                Some(annotation_model.to_string()),
            )
        } else {
            match annotate_image(
                annotation_model,
                &annotation_data,
                &settings.llm_api_key,
                &settings.llm_base_url,
            )
            .await
            {
                Some(ann) => (Some(ann), None, Some(annotation_model.to_string())),
                None => (
                    None,
                    Some("vision model returned no usable annotation".to_string()),
                    Some(annotation_model.to_string()),
                ),
            }
        };

        let (name, description, tags, annotated) = match &annotation {
            Some(ann) => {
                let desc = if ann.description.is_empty() {
                    None
                } else {
                    Some(ann.description.as_str())
                };
                (ann.name.as_str(), desc, tags_to_json(&ann.tags), true)
            }
            None => (fallback_name, None, "[]".to_string(), false),
        };

        let insert_result = image_pool::insert_pool_image(
            &state.db,
            user_id,
            &storage_path,
            &prepared.mime_type,
            byte_size,
            name,
            description,
            &tags,
        )
        .await;
        match insert_result {
            Ok(id) => Ok(PoolImageAddResult {
                id,
                name: name.to_string(),
                description: description.map(|s| s.to_string()),
                tags: crate::llm::tags_from_json(&tags),
                annotated,
                fallback_reason,
                model: model_used,
            }),
            Err(err) => {
                let _ = tokio::fs::remove_file(state.image_dir.join(&storage_path)).await;
                Err(err)
            }
        }
    }

    /// Cheap vision-model connectivity check: send a tiny PNG and expect any non-error reply.
    /// Does not write to the image pool. Used by the Settings "Test Vision Model" button.
    pub async fn test_vision_model(
        state: &AppState,
        user_id: &str,
    ) -> AppResult<VisionModelTestResult> {
        let settings = SettingsService::user_settings_get(state, user_id).await?;
        let model = preferred_vision_model(&settings.llm_vision_model, &settings.llm_model);
        if model.is_empty() {
            return Ok(VisionModelTestResult {
                ok: false,
                model: String::new(),
                message: "No vision model or default LLM model configured".to_string(),
            });
        }
        if settings.llm_api_key.trim().is_empty() {
            return Ok(VisionModelTestResult {
                ok: false,
                model: model.to_string(),
                message: "No API key configured".to_string(),
            });
        }

        // 1×1 red PNG (68 bytes). Enough for vision APIs that require an image part.
        const TINY_PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
        let data_url = format!("data:image/png;base64,{TINY_PNG_B64}");
        let response = crate::llm::transport::create_vision_completion(
            model,
            "Reply with exactly one word describing the dominant color in this image.",
            &data_url,
            &settings.llm_api_key,
            &settings.llm_base_url,
            Some(16),
        )
        .await;

        if response.content.is_empty() || response.content.starts_with("LLM Error") {
            return Ok(VisionModelTestResult {
                ok: false,
                model: model.to_string(),
                message: if response.content.is_empty() {
                    "Vision model returned an empty response".to_string()
                } else {
                    response.content
                },
            });
        }

        Ok(VisionModelTestResult {
            ok: true,
            model: model.to_string(),
            message: response.content.trim().to_string(),
        })
    }

    pub async fn list_pool_images(
        state: &AppState,
        user_id: &str,
    ) -> AppResult<Vec<image_pool::ImagePoolRecord>> {
        image_pool::list_pool_images(&state.db, user_id).await
    }

    /// Delete a pool image: remove the on-disk file then the row.
    pub async fn delete_pool_image(state: &AppState, user_id: &str, id: i64) -> AppResult<()> {
        let row = image_pool::find_pool_image(&state.db, user_id, id)
            .await?
            .ok_or_else(|| AppError::NotFound("Pool image not found".to_string()))?;
        image_pool::delete_pool_image(&state.db, user_id, id).await?;
        let _ = tokio::fs::remove_file(state.image_dir.join(&row.storage_path)).await;
        Ok(())
    }

    /// Rename a pool image and optionally update its description.
    pub async fn rename_pool_image(
        state: &AppState,
        user_id: &str,
        id: i64,
        name: &str,
        description: Option<&str>,
    ) -> AppResult<()> {
        if name.trim().is_empty() {
            return Err(AppError::Validation("name is required".to_string()));
        }
        // Preserve existing tags
        let row = image_pool::find_pool_image(&state.db, user_id, id)
            .await?
            .ok_or_else(|| AppError::NotFound("Pool image not found".to_string()))?;
        image_pool::update_pool_image_meta(&state.db, user_id, id, name, description, &row.tags)
            .await
    }

    /// Remove a single tag from a pool image.
    pub async fn remove_pool_image_tag(
        state: &AppState,
        user_id: &str,
        id: i64,
        tag: &str,
    ) -> AppResult<()> {
        let row = image_pool::find_pool_image(&state.db, user_id, id)
            .await?
            .ok_or_else(|| AppError::NotFound("Pool image not found".to_string()))?;
        let new_tags = remove_tag_json(&row.tags, tag);
        image_pool::set_pool_image_tags(&state.db, user_id, id, &new_tags).await
    }
}

async fn generate_document_title(
    state: &AppState,
    user_id: &str,
    content: &str,
) -> AppResult<String> {
    let fallback = fallback_document_title(content);
    let settings = SettingsService::user_settings_get(state, user_id).await?;
    if settings.llm_compress_model.trim().is_empty() || settings.llm_api_key.trim().is_empty() {
        return Ok(fallback);
    }

    let excerpt = content
        .chars()
        .take(TITLE_SOURCE_CHAR_CAP)
        .collect::<String>();
    let prompt = format!(
        "Write a short, specific title for the pasted document below. Use at most 10 words. \
         Return only the title with no quotes, markdown, or explanation.\n\nDocument excerpt:\n{excerpt}"
    );
    let response = crate::llm::transport::create_chat_completion(
        &settings.llm_compress_model,
        &prompt,
        &settings.llm_api_key,
        &settings.llm_compress_base_url,
        &ReasoningConfig::new(&settings.llm_compress_reasoning_effort),
        Some(TITLE_COMPLETION_TOKEN_CAP),
    )
    .await;

    Ok(clean_generated_title(&response.content).unwrap_or(fallback))
}

fn clean_generated_title(value: &str) -> Option<String> {
    if value.starts_with("LLM Error:") {
        return None;
    }
    let title = value
        .lines()
        .next()?
        .trim()
        .trim_matches(['"', '\'', '`', '#', '*'])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if title.is_empty() {
        return None;
    }
    Some(title.chars().take(TITLE_MAX_CHARS).collect())
}

fn fallback_document_title(content: &str) -> String {
    let title = content
        .split_whitespace()
        .take(10)
        .collect::<Vec<_>>()
        .join(" ");
    let title = title.chars().take(TITLE_MAX_CHARS).collect::<String>();
    if title.is_empty() {
        "Pasted document".to_string()
    } else {
        title
    }
}

async fn validate_topic_ids(state: &AppState, user_id: &str, topic_ids: &[i64]) -> AppResult<()> {
    let mut seen = std::collections::HashSet::with_capacity(topic_ids.len());
    for &topic_id in topic_ids {
        if topic_id <= 0 || !seen.insert(topic_id) {
            return Err(AppError::Validation(
                "topic_ids must contain unique positive topic ids".to_string(),
            ));
        }
        if topics::find_by_id(&state.db, user_id, topic_id)
            .await?
            .is_none()
        {
            return Err(AppError::NotFound("Topic not found".to_string()));
        }
    }
    Ok(())
}

struct ParsedDataUrl {
    mime_type: String,
    extension: String,
    bytes: Vec<u8>,
}

fn parse_data_url(value: &str) -> AppResult<ParsedDataUrl> {
    let Some((header, payload)) = value.split_once(',') else {
        return Err(AppError::Validation("Invalid image data URL".to_string()));
    };
    let mime_type = header
        .strip_prefix("data:")
        .and_then(|value| value.strip_suffix(";base64"))
        .ok_or_else(|| AppError::Validation("Invalid image data URL".to_string()))?;
    let extension = match mime_type {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => {
            return Err(AppError::Validation(
                "Only PNG, JPEG, WebP, or GIF data URLs are supported".to_string(),
            ));
        }
    };
    let bytes = STANDARD
        .decode(payload)
        .map_err(|_| AppError::Validation("Invalid base64 image data".to_string()))?;
    crate::domain::image::validate_decoded_image_size(bytes.len())?;
    Ok(ParsedDataUrl {
        mime_type: mime_type.to_string(),
        extension: extension.to_string(),
        bytes,
    })
}

fn random_pool_image_name(extension: &str) -> String {
    let suffix: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(18)
        .map(char::from)
        .collect();
    format!("pool-{suffix}.{extension}")
}

fn preferred_vision_model<'a>(vision_model: &'a str, default_model: &'a str) -> &'a str {
    let vision_model = vision_model.trim();
    if vision_model.is_empty() {
        default_model.trim()
    } else {
        vision_model
    }
}

/// Derive a human-readable title from a filename by stripping the extension
/// and replacing separators with spaces, title-casing each word.
fn title_from_filename(filename: &str) -> String {
    let stem = filename.rsplit('.').nth(1).unwrap_or(filename);
    stem.replace(['-', '_'], " ")
        .split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Extract text from uploaded file bytes based on MIME type or file extension.
fn extract_text_from_file(mime_type: &str, filename: &str, data: &[u8]) -> AppResult<String> {
    let lower_mime = mime_type.to_lowercase();
    let lower_name = filename.to_lowercase();

    let is_pdf = lower_mime == "application/pdf" || lower_name.ends_with(".pdf");
    let is_html = lower_mime.contains("html")
        || lower_name.ends_with(".html")
        || lower_name.ends_with(".htm");

    if is_pdf {
        extract_pdf_text(data)
    } else if is_html {
        let text = String::from_utf8_lossy(data).to_string();
        Ok(strip_html(&text))
    } else {
        // Treat everything else as plain text.
        Ok(String::from_utf8_lossy(data).to_string())
    }
}

/// Extract text from PDF bytes using `pdf_extract`.
fn extract_pdf_text(data: &[u8]) -> AppResult<String> {
    pdf_extract::extract_text_from_mem(data)
        .map(|output| {
            // Normalize: collapse excessive blank lines but preserve structure.
            output
                .lines()
                .map(|l| l.trim())
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string()
        })
        .map_err(|e| AppError::Validation(format!("Failed to extract PDF text: {e}")))
}

/// Fetch a link's body text. Firecrawl converts web pages and supported remote
/// documents (including PDFs) to Markdown; Tavily/default mode keeps the local,
/// capped HTML fetch used by existing installations.
async fn fetch_link_body(url: &str, settings: &crate::config::Settings) -> AppResult<String> {
    if SearchProvider::from_setting(&settings.search_provider) == SearchProvider::Firecrawl
        && !settings.search_api_key.trim().is_empty()
    {
        return scrape_with_firecrawl(url, &settings.search_base_url, &settings.search_api_key)
            .await;
    }

    let http = http_client::shared();
    let response = match http.get(url).send().await {
        Ok(res) if res.status().is_success() => res,
        _ => return Ok(String::new()),
    };
    // Bound the download: read at most the cap in bytes.
    let bytes = match response.bytes().await {
        Ok(bytes) if bytes.len() <= LINK_FETCH_BYTE_CAP => bytes,
        Ok(bytes) => bytes.slice(..LINK_FETCH_BYTE_CAP),
        Err(_) => return Ok(String::new()),
    };
    let text = String::from_utf8_lossy(&bytes).to_string();
    Ok(strip_html(&text))
}

async fn scrape_with_firecrawl(url: &str, base_url: &str, api_key: &str) -> AppResult<String> {
    let endpoint = format!("{}/v2/scrape", base_url.trim_end_matches('/'));
    let body = firecrawl_scrape_body(url);
    let value = image_store::post_public_json_bearer(&endpoint, &body, api_key)
        .await
        .map_err(|(_, message)| {
            AppError::Validation(format!(
                "Firecrawl could not scrape this document: {message}"
            ))
        })?;
    let markdown = value
        .get("data")
        .and_then(|data| data.get("markdown"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if markdown.is_empty() {
        return Err(AppError::Validation(
            "Firecrawl returned no document content".to_string(),
        ));
    }
    Ok(markdown)
}

fn firecrawl_scrape_body(url: &str) -> serde_json::Value {
    serde_json::json!({
        "url": url,
        "formats": ["markdown"],
        "onlyMainContent": true,
        "parsers": ["pdf"]
    })
}

async fn fetch_link_html(url: &str) -> AppResult<String> {
    let response = http_client::shared()
        .get(url)
        .send()
        .await
        .map_err(|_| AppError::Validation("Could not fetch that URL".to_string()))?;
    if !response.status().is_success() {
        return Err(AppError::Validation(format!(
            "URL returned HTTP {}",
            response.status()
        )));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| AppError::Validation("Could not read that URL".to_string()))?;
    if bytes.len() > LINK_FETCH_BYTE_CAP {
        return Err(AppError::Validation(
            "Documentation page is too large to explore".to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

fn extract_navigation_links(html: &str, base: &Url, root: &Url) -> Vec<ExploredLink> {
    let mut links = Vec::new();
    let mut seen = HashSet::new();
    let mut remaining = html;

    while let Some(anchor_start) = remaining.find("<a") {
        remaining = &remaining[anchor_start + 2..];
        let Some(tag_end) = remaining.find('>') else {
            break;
        };
        let attributes = &remaining[..tag_end];
        let after_tag = &remaining[tag_end + 1..];
        let Some(close) = after_tag.find("</a>") else {
            remaining = after_tag;
            continue;
        };
        let title = strip_html(&after_tag[..close]);
        remaining = &after_tag[close + 4..];

        let Some(href) = html_attribute(attributes, "href") else {
            continue;
        };
        let Ok(mut target) = base.join(&decode_entities(href)) else {
            continue;
        };
        target.set_fragment(None);
        if target.scheme() != root.scheme()
            || target.host_str() != root.host_str()
            || target.port_or_known_default() != root.port_or_known_default()
            || title.is_empty()
            || target == *root
            || target.path().ends_with("title-page.html")
            || !is_document_page(&target)
        {
            continue;
        }
        let value = target.to_string();
        if seen.insert(value.clone()) {
            links.push(ExploredLink { title, url: value });
            if links.len() == EXPLORE_LINK_CAP {
                break;
            }
        }
    }
    links
}

fn html_attribute<'a>(attributes: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("{name}=");
    let start = attributes.find(&marker)? + marker.len();
    let rest = attributes[start..].trim_start();
    let quote = rest.chars().next()?;
    if quote == '"' || quote == '\'' {
        let value = &rest[1..];
        return value.find(quote).map(|end| &value[..end]);
    }
    let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    Some(&rest[..end])
}

fn is_document_page(url: &Url) -> bool {
    let path = url.path().to_ascii_lowercase();
    ![
        ".css", ".js", ".json", ".xml", ".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp", ".ico",
        ".woff", ".woff2", ".ttf", ".zip", ".pdf",
    ]
    .iter()
    .any(|extension| path.ends_with(extension))
}

/// HTML→text: strips tags, preserves `<pre>`/`<code>` blocks as fenced code
/// (with nested syntax-highlighting spans removed), skips `<script>`/`<style>`
/// entirely, inserts newlines at block-level boundaries, decodes common
/// entities, and collapses whitespace per-line. Good for RAG indexing.
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut pos = 0;

    while pos < html.len() {
        let rest = &html[pos..];
        let tag_start = match rest.find('<') {
            Some(i) => pos + i,
            None => {
                out.push_str(&decode_entities(rest));
                break;
            }
        };

        // Text before the tag
        if tag_start > pos {
            out.push_str(&decode_entities(&html[pos..tag_start]));
        }

        let tag_rest = &html[tag_start..];
        let gt_pos = match tag_rest.find('>') {
            Some(i) => i,
            None => {
                out.push_str(&decode_entities(tag_rest));
                break;
            }
        };

        let tag_inner = tag_rest[1..gt_pos].trim();
        let tag_lower = tag_inner.to_lowercase();
        let is_closing = tag_lower.starts_with('/');
        let tag_name = tag_lower
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/' || c == '>')
            .next()
            .unwrap_or("");

        // Skip <script>/<style> content entirely — noise for text extraction.
        if (tag_name == "script" || tag_name == "style") && !is_closing {
            let close_marker = format!("</{}", tag_name);
            let after_tag = &tag_rest[gt_pos + 1..];
            if let Some(close_pos) = after_tag.to_lowercase().find(&close_marker) {
                let after_close = &after_tag[close_pos..];
                pos = tag_start
                    + gt_pos
                    + 1
                    + close_pos
                    + after_close.find('>').map(|i| i + 1).unwrap_or(0);
                continue;
            }
        }

        // Preserve <pre>/<code> content as fenced code, stripping nested
        // syntax-highlighting tags (<span>, <font>, <div>, etc.) while
        // keeping text and newlines.
        if (tag_name == "pre" || tag_name == "code") && !is_closing {
            let close_marker = format!("</{}", tag_name);
            let after_tag = &tag_rest[gt_pos + 1..];
            if let Some(close_pos) = after_tag.to_lowercase().find(&close_marker) {
                let code = strip_inline_tags(after_tag[..close_pos].trim());
                out.push_str("\n```\n");
                out.push_str(&code);
                out.push_str("\n```\n");
                let after_close = &after_tag[close_pos..];
                pos = tag_start
                    + gt_pos
                    + 1
                    + close_pos
                    + after_close.find('>').map(|i| i + 1).unwrap_or(0);
                continue;
            }
        }

        // Block-level boundaries → newline
        if is_block_tag(tag_name, is_closing) {
            out.push('\n');
        }

        pos = tag_start + gt_pos + 1;
    }

    normalize_whitespace(&out)
}

/// Strip all HTML tags from a string while preserving text content and
/// converting block-level tags to newlines. Used inside `<pre>`/`<code>`
/// blocks to remove syntax-highlighting `<span>`/`<font>` wrappers.
fn strip_inline_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pos = 0;

    while pos < s.len() {
        let rest = &s[pos..];
        let tag_start = match rest.find('<') {
            Some(i) => pos + i,
            None => {
                out.push_str(&decode_entities(rest));
                break;
            }
        };

        if tag_start > pos {
            out.push_str(&decode_entities(&s[pos..tag_start]));
        }

        let tag_rest = &s[tag_start..];
        let gt_pos = match tag_rest.find('>') {
            Some(i) => i,
            None => break,
        };

        let tag_inner = tag_rest[1..gt_pos].trim().to_lowercase();
        let is_closing = tag_inner.starts_with('/');
        let tag_name = tag_inner
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/' || c == '>')
            .next()
            .unwrap_or("");

        // Convert block-level boundaries inside code to newlines so
        // <div class="ec-line">...</div> blocks stay on separate lines.
        if matches!(tag_name, "div" | "br" | "p" | "li" | "tr")
            || (is_closing && matches!(tag_name, "div" | "p" | "li" | "tr"))
        {
            out.push('\n');
        }

        pos = tag_start + gt_pos + 1;
    }

    // Collapse runs of whitespace within a line to a single space, and
    // collapse runs of blank lines to a single newline.
    out.lines()
        .map(|l| {
            l.split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .trim_end()
                .to_string()
        })
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Tags that represent block-level boundaries (produce a line break).
fn is_block_tag(tag: &str, is_closing: bool) -> bool {
    matches!(
        tag,
        "p" | "div"
            | "br"
            | "li"
            | "tr"
            | "ul"
            | "ol"
            | "table"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "blockquote"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
    ) || (is_closing && matches!(tag, "pre" | "code"))
}

/// Decode the most common HTML entities. Not exhaustive — good enough for
/// text extraction from fetched pages.
fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&#x26;", "&")
        .replace("&nbsp;", " ")
}

/// Collapse runs of spaces/tabs to a single space per line, drop empty lines,
/// and preserve ```-fenced code blocks verbatim.
fn normalize_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_code = false;

    for line in s.lines() {
        let trimmed = line.trim();
        if trimmed == "```" || trimmed.starts_with("```") {
            in_code = !in_code;
            out.push_str(trimmed);
            out.push('\n');
            continue;
        }
        if in_code {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let collapsed: String = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
        if !collapsed.is_empty() {
            out.push_str(&collapsed);
            out.push('\n');
        }
    }

    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        clean_generated_title, extract_navigation_links, fallback_document_title,
        firecrawl_scrape_body, preferred_vision_model, strip_html,
    };
    use url::Url;

    #[test]
    fn documentation_navigation_keeps_order_and_same_origin_pages() {
        let html = r#"<ol class="chapter">
            <li><a href="title-page.html">Helix</a></li>
            <li><a href="install.html"><strong>1.</strong> Installation</a></li>
            <li><a href="guides/adding_languages.html"><strong>5.1.</strong> Adding languages</a></li>
            <li><a href="https://elsewhere.example/docs.html">Other site</a></li>
            <li><a href="theme.css">Styles</a></li>
        </ol>"#;
        let root = Url::parse("https://docs.helix-editor.com/").unwrap();
        let toc = root.join("toc.html").unwrap();
        let links = extract_navigation_links(html, &toc, &root);

        assert_eq!(links.len(), 2);
        assert_eq!(links[0].title, "1. Installation");
        assert_eq!(links[0].url, "https://docs.helix-editor.com/install.html");
        assert_eq!(
            links[1].url,
            "https://docs.helix-editor.com/guides/adding_languages.html"
        );
    }

    #[test]
    fn firecrawl_scrape_requests_markdown_and_pdf_parsing() {
        let body = firecrawl_scrape_body("https://example.com/guide.pdf");
        assert_eq!(body["url"], "https://example.com/guide.pdf");
        assert_eq!(body["formats"], serde_json::json!(["markdown"]));
        assert_eq!(body["onlyMainContent"], true);
        assert_eq!(body["parsers"], serde_json::json!(["pdf"]));
    }

    #[test]
    fn generated_document_titles_are_cleaned_and_capped() {
        assert_eq!(
            clean_generated_title("**A focused document title**\nExtra explanation"),
            Some("A focused document title".to_string())
        );
        assert_eq!(clean_generated_title("LLM Error: unavailable"), None);
        assert!(clean_generated_title(&"x".repeat(150)).unwrap().len() <= 100);
    }

    #[test]
    fn fallback_document_title_uses_only_first_ten_words() {
        assert_eq!(
            fallback_document_title("one two three four five six seven eight nine ten eleven"),
            "one two three four five six seven eight nine ten"
        );
    }

    #[test]
    fn configured_vision_model_is_preferred() {
        assert_eq!(
            preferred_vision_model(" vision/model ", "default/model"),
            "vision/model"
        );
    }

    #[test]
    fn empty_vision_model_inherits_default_model() {
        assert_eq!(
            preferred_vision_model("  ", " default/model "),
            "default/model"
        );
    }

    #[test]
    fn strip_html_drops_tags() {
        assert_eq!(strip_html("<p>hello <b>world</b></p>"), "hello world");
        assert_eq!(strip_html("plain text"), "plain text");
    }

    #[test]
    fn strip_html_preserves_code_blocks() {
        let html =
            "<p>Install with:</p>\n<pre>sudo pacman -S helix\nhx --health</pre>\n<p>Done.</p>";
        let result = strip_html(html);
        assert!(result.contains("```\nsudo pacman -S helix\nhx --health\n```"));
        assert!(result.contains("Install with:"));
        assert!(result.contains("Done."));
    }

    #[test]
    fn strip_html_preserves_inline_code() {
        let html = "<p>Run <code>hx --health</code> to check.</p>";
        let result = strip_html(html);
        assert!(result.contains("```\nhx --health\n```"));
        assert!(result.contains("Run"));
        assert!(result.contains("to check."));
    }

    #[test]
    fn strip_html_decodes_entities() {
        assert_eq!(strip_html("a &amp; b &lt; c &gt; d"), "a & b < c > d");
        assert_eq!(strip_html("it&#39;s"), "it's");
    }

    #[test]
    fn strip_html_block_breaks() {
        let html = "<h1>Title</h1><p>Para one</p><p>Para two</p>";
        let result = strip_html(html);
        assert!(result.contains("Title"));
        assert!(result.contains("Para one"));
        assert!(result.contains("Para two"));
        // Each should be on its own line
        assert!(
            result.contains("Title\nPara one\nPara two") || result.contains("Title\n\nPara one")
        );
    }

    #[test]
    fn strip_html_strips_syntax_highlighting_spans_in_pre() {
        let html = r##"<pre><span style="color:#A4A0E8">Hello</span> <font color="#5A5977"> </font><span style="color:#A4A0E8">helix!</span></pre>"##;
        let result = strip_html(html);
        assert!(result.contains("```"));
        assert!(result.contains("Hello helix!"));
        // No CSS/style attributes should leak through.
        assert!(!result.contains("color:"));
        assert!(!result.contains("style="));
        assert!(!result.contains("<span"));
        assert!(!result.contains("<font"));
    }

    #[test]
    fn strip_html_strips_font_tags_in_code() {
        let html = r##"<code><span style="--0:#d6deeb">The plates will</span></code>"##;
        let result = strip_html(html);
        assert!(result.contains("The plates will"));
        assert!(!result.contains("style="));
        assert!(!result.contains("<span"));
    }

    #[test]
    fn strip_html_skips_script_and_style_blocks() {
        let html = r##"<style>.foo { color: red; }</style><p>visible text</p><script>console.log("hidden");</script>"##;
        let result = strip_html(html);
        assert!(result.contains("visible text"));
        assert!(!result.contains("color: red"));
        assert!(!result.contains("console.log"));
        assert!(!result.contains(".foo"));
    }

    #[test]
    fn strip_html_handles_ec_line_divs_in_code() {
        let html = r##"<code><div class="ec-line"><span>The plates will</span></div><div class="ec-line"><span>and the clouds will</span></div></code>"##;
        let result = strip_html(html);
        assert!(result.contains("The plates will"));
        assert!(result.contains("and the clouds will"));
        // The two lines should be separated by a newline, not concatenated.
        assert!(result.contains("The plates will\nand the clouds will"));
        assert!(!result.contains("<div"));
        assert!(!result.contains("<span"));
    }
}
