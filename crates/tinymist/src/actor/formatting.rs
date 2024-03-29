use lsp_server::RequestId;
use lsp_types::TextEdit;
use tinymist_query::{typst_to_lsp, PositionEncoding};
use typst::syntax::Source;

use crate::{result_to_response_, FormatterMode, LspHost, LspResult, TypstLanguageServer};

#[derive(Debug, Clone)]
pub struct FormattingConfig {
    pub mode: FormatterMode,
    pub width: u32,
}

pub enum FormattingRequest {
    ChangeConfig(FormattingConfig),
    Formatting((RequestId, Source)),
}

pub fn run_format_thread(
    init_c: FormattingConfig,
    rx_req: crossbeam_channel::Receiver<FormattingRequest>,
    client: LspHost<TypstLanguageServer>,
    position_encoding: PositionEncoding,
) {
    type FmtFn = Box<dyn Fn(Source) -> LspResult<Option<Vec<TextEdit>>>>;
    let compile = |c: FormattingConfig| -> FmtFn {
        log::info!("formatting thread with config: {c:#?}");
        match c.mode {
            FormatterMode::Typstyle => {
                let cw = c.width as usize;
                let f: FmtFn = Box::new(move |e: Source| {
                    let res = typstyle_core::pretty_print(e.text(), cw);
                    Ok(calc_diff(e, res, position_encoding))
                });
                f
            }
            FormatterMode::Typstfmt => {
                let config = typstfmt_lib::Config {
                    max_line_length: 120,
                    ..typstfmt_lib::Config::default()
                };
                let f: FmtFn = Box::new(move |e: Source| {
                    let res = typstfmt_lib::format(e.text(), config);
                    Ok(calc_diff(e, res, position_encoding))
                });
                f
            }
            FormatterMode::Disable => {
                let f: FmtFn = Box::new(|_| Ok(None));
                f
            }
        }
    };

    let mut f: FmtFn = compile(init_c);
    while let Ok(req) = rx_req.recv() {
        match req {
            FormattingRequest::ChangeConfig(c) => f = compile(c),
            FormattingRequest::Formatting((id, source)) => {
                let res = f(source);
                if let Ok(response) = result_to_response_(id, res) {
                    client.respond(response);
                }
            }
        }
    }

    log::info!("formatting thread did shut down");
}

/// Poolman's implementation of the diffing algorithm, borrowed from
/// [`Source::replace`].
fn calc_diff(prev: Source, next: String, encoding: PositionEncoding) -> Option<Vec<TextEdit>> {
    let old = prev.text();
    let new = &next;

    let mut prefix = old
        .as_bytes()
        .iter()
        .zip(new.as_bytes())
        .take_while(|(x, y)| x == y)
        .count();

    if prefix == old.len() && prefix == new.len() {
        return Some(vec![]);
    }

    while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) {
        prefix -= 1;
    }

    let mut suffix = old[prefix..]
        .as_bytes()
        .iter()
        .zip(new[prefix..].as_bytes())
        .rev()
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
