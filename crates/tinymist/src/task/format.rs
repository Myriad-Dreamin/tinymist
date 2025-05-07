//! The actor that handles formatting.

use std::iter::zip;

use lsp_types::TextEdit;
use sync_ls::{just_future, SchedulableResponse};
use tinymist_query::{to_lsp_range, PositionEncoding};
use typst::syntax::Source;

use super::SyncTaskFactory;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatterConfig {
    Typstyle(Box<typstyle_core::Config>),
    Typstfmt(Box<typstfmt::Config>),
    Disable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatUserConfig {
    pub config: FormatterConfig,
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
            let formatted = match &c.config {
                FormatterConfig::Typstyle(config) => {
                    typstyle_core::Typstyle::new(config.as_ref().clone())
                        .format_source(&src)
                        .ok()
                }
                FormatterConfig::Typstfmt(config) => Some(typstfmt::format(src.text(), **config)),
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

    let range = to_lsp_range(replace, &prev, encoding);

    Some(vec![TextEdit {
        new_text: with.to_owned(),
        range,
    }])
}
