use std::{
    io::Cursor,
    net::{IpAddr, SocketAddr},
    path::Path,
    time::Duration,
};

use axum::http::StatusCode;
use base64::{Engine, engine::general_purpose::STANDARD};
use rand::{Rng, distributions::Alphanumeric};
use reqwest::{Client, redirect::Policy};
use sqlx::PgPool;
use tokio::fs;
use url::Url;

use crate::db::repositories::tipcards::{self, TipcardImageRecord};

type StatusResult<T> = Result<T, (StatusCode, String)>;

pub enum IncomingImage {
    DataUrl(String),
    Bytes { bytes: Vec<u8>, mime_type: String },
}

pub async fn replace_card_images(
    pool: &PgPool,
    image_dir: &Path,
    user_id: &str,
    card_id: i64,
    image_data: Vec<String>,
) -> StatusResult<()> {
    fs::create_dir_all(image_dir)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let mut new_records = Vec::new();
    let mut written = Vec::new();
    for (position, data_url) in image_data.iter().enumerate() {
        let parsed = parse_data_url(data_url)?;
        let prepared = crate::image_compress::prepare_image_bytes(
            parsed.bytes,
            parsed.mime_type,
            parsed.extension,
        )
        .map_err(|message| (StatusCode::BAD_REQUEST, message))?;
        let name = random_image_name(card_id, position, &prepared.extension);
        if let Err(err) = fs::write(image_dir.join(&name), &prepared.bytes).await {
            remove_files(image_dir, &written).await;
            return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()));
        }
        written.push(name.clone());
        new_records.push(TipcardImageRecord {
            id: 0,
            position: position as i64,
            storage_path: name,
            mime_type: prepared.mime_type,
            byte_size: prepared.bytes.len() as i64,
        });
    }
    let old_images =
        match tipcards::replace_image_records(pool, user_id, card_id, &new_records).await {
            Ok(images) => images,
            Err(err) => {
                remove_files(image_dir, &written).await;
                return Err(err.into_status_body());
            }
        };
    for image in old_images {
        let _ = fs::remove_file(image_dir.join(image.storage_path)).await;
    }
    Ok(())
}

/// Store one already-prepared automatic image without decoding or compressing
/// it a second time. File cleanup follows the same replace semantics as uploads.
pub async fn replace_card_prepared_image(
    pool: &PgPool,
    image_dir: &Path,
    user_id: &str,
    card_id: i64,
    prepared: crate::image_compress::PreparedImage,
) -> StatusResult<()> {
    fs::create_dir_all(image_dir)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let name = random_image_name(card_id, 0, &prepared.extension);
    fs::write(image_dir.join(&name), &prepared.bytes)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let record = TipcardImageRecord {
        id: 0,
        position: 0,
        storage_path: name.clone(),
        mime_type: prepared.mime_type,
        byte_size: prepared.bytes.len() as i64,
    };
    let old_images = match tipcards::replace_image_records(pool, user_id, card_id, &[record]).await
    {
        Ok(images) => images,
        Err(err) => {
            let _ = fs::remove_file(image_dir.join(&name)).await;
            return Err(err.into_status_body());
        }
    };
    for image in old_images {
        let _ = fs::remove_file(image_dir.join(image.storage_path)).await;
    }
    Ok(())
}

/// Downloads an external image only after checking every resolved address and
/// every redirect destination. Redirects are deliberately followed manually so
/// the same checks apply after each hop.
pub async fn download_remote_image(value: &str) -> StatusResult<IncomingImage> {
    const MAX_REDIRECTS: usize = 5;
    let mut url = checked_remote_url(value)?;

    for _ in 0..=MAX_REDIRECTS {
        validate_url_shape(&url)?;
        let addresses = resolve_public_target(&url).await?;
        let client = client_pinned_to_target(&url, &addresses)?;
        let mut response = client.get(url.clone()).send().await.map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "Unable to download image URL".to_string(),
            )
        })?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .ok_or_else(|| {
                    (
                        StatusCode::BAD_REQUEST,
                        "Image redirect has no location".to_string(),
                    )
                })?;
            let location = location.to_str().map_err(|_| {
                (
                    StatusCode::BAD_REQUEST,
                    "Image redirect location is invalid".to_string(),
                )
            })?;
            url = url.join(location).map_err(|_| {
                (
                    StatusCode::BAD_REQUEST,
                    "Image redirect location is invalid".to_string(),
                )
            })?;
            continue;
        }
        if !response.status().is_success() {
            return Err((
                StatusCode::BAD_REQUEST,
                "Image URL returned an error".to_string(),
            ));
        }
        if response
            .content_length()
            .is_some_and(|len| len as usize > crate::domain::image::MAX_IMAGE_BYTES)
        {
            return Err((
                StatusCode::BAD_REQUEST,
                "Each image must be at most 10 MB".to_string(),
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "Unable to read image URL".to_string(),
            )
        })? {
            if bytes.len().saturating_add(chunk.len()) > crate::domain::image::MAX_IMAGE_BYTES {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Each image must be at most 10 MB".to_string(),
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        let mime_type = mime_type_for_image_bytes(&bytes)?;
        return Ok(IncomingImage::Bytes { bytes, mime_type });
    }
    Err((
        StatusCode::BAD_REQUEST,
        "Image URL redirected too many times".to_string(),
    ))
}

const MAX_REMOTE_JSON_BYTES: usize = 2 * 1024 * 1024;

/// Fetch JSON from a public HTTP(S) endpoint with DNS pinning and no redirects.
/// This is used for user-configurable search and image API endpoints, where a
/// normal client would permit SSRF through private DNS answers or redirects.
pub async fn get_public_json(value: &str) -> StatusResult<serde_json::Value> {
    request_public_json(value, None, None).await
}

/// POST JSON to a public HTTP(S) endpoint with the same SSRF protections as
/// [`download_remote_image`]. Redirects are rejected to avoid forwarding API
/// credentials or request bodies to a different host.
pub async fn post_public_json(
    value: &str,
    body: &serde_json::Value,
) -> StatusResult<serde_json::Value> {
    request_public_json(value, Some(body), None).await
}

/// POST JSON with a bearer token while retaining the public-target checks and
/// redirect rejection used for other user-configurable endpoints.
pub async fn post_public_json_bearer(
    value: &str,
    body: &serde_json::Value,
    token: &str,
) -> StatusResult<serde_json::Value> {
    request_public_json(value, Some(body), Some(token)).await
}

async fn request_public_json(
    value: &str,
    body: Option<&serde_json::Value>,
    bearer_token: Option<&str>,
) -> StatusResult<serde_json::Value> {
    let url = checked_remote_url(value)?;
    validate_url_shape(&url)?;
    let addresses = resolve_public_target(&url).await?;
    let client = client_pinned_to_target(&url, &addresses)?;
    let mut request = match body {
        Some(body) => client.post(url).json(body),
        None => client.get(url),
    };
    if let Some(token) = bearer_token {
        request = request.bearer_auth(token);
    }
    let mut response = request.send().await.map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Unable to reach remote JSON endpoint".to_string(),
        )
    })?;
    if response.status().is_redirection() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Remote JSON endpoint redirects are not allowed".to_string(),
        ));
    }
    if !response.status().is_success() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Remote JSON endpoint returned an error".to_string(),
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length as usize > MAX_REMOTE_JSON_BYTES)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Remote JSON response is too large".to_string(),
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Unable to read remote JSON response".to_string(),
        )
    })? {
        if bytes.len().saturating_add(chunk.len()) > MAX_REMOTE_JSON_BYTES {
            return Err((
                StatusCode::BAD_REQUEST,
                "Remote JSON response is too large".to_string(),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Remote endpoint returned invalid JSON".to_string(),
        )
    })
}

pub async fn append_card_images(
    pool: &PgPool,
    image_dir: &Path,
    user_id: &str,
    card_id: i64,
    incoming: Vec<IncomingImage>,
) -> StatusResult<()> {
    fs::create_dir_all(image_dir)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    // Fully parse and prepare before creating any files, so malformed input
    // never leaves storage behind.
    let prepared = incoming
        .into_iter()
        .map(prepare_incoming_image)
        .collect::<StatusResult<Vec<_>>>()?;
    let mut records = Vec::with_capacity(prepared.len());
    let mut written = Vec::with_capacity(prepared.len());
    for (offset, prepared) in prepared.into_iter().enumerate() {
        let position = offset;
        let name = random_image_name(card_id, position, &prepared.extension);
        if let Err(err) = fs::write(image_dir.join(&name), &prepared.bytes).await {
            remove_files(image_dir, &written).await;
            return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()));
        }
        written.push(name.clone());
        records.push(TipcardImageRecord {
            id: 0,
            position: position as i64,
            storage_path: name,
            mime_type: prepared.mime_type,
            byte_size: prepared.bytes.len() as i64,
        });
    }
    if let Err(err) = tipcards::append_image_records(pool, user_id, card_id, &mut records).await {
        remove_files(image_dir, &written).await;
        return Err(err.into_status_body());
    }
    Ok(())
}

fn prepare_incoming_image(
    incoming: IncomingImage,
) -> StatusResult<crate::image_compress::PreparedImage> {
    match incoming {
        IncomingImage::DataUrl(value) => {
            let parsed = parse_data_url(&value)?;
            crate::image_compress::prepare_image_bytes(
                parsed.bytes,
                parsed.mime_type,
                parsed.extension,
            )
            .map_err(|message| (StatusCode::BAD_REQUEST, message))
        }
        IncomingImage::Bytes { bytes, mime_type } => {
            let mime_type = normalize_image_mime(&mime_type).ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    "Only PNG, JPEG, WebP, or GIF images are supported".to_string(),
                )
            })?;
            let extension = extension_for_mime(mime_type).expect("supported MIME has extension");
            crate::image_compress::prepare_image_bytes(bytes, mime_type, extension)
                .map_err(|message| (StatusCode::BAD_REQUEST, message))
        }
    }
}

async fn remove_files(image_dir: &Path, names: &[String]) {
    remove_stored_files(image_dir, names).await;
}

/// Remove image files whose database records were deleted. Stored paths are
/// constrained to a single filename so corrupt metadata cannot escape the image
/// directory.
pub async fn remove_stored_files(image_dir: &Path, names: &[String]) {
    for name in names {
        let mut components = Path::new(name).components();
        if !matches!(components.next(), Some(std::path::Component::Normal(_)))
            || components.next().is_some()
        {
            tracing::warn!(storage_path = name, "refusing to remove unsafe image path");
            continue;
        }
        match fs::remove_file(image_dir.join(name)).await {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                tracing::warn!(error = ?err, storage_path = name, "failed to remove image file")
            }
        }
    }
}

fn checked_remote_url(value: &str) -> StatusResult<Url> {
    let url = Url::parse(value.trim())
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid image URL".to_string()))?;
    validate_url_shape(&url)?;
    Ok(url)
}

fn validate_url_shape(url: &Url) -> StatusResult<()> {
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Image URLs must be credential-free HTTP or HTTPS URLs".to_string(),
        ));
    }
    Ok(())
}

async fn resolve_public_target(url: &Url) -> StatusResult<Vec<SocketAddr>> {
    let host = url
        .host_str()
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Invalid image URL".to_string()))?;
    let port = url.port_or_known_default().unwrap_or(80);
    let addresses: Vec<SocketAddr> = if let Ok(ip) = host.parse() {
        vec![SocketAddr::new(ip, port)]
    } else {
        tokio::net::lookup_host((host, port))
            .await
            .map_err(|_| {
                (
                    StatusCode::BAD_REQUEST,
                    "Image URL host could not be resolved".to_string(),
                )
            })?
            .collect()
    };
    if addresses.is_empty()
        || addresses
            .iter()
            .any(|address| !is_public_address(address.ip()))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Image URLs may not target private network addresses".to_string(),
        ));
    }
    Ok(addresses)
}

fn client_pinned_to_target(url: &Url, addresses: &[SocketAddr]) -> StatusResult<Client> {
    let host = url
        .host_str()
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Invalid image URL".to_string()))?;
    let mut builder = Client::builder()
        // Proxy-side DNS resolution would bypass the validated, pinned addresses.
        .no_proxy()
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30));
    if host.parse::<IpAddr>().is_err() {
        // Keep the URL hostname intact so HTTPS SNI and certificate validation
        // use the original host, while reqwest connects only to these addresses.
        builder = builder.resolve_to_addrs(host, addresses);
    }
    builder
        .build()
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
}

fn is_public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(ip) => {
            let [first, second, ..] = ip.octets();
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_multicast()
                && !ip.is_unspecified()
                && !ip.is_broadcast()
                && first != 0
                && !(first == 100 && (64..=127).contains(&second))
        }
        IpAddr::V6(ip) => {
            let octets = ip.octets();
            let ipv4_compatible = octets[..12].iter().all(|byte| *byte == 0);
            let ipv4_mapped = octets[..10].iter().all(|byte| *byte == 0)
                && octets[10] == 0xff
                && octets[11] == 0xff;
            if ipv4_compatible || ipv4_mapped {
                return is_public_address(IpAddr::from([
                    octets[12], octets[13], octets[14], octets[15],
                ]));
            }
            !ip.is_loopback()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && !ip.is_unique_local()
                && !ip.is_unicast_link_local()
        }
    }
}

fn mime_type_for_image_bytes(bytes: &[u8]) -> StatusResult<String> {
    let format = image::guess_format(bytes).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Image URL did not return a supported image".to_string(),
        )
    })?;
    let mime_type = match format {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::WebP => "image/webp",
        image::ImageFormat::Gif => "image/gif",
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "Image URL did not return a supported image".to_string(),
            ));
        }
    };
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "Image URL returned invalid image data".to_string(),
            )
        })?;
    reader.decode().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Image URL returned invalid image data".to_string(),
        )
    })?;
    Ok(mime_type.to_string())
}

fn normalize_image_mime(value: &str) -> Option<&'static str> {
    match value
        .split(';')
        .next()?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "image/png" => Some("image/png"),
        "image/jpeg" | "image/jpg" => Some("image/jpeg"),
        "image/webp" => Some("image/webp"),
        "image/gif" => Some("image/gif"),
        _ => None,
    }
}

fn extension_for_mime(mime_type: &str) -> Option<&'static str> {
    match mime_type {
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/webp" => Some("webp"),
        "image/gif" => Some("gif"),
        _ => None,
    }
}

pub async fn migrate_legacy_images(pool: &PgPool, image_dir: &Path) -> StatusResult<()> {
    fs::create_dir_all(image_dir)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let rows = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT id, user_id, image_data
         FROM tipcards
         WHERE image_data IS NOT NULL AND image_data != '' AND image_data != '[]'",
    )
    .fetch_all(pool)
    .await
    .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    for (card_id, user_id, raw) in rows {
        let existing = tipcards::list_images(pool, &user_id, card_id)
            .await
            .map_err(|err| err.into_status_body())?;
        if !existing.is_empty() {
            continue;
        }
        let Ok(images) = serde_json::from_str::<Vec<String>>(&raw) else {
            continue;
        };
        replace_card_images(pool, image_dir, &user_id, card_id, images).await?;
    }
    Ok(())
}

struct ParsedImage<'a> {
    mime_type: &'a str,
    extension: &'a str,
    bytes: Vec<u8>,
}

fn parse_data_url(value: &str) -> StatusResult<ParsedImage<'_>> {
    let Some((header, payload)) = value.split_once(',') else {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid image data URL".to_string(),
        ));
    };
    let mime_type = header
        .strip_prefix("data:")
        .and_then(|value| value.strip_suffix(";base64"))
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "Invalid image data URL".to_string(),
            )
        })?;
    let extension = match mime_type {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "Only PNG, JPEG, WebP, or GIF data URLs are supported".to_string(),
            ));
        }
    };
    let bytes = STANDARD.decode(payload).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid base64 image data".to_string(),
        )
    })?;
    crate::domain::image::validate_decoded_image_size(bytes.len())
        .map_err(|err| err.into_status_body())?;
    Ok(ParsedImage {
        mime_type,
        extension,
        bytes,
    })
}

fn random_image_name(card_id: i64, position: usize, extension: &str) -> String {
    let suffix: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(18)
        .map(char::from)
        .collect();
    format!("{card_id}-{position}-{suffix}.{extension}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_url_requires_safe_credential_free_http() {
        assert!(checked_remote_url("https://example.com/image.png").is_ok());
        assert!(checked_remote_url("ftp://example.com/image.png").is_err());
        assert!(checked_remote_url("https://user@example.com/image.png").is_err());
        assert!(checked_remote_url("https://example.com/image.png").is_ok());
    }

    #[test]
    fn private_and_special_addresses_are_not_public() {
        for address in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.1.1",
            "224.0.0.1",
            "::1",
            "fe80::1",
            "fc00::1",
            "::ffff:127.0.0.1",
            "::127.0.0.1",
        ] {
            assert!(!is_public_address(address.parse().unwrap()), "{address}");
        }
        assert!(is_public_address("8.8.8.8".parse().unwrap()));
        assert!(is_public_address("::ffff:8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn remote_image_type_comes_from_decoded_bytes() {
        // Generate a complete PNG, deliberately independent of any HTTP header.
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(1, 1)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let png = png.into_inner();
        assert_eq!(mime_type_for_image_bytes(&png).unwrap(), "image/png");
        assert!(mime_type_for_image_bytes(b"not an image").is_err());
    }
}
