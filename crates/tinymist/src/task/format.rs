//! The actor that handles formatting.

use std::{iter::zip, sync::Arc};

use lsp_types::TextEdit;
use sync_lsp::{just_future, SchedulableResponse};
use tinymist_query::{typst_to_lsp, PositionEncoding};
use typst::syntax::Source;

use crate::{FormatterMode, LspResult};

use super::SyncTaskFactory;

#[derive(Debug, Clone)]
pub struct FormatConfig {
    pub mode: FormatterMode,
    pub width: u32,
    pub position_encoding: PositionEncoding,
}

type FmtFn = Arc<dyn Fn(Source) -> LspResult<Option<Vec<TextEdit>>> + Send + Sync>;

#[derive(Clone)]
pub struct FormatTask {
    factory: SyncTaskFactory<FormatterTaskData>,
}

impl FormatTask {
    pub fn new(c: FormatConfig) -> Self {
        let factory = SyncTaskFactory::default();
        let this = Self { factory };

        this.change_config(c);
        this
    }

    pub fn change_config(&self, c: FormatConfig) {
        self.factory.mutate(|data| {
            data.0 = match c.mode {
                FormatterMode::Typstyle => {
                    let cw = c.width as usize;
                    Arc::new(move |e: Source| {
                        let res =
                            typstyle_core::Typstyle::new_with_src(e.clone(), cw).pretty_print();
                        Ok(calc_diff(e, res, c.position_encoding))
                    })
                }
                FormatterMode::Typstfmt => {
                    let config = typstfmt_lib::Config {
                        max_line_length: c.width as usize,
                        ..typstfmt_lib::Config::default()
                    };
                    Arc::new(move |e: Source| {
                        let res = typstfmt_lib::format(e.text(), config);
                        Ok(calc_diff(e, res, c.position_encoding))
                    })
                }
                FormatterMode::Disable => Arc::new(|_| Ok(None)),
            }
        });
    }

    pub fn exec(&self, source: Source) -> SchedulableResponse<Option<Vec<TextEdit>>> {
        let data = self.factory.task();
        just_future(async move { (data.0)(source) })
    }
}

#[derive(Clone)]
pub struct FormatterTaskData(FmtFn);

impl Default for FormatterTaskData {
    fn default() -> Self {
        Self(Arc::new(|_| Ok(None)))
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
