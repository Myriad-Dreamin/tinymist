use super::ir::{
    FlatGlyphItem, FlatModule, GlyphRef, LayoutRegionNode, LayoutSourceMapping, Module,
    ModuleMetadata, MultiVecDocument, Page, SourceMappingNode,
};
use crate::{error::prelude::*, TakeAs};

/// maintains the data of the incremental rendering at client side
#[derive(Default)]
pub struct IncrDocClient {
    /// Full information of the current document from server.
    pub doc: MultiVecDocument,
    /// Hold glyphs.
    pub glyphs: Vec<(GlyphRef, FlatGlyphItem)>,

    /// checkout of the current document.
    pub layout: Option<LayoutRegionNode>,
    /// Optional source mapping data.
    pub source_mapping_data: Vec<SourceMappingNode>,
    /// Optional page source mapping references.
    pub page_source_mapping: LayoutSourceMapping,
}

impl IncrDocClient {
    /// Merge the delta from server.
    pub fn merge_delta(&mut self, delta: FlatModule) {
        self.doc.merge_delta(&delta);
        for metadata in delta.metadata {
            match metadata {
                ModuleMetadata::Glyph(data) => {
                    self.glyphs.extend(data.take().items.into_iter());
                }
                ModuleMetadata::SourceMappingData(data) => {
                    self.source_mapping_data = data;
                }
                ModuleMetadata::PageSourceMapping(data) => {
                    self.page_source_mapping = data.take();
                }
                _ => {}
            }
        }
    }

    /// Set the current layout of the document.
    /// This is so bare-bone that stupidly takes a selected layout.
    ///
    /// Please wrap this for your own use case.
    pub fn set_layout(&mut self, layout: LayoutRegionNode) {
        self.layout = Some(layout);
    }

    /// Kern of the client without leaking abstraction.
    pub fn kern(&self) -> IncrDocClientKern<'_> {
        IncrDocClientKern::new(self)
    }

    pub fn module(&self) -> &Module {
        &self.doc.module
    }

    pub fn module_mut(&mut self) -> &mut Module {
        &mut self.doc.module
    }
}

fn access_slice<'a, T>(v: &'a [T], idx: usize, kind: &'static str, pos: usize) -> ZResult<&'a T> {
    v.get(idx).ok_or_else(
        || error_once!("out of bound access", pos: pos, kind: kind, idx: idx, actual: v.len()),
    )
}

pub struct IncrDocClientKern<'a>(&'a IncrDocClient);

impl<'a> IncrDocClientKern<'a> {
    pub fn new(client: &'a IncrDocClient) -> Self {
        Self(client)
    }

    /// Get current pages meta of the selected document.
    pub fn pages_meta(&self) -> Option<&[Page]> {
        let layout = self.0.layout.as_ref();
        layout.and_then(LayoutRegionNode::pages_meta)
    }

    /// Get estimated width of the document (in flavor of PDF Viewer).
    pub fn doc_width(&self) -> Option<f32> {
        let view = self.pages_meta()?.iter();
        Some(view.map(|p| p.size.x).max().unwrap_or_default().0)
    }

    /// Get estimated height of the document (in flavor of PDF Viewer).
    pub fn doc_height(&self) -> Option<f32> {
        let view = self.pages_meta()?.iter();
        Some(view.map(|p| p.size.y.0).sum())
    }

    /// Get the source location of the given path.
    pub fn source_span(&self, path: &[u32]) -> ZResult<Option<String>> {
        const SOURCE_MAPPING_TYPE_TEXT: u32 = 0;
        const SOURCE_MAPPING_TYPE_GROUP: u32 = 1;
        const SOURCE_MAPPING_TYPE_IMAGE: u32 = 2;
        const SOURCE_MAPPING_TYPE_SHAPE: u32 = 3;
        const SOURCE_MAPPING_TYPE_PAGE: u32 = 4;

        if self.0.page_source_mapping.is_empty() {
            return Ok(None);
        }

        let mut index_item: Option<&SourceMappingNode> = None;

        let source_mapping = self.0.source_mapping_data.as_slice();
        let page_sources = self.0.page_source_mapping[0]
            .source_mapping(&self.0.doc.module)
            .unwrap();
        let page_sources = page_sources.source_mapping();

        for (chunk_idx, v) in path.chunks_exact(2).enumerate() {
            let (ty, idx) = (v[0], v[1] as usize);

            let this_item: &SourceMappingNode = match index_item {
                Some(SourceMappingNode::Group(q)) => {
                    let idx = *access_slice(q, idx, "group_index", chunk_idx)? as usize;
                    access_slice(source_mapping, idx, "source_mapping", chunk_idx)?
                }
                Some(_) => {
                    return Err(error_once!("cannot index", pos:
        chunk_idx, indexing: format!("{:?}", index_item)))
                }
                None => access_slice(page_sources, idx, "page_sources", chunk_idx)?,
            };

            match (ty, this_item) {
                (SOURCE_MAPPING_TYPE_PAGE, SourceMappingNode::Page(page_index)) => {
                    index_item = Some(access_slice(
                        source_mapping,
                        *page_index as usize,
                        "source_mapping",
                        chunk_idx,
                    )?);
                }
                (SOURCE_MAPPING_TYPE_GROUP, SourceMappingNode::Group(_)) => {
                    index_item = Some(this_item);
                }
                (SOURCE_MAPPING_TYPE_TEXT, SourceMappingNode::Text(n))
                | (SOURCE_MAPPING_TYPE_IMAGE, SourceMappingNode::Image(n))
                | (SOURCE_MAPPING_TYPE_SHAPE, SourceMappingNode::Shape(n)) => {
                    return Ok(Some(format!("{n:x}")));
                }
                _ => {
                    return Err(error_once!("invalid/mismatch node
                    type",                         pos: chunk_idx, ty: ty,
                        actual: format!("{:?}", this_item),
                        parent: format!("{:?}", index_item),
                        child_idx_in_parent: idx,
                    ))
                }
            }
        }

        Ok(None)
    }
}
