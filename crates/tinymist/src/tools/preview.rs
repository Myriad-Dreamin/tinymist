use std::sync::Arc;

use typst_ts_core::TypstDocument;

#[cfg(feature = "preview")]
pub use typst_preview::CompileStatus;
#[cfg(not(feature = "preview"))]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum CompileStatus {
    Compiling,
    CompileSuccess,
    CompileError,
}

#[cfg(feature = "preview")]
pub use typst_preview::CompilationHandle;
#[cfg(not(feature = "preview"))]
pub trait CompilationHandle: Send + 'static {
    fn status(&self, status: CompileStatus);
    fn notify_compile(&self, res: Result<Arc<TypstDocument>, CompileStatus>);
}

#[cfg(feature = "preview")]
mod preview_exts {
    use std::path::Path;

    use typst::layout::Position;
    use typst::syntax::Span;
    use typst_preview::{
        CompileHost, DocToSrcJumpInfo, EditorServer, Location, MemoryFiles, MemoryFilesShort,
        SourceFileServer,
    };
    use typst_ts_compiler::vfs::notify::FileChangeSet;
    use typst_ts_compiler::vfs::notify::MemoryEvent;
    use typst_ts_core::debug_loc::SourceSpanOffset;
    use typst_ts_core::Error;

    use crate::actor::typst::CompileActor;

    impl SourceFileServer for CompileActor {
        async fn resolve_source_span(
            &mut self,
            loc: Location,
        ) -> Result<Option<SourceSpanOffset>, Error> {
            let Location::Src(src_loc) = loc;
            self.inner().resolve_src_location(src_loc).await
        }

        async fn resolve_document_position(
            &mut self,
            loc: Location,
        ) -> Result<Option<Position>, Error> {
            let Location::Src(src_loc) = loc;

            let path = Path::new(&src_loc.filepath).to_owned();
            let line = src_loc.pos.line;
            let column = src_loc.pos.column;

            self.inner()
                .resolve_src_to_doc_jump(path, line, column)
                .await
        }

        async fn resolve_source_location(
            &mut self,
            s: Span,
            offset: Option<usize>,
        ) -> Result<Option<DocToSrcJumpInfo>, Error> {
            Ok(self
                .inner()
                .resolve_span_and_offset(s, offset)
                .await
                .map_err(|err| {
                    log::error!("TypstActor: failed to resolve span and offset: {:#}", err);
                })
                .ok()
                .flatten()
                .map(|e| DocToSrcJumpInfo {
                    filepath: e.filepath,
                    start: e.start,
                    end: e.end,
                }))
        }
    }

    impl EditorServer for CompileActor {
        async fn update_memory_files(
            &mut self,
            files: MemoryFiles,
            reset_shadow: bool,
        ) -> Result<(), Error> {
            // todo: is it safe to believe that the path is normalized?
            let now = std::time::SystemTime::now();
            let files = FileChangeSet::new_inserts(
                files
                    .files
                    .into_iter()
                    .map(|(path, content)| {
                        let content = content.as_bytes().into();
                        // todo: cloning PathBuf -> Arc<Path>
                        (path.into(), Ok((now, content)).into())
                    })
                    .collect(),
            );
            self.inner().add_memory_changes(if reset_shadow {
                MemoryEvent::Sync(files)
            } else {
                MemoryEvent::Update(files)
            });

            Ok(())
        }

        async fn remove_shadow_files(&mut self, files: MemoryFilesShort) -> Result<(), Error> {
            // todo: is it safe to believe that the path is normalized?
            let files =
                FileChangeSet::new_removes(files.files.into_iter().map(From::from).collect());
            self.inner().add_memory_changes(MemoryEvent::Update(files));

            Ok(())
        }
    }

    impl CompileHost for CompileActor {}
}
