use lsp_types::Command;

use crate::{SemanticRequest, prelude::*};

/// The [`textDocument/codeLens`] request is sent from the client to the server
/// to compute code lenses for a given text document.
///
/// [`textDocument/codeLens`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_codeLens
#[derive(Debug, Clone)]
pub struct CodeLensRequest {
    /// The path of the document to request for.
    pub path: PathBuf,
    /// Whether this document is the one currently pinned as the main file by
    /// the user (via `tinymist.pinMain`).
    pub is_pinned_here: bool,
}

impl SemanticRequest for CodeLensRequest {
    type Response = Vec<CodeLens>;

    fn request(self, ctx: &mut LocalContext) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;

        let mut res = vec![];

        let doc_start = ctx.to_lsp_range(0..0, &source);

        // Unlike the lenses below, `tinymist.pinMain` is a real, directly
        // executable LSP command, so this lens works for every client and is
        // not gated behind `support_client_codelens`. It is the only way for a
        // client that cannot run arbitrary editor commands (e.g. Zed, Helix,
        // Neovim) to keep the background/browsing preview on one document:
        // that preview otherwise follows whichever document was most recently
        // opened, and closing a second document does not hand it back. See
        // `ServerState::focus_main_file`.
        let pin_title = if self.is_pinned_here {
            tinymist_l10n::t!("tinymist-query.code-lens.unpinMain", "📌 Unpin Preview")
        } else {
            tinymist_l10n::t!(
                "tinymist-query.code-lens.pinMain",
                "📌 Pin Preview to This File"
            )
        };
        let pin_arg = if self.is_pinned_here {
            JsonValue::Null
        } else {
            self.path.display().to_string().into()
        };
        res.push(CodeLens {
            range: doc_start,
            command: Some(Command {
                title: pin_title.to_string(),
                command: "tinymist.pinMain".to_string(),
                arguments: Some(vec![pin_arg]),
            }),
            data: None,
        });

        let mut doc_lens = |title: &str, args: Vec<JsonValue>| {
            if !ctx.analysis.support_client_codelens {
                return;
            }
            res.push(CodeLens {
                range: doc_start,
                command: Some(Command {
                    title: title.to_string(),
                    command: "tinymist.runCodeLens".to_string(),
                    arguments: Some(args),
                }),
                data: None,
            })
        };

        doc_lens(
            &tinymist_l10n::t!("tinymist-query.code-action.profile", "Profile"),
            vec!["profile".into()],
        );
        doc_lens(
            &tinymist_l10n::t!("tinymist-query.code-action.preview", "Preview"),
            vec!["preview".into()],
        );

        let is_html = ctx
            .world()
            .library
            .features
            .is_enabled(typst::Feature::Html);

        doc_lens(
            &tinymist_l10n::t!("tinymist-query.code-action.export", "Export"),
            vec!["export".into()],
        );
        if is_html {
            doc_lens("HTML", vec!["export-html".into()]);
        } else {
            doc_lens("PDF", vec!["export-pdf".into()]);
        }

        doc_lens(
            &tinymist_l10n::t!("tinymist-query.code-action.more", "More .."),
            vec!["more".into()],
        );

        if !ctx.analysis.support_client_codelens
            && let Some(uri) = path_to_url(&self.path)
                .ok()
                .and_then(|u| serde_json::to_value(u).ok())
        {
            res.push(CodeLens {
                range: doc_start,
                command: Some(Command {
                    title: if is_html {
                        tinymist_l10n::t!("tinymist-query.code-action.exportHtml", "Export HTML")
                    } else {
                        tinymist_l10n::t!("tinymist-query.code-action.exportPdf", "Export PDF")
                    }
                    .to_string(),
                    command: if is_html {
                        "tinymist.exportHtml"
                    } else {
                        "tinymist.exportPdf"
                    }
                    .to_string(),
                    arguments: Some(vec![uri]),
                }),
                data: None,
            })
        }

        Some(res)
    }
}
