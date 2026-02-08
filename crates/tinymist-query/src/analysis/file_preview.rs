//! Shared file preview functionality for completion and hover

use base64::Engine;

/// Image file extensions that can be previewed
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "svg", "webp"];

/// Text file extensions that can be previewed
const TEXT_EXTENSIONS: &[&str] = &[
    "xml", "yml", "yaml", "toml", "json", "csv", "txt", "md", "bib", "csl", "typ",
];

/// Maximum file size for preview (1MB)
const MAX_PREVIEW_SIZE: u64 = 1024 * 1024;

/// Check if a file extension is previewable
pub fn is_previewable(extension: &str) -> bool {
    IMAGE_EXTENSIONS.contains(&extension) || TEXT_EXTENSIONS.contains(&extension)
}

/// Generate preview documentation for a file
pub fn generate_file_preview(
    content: &[u8],
    extension: &str,
    supports_html: bool,
) -> Option<String> {
    let size = content.len() as u64;

    if size > MAX_PREVIEW_SIZE {
        return Some(format!(
            "File too large to preview ({:.1} MB)",
            size as f64 / 1024.0 / 1024.0
        ));
    }

    if IMAGE_EXTENSIONS.contains(&extension) {
        return generate_image_preview(content, extension, size, supports_html);
    }

    if TEXT_EXTENSIONS.contains(&extension) {
        return generate_text_preview(content, extension, size);
    }

    None
}

/// Generate image preview with base64 encoding
fn generate_image_preview(
    content: &[u8],
    extension: &str,
    size: u64,
    supports_html: bool,
) -> Option<String> {
    // Encode as base64 for all image formats
    let base64_content = base64::engine::general_purpose::STANDARD.encode(content);

    // Determine MIME type
    let mime_type = match extension {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        _ => return None,
    };

    // Create HTML img tag with auto-fit width. Fall back to markdown if HTML not supported.
    let preview = if supports_html {
        format!(
            "<img src=\"data:{};base64,{}\" style=\"max-width: 100%; height: auto;\" alt=\"Image Preview\" />\n\n**Size**: {:.1} KB",
            mime_type,
            base64_content,
            size as f64 / 1024.0
        )
    } else {
        format!(
            "![Image Preview](data:{};base64,{})\n\n**Size**: {:.1} KB",
            mime_type,
            base64_content,
            size as f64 / 1024.0
        )
    };

    Some(preview)
}

/// Generate text file preview
fn generate_text_preview(content: &[u8], extension: &str, size: u64) -> Option<String> {
    const LENGTH_LIMIT: usize = 500;
    const LINE_LIMIT: usize = 20;

    // Convert to string
    let text_content = String::from_utf8_lossy(content);

    // Limit preview length (first LENGTH_LIMIT characters or LINE_LIMIT lines)
    let lines: Vec<&str> = text_content.lines().take(LINE_LIMIT).collect();
    let preview_text = lines.join("\n");
    let preview_text = if preview_text.len() > LENGTH_LIMIT {
        let mut boundary = LENGTH_LIMIT;
        while !preview_text.is_char_boundary(boundary) {
            boundary -= 1;
        }
        format!("{}...", &preview_text[..boundary])
    } else {
        preview_text
    };

    // Determine syntax highlighting language
    let language = match extension {
        "xml" => "xml",
        "yml" | "yaml" => "yaml",
        "toml" => "toml",
        "json" => "json",
        "csv" => "csv",
        "md" => "markdown",
        "txt" => "text",
        "bib" => "bibtex",
        "csl" => "xml",
        "typ" => "typ",
        _ => "text",
    };

    let truncated = if lines.len() >= LINE_LIMIT || text_content.len() > LENGTH_LIMIT {
        " (truncated)"
    } else {
        ""
    };

    // Create markdown with syntax-highlighted code block
    let preview = format!(
        "```{}\n{}\n```\n\n**Size**: {:.1} KB{}",
        language,
        preview_text,
        size as f64 / 1024.0,
        truncated
    );

    Some(preview)
}
