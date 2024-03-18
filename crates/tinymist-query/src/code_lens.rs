use lsp_types::Command;

use crate::{prelude::*, SyntaxRequest};

/// The [`textDocument/codeLens`] request is sent from the client to the server
/// to compute code lenses for a given text document.
///
/// [`textDocument/codeLens`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_codeLens
#[derive(Debug, Clone)]
pub struct CodeLensRequest {
    /// The path of the document to request for.
    pub path: PathBuf,
}

impl SyntaxRequest for CodeLensRequest {
    type Response = Vec<CodeLens>;

    fn request(self, ctx: &mut AnalysisContext) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;

        let doc_start = ctx.to_lsp_range(0..0, &source);

        let mut res = vec![];

        let run_code_lens_cmd = |title: &str, args: Vec<JsonValue>| Command {
            title: title.to_string(),
            command: "tinymist.runCodeLens".to_string(),
            arguments: Some(args),
        };

        let doc_lens = |title: &str, args: Vec<JsonValue>| CodeLens {
            range: doc_start,
            command: Some(run_code_lens_cmd(title, args)),
            data: None,
        };

        res.push(doc_lens("Preview", vec!["preview".into()]));
        res.push(doc_lens("Preview in ..", vec!["preview-in".into()]));
        res.push(doc_lens("Export PDF", vec!["export-pdf".into()]));
        res.push(doc_lens("Export as ..", vec!["export-as".into()]));

        Some(res)
    }
}
