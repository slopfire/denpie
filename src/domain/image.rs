use crate::error::{AppError, AppResult};

/// Hard reject limit for a single decoded image payload.
pub const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;

/// Server-side recompression threshold — images above this are run through libcaesium.
pub const TARGET_IMAGE_BYTES: usize = 800 * 1024;

/// Longest edge for stored tipcard images (matches browser canvas downscale).
pub const MAX_IMAGE_EDGE_PX: u32 = 2048;

pub fn validate_decoded_image_size(byte_len: usize) -> AppResult<()> {
    if byte_len > MAX_IMAGE_BYTES {
        return Err(AppError::Validation(format!(
            "Each image must be at most {} MB",
            MAX_IMAGE_BYTES / 1024 / 1024
        )));
    }
    Ok(())
}

pub fn needs_server_compression(byte_len: usize) -> bool {
    byte_len > TARGET_IMAGE_BYTES
}
