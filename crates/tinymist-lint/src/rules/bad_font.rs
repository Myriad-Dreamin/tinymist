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
                self.check_bad_font_expr(arg.expr().to_untyped());
                if let Some(array) = arg.expr().to_untyped().cast::<ast::Array>() {
                    for item in array.items() {
                        self.check_bad_font_expr(item.to_untyped());
                    }
                }
            }
        }
    }

    fn check_bad_font_expr(&mut self, expr: &SyntaxNode) {
        self.check_variable_font_object(expr);
        self.check_unknown_font(expr);
    }

    fn check_variable_font_object(&mut self, expr: &SyntaxNode) -> Option<()> {
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

    fn check_unknown_font(&mut self, expr: &SyntaxNode) -> Option<()> {
        // Check if this span has a known unknown font warning from compiler
        let unknown_font = self.known_issues.get_unknown_font(expr.span())?;
        // Get available fonts from the font book
        let available_fonts = self.available_fonts.get_or_init(|| {
            let book = self.world.font_resolver.font_book();
            book.families().map(|(name, _)| name).collect()
        });

        let mut diag = SourceDiagnostic::warning(
            expr.span(),
            eco_format!("unknown font family: {unknown_font}"),
        );

        // 1. Check for common character errors
        if unknown_font.contains(',') {
            diag.hint("multiple fonts should be specified as an array, e.g., (\"Times New Roman\", \"Arial\") instead of a comma-separated string.");
        }

        // 2. Check for non-ASCII characters
        if !unknown_font.is_ascii() {
            diag .hint("font family names in Typst should usually be English (PostScript) names. Try using the English name of the font.");
        }

        // 3. Check for localized mapping
        let mapped_suggestions: Vec<_> = LOCALIZED_FONT_MAPPING
            .iter()
            .find(|(localized, _)| *localized == unknown_font)
            .map(|(_, english)| {
                english
                    .iter()
                    .filter(|&name| available_fonts.contains(name))
                    .copied()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if !mapped_suggestions.is_empty() {
            if mapped_suggestions.len() == 1 {
                diag.hint(eco_format!("did you mean '{}'?", mapped_suggestions[0]));
            } else {
                let suggestion_list = mapped_suggestions.join("', '");
                diag.hint(eco_format!("did you mean one of: '{}'?", suggestion_list));
            }
            self.diag.push(diag);
            return Some(());
        }

        // 4. Find the best matching fonts using string similarity
        let suggestions = find_similar_fonts(unknown_font.as_str(), available_fonts, 3);

        if !suggestions.is_empty() {
            if suggestions.len() == 1 {
                diag.hint(eco_format!("did you mean '{}'?", suggestions[0]));
            } else {
                let suggestion_list = suggestions.join("', '");
                diag.hint(eco_format!("did you mean one of: '{}'?", suggestion_list));
            }
        }

        // Only push if we added some hint, otherwise it's just a duplicate of compiler warning
        if !diag.hints.is_empty() {
            self.diag.push(diag);
        }

        Some(())
    }
}

/// Extracts the font name from an "unknown font" diagnostic message.
/// Expected format: "unknown font family: {font_name}"
pub(crate) fn extract_unknown_font(message: &str) -> Option<EcoString> {
    // Try to match patterns like "unknown font family: FontName"
    let font_name = message.strip_prefix("unknown font family:")?.trim();
    Some(font_name.into())
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

    const SIMILARITY_THRESHOLD: f64 = 0.6; // Only suggest if similarity is > 60%

    let target_lower = target.to_lowercase();
    let mut scored_fonts: Vec<_> = available
        .iter()
        .map(|&font| {
            let font_lower = font.to_lowercase();
            let similarity = jaro_winkler(&target_lower, &font_lower);
            (font, similarity)
        })
        .filter(|(_, similarity)| *similarity > SIMILARITY_THRESHOLD)
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

/// A mapping of common localized font names to their English equivalents.
const LOCALIZED_FONT_MAPPING: &[(&str, &[&str])] = &[
    // Chinese Simplified
    ("宋体", &["SimSun"]),
    ("新宋体", &["NSimSun"]),
    ("黑体", &["SimHei"]),
    ("微软雅黑", &["Microsoft YaHei"]),
    ("楷体", &["KaiTi", "KaiTi_GB2312"]),
    ("仿宋", &["FangSong", "FangSong_GB2312"]),
    ("等线", &["DengXian"]),
    ("幼圆", &["YouYuan"]),
    ("华文宋体", &["STSong"]),
    ("华文楷体", &["STKaiti"]),
    ("华文仿宋", &["STFangsong"]),
    ("华文黑体", &["STHeiti"]),
    ("华文细黑", &["STXihei"]),
    // Chinese Traditional
    ("新細明體", &["PMingLiU"]),
    ("細明體", &["MingLiU"]),
    ("標楷體", &["DFKai-SB"]),
    ("微軟正黑體", &["Microsoft JhengHei"]),
    // Japanese
    ("ＭＳ 明朝", &["MS Mincho"]),
    ("ＭＳ Ｐ明朝", &["MS PMincho"]),
    ("ＭＳ ゴシック", &["MS Gothic"]),
    ("ＭＳ Ｐゴシック", &["MS PGothic"]),
    ("メイリオ", &["Meiryo"]),
    // Korean
    ("맑은 고딕", &["Malgun Gothic"]),
    ("바탕", &["Batang"]),
    ("돋움", &["Dotum"]),
    ("궁서", &["Gungsuh"]),
    ("굴림", &["Gulim"]),
];
