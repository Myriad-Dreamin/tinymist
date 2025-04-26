//! Utility functions for DOCX conversion

use image::GenericImageView;


/// Get image dimensions
pub fn get_image_size(img_data: &[u8]) -> Option<(u32, u32)> {
    match image::load_from_memory(img_data) {
        Ok(img) => {
            let (width, height) = img.dimensions();
            Some((width, height))
        }
        Err(_) => None,
    }
}

/// Calculate image dimensions for DOCX
pub fn calculate_image_dimensions(img_data: &[u8], scale_factor: Option<f32>) -> (u32, u32) {
    let actual_scale = scale_factor.unwrap_or(1.0);

    if let Some((w, h)) = get_image_size(img_data) {
        let max_width = 5486400;
        let scaled_w = (w as f32 * actual_scale) as u32;
        let scaled_h = (h as f32 * actual_scale) as u32;

        if scaled_w > max_width {
            let ratio = scaled_h as f32 / scaled_w as f32;
            let new_width = max_width;
            let new_height = (max_width as f32 * ratio) as u32;
            (new_width, new_height)
        } else {
            (scaled_w * 9525, scaled_h * 9525)
        }
    } else {
        (4000000, 3000000)
    }
}
