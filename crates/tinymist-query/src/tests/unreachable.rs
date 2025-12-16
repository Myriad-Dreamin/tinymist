use serde::Serialize;
use tinymist_lint::KnownIssues;
use tinymist_tests::Settings;

use crate::DiagWorker;
use crate::tests::*;

#[derive(Debug, Clone, Serialize)]
struct SimpleDiag {
    range: String,
    message: String,
}

#[test]
fn unreachable() {
    let mut settings = Settings::new();
    settings.set_prepend_module_to_snapshot(false);
    settings.set_snapshot_path("../fixtures/unreachable/snaps");
    settings.bind(|| {
        tinymist_tests::glob!("../fixtures", "unreachable/*.typ", |path| {
            let contents = std::fs::read_to_string(path).unwrap();
            #[cfg(windows)]
            let contents = contents.replace("\r\n", "\n");

            run_with_sources(&contents, |verse, pw| {
                run_with_ctx(verse, pw, &|ctx, path| {
                    let source = ctx.source_by_path(&path).unwrap();
                    let lint_diags = ctx.lint(&source, &KnownIssues::default());

                    let lsp = DiagWorker::new(ctx).convert_all(lint_diags.iter());
                    let mut diags: Vec<SimpleDiag> = lsp
                        .into_values()
                        .flatten()
                        .filter(|d| d.message == "unreachable code")
                        .map(|d| SimpleDiag {
                            range: JsonRepr::range(d.range),
                            message: d.message,
                        })
                        .collect();
                    diags.sort_by(|a, b| {
                        a.range
                            .cmp(&b.range)
                            .then_with(|| a.message.cmp(&b.message))
                    });

                    assert_snapshot!(JsonRepr::new_pure(diags));
                })
            });
        });
    });
}
