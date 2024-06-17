use await_tree::InstrumentAwait;

use log::{debug, error, info};
use tokio::sync::{broadcast, mpsc};
use typst::syntax::Span;
use typst_ts_core::debug_loc::{CharPosition, DocumentPosition, SourceLocation, SourceSpanOffset};

use crate::await_tree::REGISTRY;
use crate::{
    ChangeCursorPositionRequest, EditorServer, MemoryFiles, MemoryFilesShort, SourceFileServer,
    SrcToDocJumpRequest,
};

use super::render::RenderActorRequest;
use super::{editor::EditorActorRequest, webview::WebviewActorRequest};
use crate::DocToSrcJumpInfo;

#[derive(Debug)]
pub enum TypstActorRequest {
    DocToSrcJumpResolve((SourceSpanOffset, SourceSpanOffset)),
    ChangeCursorPosition(ChangeCursorPositionRequest),
    SrcToDocJumpResolve(SrcToDocJumpRequest),

    SyncMemoryFiles(MemoryFiles),
    UpdateMemoryFiles(MemoryFiles),
    RemoveMemoryFiles(MemoryFilesShort),
}

pub struct TypstActor<T> {
    client: T,

    mailbox: mpsc::UnboundedReceiver<TypstActorRequest>,

    editor_conn_sender: mpsc::UnboundedSender<EditorActorRequest>,
    webview_conn_sender: broadcast::Sender<WebviewActorRequest>,
    renderer_sender: broadcast::Sender<RenderActorRequest>,
}

type MpScChannel<T> = (mpsc::UnboundedSender<T>, mpsc::UnboundedReceiver<T>);
type BroadcastChannel<T> = (broadcast::Sender<T>, broadcast::Receiver<T>);

pub struct Channels {
    pub typst_mailbox: MpScChannel<TypstActorRequest>,
    pub renderer_mailbox: BroadcastChannel<RenderActorRequest>,
    pub editor_conn: MpScChannel<EditorActorRequest>,
    pub webview_conn: BroadcastChannel<WebviewActorRequest>,
}

impl<T> TypstActor<T> {
    pub fn set_up_channels() -> Channels {
        let typst_mailbox = mpsc::unbounded_channel();
        let renderer_mailbox = broadcast::channel(1024);
        let editor_conn = mpsc::unbounded_channel();
        let webview_conn = broadcast::channel(32);
        Channels {
            typst_mailbox,
            renderer_mailbox,
            editor_conn,
            webview_conn,
        }
    }

    pub fn new(
        client: T,
        mailbox: mpsc::UnboundedReceiver<TypstActorRequest>,
        renderer_sender: broadcast::Sender<RenderActorRequest>,
        editor_conn_sender: mpsc::UnboundedSender<EditorActorRequest>,
        webview_conn_sender: broadcast::Sender<WebviewActorRequest>,
    ) -> Self {
        Self {
            client,
            mailbox,
            renderer_sender,
            editor_conn_sender,
            webview_conn_sender,
        }
    }
}

impl<T: SourceFileServer + EditorServer> TypstActor<T> {
    pub async fn run(self) {
        let root = REGISTRY
            .lock()
            .await
            .register("typst actor".into(), "typst actor");
        root.instrument(self.run_instrumented()).await;
    }

    pub async fn run_instrumented(mut self) {
        debug!("TypstActor: waiting for message");
        while let Some(mail) = self
            .mailbox
            .recv()
            .instrument_await("waiting for message")
            .await
        {
            self.process_mail(mail)
                .instrument_await("processing mail")
                .await;
        }
        info!("TypstActor: exiting");
    }

    async fn process_mail(&mut self, mail: TypstActorRequest) {
        match mail {
            TypstActorRequest::DocToSrcJumpResolve(span_range) => {
                debug!("TypstActor: processing doc2src: {:?}", span_range);
                let res = self
                    .resolve_span_range(span_range)
                    .instrument_await("resolve span range")
                    .await;

                if let Some(info) = res {
                    let _ = self
                        .editor_conn_sender
                        .send(EditorActorRequest::DocToSrcJump(info));
                }
            }
            TypstActorRequest::ChangeCursorPosition(req) => {
                debug!("TypstActor: processing src2doc: {:?}", req);

                let res = self
                    .client
                    .resolve_source_span(crate::Location::Src(SourceLocation {
                        filepath: req.filepath.to_string_lossy().to_string(),
                        pos: CharPosition {
                            line: req.line,
                            column: req.character,
                        },
                    }))
                    .instrument_await("resolve span range")
                    .await
                    .map_err(|err| {
                        error!("TypstActor: failed to resolve cursor position: {:#}", err);
                    })
                    .ok()
                    .flatten();

                if let Some(info) = res {
                    let _ = self
                        .renderer_sender
                        .send(RenderActorRequest::ChangeCursorPosition(info));
                }
            }
            TypstActorRequest::SrcToDocJumpResolve(req) => {
                debug!("TypstActor: processing src2doc: {:?}", req);

                // todo: change name to resolve resolve src position
                let res = self
                    .client
                    .resolve_document_position(crate::Location::Src(SourceLocation {
                        filepath: req.filepath.to_string_lossy().to_string(),
                        pos: CharPosition {
                            line: req.line,
                            column: req.character,
                        },
                    }))
                    .instrument_await("resolve doc position")
                    .await
                    .map_err(|err| {
                        error!("TypstActor: failed to resolve src to doc jump: {:#}", err);
                    })
                    .ok()
                    .flatten();
                // impl From<TypstPosition> for DocumentPosition {
                //     fn from(position: TypstPosition) -> Self {
                //         Self {
                //             page_no: position.page.into(),
                //             x: position.point.x.to_pt() as f32,
                //             y: position.point.y.to_pt() as f32,
                //         }
                //     }
                // }

                if let Some(info) = res {
                    let _ = self
                        .webview_conn_sender
                        .send(WebviewActorRequest::SrcToDocJump(DocumentPosition {
                            page_no: info.page.into(),
                            x: info.point.x.to_pt() as f32,
                            y: info.point.y.to_pt() as f32,
                        }));
                }
            }
            TypstActorRequest::SyncMemoryFiles(m) => {
                debug!(
                    "TypstActor: processing SYNC memory files: {:?}",
                    m.files.keys().collect::<Vec<_>>()
                );
                handle_error(
                    "SyncMemoryFiles",
                    self.client
                        .update_memory_files(m, true)
                        .instrument_await("sync memory files")
                        .await,
                );
            }
            TypstActorRequest::UpdateMemoryFiles(m) => {
                debug!(
                    "TypstActor: processing UPDATE memory files: {:?}",
                    m.files.keys().collect::<Vec<_>>()
                );
                handle_error(
                    "UpdateMemoryFiles",
                    self.client
                        .update_memory_files(m, false)
                        .instrument_await("update memory files")
                        .await,
                );
            }
            TypstActorRequest::RemoveMemoryFiles(m) => {
                debug!("TypstActor: processing REMOVE memory files: {:?}", m.files);
                handle_error(
                    "RemoveMemoryFiles",
                    self.client
                        .remove_shadow_files(m)
                        .instrument_await("remove memory files")
                        .await,
                );
            }
        }
    }

    async fn resolve_span(&mut self, s: Span, offset: Option<usize>) -> Option<DocToSrcJumpInfo> {
        self.client
            .resolve_source_location(s, offset)
            .instrument_await("resolve span")
            .await
            .map_err(|err| {
                error!("TypstActor: failed to resolve doc to src jump: {:#}", err);
            })
            .ok()
            .flatten()
    }

    async fn resolve_span_offset(&mut self, s: SourceSpanOffset) -> Option<DocToSrcJumpInfo> {
        self.resolve_span(s.span, Some(s.offset))
            .instrument_await("resolve span offset")
            .await
    }

    async fn resolve_span_range(
        &mut self,
        span_range: (SourceSpanOffset, SourceSpanOffset),
    ) -> Option<DocToSrcJumpInfo> {
        // Resolves FileLoC of start, end, and the element wide
        let st_res = self.resolve_span_offset(span_range.0).await;
        let ed_res = self.resolve_span_offset(span_range.1).await;
        let elem_res = self
            .resolve_span(span_range.1.span, None)
            .instrument_await("resolve span")
            .await;

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
            (Some(e), None) | (None, Some(e)) => Some(e),
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
            (.., Some(e)) | (Some(e), None) => Some(e),
            (None, None) => None,
        }
    }
}

fn handle_error<T>(loc: &'static str, m: Result<T, typst_ts_core::Error>) -> Option<T> {
    if let Err(err) = &m {
        error!("TypstActor: failed to {loc}: {err:#}");
    }

    m.ok()
}
