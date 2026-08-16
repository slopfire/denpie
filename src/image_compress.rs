use caesium::{compress_in_memory, compress_to_size_in_memory, parameters::CSParameters};
use tracing::warn;

use crate::domain::image::{MAX_IMAGE_BYTES, MAX_IMAGE_EDGE_PX, TARGET_IMAGE_BYTES};

pub struct PreparedImage {
    pub bytes: Vec<u8>,
    pub mime_type: String,
    pub extension: String,
}

impl std::fmt::Debug for PreparedImage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedImage")
            .field("byte_len", &self.bytes.len())
            .field("mime_type", &self.mime_type)
            .field("extension", &self.extension)
            .finish()
    }
}

pub fn prepare_image_bytes(
    bytes: Vec<u8>,
    mime_type: &str,
    extension: &str,
) -> Result<PreparedImage, String> {
    crate::domain::image::validate_decoded_image_size(bytes.len()).map_err(|err| err.message())?;

    if !crate::domain::image::needs_server_compression(bytes.len()) {
        return Ok(PreparedImage {
            bytes,
            mime_type: mime_type.to_string(),
            extension: extension.to_string(),
        });
    }

    let mut params = server_compression_params();
    // Fallback moves `bytes` instead of cloning again; worst case now holds one clone
    // plus the original, not the original plus two concurrent clones.
    let compressed = match compress_in_memory(bytes.clone(), &params) {
        Ok(result) if result.len() < bytes.len() => result,
        Ok(_) | Err(_) => {
            warn!("libcaesium compress_in_memory did not shrink image; trying size target");
            compress_to_size_in_memory(bytes, &mut params, TARGET_IMAGE_BYTES, true)
                .map_err(|err| err.message)?
        }
    };

    if compressed.len() > MAX_IMAGE_BYTES {
        return Err(format!(
            "Image is still too large after compression ({} MB)",
            compressed.len() / 1024 / 1024
        ));
    }

    Ok(PreparedImage {
        bytes: compressed,
        mime_type: mime_type.to_string(),
        extension: extension.to_string(),
    })
}

fn server_compression_params() -> CSParameters {
    let mut params = CSParameters::new();
    params.keep_metadata = false;
    params.width = MAX_IMAGE_EDGE_PX;
    params.height = 0;
    params.jpeg.quality = 82;
    params.jpeg.progressive = true;
    params.jpeg.optimize = true;
    params.png.quality = 80;
    params.png.optimization_level = 4;
    params.png.optimize = true;
    params.webp.quality = 82;
    params.webp.lossless = false;
    params.gif.quality = 80;
    params
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::STANDARD};

    #[test]
    fn tiny_png_skips_recompression() {
        let bytes = STANDARD.decode("iVBORw0KGgo=").unwrap();
        let prepared = prepare_image_bytes(bytes.clone(), "image/png", "png").unwrap();
        assert_eq!(prepared.bytes, bytes);
    }

    #[test]
    fn prepared_image_debug_omits_payload_bytes() {
        let prepared = PreparedImage {
            bytes: vec![82, 73, 70, 70],
            mime_type: "image/webp".to_string(),
            extension: "webp".to_string(),
        };

        let debug = format!("{prepared:?}");

        assert_eq!(
            debug,
            "PreparedImage { byte_len: 4, mime_type: \"image/webp\", extension: \"webp\" }"
        );
        assert!(!debug.contains("82"));
    }
}
