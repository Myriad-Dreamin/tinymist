//! The grammar (checking) actor

use std::sync::Arc;

use crate::{tool::lint::typos::typos_check, SpellCheckerMode};
use reflexo::ImmutPath;
use sync_lsp::TypedLspClient;
use tinymist_query::{LspDiagnostic, PositionEncoding};
use typst::syntax::Source;

use super::SyncTaskFactory;

#[derive(Debug, Clone)]
pub struct SpellCheckUserConfig {
    pub mode: SpellCheckerMode,
    pub position_encoding: PositionEncoding,
}

#[derive(Clone)]
pub struct SpellCheckTask {
    /// The lsp client
    pub client: TypedLspClient<Self>,

    factory: SyncTaskFactory<SpellCheckUserConfig>,
}

impl SpellCheckTask {
    pub fn new(client: TypedLspClient<Self>, c: SpellCheckUserConfig) -> Self {
        Self {
            client,
            factory: SyncTaskFactory::new(c),
        }
    }

    pub fn change_config(&self, c: SpellCheckUserConfig) {
        self.factory.mutate(|data| *data = c);
    }

    pub fn syntax_level(&self, src: Source) {
        let c = self.factory.task();
        self.client.handle.spawn(async move {
            let res = Self::syntax_level_(c, src).await;
            let res = match res {
                Ok(res) => res,
                Err(err) => {
                    log::error!("SpellCheckTask: failed to check syntax: {:#}", err);
                    return;
                }
            };
            println!("{res:#?}");
        });
    }

    pub async fn syntax_level_(
        this: Arc<SpellCheckUserConfig>,
        src: Source,
    ) -> anyhow::Result<Option<Vec<LspDiagnostic>>> {
        match this.mode {
            SpellCheckerMode::Typos => {
                typos_check(&src, |span, offset, corrections| {
                    let _ = span;
                    let _ = offset;
                    let _ = corrections;
                    let _ = Self::convert_suggestions;

                    // tinymist_query::convert_diagnostics(ctx, errors)

                    // let mut accum = AccumulatePosition::new();
                    // let mut ignores: Option<Ignores> = None;
                    todo!()
                });

                Ok(None)
            }
            SpellCheckerMode::Disable => Ok(None),
        }
    }

    pub fn remove_syntax_level(&self, p: ImmutPath) {
        let _ = p;
    }

    fn convert_suggestions() {
        // log::trace!("notify suggestions: {:#?}", suggestions);
        // let suggestions = suggestions.unwrap_or_default();
        // let suggestions = suggestions
        //     .into_iter()
        //     .map(diag_from_suggestion)
        //     .collect::<Vec<_>>();

        // let suggestions =
        //     self.run_analysis(|ctx| tinymist_query::convert_diagnostics(ctx,
        // suggestions.iter()));

        // match suggestions {
        //     Ok(suggestions) => {
        //         // todo: better way to remove suggestions
        //         // todo: check all errors in this file
        //         let detached = self.inner.world().entry.is_inactive();
        //         let valid = !detached;
        //         self.handler
        //             .push_diagnostics(valid.then_some(suggestions),
        // Some("grammar"));     }
        //     Err(err) => {
        //         log::error!("TypstActor: failed to convert diagnostics:
        // {:#}", err);         self.handler.push_diagnostics(None,
        // Some("grammar"));     }
        // }
    }
}

//     // todo: check grammar
//     let doc = self.document.borrow().clone();
//     let Some(doc) = doc else {
//         continue;
//     };
//    let res=  checkers::nlprule::nlp_check_docs(doc.document);
//     self.suggestions.send(VersionedSuggestions {
//         version: doc.version as u64,
//         suggestions: res,
//     }).await.unwrap();
