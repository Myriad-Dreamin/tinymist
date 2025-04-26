//! Utility functions for DOCX conversion

/// Utility functions for DOCX conversion
pub struct DocxUtils;

impl DocxUtils {
    /// Create a new utility instance
    pub fn new() -> Self {
        Self
    }

    /// Check if file extension is a supported image format
    pub fn is_supported_image(filepath: &str) -> bool {
        let lowercase = filepath.to_lowercase();
        let extensions = [".png", ".jpg", ".jpeg", ".gif", ".bmp", ".webp", ".tiff", ".svg"];
        extensions.iter().any(|ext| lowercase.ends_with(ext))
    }

    /// Check if MIME type is a supported image format
    pub fn is_supported_image_mime(mime: &str) -> bool {
        mime.starts_with("image/") && 
            ["png", "jpeg", "gif", "bmp", "webp", "tiff", "svg"]
                .iter()
                .any(|ext| mime.contains(ext))
    }
}
