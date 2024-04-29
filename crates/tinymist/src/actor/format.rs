//! The actor that handles formatting.

use std::iter::zip;

use lsp_server::RequestId;
use lsp_types::TextEdit;
use tinymist_query::{typst_to_lsp, PositionEncoding};
use typst::syntax::Source;

use crate::{result_to_response_, FormatterMode, LspHost, LspResult, TypstLanguageServer};

#[derive(Debug, Clone)]
pub struct FormatConfig {
    pub mode: FormatterMode,
    pub width: u32,
}

pub enum FormatRequest {
    ChangeConfig(FormatConfig),
    Format(RequestId, Source),
}

pub fn run_format_thread(
    config: FormatConfig,
    format_rx: crossbeam_channel::Receiver<FormatRequest>,
    client: LspHost<TypstLanguageServer>,
    position_encoding: PositionEncoding,
) {
    type FmtFn = Box<dyn Fn(Source) -> LspResult<Option<Vec<TextEdit>>>>;
    let compile = |c: FormatConfig| -> FmtFn {
        log::info!("formatting thread with config: {c:#?}");
        match c.mode {
            FormatterMode::Typstyle => {
                let cw = c.width as usize;
                Box::new(move |e: Source| {
                    let res = typstyle_core::Typstyle::new_with_src(e.clone(), cw).pretty_print();
                    Ok(calc_diff(e, res, position_encoding))
                })
            }
            FormatterMode::Typstfmt => {
                let config = typstfmt_lib::Config {
                    max_line_length: c.width as usize,
                    ..typstfmt_lib::Config::default()
                };
                Box::new(move |e: Source| {
                    let res = typstfmt_lib::format(e.text(), config);
                    Ok(calc_diff(e, res, position_encoding))
                })
            }
            FormatterMode::Disable => Box::new(|_| Ok(None)),
        }
    };

    let mut f: FmtFn = compile(config);
    while let Ok(req) = format_rx.recv() {
        match req {
            FormatRequest::ChangeConfig(c) => f = compile(c),
            FormatRequest::Format(id, source) => {
                let res = f(source);
                if let Ok(response) = result_to_response_(id, res) {
                    client.respond(response);
                }
            }
        }
    }

    log::info!("formatting thread did shut down");
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
