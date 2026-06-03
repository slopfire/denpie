use futures_channel::oneshot;
use gloo_file::File;
use std::cmp::max;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    CanvasRenderingContext2d, File as WebFile, HtmlCanvasElement, HtmlImageElement, Url,
};

/// Matches backend `MAX_IMAGE_BYTES`.
pub const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;

/// Skip canvas work when the source is already small.
const SKIP_IF_SMALLER_BYTES: u64 = 200 * 1024;

/// Longest edge after browser downscale (matches backend libcaesium resize).
const MAX_EDGE_PX: u32 = 2048;

/// WebP/JPEG quality in the 0.80–0.85 sweet spot for photos.
const OUTPUT_QUALITY: f64 = 0.82;

pub fn collect_files(list: &web_sys::FileList) -> Vec<WebFile> {
    (0..list.length())
        .filter_map(|index| list.get(index))
        .collect()
}

pub async fn compress_files_to_data_urls(files: Vec<WebFile>) -> Result<Vec<String>, String> {
    let mut data_urls = Vec::with_capacity(files.len());
    for file in files {
        data_urls.push(compress_file_to_data_url(&file).await?);
    }
    Ok(data_urls)
}

pub async fn compress_file_to_data_url(file: &WebFile) -> Result<String, String> {
    let size = file.size() as u64;
    if size > MAX_IMAGE_BYTES {
        return Err(format!(
            "Image exceeds {} MB limit",
            MAX_IMAGE_BYTES / 1024 / 1024
        ));
    }

    let mime = file.type_();
    if mime == "image/gif" || mime == "image/svg+xml" {
        return read_file_as_data_url(file).await;
    }

    if size <= SKIP_IF_SMALLER_BYTES {
        return read_file_as_data_url(file).await;
    }

    match compress_with_canvas(file).await {
        Ok(data_url) => Ok(data_url),
        Err(_) => read_file_as_data_url(file).await,
    }
}

async fn read_file_as_data_url(file: &WebFile) -> Result<String, String> {
    let (sender, receiver) = oneshot::channel();
    let _reader =
        gloo_file::callbacks::read_as_data_url(&File::from(file.clone()), move |result| {
            let _ = sender.send(result.map_err(|err| err.to_string()));
        });
    receiver
        .await
        .map_err(|_| "Image read was cancelled".to_string())?
}

async fn compress_with_canvas(file: &WebFile) -> Result<String, String> {
    let object_url = Url::create_object_url_with_blob(file)
        .map_err(|_| "Failed to prepare image for compression".to_string())?;

    let image = match load_image(&object_url).await {
        Ok(image) => image,
        Err(err) => {
            let _ = Url::revoke_object_url(&object_url);
            return Err(err);
        }
    };
    let _ = Url::revoke_object_url(&object_url);

    let (target_w, target_h) =
        fit_within(image.natural_width(), image.natural_height(), MAX_EDGE_PX);

    let window = web_sys::window().ok_or("Browser window unavailable")?;
    let document = window.document().ok_or("Browser document unavailable")?;
    let canvas = document
        .create_element("canvas")
        .map_err(|_| "Failed to create canvas")?
        .dyn_into::<HtmlCanvasElement>()
        .map_err(|_| "Failed to initialize canvas")?;
    canvas.set_width(target_w);
    canvas.set_height(target_h);

    let context = canvas
        .get_context("2d")
        .map_err(|_| "Failed to acquire canvas context")?
        .ok_or("Canvas 2D context unavailable")?
        .dyn_into::<CanvasRenderingContext2d>()
        .map_err(|_| "Canvas 2D context unavailable")?;
    context
        .draw_image_with_html_image_element_and_dw_and_dh(
            &image,
            0.0,
            0.0,
            target_w as f64,
            target_h as f64,
        )
        .map_err(|_| "Failed to draw image for compression")?;

    if let Ok(webp) = canvas
        .to_data_url_with_type_and_encoder_options("image/webp", &JsValue::from_f64(OUTPUT_QUALITY))
    {
        if webp.starts_with("data:image/webp") {
            return Ok(webp);
        }
    }

    canvas
        .to_data_url_with_type_and_encoder_options("image/jpeg", &JsValue::from_f64(OUTPUT_QUALITY))
        .map_err(|_| "Failed to encode compressed image".to_string())
}

async fn load_image(src: &str) -> Result<HtmlImageElement, String> {
    let image = HtmlImageElement::new().map_err(|_| "Failed to create image element")?;
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let onload_target = image.clone();
        let onerror_target = image.clone();
        let onload = Closure::once(move || {
            let _ = resolve.call0(&JsValue::UNDEFINED);
        });
        let onerror = Closure::once(move || {
            let _ = reject.call1(&JsValue::UNDEFINED, &JsValue::from_str("Image load failed"));
        });
        onload_target.set_onload(Some(onload.as_ref().unchecked_ref()));
        onerror_target.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        onload.forget();
        onerror.forget();
        onerror_target.set_src(src);
    });

    JsFuture::from(promise)
        .await
        .map_err(|_| "Failed to load image".to_string())?;
    Ok(image)
}

fn fit_within(width: u32, height: u32, max_edge: u32) -> (u32, u32) {
    if width == 0 || height == 0 {
        return (width, height);
    }
    let longest = max(width, height);
    if longest <= max_edge {
        return (width, height);
    }
    let scale = max_edge as f64 / longest as f64;
    let next_w = max(1, (width as f64 * scale).round() as u32);
    let next_h = max(1, (height as f64 * scale).round() as u32);
    (next_w, next_h)
}

#[cfg(test)]
mod tests {
    use super::fit_within;

    #[test]
    fn fit_within_keeps_small_images() {
        assert_eq!(fit_within(800, 600, 2048), (800, 600));
    }

    #[test]
    fn fit_within_scales_longest_edge() {
        assert_eq!(fit_within(4096, 2048, 2048), (2048, 1024));
    }

    #[test]
    fn fit_within_handles_portrait() {
        assert_eq!(fit_within(1080, 1920, 2048), (1080, 1920));
        assert_eq!(fit_within(2160, 3840, 2048), (1152, 2048));
    }
}
