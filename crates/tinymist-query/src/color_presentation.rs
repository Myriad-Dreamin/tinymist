use typst::foundations::Repr;

use crate::prelude::*;

/// The [`textDocument/colorPresentation`] request is sent from the client to
/// the server to obtain a list of presentations for a color value at a given
/// location.
///
/// [`textDocument/colorPresentation`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_colorPresentation
///
/// Clients can use the result to:
///
/// * Modify a color reference
/// * Show in a color picker and let users pick one of the presentations
///
/// # Compatibility
///
/// This request was introduced in specification version 3.6.0.
///
/// This request has no special capabilities and registration options since it
/// is sent as a resolve request for the
/// [`textDocument/documentColor`](Self::document_color) request.
#[derive(Debug, Clone)]
pub struct ColorPresentationRequest {
    /// The path of the document to request color presentations for.
    pub path: PathBuf,
    /// The color to request presentations for.
    pub color: lsp_types::Color,
    /// The range of the color to request presentations for.
    pub range: LspRange,
}

impl ColorPresentationRequest {
    /// Serve the request.
    pub fn request(self) -> Option<Vec<ColorPresentation>> {
        let color = typst::visualize::Color::Rgb(typst::visualize::Rgb::new(
            self.color.red,
            self.color.green,
            self.color.blue,
            self.color.alpha,
        ));
        Some(vec![
            simple(format!("{:?}", color.to_hex())),
            simple(color.to_rgb().repr().to_string()),
            simple(color.to_luma().repr().to_string()),
            simple(color.to_oklab().repr().to_string()),
            simple(color.to_oklch().repr().to_string()),
            simple(color.to_rgb().repr().to_string()),
            simple(color.to_linear_rgb().repr().to_string()),
            simple(color.to_cmyk().repr().to_string()),
            simple(color.to_hsl().repr().to_string()),
            simple(color.to_hsv().repr().to_string()),
        ])
    }
}

fn simple(label: String) -> ColorPresentation {
    ColorPresentation {
        label,
        ..ColorPresentation::default()
    }
}
