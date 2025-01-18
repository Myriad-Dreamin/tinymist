use super::preludes::*;

/// Item representing an `<a/>` element.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct LinkItem {
    /// The target of the link item.
    pub href: ImmutStr,
    /// The box size of the link item.
    pub size: Size,
}

/// Source mapping from vec item to source span.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub enum SourceMappingNode {
    Group(Arc<[u64]>),
    Text(SpanId),
    Image(SpanId),
    Shape(SpanId),
    Page(u64),
}
