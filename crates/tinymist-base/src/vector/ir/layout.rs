use core::fmt;
use std::{
    borrow::Cow,
    collections::HashMap,
    ops::{Deref, Index},
    sync::Arc,
};

#[cfg(feature = "rkyv")]
use rkyv::{Archive, Deserialize as rDeser, Serialize as rSer};
use serde::{Deserialize, Serialize};

use super::{Module, ModuleView, Page, PageMetadata, Scalar, SourceMappingNode};
use crate::{error::prelude::*, ImmutBytes, ImmutStr, TakeAs};

/// Describing
#[derive(Debug, Clone)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
#[repr(C)]
pub enum LayoutRegionNode {
    // next indirection
    Indirect(usize),
    // flat page layout
    Pages(Arc<(Vec<PageMetadata>, Vec<Page>)>),
    // source mapping node per page
    SourceMapping(Arc<(Vec<PageMetadata>, Vec<SourceMappingNode>)>),
}

impl LayoutRegionNode {
    pub fn new_pages(pages: Vec<Page>) -> Self {
        Self::Pages(Arc::new((Default::default(), pages)))
    }

    pub fn new_source_mapping(source_mapping: Vec<SourceMappingNode>) -> Self {
        Self::SourceMapping(Arc::new((Default::default(), source_mapping)))
    }

    pub fn pages_meta(&self) -> Option<&[Page]> {
        let Self::Pages(v) = self else {
            return None;
        };

        Some(&v.1)
    }

    pub fn pages<'a>(&'a self, module: &'a Module) -> Option<LayoutRegionPagesRAII<'a>> {
        let Self::Pages(v) = self else {
            return None;
        };

        Some(LayoutRegionPagesRAII {
            module: Cow::Borrowed(ModuleView::new(module)),
            meta: &v.0,
            pages: &v.1,
        })
    }

    pub fn source_mapping<'a>(
        &'a self,
        module: &'a Module,
    ) -> Option<LayoutRegionSourceMappingRAII<'a>> {
        let v = if let Self::SourceMapping(v) = self {
            v
        } else {
            return None;
        };

        if v.0.is_empty() {
            return Some(LayoutRegionSourceMappingRAII {
                module: Cow::Borrowed(ModuleView::new(module)),
                source_mapping: &v.1,
            });
        }

        None
    }

    pub fn visit_pages(&self, f: &mut impl FnMut(&(Vec<PageMetadata>, Vec<Page>))) {
        match self {
            Self::Pages(v) => f(v),
            Self::SourceMapping(..) | Self::Indirect(..) => {}
        }
    }

    pub fn mutate_pages(self, f: &mut impl FnMut(&mut (Vec<PageMetadata>, Vec<Page>))) -> Self {
        match self {
            Self::Pages(v) => Self::Pages(Arc::new({
                let mut v = v.take();
                f(&mut v);
                v
            })),
            Self::SourceMapping(..) | Self::Indirect(..) => self,
        }
    }

    pub fn customs(v: &[PageMetadata]) -> impl Iterator<Item = &'_ (ImmutStr, ImmutBytes)> {
        v.iter()
            .flat_map(move |meta| match meta {
                PageMetadata::Custom(customs) => Some(customs.iter()),
                _ => None,
            })
            .flatten()
    }
}

pub struct LayoutRegionPagesRAII<'a> {
    module: Cow<'a, ModuleView>,
    meta: &'a [PageMetadata],
    pages: &'a [Page],
    // todo: chaining module
}

impl<'a> LayoutRegionPagesRAII<'a> {
    pub fn module(&self) -> &Module {
        self.module.as_ref().as_ref()
    }

    pub fn pages(&self) -> &'a [Page] {
        self.pages
    }

    pub fn meta(&self) -> &'a [PageMetadata] {
        self.meta
    }

    pub fn customs(&self) -> impl Iterator<Item = &'_ (ImmutStr, ImmutBytes)> {
        LayoutRegionNode::customs(self.meta)
    }
}

pub struct LayoutRegionSourceMappingRAII<'a> {
    module: Cow<'a, ModuleView>,
    source_mapping: &'a [SourceMappingNode],
    // todo: chaining module
}

impl<'a> LayoutRegionSourceMappingRAII<'a> {
    pub fn module(&self) -> &Module {
        self.module.as_ref().as_ref()
    }

    pub fn source_mapping(&self) -> &'a [SourceMappingNode] {
        self.source_mapping
    }
}

pub trait LayoutSelector {
    fn select_by_scalar(
        &self,
        kind: &str,
        layouts: &[(Scalar, LayoutRegionNode)],
    ) -> ZResult<LayoutRegionNode>;

    fn select_by_str(
        &self,
        kind: &str,
        layouts: &[(ImmutStr, LayoutRegionNode)],
    ) -> ZResult<LayoutRegionNode>;

    fn resolve_indirect(&self, ind: usize) -> ZResult<&LayoutRegion> {
        Err(error_once!(
            "LayoutSelector: unimplemented indirect layout selector",
            ind: ind,
        ))
    }
}

/// Describing
#[derive(Debug, Clone)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct LayoutRegionRepr<T> {
    pub kind: ImmutStr,
    pub layouts: Vec<(T, LayoutRegionNode)>,
}

/// Describing
#[derive(Clone)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub enum LayoutRegion {
    ByScalar(LayoutRegionRepr<Scalar>),
    ByStr(LayoutRegionRepr<ImmutStr>),
}

impl LayoutRegion {
    pub fn new_single(layout: LayoutRegionNode) -> Self {
        Self::ByScalar(LayoutRegionRepr {
            kind: "_".into(),
            layouts: vec![(Default::default(), layout)],
        })
    }

    pub fn new_by_scalar(kind: ImmutStr, layouts: Vec<(Scalar, LayoutRegionNode)>) -> Self {
        Self::ByScalar(LayoutRegionRepr { kind, layouts })
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::ByScalar(v) => v.layouts.is_empty(),
            Self::ByStr(v) => v.layouts.is_empty(),
        }
    }

    pub fn unwrap_single(&self) -> LayoutRegionNode {
        self.by_selector(&LayoutSelectorExpr::Any).unwrap()
    }

    pub fn by_scalar(&self) -> Option<&[(Scalar, LayoutRegionNode)]> {
        if let Self::ByScalar(v) = self {
            Some(&v.layouts)
        } else {
            None
        }
    }

    pub fn by_selector(&self, selector: &impl LayoutSelector) -> ZResult<LayoutRegionNode> {
        let mut t = Ok(self);
        loop {
            let next = match t? {
                Self::ByScalar(v) => selector.select_by_scalar(&v.kind, &v.layouts),
                Self::ByStr(v) => selector.select_by_str(&v.kind, &v.layouts),
            }?;

            if let LayoutRegionNode::Indirect(i) = next {
                t = selector.resolve_indirect(i);
            } else {
                return Ok(next);
            }
        }
    }

    pub fn visit_pages(&self, f: &mut impl FnMut(&(Vec<PageMetadata>, Vec<Page>))) {
        match self {
            Self::ByScalar(v) => {
                for (_, v) in v.layouts.iter() {
                    v.visit_pages(f)
                }
            }
            Self::ByStr(v) => {
                for (_, v) in v.layouts.iter() {
                    v.visit_pages(f)
                }
            }
        }
    }

    pub fn mutate_pages(self, f: &mut impl FnMut(&mut (Vec<PageMetadata>, Vec<Page>))) -> Self {
        match self {
            Self::ByScalar(v) => Self::ByScalar(LayoutRegionRepr {
                kind: v.kind,
                layouts: v
                    .layouts
                    .into_iter()
                    .map(|(k, v)| (k, v.mutate_pages(f)))
                    .collect(),
            }),
            Self::ByStr(v) => Self::ByStr(LayoutRegionRepr {
                kind: v.kind,
                layouts: v
                    .layouts
                    .into_iter()
                    .map(|(k, v)| (k, v.mutate_pages(f)))
                    .collect(),
            }),
        }
    }
}

impl Index<usize> for LayoutRegion {
    type Output = LayoutRegionNode;

    fn index(&self, index: usize) -> &Self::Output {
        match self {
            Self::ByScalar(v) => &v.layouts[index].1,
            Self::ByStr(v) => &v.layouts[index].1,
        }
    }
}

impl fmt::Debug for LayoutRegion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ByScalar(v) => {
                write!(f, "LayoutRegion({:?})", v.kind)?;

                #[allow(clippy::map_identity)]
                let vs = v.layouts.iter().map(|(k, v)| (k, v));
                f.debug_map().entries(vs).finish()
            }
            Self::ByStr(v) => {
                write!(f, "LayoutRegion({:?})", v.kind)?;

                #[allow(clippy::map_identity)]
                let vs = v.layouts.iter().map(|(k, v)| (k, v));
                f.debug_map().entries(vs).finish()
            }
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct LayoutSourceMapping(pub LayoutRegion);

impl Default for LayoutSourceMapping {
    fn default() -> Self {
        Self::new_single(Default::default())
    }
}

impl LayoutSourceMapping {
    pub fn new_single(source_mapping: Vec<SourceMappingNode>) -> Self {
        Self(LayoutRegion::new_single(
            LayoutRegionNode::new_source_mapping(source_mapping),
        ))
    }
}

impl Deref for LayoutSourceMapping {
    type Target = LayoutRegion;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", content = "v")]
pub enum LayoutSelectorExpr {
    /// Selects any layout (smartly).
    Any,
    /// Selects the first layout.
    First,
    /// Selects the last layout.
    Last,
    /// Selects the max first layout with scalar value less than the given
    /// value.
    ScalarLB(f32),
    /// Selects the min last layout with scalar value greater than the given
    /// value.
    ScalarUB(f32),
    /// Selects the last layout with string value equal to the given value.
    StrEQ(String),
}

impl Default for LayoutSelectorExpr {
    fn default() -> Self {
        Self::Any
    }
}

impl LayoutSelector for LayoutSelectorExpr {
    fn select_by_scalar(
        &self,
        kind: &str,
        layouts: &[(Scalar, LayoutRegionNode)],
    ) -> ZResult<LayoutRegionNode> {
        let t = match self {
            LayoutSelectorExpr::Any | LayoutSelectorExpr::First => layouts.first(),
            LayoutSelectorExpr::Last => layouts.last(),
            LayoutSelectorExpr::ScalarLB(v) => {
                layouts.iter().filter(|(scalar, _)| scalar.0 < *v).last()
            }
            LayoutSelectorExpr::ScalarUB(v) => layouts
                .iter()
                .rev()
                .filter(|(scalar, _)| scalar.0 > *v)
                .last(),
            LayoutSelectorExpr::StrEQ(..) => {
                return Err(
                    error_once!("LayoutMappingSelector: cannot select kind by scalar type", kind: kind.to_owned()),
                )
            }
        };
        let t = t.map(|(_, v)| v.clone());

        t.ok_or_else(
            || error_once!("LayoutMappingSelector: no layout found by kind", kind: kind.to_owned(), is_not_found: true),
        )
    }

    fn select_by_str(
        &self,
        kind: &str,
        layouts: &[(ImmutStr, LayoutRegionNode)],
    ) -> ZResult<LayoutRegionNode> {
        let t = match self {
            LayoutSelectorExpr::Any | LayoutSelectorExpr::First => {
                layouts.first().map(|(_, v)| v.clone())
            }
            LayoutSelectorExpr::Last => layouts.last().map(|(_, v)| v.clone()),
            LayoutSelectorExpr::StrEQ(v) => layouts
                .iter()
                .filter(|(s, _)| s.as_ref() == v)
                .last()
                .map(|(_, v)| v.clone()),
            LayoutSelectorExpr::ScalarLB(..) | LayoutSelectorExpr::ScalarUB(..) => {
                return Err(
                    error_once!("LayoutMappingSelector: cannot select kind by str type", kind: kind.to_owned()),
                )
            }
        };

        t.ok_or_else(
            || error_once!("LayoutMappingSelector: no layout found by kind", kind: kind.to_owned(), is_not_found: true),
        )
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct LayoutMappingSelector {
    pub selectors: HashMap<String, LayoutSelectorExpr>,
}

impl LayoutSelector for LayoutMappingSelector {
    fn select_by_scalar(
        &self,
        kind: &str,
        layouts: &[(Scalar, LayoutRegionNode)],
    ) -> ZResult<LayoutRegionNode> {
        self.selectors
            .get(kind)
            .unwrap_or(&LayoutSelectorExpr::Any)
            .select_by_scalar(kind, layouts)
    }

    fn select_by_str(
        &self,
        kind: &str,
        layouts: &[(ImmutStr, LayoutRegionNode)],
    ) -> ZResult<LayoutRegionNode> {
        self.selectors
            .get(kind)
            .unwrap_or(&LayoutSelectorExpr::Any)
            .select_by_str(kind, layouts)
    }
}

#[derive(Default, Debug, Clone)]
pub struct LayoutNestSelector<'l, T> {
    pub layouts: &'l [LayoutRegion],
    pub inner: T,
}

impl<T: LayoutSelector> LayoutSelector for LayoutNestSelector<'_, T> {
    fn select_by_scalar(
        &self,
        kind: &str,
        layouts: &[(Scalar, LayoutRegionNode)],
    ) -> ZResult<LayoutRegionNode> {
        self.inner.select_by_scalar(kind, layouts)
    }

    fn select_by_str(
        &self,
        kind: &str,
        layouts: &[(ImmutStr, LayoutRegionNode)],
    ) -> ZResult<LayoutRegionNode> {
        self.inner.select_by_str(kind, layouts)
    }

    fn resolve_indirect(&self, ind: usize) -> ZResult<&LayoutRegion> {
        self.layouts
            .get(ind)
            .ok_or_else(|| error_once!("LayoutNestSelector: indirect layout not found", ind: ind))
    }
}
