pub use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    ops::{Deref, Range},
    sync::Arc,
};

pub use comemo::Track;
pub use reflexo::vector::ir::DefId;
pub use serde::Serialize;
pub use typst::syntax::FileId as TypstFileId;
pub use typst::syntax::Source;

pub use super::AnalysisContext;
pub use super::SearchCtx;
pub use crate::adt::snapshot_map::SnapshotMap;
pub(crate) use crate::syntax::{
    IdentDef, IdentRef, LexicalHierarchy, LexicalKind, LexicalModKind, LexicalVarKind, ModSrc,
};
