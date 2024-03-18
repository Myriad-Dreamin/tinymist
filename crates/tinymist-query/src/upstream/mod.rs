use std::fmt::Write;

use ecow::{eco_format, EcoString};
use typst::text::{FontInfo, FontStyle};

mod tooltip;
pub use tooltip::*;

/// Create a short description of a font family.
fn summarize_font_family<'a>(variants: impl Iterator<Item = &'a FontInfo>) -> EcoString {
    let mut infos: Vec<_> = variants.collect();
    infos.sort_by_key(|info: &&FontInfo| info.variant);

    let mut has_italic = false;
    let mut min_weight = u16::MAX;
    let mut max_weight = 0;
    for info in &infos {
        let weight = info.variant.weight.to_number();
        has_italic |= info.variant.style == FontStyle::Italic;
        min_weight = min_weight.min(weight);
        max_weight = min_weight.max(weight);
    }

    let count = infos.len();
    let mut detail = eco_format!("{count} variant{}.", if count == 1 { "" } else { "s" });

    if min_weight == max_weight {
        write!(detail, " Weight {min_weight}.").unwrap();
    } else {
        write!(detail, " Weights {min_weight}–{max_weight}.").unwrap();
    }

    if has_italic {
        detail.push_str(" Has italics.");
    }

    detail
}
