use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};
use tinymist_std::debug_loc::DataSource;
use tinymist_std::typst::TypstDocument;
use typst::text::{Font, FontStretch, FontStyle, FontWeight};
use typst::{
    layout::{Frame, FrameItem},
    syntax::Span,
    text::TextItem,
};

use crate::prelude::*;

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
    /// The source Typst file of the locatable text element
    /// in which the font first occurs.
    pub first_occur_file: Option<String>,
    /// The line number of the locatable text element
    /// in which the font first occurs.
    pub first_occur_line: Option<u32>,
    /// The column number of the locatable text element
    /// in which the font first occurs.
    pub first_occur_column: Option<u32>,
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
        ctx: &mut LocalContext,
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

#[derive(Default)]
struct FontInfoValue {
    uses: u32,
    first_occur_file: Option<String>,
    first_occur_line: Option<u32>,
    first_occur_column: Option<u32>,
}

struct DocumentMetricsWorker<'a> {
    ctx: &'a mut LocalContext,
    span_info: HashMap<Arc<DataSource>, u32>,
    span_info2: Vec<DataSource>,
    font_info: HashMap<Font, FontInfoValue>,
}

impl DocumentMetricsWorker<'_> {
    fn work(&mut self, doc: &TypstDocument) -> Option<()> {
        match doc {
            TypstDocument::Paged(paged_doc) => {
                for page in &paged_doc.pages {
                    self.work_frame(&page.frame)?;
                }

                Some(())
            }
            _ => None,
        }
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
            FrameItem::Shape(..)
            | FrameItem::Image(..)
            | FrameItem::Tag(..)
            | FrameItem::Link(..) => Some(()),
            #[cfg(not(feature = "no-content-hint"))]
            FrameItem::ContentHint(..) => Some(()),
        }
    }

    fn work_text(&mut self, text: &TextItem) -> Option<()> {
        let font_key = text.font.clone();
        let glyph_len = text.glyphs.len();

        let has_source_info = if let Some(font_info) = self.font_info.get(&font_key) {
            font_info.first_occur_file.is_some()
        } else {
            false
        };

        if !has_source_info && glyph_len > 0 {
            let (span, span_offset) = text.glyphs[0].span;

            if let Some((filepath, line, column)) = self.source_code_file_line(span, span_offset) {
                let uses = self.font_info.get(&font_key).map_or(0, |info| info.uses);
                self.font_info.insert(
                    font_key.clone(),
                    FontInfoValue {
                        uses,
                        first_occur_file: Some(filepath),
                        first_occur_line: Some(line),
                        first_occur_column: Some(column),
                    },
                );
            }
        }

        let font_info_value = self.font_info.entry(font_key).or_default();
        font_info_value.uses = font_info_value.uses.checked_add(glyph_len as u32)?;

        Some(())
    }

    fn source_code_file_line(&self, span: Span, span_offset: u16) -> Option<(String, u32, u32)> {
        let world = self.ctx.world();
        let file_id = span.id()?;
        let source = world.source(file_id).ok()?;
        let range = source.range(span)?;
        let byte_index = range.start + usize::from(span_offset);
        let byte_index = byte_index.min(range.end - 1);
        let line = source.byte_to_line(byte_index)?;
        let column = source.byte_to_column(byte_index)?;

        let filepath = self.ctx.path_for_id(file_id).ok()?;
        let filepath_str = filepath.as_path().display().to_string();

        Some((filepath_str, line as u32 + 1, column as u32 + 1))
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
            .map(|(font, font_info_value)| {
                let extra = self.ctx.font_info(font.clone());
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
                    source: extra.map(|source| self.internal_source(source)),
                    index: Some(font.index()),
                    uses_scale: Some(font_info_value.uses),
                    uses: None,
                    first_occur_file: font_info_value.first_occur_file,
                    first_occur_line: font_info_value.first_occur_line,
                    first_occur_column: font_info_value.first_occur_column,
                }
            })
            .collect();

        Some(font_info)
    }
}
