//! Document preview tool for Typst

use std::num::NonZeroUsize;
use std::{borrow::Cow, collections::HashMap, net::SocketAddr, path::Path, sync::Arc};

use actor::typ_server::SucceededArtifact;
use anyhow::Context;
use hyper::service::{make_service_fn, service_fn};
use lsp_types::notification::Notification;
use serde::Serialize;
use serde_json::Value as JsonValue;
use sync_lsp::just_ok;
use tinymist_assets::TYPST_PREVIEW_HTML;
use tinymist_query::{analysis::Analysis, PositionEncoding};
use tokio::sync::{mpsc, oneshot};
use typst::foundations::{Str, Value};
use typst::layout::{Frame, FrameItem, Point, Position};
use typst::syntax::{LinkedNode, Source, Span, SyntaxKind, VirtualPath};
use typst::World;
pub use typst_preview::CompileStatus;
use typst_preview::{
    CompileHost, ControlPlaneMessage, ControlPlaneResponse, DocToSrcJumpInfo, EditorServer,
    Location, LspControlPlaneRx, LspControlPlaneTx, MemoryFiles, MemoryFilesShort, PreviewArgs,
    PreviewBuilder, PreviewMode, Previewer, SourceFileServer,
};
use typst_ts_compiler::vfs::notify::{FileChangeSet, MemoryEvent};
use typst_ts_compiler::EntryReader;
use typst_ts_core::config::{compiler::EntryOpts, CompileOpts};
use typst_ts_core::debug_loc::SourceSpanOffset;
use typst_ts_core::{Error, TypstDocument, TypstFileId};

use crate::world::{LspCompilerFeat, LspWorld};
use crate::*;
use actor::{
    preview::{PreviewActor, PreviewRequest, PreviewTab},
    typ_client::CompileHandler,
    typ_server::CompileServerActor,
};

impl CompileHost for CompileHandler {}

impl CompileHandler {
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

    async fn resolve_document_position(
        snap: &SucceededArtifact<LspCompilerFeat>,
        loc: Location,
    ) -> Option<Position> {
        let Location::Src(src_loc) = loc;

        let path = Path::new(&src_loc.filepath).to_owned();
        let line = src_loc.pos.line;
        let column = src_loc.pos.column;

        let doc = snap.success_doc();
        let doc = doc.as_deref()?;
        let world = snap.world();

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

impl SourceFileServer for CompileHandler {
    /// fixme: character is 0-based, UTF-16 code unit.
    /// We treat it as UTF-8 now.
    async fn resolve_source_span(&self, loc: Location) -> Result<Option<SourceSpanOffset>, Error> {
        let snap = self.snapshot()?.receive().await?;
        Ok(Self::resolve_source_span(&snap.world, loc))
    }

    /// fixme: character is 0-based, UTF-16 code unit.
    /// We treat it as UTF-8 now.
    async fn resolve_document_position(&self, loc: Location) -> Result<Option<Position>, Error> {
        let snap = self.succeeded_artifact()?.receive().await?;
        Ok(Self::resolve_document_position(&snap, loc).await)
    }

    async fn resolve_source_location(
        &self,
        span: Span,
        offset: Option<usize>,
    ) -> Result<Option<DocToSrcJumpInfo>, Error> {
        let snap = self.snapshot()?.receive().await?;
        Ok(Self::resolve_source_location(&snap.world, span, offset))
    }
}

impl EditorServer for CompileHandler {
    async fn update_memory_files(
        &self,
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

    async fn remove_shadow_files(&self, files: MemoryFilesShort) -> Result<(), Error> {
        // todo: is it safe to believe that the path is normalized?
        let files = FileChangeSet::new_removes(files.files.into_iter().map(From::from).collect());
        self.add_memory_changes(MemoryEvent::Update(files));

        Ok(())
    }
}

/// CLI Arguments for the preview tool.
#[derive(Debug, Clone, clap::Parser)]
pub struct PreviewCliArgs {
    /// Preview arguments
    #[clap(flatten)]
    pub preview: PreviewArgs,

    /// Compile arguments
    #[clap(flatten)]
    pub compile: CompileOnceArgs,

    /// Preview mode
    #[clap(long = "preview-mode", default_value = "document", value_name = "MODE")]
    pub preview_mode: PreviewMode,

    /// (File) Host for the preview server
    #[clap(
        long = "host",
        value_name = "HOST",
        default_value = "127.0.0.1:23627",
        alias = "static-file-host"
    )]
    pub static_file_host: String,

    /// Don't open the preview in the browser after compilation.
    #[clap(long = "no-open")]
    pub dont_open_in_browser: bool,
}

/// The global state of the preview tool.
pub struct PreviewState {
    /// Connection to the LSP client.
    client: TypedLspClient<PreviewState>,
    /// The backend running actor.
    preview_tx: mpsc::UnboundedSender<PreviewRequest>,
}

impl PreviewState {
    /// Create a new preview state.
    pub fn new(client: TypedLspClient<PreviewState>) -> Self {
        let (preview_tx, preview_rx) = mpsc::unbounded_channel();

        client.handle.spawn(
            PreviewActor {
                client: client.clone().to_untyped(),
                tabs: HashMap::default(),
                preview_rx,
            }
            .run(),
        );

        Self { client, preview_tx }
    }
}

/// Response for starting a preview.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartPreviewResponse {
    static_server_port: Option<u16>,
    static_server_addr: Option<String>,
    data_plane_port: Option<u16>,
}

impl PreviewState {
    /// Start a preview on a given compiler.
    pub fn start(
        &self,
        args: PreviewCliArgs,
        mut previewer: PreviewBuilder,
        compile_handler: Arc<CompileHandler>,
    ) -> SchedulableResponse<StartPreviewResponse> {
        let task_id = args.preview.task_id.clone();
        log::info!("PreviewTask({task_id}): arguments: {args:#?}");

        let (lsp_tx, lsp_rx) = LspControlPlaneTx::new();
        let LspControlPlaneRx {
            resp_rx,
            ctl_tx,
            mut shutdown_rx,
        } = lsp_rx;

        // Create a previewer
        previewer = previewer.with_lsp_connection(Some(lsp_tx));
        let previewer = previewer.start(compile_handler.clone(), TYPST_PREVIEW_HTML);

        // Forward preview responses to lsp client
        let tid = task_id.clone();
        let client = self.client.clone();
        self.client.handle.spawn(async move {
            let mut resp_rx = resp_rx;
            while let Some(resp) = resp_rx.recv().await {
                use ControlPlaneResponse::*;

                match resp {
                    // ignoring compile status per task.
                    CompileStatus(..) => {}
                    SyncEditorChanges(..) => {
                        log::warn!("PreviewTask({tid}): is sending SyncEditorChanges in lsp mode");
                    }
                    EditorScrollTo(s) => client.send_notification::<ScrollSource>(s),
                    Outline(s) => client.send_notification::<NotifDocumentOutline>(s),
                }
            }

            log::info!("PreviewTask({tid}): response channel closed");
        });

        // Process preview shutdown
        let tid = task_id.clone();
        let preview_tx = self.preview_tx.clone();
        self.client.handle.spawn(async move {
            // shutdown_rx
            let Some(()) = shutdown_rx.recv().await else {
                return;
            };

            log::info!("PreviewTask({tid}): internal killing");
            let (tx, rx) = oneshot::channel();
            preview_tx.send(PreviewRequest::Kill(tid.clone(), tx)).ok();
            rx.await.ok();
            log::info!("PreviewTask({tid}): internal killed");
        });

        let preview_tx = self.preview_tx.clone();
        just_future(async move {
            let previewer = previewer.await;

            // Put a fence to ensure the previewer can receive the first compilation.   z
            // The fence must be put after the previewer is initialized.
            compile_handler.flush_compile();

            let (ss_addr, ss_killer, ss_handle) =
                make_static_host(&previewer, args.static_file_host, args.preview_mode);
            log::info!("PreviewTask({task_id}): static file server listening on: {ss_addr}");

            let resp = StartPreviewResponse {
                static_server_port: Some(ss_addr.port()),
                static_server_addr: Some(ss_addr.to_string()),
                data_plane_port: Some(previewer.data_plane_port()),
            };

            let sent = preview_tx.send(PreviewRequest::Started(PreviewTab {
                task_id,
                previewer,
                ss_killer,
                ss_handle,
                ctl_tx,
                compile_handler,
            }));
            sent.map_err(|_| internal_error("failed to register preview tab"))?;

            Ok(resp)
        })
    }

    /// Kill a preview task. Ignore if the task is not found.
    pub fn kill(&self, task_id: String) -> AnySchedulableResponse {
        let (tx, rx) = oneshot::channel();

        let sent = self.preview_tx.send(PreviewRequest::Kill(task_id, tx));
        sent.map_err(|_| internal_error("failed to send kill request"))?;

        just_future(async move { rx.await.map_err(|_| internal_error("cancelled"))? })
    }

    /// Scroll the preview to a given position.
    pub fn scroll(&self, task_id: String, req: ControlPlaneMessage) -> AnySchedulableResponse {
        let sent = self.preview_tx.send(PreviewRequest::Scroll(task_id, req));
        sent.map_err(|_| internal_error("failed to send scroll request"))?;

        just_ok(JsonValue::Null)
    }
}

/// Create a static file server for the previewer.
pub fn make_static_host(
    previewer: &Previewer,
    static_file_addr: String,
    mode: PreviewMode,
) -> (SocketAddr, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let frontend_html = previewer.frontend_html(mode);
    let make_service = make_service_fn(move |_| {
        let html = frontend_html.clone();
        async move {
            Ok::<_, hyper::http::Error>(service_fn(move |req| {
                // todo: clone may not be necessary
                let html = html.as_ref().to_owned();
                async move {
                    if req.uri().path() == "/" {
                        log::info!("Serve frontend: {:?}", mode);
                        Ok::<_, hyper::Error>(hyper::Response::new(hyper::Body::from(html)))
                    } else {
                        // jump to /
                        let mut res = hyper::Response::new(hyper::Body::empty());
                        *res.status_mut() = hyper::StatusCode::FOUND;
                        res.headers_mut().insert(
                            hyper::header::LOCATION,
                            hyper::header::HeaderValue::from_static("/"),
                        );
                        Ok(res)
                    }
                }
            }))
        }
    });
    let server = hyper::Server::bind(&static_file_addr.parse().unwrap()).serve(make_service);
    let addr = server.local_addr();

    let (tx, rx) = tokio::sync::oneshot::channel();
    let (final_tx, final_rx) = tokio::sync::oneshot::channel();
    let graceful = server.with_graceful_shutdown(async {
        final_rx.await.ok();
        log::info!("Static file server stop requested");
    });

    let join_handle = tokio::spawn(async move {
        tokio::select! {
            Err(err) = graceful => {
                log::error!("Static file server error: {err:?}");
            }
            _ = rx => {
                final_tx.send(()).ok();
            }
        }
        log::info!("Static file server joined");
    });
    (addr, tx, join_handle)
}

/// Entry point of the preview tool.
pub async fn preview_main(args: PreviewCliArgs) -> anyhow::Result<()> {
    log::info!("Arguments: {args:#?}");

    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        log::info!("Ctrl-C received, exiting");
        std::process::exit(0);
    });

    let entry = {
        let input = args.compile.input.context("entry file must be provided")?;
        let input = Path::new(&input);
        let entry = if input.is_absolute() {
            input.to_owned()
        } else {
            std::env::current_dir().unwrap().join(input)
        };

        let root = if let Some(root) = &args.compile.root {
            if root.is_absolute() {
                root.clone()
            } else {
                std::env::current_dir().unwrap().join(root)
            }
        } else {
            std::env::current_dir().unwrap()
        };

        if !entry.starts_with(&root) {
            log::error!("entry file must be in the root directory");
            std::process::exit(1);
        }

        let relative_entry = match entry.strip_prefix(&root) {
            Ok(e) => e,
            Err(_) => {
                log::error!("entry path must be inside the root: {}", entry.display());
                std::process::exit(1);
            }
        };

        EntryOpts::new_rooted(root.clone(), Some(relative_entry.to_owned()))
    };

    let inputs = args
        .compile
        .inputs
        .iter()
        .map(|(k, v)| (Str::from(k.as_str()), Value::Str(Str::from(v.as_str()))))
        .collect();

    let world = LspUniverse::new(CompileOpts {
        entry,
        inputs,
        no_system_fonts: args.compile.font.ignore_system_fonts,
        font_paths: args.compile.font.font_paths.clone(),
        with_embedded_fonts: typst_assets::fonts().map(Cow::Borrowed).collect(),
        ..CompileOpts::default()
    })
    .expect("incorrect options");

    let (service, handle) = {
        // type EditorSender = mpsc::UnboundedSender<EditorRequest>;
        let (editor_tx, mut editor_rx) = mpsc::unbounded_channel();
        let (intr_tx, intr_rx) = mpsc::unbounded_channel();

        let handle = Arc::new(CompileHandler {
            inner: Default::default(),
            diag_group: "main".to_owned(),
            intr_tx: intr_tx.clone(),
            // export_tx,
            export: Default::default(),
            editor_tx,
            analysis: Analysis {
                position_encoding: PositionEncoding::Utf16,
                enable_periscope: false,
                caches: Default::default(),
            },
            periscope: tinymist_render::PeriscopeRenderer::default(),
            notified_revision: parking_lot::Mutex::new(0),
        });

        // Consume editor_rx
        tokio::spawn(async move { while editor_rx.recv().await.is_some() {} });

        let service =
            CompileServerActor::new(world, intr_tx, intr_rx).with_watch(Some(handle.clone()));

        (service, handle)
    };

    let previewer = PreviewBuilder::new(args.preview);
    let registered = handle.register_preview(previewer.compile_watcher());
    assert!(registered, "failed to register preview");
    let previewer = previewer.start(handle.clone(), TYPST_PREVIEW_HTML).await;
    tokio::spawn(service.run());

    let (static_server_addr, _tx, static_server_handle) =
        make_static_host(&previewer, args.static_file_host, args.preview_mode);
    log::info!("Static file server listening on: {}", static_server_addr);

    if !args.dont_open_in_browser {
        if let Err(e) = open::that_detached(format!("http://{static_server_addr}")) {
            log::error!("failed to open browser: {}", e);
        };
    }

    let _ = tokio::join!(previewer.join(), static_server_handle);

    Ok(())
}

struct ScrollSource;

impl Notification for ScrollSource {
    type Params = DocToSrcJumpInfo;
    const METHOD: &'static str = "tinymist/preview/scrollSource";
}

struct NotifDocumentOutline;

impl Notification for NotifDocumentOutline {
    type Params = typst_preview::Outline;
    const METHOD: &'static str = "tinymist/documentOutline";
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
