use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf};

use reflexo::debug_loc::DataSource;
use serde::{Deserialize, Serialize};
use typst::text::{Font, FontStretch, FontStyle, FontWeight};
use typst::{
    layout::{Frame, FrameItem},
    model::Document,
    text::TextItem,
};

use crate::{AnalysisContext, StatefulRequest, VersionedDocument};

/// Span information for some content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpanInfo {
    /// The sources that are used in the span information.
    pub sources: Vec<DataSource>,
}

/// Annotated content for a font.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotatedContent {
    /// A string of the content for slicing.
    pub content: String,
    /// The kind of the span encoding.
    pub span_kind: String,
    /// Encoded spans.
    pub spans: Vec<i32>,
}

/// Information about a font.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentFontInfo {
    /// The display name of the font, which is computed by this crate and
    /// unnecessary from any fields of the font file.
    pub name: String,
    /// The style of the font.
    pub style: FontStyle,
    /// The weight of the font.
    pub weight: FontWeight,
    /// The stretch of the font.
    pub stretch: FontStretch,
    /// The PostScript name of the font.
    pub postscript_name: Option<String>,
    /// The Family in font file.
    pub family: Option<String>,
    /// The Full Name in font file.
    pub full_name: Option<String>,
    /// The Fixed Family used by Typst.
    pub fixed_family: Option<String>,
    /// The source of the font.
    pub source: Option<u32>,
    /// The index of the font in the source.
    pub index: Option<u32>,
    /// The annotated content length of the font.
    /// If it is None, the uses is not calculated.
    /// Otherwise, it is the length of the uses.
    pub uses_scale: Option<u32>,
    /// The annotated content of the font.
    /// If it is not None, the uses_scale must be provided.
    pub uses: Option<AnnotatedContent>,
}

/// The response to a DocumentMetricsRequest.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMetricsResponse {
    /// File span information.
    pub span_info: SpanInfo,
    /// Font information.
    pub font_info: Vec<DocumentFontInfo>,
}

/// A request to compute DocumentMetrics for a document.
///
/// This is not part of the LSP protocol.
#[derive(Debug, Clone)]
pub struct DocumentMetricsRequest {
    /// The path of the document to compute DocumentMetricss.
    pub path: PathBuf,
}

impl StatefulRequest for DocumentMetricsRequest {
    type Response = DocumentMetricsResponse;

    fn request(
        self,
        ctx: &mut AnalysisContext,
        doc: Option<VersionedDocument>,
    ) -> Option<Self::Response> {
        let doc = doc?;
        let doc = doc.document;

        let mut worker = DocumentMetricsWorker {
            ctx,
            span_info: Default::default(),
            span_info2: Default::default(),
            font_info: Default::default(),
        };

        worker.work(&doc)?;

        let font_info = worker.compute()?;
        let span_info = SpanInfo {
            sources: worker.span_info2,
        };
        Some(DocumentMetricsResponse {
            span_info,
            font_info,
        })
    }
}

struct DocumentMetricsWorker<'a, 'w> {
    ctx: &'a mut AnalysisContext<'w>,
    span_info: HashMap<Arc<DataSource>, u32>,
    span_info2: Vec<DataSource>,
    font_info: HashMap<Font, u32>,
}

impl<'a, 'w> DocumentMetricsWorker<'a, 'w> {
    fn work(&mut self, doc: &Document) -> Option<()> {
        for page in &doc.pages {
            self.work_frame(&page.frame)?;
        }

        Some(())
    }

    fn work_frame(&mut self, frame: &Frame) -> Option<()> {
        for (_, elem) in frame.items() {
            self.work_elem(elem)?;
        }

        Some(())
    }

    fn work_elem(&mut self, elem: &FrameItem) -> Option<()> {
        match elem {
            FrameItem::Text(text) => self.work_text(text),
            FrameItem::Group(frame) => self.work_frame(&frame.frame),
            FrameItem::Shape(..) | FrameItem::Image(..) | FrameItem::Meta(..) => Some(()),
        }
    }

    fn work_text(&mut self, text: &TextItem) -> Option<()> {
        let use_cnt = self.font_info.entry(text.font.clone()).or_default();
        *use_cnt = use_cnt.checked_add(text.glyphs.len() as u32)?;

        Some(())
    }

    fn internal_source(&mut self, source: Arc<DataSource>) -> u32 {
        if let Some(&id) = self.span_info.get(source.as_ref()) {
            return id;
        }
        let id = self.span_info2.len() as u32;
        self.span_info2.push(source.as_ref().clone());
        self.span_info.insert(source, id);
        id
    }

    fn compute(&mut self) -> Option<Vec<DocumentFontInfo>> {
        use ttf_parser::name_id::*;
        let font_info = std::mem::take(&mut self.font_info)
            .into_iter()
            .map(|(font, uses)| {
                let extra = self.ctx.resources.font_info(font.clone());
                let info = &font.info();
                DocumentFontInfo {
                    name: info.family.clone(),
                    style: info.variant.style,
                    weight: info.variant.weight,
                    stretch: info.variant.stretch,
                    postscript_name: font.find_name(POST_SCRIPT_NAME),
                    full_name: font.find_name(FULL_NAME),
                    family: font.find_name(FAMILY),
                    fixed_family: Some(info.family.clone()),
                    source: extra.map(|e| self.internal_source(e)),
                    index: Some(font.index()),
                    uses_scale: Some(uses),
                    uses: None,
                }
            })
            .collect();

        Some(font_info)
    }
}
