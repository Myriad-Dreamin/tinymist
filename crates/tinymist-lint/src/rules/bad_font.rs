use tinymist_project::font::FontResolver;
use typst::{
    diag::{EcoString, SourceDiagnostic, eco_format},
    syntax::{
        SyntaxNode,
        ast::{self, AstNode},
    },
};

use crate::Linter;

impl<'w> Linter<'w> {
    pub(crate) fn check_bad_font<'a>(&mut self, args: impl IntoIterator<Item = ast::Arg<'a>>) {
        for arg in args {
            if let ast::Arg::Named(arg) = arg
                && arg.name().as_str() == "font"
            {
                self.check_variable_font_object(arg.expr().to_untyped());
                self.check_unknown_font(arg.expr().to_untyped());
                if let Some(array) = arg.expr().to_untyped().cast::<ast::Array>() {
                    for item in array.items() {
                        self.check_variable_font_object(item.to_untyped());
                        self.check_unknown_font(item.to_untyped());
                    }
                }
            }
        }
    }

    pub(crate) fn check_variable_font_object(&mut self, expr: &SyntaxNode) -> Option<()> {
        if let Some(font_dict) = expr.cast::<ast::Dict>() {
            for item in font_dict.items() {
                if let ast::DictItem::Named(arg) = item
                    && arg.name().as_str() == "name"
                {
                    self.check_variable_font_str(arg.expr().to_untyped());
                }
            }
        }

        self.check_variable_font_str(expr)
    }

    fn check_variable_font_str(&mut self, expr: &SyntaxNode) -> Option<()> {
        if !expr.cast::<ast::Str>()?.get().ends_with("VF") {
            return None;
        }

        let _ = self.world;

        let diag =
            SourceDiagnostic::warning(expr.span(), "variable font is not supported by typst yet");
        let diag = diag.with_hint("consider using a static font instead. For more information, see https://github.com/typst/typst/issues/185");
        self.diag.push(diag);

        Some(())
    }

    pub(crate) fn check_unknown_font(&mut self, expr: &SyntaxNode) -> Option<()> {
        // Check if this span has a known unknown font warning from compiler
        let unknown_font = self.known_issues.get_unknown_font(expr.span())?;
        // Get available fonts from the font book
        let book = self.world.font_resolver.font_book();
        let available_fonts: Vec<_> = book.families().map(|(name, _)| name).collect();

        // Find the best matching fonts using string similarity
        let suggestions = find_similar_fonts(unknown_font.as_str(), &available_fonts, 3);

        if !suggestions.is_empty() {
            let mut diag = SourceDiagnostic::warning(
                expr.span(),
                eco_format!("unknown font family: {}", unknown_font),
            );

            if suggestions.len() == 1 {
                diag = diag.with_hint(eco_format!("did you mean '{}'?", suggestions[0]));
            } else {
                let suggestion_list = suggestions.join("', '");
                diag = diag.with_hint(eco_format!("did you mean one of: '{}'?", suggestion_list));
            }

            self.diag.push(diag);
        }

        Some(())
    }
}

/// Extracts the font name from an "unknown font" diagnostic message.
/// Expected format: "unknown font family: {font_name}"
pub(crate) fn extract_unknown_font(message: &str) -> Option<EcoString> {
    // Try to match patterns like "unknown font family: FontName"
    if message.starts_with("unknown font family:") {
        let font_name = message.strip_prefix("unknown font family:")?.trim();
        return Some(EcoString::from(font_name));
    }
    None
}

/// Finds fonts similar to the given font name from a list of available fonts.
/// Uses Jaro-Winkler similarity algorithm which is excellent for short strings like font names.
/// Returns up to `max_suggestions` fonts sorted by similarity score (highest first).
fn find_similar_fonts<'a>(
    target: &str,
    available: &[&'a str],
    max_suggestions: usize,
) -> Vec<&'a str> {
    use strsim::jaro_winkler;

    let target_lower = target.to_lowercase();
    let mut scored_fonts: Vec<_> = available
        .iter()
        .map(|&font| {
            let font_lower = font.to_lowercase();
            let similarity = jaro_winkler(&target_lower, &font_lower);
            (font, similarity)
        })
        .filter(|(_, similarity)| *similarity > 0.6) // Only suggest if similarity is > 60%
        .collect();

    // Sort by similarity (descending)
    scored_fonts.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Return top N suggestions
    scored_fonts
        .into_iter()
        .take(max_suggestions)
        .map(|(font, _)| font)
        .collect()
}
