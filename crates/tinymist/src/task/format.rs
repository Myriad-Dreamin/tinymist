//! The actor that handles formatting.

use std::iter::zip;

use lsp_types::TextEdit;
use sync_lsp::{just_future, SchedulableResponse};
use tinymist_query::{to_lsp_range, PositionEncoding};
use typst::syntax::Source;
use typstyle_core::PrinterConfig;

use super::SyncTaskFactory;
use crate::FormatterMode;

#[derive(Debug, Clone, PartialEq)]
pub struct FormatUserConfig {
    pub mode: FormatterMode,
    pub width: u32,
    pub position_encoding: PositionEncoding,
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
            let formatted = match c.mode {
                FormatterMode::Typstyle => typstyle_core::Typstyle::new_with_src(
                    src.clone(),
                    PrinterConfig::new_with_width(c.width as usize),
                )
                .pretty_print()
                .ok(),
                FormatterMode::Typstfmt => Some(typstfmt_lib::format(
                    src.text(),
                    typstfmt_lib::Config {
                        max_line_length: c.width as usize,
                        ..typstfmt_lib::Config::default()
                    },
                )),
                FormatterMode::Disable => None,
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

    let range = to_lsp_range(replace, &prev, encoding);

    Some(vec![TextEdit {
        new_text: with.to_owned(),
        range,
    }])
}
