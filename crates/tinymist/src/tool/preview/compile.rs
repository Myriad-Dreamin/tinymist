//! Document preview tool for Typst

use std::path::Path;

use reflexo::debug_loc::SourceSpanOffset;
use reflexo_typst::{error::prelude::*, Bytes, Error, TypstDocument};
use tinymist_project::LspCompiledArtifact;
use tinymist_query::{jump_from_click, jump_from_cursor};
use typst::layout::{Abs, Point, Position};
use typst::syntax::{LinkedNode, Source, Span, SyntaxKind};
use typst::World;
use typst_preview::{
    CompileStatus, DocToSrcJumpInfo, EditorServer, Location, MemoryFiles, MemoryFilesShort,
};
use typst_shim::syntax::LinkedNodeExt;

use crate::project::{LspInterrupt, ProjectClient, ProjectInsId};
use crate::world::vfs::{notify::MemoryEvent, FileChangeSet};
use crate::*;

/// The compiler's view of the a preview task (server).
pub struct ProjectPreviewHandler {
    /// The project id.
    pub project_id: ProjectInsId,
    /// The connection to the compiler compiling projects (language server).
    pub(crate) client: Box<dyn ProjectClient>,
}

impl ProjectPreviewHandler {
    /// Requests the compiler to compile the project.
    pub fn flush_compile(&self) {
        let _ = self.project_id;
        self.client
            .interrupt(LspInterrupt::Compile(self.project_id.clone()));
    }

    /// Requests the compiler to settle the project.
    pub fn settle(&self) -> Result<(), Error> {
        self.client
            .interrupt(LspInterrupt::Settle(self.project_id.clone()));
        Ok(())
    }

    /// Requests the compiler to unpin the primary project.
    pub fn unpin_primary(&self) {
        self.client.server_event(ServerEvent::UnpinPrimaryByPreview);
    }
}

impl EditorServer for ProjectPreviewHandler {
    async fn update_memory_files(
        &self,
        files: MemoryFiles,
        reset_shadow: bool,
    ) -> Result<(), Error> {
        // todo: is it safe to believe that the path is normalized?
        let files = FileChangeSet::new_inserts(
            files
                .files
                .into_iter()
                .map(|(path, content)| {
                    // todo: cloning PathBuf -> Arc<Path>
                    (path.into(), Ok(Bytes::from_string(content)).into())
                })
                .collect(),
        );

        let intr = LspInterrupt::Memory(if reset_shadow {
            MemoryEvent::Sync(files)
        } else {
            MemoryEvent::Update(files)
        });
        self.client.interrupt(intr);

        Ok(())
    }

    async fn remove_memory_files(&self, files: MemoryFilesShort) -> Result<(), Error> {
        // todo: is it safe to believe that the path is normalized?
        let files = FileChangeSet::new_removes(files.files.into_iter().map(From::from).collect());
        self.client
            .interrupt(LspInterrupt::Memory(MemoryEvent::Update(files)));

        Ok(())
    }
}

/// The preview's view of the compiled artifact.
pub struct PreviewCompileView {
    /// The compiled artifact.
    pub art: LspCompiledArtifact,
}

impl typst_preview::CompileView for PreviewCompileView {
    fn doc(&self) -> Option<TypstDocument> {
        self.art.doc.clone()
    }

    fn status(&self) -> CompileStatus {
        match self.art.doc {
            Some(_) => CompileStatus::CompileSuccess,
            None => CompileStatus::CompileError,
        }
    }

    fn is_on_saved(&self) -> bool {
        self.art.snap.signal.by_fs_events
    }

    fn is_by_entry_update(&self) -> bool {
        self.art.snap.signal.by_entry_update
    }

    fn resolve_source_span(&self, loc: Location) -> Option<SourceSpanOffset> {
        let world = self.art.world();
        let Location::Src(loc) = loc;

        let source_id = world.id_for_path(Path::new(&loc.filepath))?;

        let source = world.source(source_id).ok()?;
        let cursor =
            source.line_column_to_byte(loc.pos.line as usize, loc.pos.character as usize)?;

        let node = LinkedNode::new(source.root()).leaf_at_compat(cursor)?;
        if !matches!(node.kind(), SyntaxKind::Text | SyntaxKind::MathText) {
            return None;
        }
        let span = node.span();
        // todo: unicode char
        let offset = cursor.saturating_sub(node.offset());

        Some(SourceSpanOffset { span, offset })
    }

    // todo: use vec2bbox to handle bbox correctly
    fn resolve_frame_loc(
        &self,
        pos: &reflexo::debug_loc::DocumentPosition,
    ) -> Option<(SourceSpanOffset, SourceSpanOffset)> {
        let TypstDocument::Paged(doc) = self.doc()? else {
            return None;
        };
        let world = self.art.world();

        let page = pos.page_no.checked_sub(1)?;
        let page = doc.pages.get(page)?;

        let click = Point::new(Abs::pt(pos.x as f64), Abs::pt(pos.y as f64));
        jump_from_click(world, &page.frame, click)
    }

    fn resolve_document_position(&self, loc: Location) -> Vec<Position> {
        let world = self.art.world();
        let Location::Src(src_loc) = loc;

        let line = src_loc.pos.line as usize;
        let column = src_loc.pos.character as usize;

        let doc = self.art.success_doc();
        let Some(doc) = doc.as_ref() else {
            return vec![];
        };

        let Some(source_id) = world.id_for_path(Path::new(&src_loc.filepath)) else {
            return vec![];
        };
        let Some(source) = world.source(source_id).ok() else {
            return vec![];
        };
        let Some(cursor) = source.line_column_to_byte(line, column) else {
            return vec![];
        };

        jump_from_cursor(doc, &source, cursor)
    }

    fn resolve_span(&self, span: Span, offset: Option<usize>) -> Option<DocToSrcJumpInfo> {
        let world = self.art.world();
        let resolve_off =
            |src: &Source, off: usize| src.byte_to_line(off).zip(src.byte_to_column(off));

        let source = world.source(span.id()?).ok()?;
        let mut range = source.find(span)?.range();
        if let Some(off) = offset {
            if off < range.len() {
                range.start += off;
            }
        }

        // todo: resolve untitled uri.
        let filepath = world.path_for_id(span.id()?).ok()?.to_err().ok()?;
        Some(DocToSrcJumpInfo {
            filepath: filepath.to_string_lossy().to_string(),
            start: resolve_off(&source, range.start),
            end: resolve_off(&source, range.end),
        })
    }
}
