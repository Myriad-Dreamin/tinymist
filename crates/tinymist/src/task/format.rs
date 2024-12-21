//! The actor that handles formatting.

use std::iter::zip;

use lsp_types::TextEdit;
use sync_lsp::{just_future, SchedulableResponse};
use tinymist_query::{typst_to_lsp, PositionEncoding};
use typst::syntax::Source;

use super::SyncTaskFactory;

#[derive(Debug, Clone)]
pub enum FormatterConfig {
    Typstyle(Box<typstyle_core::PrinterConfig>),
    Typstfmt(Box<typstfmt_lib::Config>),
    Disable,
}

impl FormatterConfig {
    /// The configuration structs doesn't implement `PartialEq`, so bad.
    pub fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Typstyle(a), Self::Typstyle(b)) => {
                a.tab_spaces == b.tab_spaces
                    && a.max_width == b.max_width
                    && a.chain_width_ratio == b.chain_width_ratio
                    && a.blank_lines_upper_bound == b.blank_lines_upper_bound
            }
            (Self::Typstfmt(a), Self::Typstfmt(b)) => {
                let a = serde_json::to_value(a).ok();
                let b = serde_json::to_value(b).ok();
                a == b
            }
            (Self::Disable, Self::Disable) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FormatUserConfig {
    pub config: FormatterConfig,
    pub position_encoding: PositionEncoding,
}

impl FormatUserConfig {
    /// The configuration structs doesn't implement `PartialEq`, so bad.
    pub fn eq(&self, other: &Self) -> bool {
        self.config.eq(&other.config) && self.position_encoding == other.position_encoding
    }
}

#[derive(Clone)]
pub struct FormatTask {
    factory: SyncTaskFactory<FormatUserConfig>,
}

impl FormatTask {
    pub fn new(c: FormatUserConfig) -> Self {
        Self {
            factory: SyncTaskFactory::new(c),
        }
    }

    pub fn change_config(&self, c: FormatUserConfig) {
        self.factory.mutate(|data| *data = c);
    }

    pub fn run(&self, src: Source) -> SchedulableResponse<Option<Vec<TextEdit>>> {
        let c = self.factory.task();
        just_future(async move {
            let formatted = match &c.config {
                FormatterConfig::Typstyle(config) => {
                    typstyle_core::Typstyle::new_with_src(src.clone(), config.as_ref().clone())
                        .pretty_print()
                        .ok()
                }
                FormatterConfig::Typstfmt(config) => {
                    Some(typstfmt_lib::format(src.text(), **config))
                }
                FormatterConfig::Disable => None,
            };

            Ok(formatted.and_then(|formatted| calc_diff(src, formatted, c.position_encoding)))
        })
    }
}

/// A simple implementation of the diffing algorithm, borrowed from
/// [`Source::replace`].
fn calc_diff(prev: Source, next: String, encoding: PositionEncoding) -> Option<Vec<TextEdit>> {
    let old = prev.text();
    let new = &next;

    let mut prefix = zip(old.bytes(), new.bytes())
        .take_while(|(x, y)| x == y)
        .count();

    if prefix == old.len() && prefix == new.len() {
        return Some(vec![]);
    }

    while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) {
        prefix -= 1;
    }

    let mut suffix = zip(old[prefix..].bytes().rev(), new[prefix..].bytes().rev())
        .take_while(|(x, y)| x == y)
        .count();

    while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) {
        suffix += 1;
    }

    let replace = prefix..old.len() - suffix;
    let with = &new[prefix..new.len() - suffix];

    let range = typst_to_lsp::range(replace, &prev, encoding);

    Some(vec![TextEdit {
        new_text: with.to_owned(),
        range,
    }])
}
