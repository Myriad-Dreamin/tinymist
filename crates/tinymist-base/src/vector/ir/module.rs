use std::{
    borrow::Borrow,
    collections::{BTreeMap, HashMap},
    hash::Hash,
    ops::Deref,
    sync::{atomic::AtomicU64, Arc},
};

use comemo::Prehashed;

use crate::{hash::Fingerprint, ImmutStr, TakeAs};

use super::{preludes::*, *};

pub type ItemMap = BTreeMap<Fingerprint, VecItem>;

pub type RefItemMap = HashMap<Fingerprint, (u64, VecItem)>;
#[cfg(feature = "item-dashmap")]
pub type RefItemMapSync = crate::adt::CHashMap<Fingerprint, (AtomicU64, VecItem)>;
pub type RefItemMapT<V> = crate::adt::FingerprintMap<V>;
pub type RefItemMapSync = RefItemMapT<(AtomicU64, VecItem)>;

pub trait ToItemMap {
    fn to_item_map(self) -> ItemMap;
}

impl ToItemMap for RefItemMap {
    fn to_item_map(self) -> ItemMap {
        self.into_iter().map(|(k, (_, v))| (k, v)).collect::<_>()
    }
}

impl ToItemMap for RefItemMapSync {
    fn to_item_map(self) -> ItemMap {
        self.into_items().map(|(k, (_, v))| (k, v)).collect::<_>()
    }
}

/// Trait of a streaming representation of a module.
pub trait ModuleStream {
    fn items(&self) -> ItemPack;
    fn layouts(&self) -> Arc<Vec<LayoutRegion>>;
    fn fonts(&self) -> Arc<IncrFontPack>;
    fn glyphs(&self) -> Arc<IncrGlyphPack>;
    fn gc_items(&self) -> Option<Vec<Fingerprint>> {
        // never gc items
        None
    }
}

/// A finished module that stores all the vector items.
/// The vector items shares the underlying data.
/// The vector items are flattened and ready to be serialized.
#[derive(Debug, Default, Clone, Hash)]
pub struct Module {
    pub fonts: Vec<FontItem>,
    pub glyphs: Vec<(GlyphRef, FlatGlyphItem)>,
    pub items: ItemMap,
}

impl Module {
    pub fn freeze(self) -> FrozenModule {
        FrozenModule(Arc::new(Prehashed::new(self)))
    }

    pub fn prepare_glyphs(&mut self) {
        let glyphs = std::mem::take(&mut self.glyphs);
        if glyphs.is_empty() {
            return;
        }
        let mut hash2idx = HashMap::new();
        for (id, item) in glyphs.into_iter() {
            let idx = hash2idx.entry(id.font_hash).or_insert_with(|| {
                self.fonts
                    .iter()
                    .position(|f| f.hash == id.font_hash)
                    .unwrap()
            });
            let font = &mut self.fonts[*idx];
            if font.glyphs.len() <= id.glyph_idx as usize {
                font.glyphs
                    .resize(id.glyph_idx as usize + 1, Arc::new(FlatGlyphItem::None));
            }
            font.glyphs[id.glyph_idx as usize] = Arc::new(item);
            if font.glyph_cov.is_empty() {
                font.glyph_cov = bitvec::vec::BitVec::repeat(false, 65536);
            }
            font.glyph_cov.set(id.glyph_idx as usize, true);
        }
    }

    /// Get a font item by its stable ref.
    pub fn get_font(&self, id: &FontRef) -> Option<&FontItem> {
        self.fonts.get(id.idx as usize)
    }

    /// Get a svg item by its stable ref.
    pub fn get_item(&self, id: &Fingerprint) -> Option<&VecItem> {
        self.items.get(id)
    }

    pub fn merge_delta(&mut self, v: impl ModuleStream) {
        let item_pack: ItemPack = v.items();
        if let Some(gc_items) = v.gc_items() {
            for id in gc_items {
                self.items.remove(&id);
            }
        }
        self.items.extend(item_pack.0);

        let fonts = v.fonts();
        self.fonts.extend(fonts.take().items);

        let glyphs = v.glyphs();
        if !glyphs.items.is_empty() {
            self.glyphs = glyphs.take().items;
            self.prepare_glyphs();
        }
    }

    pub fn glyphs_all(&self) -> impl Iterator<Item = (GlyphRef, &FlatGlyphItem)> {
        self.fonts.iter().flat_map(|font| {
            font.glyph_cov.iter_ones().map(move |glyph_idx| {
                (
                    GlyphRef {
                        font_hash: font.hash,
                        glyph_idx: glyph_idx as u32,
                    },
                    font.glyphs[glyph_idx].deref(),
                )
            })
        })
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct FrozenModule(pub Arc<Prehashed<Module>>);

pub struct ModuleView {
    module: Module,
}

impl ModuleView {
    /// See [`std::path::Path`]
    pub fn new<M: AsRef<Module> + ?Sized>(m: &M) -> &Self {
        // SAFETY: The std::path::Path does similar conversion and is safe.
        unsafe { &*(m.as_ref() as *const Module as *const ModuleView) }
    }
}

impl ToOwned for ModuleView {
    type Owned = Module;

    fn to_owned(&self) -> Self::Owned {
        self.module.clone()
    }
}

impl AsRef<Module> for ModuleView {
    #[inline]
    fn as_ref(&self) -> &Module {
        &self.module
    }
}

impl AsRef<Module> for Module {
    #[inline]
    fn as_ref(&self) -> &Module {
        self
    }
}

impl AsRef<Module> for FrozenModule {
    #[inline]
    fn as_ref(&self) -> &Module {
        self.0.deref().deref()
    }
}

impl AsRef<FrozenModule> for FrozenModule {
    #[inline]
    fn as_ref(&self) -> &FrozenModule {
        self
    }
}

impl Borrow<ModuleView> for FrozenModule {
    fn borrow(&self) -> &ModuleView {
        ModuleView::new(self)
    }
}

impl Borrow<ModuleView> for Module {
    fn borrow(&self) -> &ModuleView {
        ModuleView::new(self)
    }
}

impl Borrow<Module> for FrozenModule {
    fn borrow(&self) -> &Module {
        self.0.deref().deref()
    }
}

/// metadata that can be attached to a module.
#[derive(Clone)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
#[repr(C, align(32))]
pub enum PageMetadata {
    GarbageCollection(Vec<Fingerprint>),
    Item(ItemPack),
    Glyph(Arc<IncrGlyphPack>),
    Custom(Vec<(ImmutStr, ImmutBytes)>),
}

impl fmt::Debug for PageMetadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PageMetadata::GarbageCollection(v) => f
                .debug_struct("GarbageCollection")
                .field("len", &v.len())
                .finish(),
            PageMetadata::Item(v) => f.debug_struct("Item").field("len", &v.0.len()).finish(),
            PageMetadata::Glyph(v) => f
                .debug_struct("Glyph")
                .field("len", &v.items.len())
                .field("base", &v.incremental_base)
                .finish(),
            PageMetadata::Custom(v) => {
                write!(f, "Custom")?;
                f.debug_map()
                    .entries(
                        v.iter()
                            .map(|(k, v)| (k.as_ref(), format!("Bytes({})", v.len()))),
                    )
                    .finish()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct BuildInfo {
    pub version: ImmutStr,
    pub compiler: ImmutStr,
}

/// metadata that can be attached to a module.
#[derive(Debug, Clone)]
#[repr(C, align(32))]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub enum ModuleMetadata {
    BuildVersion(Arc<BuildInfo>),
    SourceMappingData(Vec<SourceMappingNode>),
    PageSourceMapping(Arc<LayoutSourceMapping>),
    GarbageCollection(Vec<Fingerprint>),
    Item(ItemPack),
    Font(Arc<IncrFontPack>),
    Glyph(Arc<IncrGlyphPack>),
    Layout(Arc<Vec<LayoutRegion>>),
}

const _: () = assert!(core::mem::size_of::<ModuleMetadata>() == 32);

#[repr(usize)]
#[allow(dead_code)]
enum MetaIndices {
    Version,
    SourceMapping,
    PageSourceMapping,
    GarbageCollection,
    Item,
    Font,
    Glyph,
    Layout,
    Max,
}

const META_INDICES_MAX: usize = MetaIndices::Max as usize;

/// Flatten module so that it can be serialized.
#[derive(Debug)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct FlatModule {
    pub magic: [u8; 8],
    pub metadata: Vec<ModuleMetadata>,

    #[cfg_attr(feature = "rkyv", with(rkyv::with::Skip))]
    #[allow(unused)]
    meta_indices: [std::sync::OnceLock<usize>; META_INDICES_MAX],
}

impl Default for FlatModule {
    fn default() -> Self {
        Self {
            magic: *b"tsvr\x00\x00\x00\x00",
            metadata: vec![],
            meta_indices: Default::default(),
        }
    }
}

#[cfg(feature = "rkyv")]
impl FlatModule {
    pub fn new(metadata: Vec<ModuleMetadata>) -> Self {
        Self {
            metadata,
            ..Default::default()
        }
    }

    pub fn to_bytes(self: &FlatModule) -> Vec<u8> {
        // Or you can customize your serialization for better performance
        // and compatibility with #![no_std] environments
        use rkyv::ser::{serializers::AllocSerializer, Serializer};

        let mut serializer = AllocSerializer::<0>::default();
        serializer.serialize_value(self).unwrap();
        let bytes = serializer.into_serializer().into_inner();

        bytes.into_vec()
    }
}

// todo: for archived module.
// todo: zero copy
#[cfg(feature = "rkyv")]
impl ModuleStream for &FlatModule {
    fn items(&self) -> ItemPack {
        // cache the index
        let sz = &self.meta_indices[MetaIndices::Item as usize];
        let sz = sz.get_or_init(|| {
            let mut sz = usize::MAX; // will panic if not found
            for (idx, m) in self.metadata.iter().enumerate() {
                if let ModuleMetadata::Item(_) = m {
                    sz = idx;
                    break;
                }
            }
            sz
        });

        // get the item pack
        let m = &self.metadata[*sz];
        if let ModuleMetadata::Item(v) = m {
            v.clone()
        } else {
            unreachable!()
        }
    }

    fn layouts(&self) -> Arc<Vec<LayoutRegion>> {
        // cache the index
        let sz = &self.meta_indices[MetaIndices::Layout as usize];
        let sz = sz.get_or_init(|| {
            let mut sz = usize::MAX; // will panic if not found
            for (idx, m) in self.metadata.iter().enumerate() {
                if let ModuleMetadata::Layout(_) = m {
                    sz = idx;
                    break;
                }
            }
            sz
        });

        // get the item pack
        let m = &self.metadata[*sz];
        if let ModuleMetadata::Layout(v) = m {
            v.clone()
        } else {
            unreachable!()
        }
    }

    fn fonts(&self) -> Arc<IncrFontPack> {
        // cache the index
        let sz = &self.meta_indices[MetaIndices::Font as usize];
        let sz = sz.get_or_init(|| {
            let mut sz = usize::MAX; // will panic if not found
            for (idx, m) in self.metadata.iter().enumerate() {
                if let ModuleMetadata::Font(_) = m {
                    sz = idx;
                    break;
                }
            }
            sz
        });

        // get the item pack
        let m = &self.metadata[*sz];
        if let ModuleMetadata::Font(v) = m {
            v.clone()
        } else {
            unreachable!()
        }
    }

    fn glyphs(&self) -> Arc<IncrGlyphPack> {
        // cache the index
        let sz = &self.meta_indices[MetaIndices::Glyph as usize];
        let sz = sz.get_or_init(|| {
            let mut sz = usize::MAX; // will panic if not found
            for (idx, m) in self.metadata.iter().enumerate() {
                if let ModuleMetadata::Glyph(_) = m {
                    sz = idx;
                    break;
                }
            }
            sz
        });

        // get the item pack
        let m = &self.metadata[*sz];
        if let ModuleMetadata::Glyph(v) = m {
            v.clone()
        } else {
            unreachable!()
        }
    }

    fn gc_items(&self) -> Option<Vec<Fingerprint>> {
        for m in &self.metadata {
            if let ModuleMetadata::GarbageCollection(v) = m {
                return Some(v.clone());
            }
        }
        None
    }
}
