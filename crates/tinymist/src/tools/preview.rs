use std::num::NonZeroUsize;
use std::path::Path;

use typst::layout::{Frame, FrameItem, Point, Position};
use typst::syntax::{LinkedNode, Source, Span, SyntaxKind, VirtualPath};
use typst::World;
pub use typst_preview::{CompilationHandle, CompileStatus};
use typst_preview::{
    CompileHost, DocToSrcJumpInfo, EditorServer, Location, MemoryFiles, MemoryFilesShort,
    SourceFileServer,
};
use typst_ts_compiler::vfs::notify::{FileChangeSet, MemoryEvent};
use typst_ts_compiler::EntryReader;
use typst_ts_core::debug_loc::SourceSpanOffset;
use typst_ts_core::{Error, TypstDocument, TypstFileId};

use crate::actor::typ_client::CompileClientActor;
use crate::actor::typ_server::CompileSnapshot;
use crate::world::{LspCompilerFeat, LspWorld};

impl CompileClientActor {
    /// fixme: character is 0-based, UTF-16 code unit.
    /// We treat it as UTF-8 now.
    fn resolve_source_span(world: &LspWorld, loc: Location) -> Option<SourceSpanOffset> {
        let Location::Src(loc) = loc;

        let filepath = Path::new(&loc.filepath);
        let relative_path = filepath.strip_prefix(&world.workspace_root()?).ok()?;

        let source_id = TypstFileId::new(None, VirtualPath::new(relative_path));
        let source = world.source(source_id).ok()?;
        let cursor = source.line_column_to_byte(loc.pos.line, loc.pos.column)?;

        let node = LinkedNode::new(source.root()).leaf_at(cursor)?;
        if node.kind() != SyntaxKind::Text {
            return None;
        }
        let span = node.span();
        // todo: unicode char
        let offset = cursor.saturating_sub(node.offset());

        Some(SourceSpanOffset { span, offset })
    }

    // resolve_document_position
    fn resolve_document_position(
        snap: &CompileSnapshot<LspCompilerFeat>,
        loc: Location,
    ) -> Option<Position> {
        let Location::Src(src_loc) = loc;

        let path = Path::new(&src_loc.filepath).to_owned();
        let line = src_loc.pos.line;
        let column = src_loc.pos.column;

        let doc = snap.doc().ok();
        let doc = doc.as_deref()?;
        let world = &snap.world;

        let relative_path = path.strip_prefix(&world.workspace_root()?).ok()?;

        let source_id = TypstFileId::new(None, VirtualPath::new(relative_path));
        let source = world.source(source_id).ok()?;
        let cursor = source.line_column_to_byte(line, column)?;

        jump_from_cursor(doc, &source, cursor)
    }

    fn resolve_source_location(
        world: &LspWorld,
        span: Span,
        offset: Option<usize>,
    ) -> Option<DocToSrcJumpInfo> {
        let resolve_off =
            |src: &Source, off: usize| src.byte_to_line(off).zip(src.byte_to_column(off));

        let source = world.source(span.id()?).ok()?;
        let mut range = source.find(span)?.range();
        if let Some(off) = offset {
            if off < range.len() {
                range.start += off;
            }
        }
        let filepath = world.path_for_id(span.id()?).ok()?;
        Some(DocToSrcJumpInfo {
            filepath: filepath.to_string_lossy().to_string(),
            start: resolve_off(&source, range.start),
            end: resolve_off(&source, range.end),
        })
    }
}

impl SourceFileServer for CompileClientActor {
    /// fixme: character is 0-based, UTF-16 code unit.
    /// We treat it as UTF-8 now.
    async fn resolve_source_span(
        &mut self,
        loc: Location,
    ) -> Result<Option<SourceSpanOffset>, Error> {
        let snap = self.snapshot()?.snapshot().await?;
        Ok(Self::resolve_source_span(&snap.world, loc))
    }

    /// fixme: character is 0-based, UTF-16 code unit.
    /// We treat it as UTF-8 now.
    async fn resolve_document_position(
        &mut self,
        loc: Location,
    ) -> Result<Option<Position>, Error> {
        let snap = self.snapshot()?.snapshot().await?;
        Ok(Self::resolve_document_position(&snap, loc))
    }

    async fn resolve_source_location(
        &mut self,
        span: Span,
        offset: Option<usize>,
    ) -> Result<Option<DocToSrcJumpInfo>, Error> {
        let snap = self.snapshot()?.snapshot().await?;
        Ok(Self::resolve_source_location(&snap.world, span, offset))
    }
}

/// Find the output location in the document for a cursor position.
fn jump_from_cursor(document: &TypstDocument, source: &Source, cursor: usize) -> Option<Position> {
    let node = LinkedNode::new(source.root()).leaf_at(cursor)?;
    if node.kind() != SyntaxKind::Text {
        return None;
    }

    let mut min_dis = u64::MAX;
    let mut p = Point::default();
    let mut ppage = 0usize;

    let span = node.span();
    for (i, page) in document.pages.iter().enumerate() {
        let t_dis = min_dis;
        if let Some(pos) = find_in_frame(&page.frame, span, &mut min_dis, &mut p) {
            return Some(Position {
                page: NonZeroUsize::new(i + 1)?,
                point: pos,
            });
        }
        if t_dis != min_dis {
            ppage = i;
        }
    }

    if min_dis == u64::MAX {
        return None;
    }

    Some(Position {
        page: NonZeroUsize::new(ppage + 1)?,
        point: p,
    })
}

/// Find the position of a span in a frame.
fn find_in_frame(frame: &Frame, span: Span, min_dis: &mut u64, p: &mut Point) -> Option<Point> {
    for (mut pos, item) in frame.items() {
        if let FrameItem::Group(group) = item {
            // TODO: Handle transformation.
            if let Some(point) = find_in_frame(&group.frame, span, min_dis, p) {
                return Some(point + pos);
            }
        }

        if let FrameItem::Text(text) = item {
            for glyph in &text.glyphs {
                if glyph.span.0 == span {
                    return Some(pos);
                }
                if glyph.span.0.id() == span.id() {
                    let dis = glyph.span.0.number().abs_diff(span.number());
                    if dis < *min_dis {
                        *min_dis = dis;
                        *p = pos;
                    }
                }
                pos.x += glyph.x_advance.at(text.size);
            }
        }
    }

    None
}

impl EditorServer for CompileClientActor {
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
        self.add_memory_changes(if reset_shadow {
            MemoryEvent::Sync(files)
        } else {
            MemoryEvent::Update(files)
        });

        Ok(())
    }

    async fn remove_shadow_files(&mut self, files: MemoryFilesShort) -> Result<(), Error> {
        // todo: is it safe to believe that the path is normalized?
        let files = FileChangeSet::new_removes(files.files.into_iter().map(From::from).collect());
        self.add_memory_changes(MemoryEvent::Update(files));

        Ok(())
    }
}

impl CompileHost for CompileClientActor {}
