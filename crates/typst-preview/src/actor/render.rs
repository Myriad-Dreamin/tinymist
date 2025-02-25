use std::ops::Range;
use std::sync::Arc;

use reflexo_typst::debug_loc::{
    DocumentPosition, ElementPoint, LspPosition, SourceLocation, SourceSpanOffset,
};
use reflexo_vec2svg::IncrSvgDocServer;
use tinymist_std::typst::TypstDocument;
use tokio::sync::{broadcast, mpsc};

use super::{editor::EditorActorRequest, webview::WebviewActorRequest};
use crate::debug_loc::SpanInterner;
use crate::outline::Outline;
use crate::{ChangeCursorPositionRequest, CompileView, DocToSrcJumpInfo, ResolveSourceLocRequest};

#[derive(Debug, Clone)]
pub struct ResolveSpanRequest(pub Vec<ElementPoint>);

#[derive(Debug, Clone)]
pub enum RenderActorRequest {
    RenderFullLatest,
    RenderIncremental,
    EditorResolveSpanRange(Range<SourceSpanOffset>),
    WebviewResolveSpan(ResolveSpanRequest),
    WebviewResolveFrameLoc(DocumentPosition),
    ResolveSourceLoc(ResolveSourceLocRequest),
    ChangeCursorPosition(ChangeCursorPositionRequest),
}

impl RenderActorRequest {
    pub fn is_full_render(&self) -> bool {
        match self {
            Self::RenderFullLatest => true,
            Self::RenderIncremental => false,
            Self::EditorResolveSpanRange(_) => false,
            Self::WebviewResolveSpan(_) => false,
            Self::ResolveSourceLoc(_) => false,
            Self::WebviewResolveFrameLoc(_) => false,
            Self::ChangeCursorPosition(_) => false,
        }
    }
}

pub struct RenderActor {
    mailbox: broadcast::Receiver<RenderActorRequest>,
    view: Arc<parking_lot::RwLock<Option<Arc<dyn CompileView>>>>,
    renderer: IncrSvgDocServer,
    editor_conn_sender: mpsc::UnboundedSender<EditorActorRequest>,
    svg_sender: mpsc::UnboundedSender<Vec<u8>>,
    webview_sender: broadcast::Sender<WebviewActorRequest>,
}

impl RenderActor {
    pub fn new(
        mailbox: broadcast::Receiver<RenderActorRequest>,
        view: Arc<parking_lot::RwLock<Option<Arc<dyn CompileView>>>>,
        editor_conn_sender: mpsc::UnboundedSender<EditorActorRequest>,
        svg_sender: mpsc::UnboundedSender<Vec<u8>>,
        webview_sender: broadcast::Sender<WebviewActorRequest>,
    ) -> Self {
        let mut res = Self {
            mailbox,
            view,
            renderer: IncrSvgDocServer::default(),
            editor_conn_sender,
            svg_sender,
            webview_sender,
        };
        res.renderer.set_should_attach_debug_info(true);
        res
    }

    async fn process_message(&mut self, msg: RenderActorRequest) -> bool {
        log::trace!("RenderActor: received message: {msg:?}");

        let res = msg.is_full_render();
        match msg {
            RenderActorRequest::EditorResolveSpanRange(span_range) => {
                log::debug!("RenderActor: resolving EditorResolveSpanRange: {span_range:?}");

                self.editor_resolve_span_range(span_range);
            }
            RenderActorRequest::WebviewResolveSpan(ResolveSpanRequest(element_path)) => {
                log::debug!("RenderActor: resolving WebviewResolveSpan: {element_path:?}");
                let spans = match self.renderer.resolve_span_by_element_path(&element_path) {
                    Ok(spans) => spans,
                    Err(err) => {
                        log::info!("RenderActor: failed to resolve span: {err}");
                        return false;
                    }
                };

                log::debug!("RenderActor: resolved WebviewResolveSpan: {spans:?}");
                // end position is used
                if let Some(spans) = spans {
                    self.editor_resolve_span_range(spans.0..spans.1);
                }
            }
            RenderActorRequest::WebviewResolveFrameLoc(frame_loc) => {
                log::debug!("RenderActor: resolving WebviewResolveFrameLoc: {frame_loc:?}");
                let spans = self.resolve_span_by_frame_loc(&frame_loc);

                log::debug!("RenderActor: resolved WebviewResolveSpan: {spans:?}");
                // end position is used
                if let Some(spans) = spans {
                    self.editor_resolve_span_range(spans.0..spans.1);
                }
            }
            RenderActorRequest::ResolveSourceLoc(req) => {
                log::debug!("RenderActor: resolving ResolveSourceLoc: {req:?}");

                self.resolve_source_loc(req);
            }
            RenderActorRequest::ChangeCursorPosition(req) => {
                log::debug!("RenderActor: processing ChangeCursorPosition: {req:?}");

                self.change_cursor_position(req);
            }
            RenderActorRequest::RenderFullLatest | RenderActorRequest::RenderIncremental => {}
        }

        res
    }

    pub async fn run(mut self) {
        loop {
            let mut has_full_render = false;
            log::debug!("RenderActor: waiting for message");
            match self.mailbox.recv().await {
                Ok(msg) => {
                    has_full_render |= self.process_message(msg).await;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    log::info!("RenderActor: no more messages");
                    break;
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    log::info!("RenderActor: lagged message. Some events are dropped");
                }
            }
            // read the queue to empty
            while let Ok(msg) = self.mailbox.try_recv() {
                has_full_render |= self.process_message(msg).await;
            }
            // if a full render is requested, we render the latest document
            // otherwise, we render the incremental changes for only once
            let has_full_render = has_full_render;
            log::debug!("RenderActor: has_full_render: {has_full_render}");
            let Some(document) = self.view.read().as_ref().and_then(|view| view.doc()) else {
                log::info!("RenderActor: document is not ready");
                continue;
            };

            let data = if has_full_render {
                if let Some(data) = self.renderer.pack_current() {
                    data
                } else {
                    self.renderer.pack_delta(&document)
                }
            } else {
                self.renderer.pack_delta(&document)
            };
            let Ok(_) = self.svg_sender.send(data) else {
                log::info!("RenderActor: svg_sender is dropped");
                break;
            };
        }
        log::info!("RenderActor: exiting")
    }

    fn view(&self) -> Option<Arc<dyn CompileView>> {
        self.view.read().clone()
    }

    fn editor_resolve_span_range(&self, span_range: Range<SourceSpanOffset>) -> Option<()> {
        let req = EditorActorRequest::DocToSrcJump(self.resolve_span_range(span_range)?);
        let _ = self.editor_conn_sender.send(req);

        Some(())
    }

    fn resolve_span_range(&self, range: Range<SourceSpanOffset>) -> Option<DocToSrcJumpInfo> {
        let view = self.view()?;
        // Resolves FileLoC of start, end, and the element wide
        let st_res = view.resolve_span(range.start.span, Some(range.start.offset));
        let ed_res = view.resolve_span(range.end.span, Some(range.end.offset));
        let elem_res = view.resolve_span(range.end.span, None);

        // Combines the result of start and end
        let range_res = match (st_res, ed_res) {
            (Some(st), Some(ed)) => {
                if st.filepath == ed.filepath
                    && matches!((&st.start, &st.end), (Some(x), Some(y)) if x <= y)
                {
                    Some(DocToSrcJumpInfo {
                        filepath: st.filepath,
                        start: st.start,
                        end: ed.start,
                    })
                } else {
                    Some(ed)
                }
            }
            (Some(info), None) | (None, Some(info)) => Some(info),
            (None, None) => None,
        };

        // Account for the case where the start and end are out of order.
        //
        // This could happen because typst supports scripting, which makes text out of
        // order
        let range_res = {
            let mut range_res = range_res;
            if let Some(info) = &mut range_res {
                if let Some((x, y)) = info.start.zip(info.end) {
                    if y <= x {
                        std::mem::swap(&mut info.start, &mut info.end);
                    }
                }
            }

            range_res
        };

        // Restricts the range to the element's range
        match (elem_res, range_res) {
            (Some(elem), Some(mut rng)) if elem.filepath == rng.filepath => {
                // Account for the case where the element's range is out of order.
                let elem_start = elem.start.or(elem.end);
                let elem_end = elem.end.or(elem_start);

                // Account for the case where the range is out of order.
                let rng_start = rng.start.or(rng.end);
                let rng_end = rng.end.or(rng_start);

                if let Some((((u, inner_u), inner_v), v)) =
                    elem_start.zip(rng_start).zip(rng_end).zip(elem_end)
                {
                    rng.start = Some(inner_u.max(u).min(v));
                    rng.end = Some(inner_v.max(u).min(v));
                }
                Some(rng)
            }
            (.., Some(info)) | (Some(info), None) => Some(info),
            (None, None) => None,
        }
    }

    fn change_cursor_position(&mut self, req: ChangeCursorPositionRequest) -> Option<()> {
        let span = self
            .view()?
            .resolve_source_span(crate::Location::Src(SourceLocation {
                filepath: req.filepath.to_string_lossy().to_string(),
                pos: LspPosition {
                    line: req.line,
                    character: req.character,
                },
            }))?;
        log::info!("RenderActor: changing cursor position: {span:?}");

        let paths = self.renderer.resolve_element_paths_by_span(span).ok()?;
        log::info!("RenderActor: resolved element paths: {paths:?}");
        let _ = self
            .webview_sender
            .send(WebviewActorRequest::CursorPaths(paths));

        Some(())
    }

    fn resolve_source_loc(&self, req: ResolveSourceLocRequest) -> Option<()> {
        // todo: change name to resolve resolve src position
        let info = self
            .view()?
            .resolve_document_position(crate::Location::Src(SourceLocation {
                filepath: req.filepath.to_string_lossy().to_string(),
                pos: LspPosition {
                    line: req.line,
                    character: req.character,
                },
            }));

        if info.is_empty() {
            return None;
        }

        let _ = self.webview_sender.send(WebviewActorRequest::SrcToDocJump(
            info.into_iter()
                .map(|info| DocumentPosition {
                    page_no: info.page.into(),
                    x: info.point.x.to_pt() as f32,
                    y: info.point.y.to_pt() as f32,
                })
                .collect(),
        ));

        Some(())
    }

    /// Gets the span range of the given frame loc.
    pub fn resolve_span_by_frame_loc(
        &mut self,
        pos: &DocumentPosition,
    ) -> Option<(SourceSpanOffset, SourceSpanOffset)> {
        let view = self.view.read();
        view.as_ref()?.resolve_frame_loc(pos)
    }
}

pub struct OutlineRenderActor {
    signal: broadcast::Receiver<RenderActorRequest>,
    document: Arc<parking_lot::RwLock<Option<Arc<dyn CompileView>>>>,
    editor_tx: mpsc::UnboundedSender<EditorActorRequest>,

    span_interner: SpanInterner,
}

impl OutlineRenderActor {
    pub fn new(
        signal: broadcast::Receiver<RenderActorRequest>,
        document: Arc<parking_lot::RwLock<Option<Arc<dyn CompileView>>>>,
        editor_tx: mpsc::UnboundedSender<EditorActorRequest>,
        span_interner: SpanInterner,
    ) -> Self {
        Self {
            signal,
            document,
            editor_tx,
            span_interner,
        }
    }

    pub async fn run(mut self) {
        loop {
            log::debug!("OutlineRenderActor: waiting for message");
            match self.signal.recv().await {
                Ok(msg) => {
                    log::debug!("OutlineRenderActor: received message: {:?}", msg);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    log::info!("OutlineRenderActor: no more messages");
                    break;
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    log::info!("OutlineRenderActor: lagged message. Some events are dropped");
                }
            }
            // read the queue to empty
            while self.signal.try_recv().is_ok() {}
            // if a full render is requested, we render the latest document
            // otherwise, we render the incremental changes for only once
            let Some(document) = self.document.read().as_ref().and_then(|view| view.doc()) else {
                log::info!("OutlineRenderActor: document is not ready");
                continue;
            };
            let data = self.outline(&document).await;
            log::debug!("OutlineRenderActor: sending outline");
            let Ok(_) = self.editor_tx.send(EditorActorRequest::Outline(data)) else {
                log::info!("OutlineRenderActor: outline_sender is dropped");
                break;
            };
        }
        log::info!("OutlineRenderActor: exiting")
    }

    async fn outline(&self, document: &TypstDocument) -> Outline {
        self.span_interner
            .with_writer(|interner| {
                interner.reset();
                crate::outline::outline(interner, document)
            })
            .await
    }
}
