use lsp_server::RequestId;
use lsp_types::{Position, Range, TextEdit};
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
) {
    type FmtFn = Box<dyn Fn(Source) -> LspResult<Option<Vec<TextEdit>>>>;
    let compile = |c: FormattingConfig| -> FmtFn {
        log::info!("formatting thread with config: {c:#?}");
        match c.mode {
            FormatterMode::Typstyle => {
                let cw = c.width as usize;
                let f: FmtFn = Box::new(move |e: Source| {
                    let res = typstyle_core::pretty_print(e.text(), cw);
                    Ok(Some(vec![TextEdit {
                        new_text: res,
                        range: Range::new(
                            Position {
                                line: 0,
                                character: 0,
                            },
                            Position {
                                line: u32::MAX,
                                character: u32::MAX,
                            },
                        ),
                    }]))
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
