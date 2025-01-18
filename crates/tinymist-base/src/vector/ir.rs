//! Intermediate representation of vector items.
//!
//! VectorDoc and Module Relation:
//!
//! ┌──────────────┐ serialize  ┌────────────────────────────────────┐
//! │[`FlatModule`]├───────────►│[`super::stream::BytesModuleStream`]│
//! └──────────────┘            └───────────┬────────────────────────┘
//!      ▲                                  │
//!      │flatten                           │implement
//!      │                                  ▼
//! ┌────┴─────┐        merge       ┌────────────────┐
//! │[`Module`]│◄───────────────────┤[`ModuleStream`]│
//! └────┬─────┘                    └───────┬────────┘
//!      │                                  │
//!      │Store data of                     │merge
//!      ▼                                  ▼
//! ┌───────────────┐  select layout ┌────────────────────┐
//! │[`VecDocument`]│◄───────────────┤[`MultiVecDocument`]│
//! └───────────────┘                └────────────────────┘

use core::fmt;
use std::sync::Arc;

mod color;
mod compose;
pub mod geom;
pub mod layout;
mod meta;
pub mod module;
mod preludes;
mod primitives;
mod text;
mod visualize;

pub use color::*;
pub use compose::*;
pub use geom::*;
pub use layout::*;
pub use meta::*;
pub use module::*;
pub use primitives::*;
pub use text::*;
pub use visualize::*;

#[cfg(feature = "rkyv")]
use rkyv::{Archive, Deserialize as rDeser, Serialize as rSer};

use crate::{hash::Fingerprint, TakeAs};

pub use crate::ImmutStr;

/// A vector item that is specialized for representing
/// `typst::model::Document` or its subtypes.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub enum VecItem {
    None,
    Image(ImageItem),
    Link(LinkItem),
    Path(PathItem),
    Text(TextItem),
    Item(TransformedRef),
    Group(GroupRef),
    Color32(Color32Item),
    Gradient(Arc<GradientItem>),
    Pattern(Arc<PatternItem>),
    ContentHint(char),
    ColorTransform(Arc<ColorTransform>),
    Html(HtmlItem),
    Labelled(LabelledRef),
}

/// Module with page references, corresponding to a `typst::model::Document`.
pub struct VecDocument {
    /// module containing all of the data related to this document.
    pub module: Module,
    /// References to the page frames.
    /// Use [`Module::get_item`] to get the actual item.
    pub pages: Vec<Page>,
}

impl VecDocument {
    pub fn to_multi(self) -> MultiVecDocument {
        let Self { pages, module } = self;

        MultiVecDocument {
            module,
            layouts: vec![LayoutRegion::ByScalar(LayoutRegionRepr {
                kind: "width".into(),
                layouts: vec![(
                    Default::default(),
                    LayoutRegionNode::Pages(Arc::new((Default::default(), pages))),
                )],
            })],
        }
    }
}

#[cfg(feature = "rkyv")]
impl VecDocument {
    pub fn to_bytes(self) -> Vec<u8> {
        self.to_multi().to_bytes()
    }
}

/// Module with multiple documents, corresponding to multiple
/// `typst::model::Document` rearranged by [`LayoutRegion`].
pub struct MultiVecDocument {
    /// module containing all of the data related to this document.
    pub module: Module,
    /// References to the page frames.
    /// Use [`Module::get_item`] to get the actual item.
    pub layouts: Vec<LayoutRegion>,
}

impl Default for MultiVecDocument {
    fn default() -> Self {
        let pages = LayoutRegionNode::new_pages(Default::default());
        Self {
            module: Default::default(),
            layouts: vec![LayoutRegion::new_single(pages)],
        }
    }
}

impl MultiVecDocument {
    pub fn merge_delta(&mut self, v: impl ModuleStream) {
        self.layouts = v.layouts().take();
        self.module.merge_delta(v);
    }
}

#[cfg(feature = "rkyv")]
impl MultiVecDocument {
    pub fn from_slice(v: &[u8]) -> Self {
        type DocStream<'a> = super::stream::BytesModuleStream<'a>;

        let mut res = Self::default();
        res.merge_delta(&DocStream::from_slice(v).checkout_owned());
        res
    }

    pub fn to_bytes(self) -> Vec<u8> {
        let flatten_module = FlatModule::new(vec![
            ModuleMetadata::Item(ItemPack(self.module.items.into_iter().collect())),
            ModuleMetadata::Font(Arc::new(self.module.fonts.into())),
            ModuleMetadata::Glyph(Arc::new(self.module.glyphs.into())),
            ModuleMetadata::Layout(Arc::new(self.layouts)),
        ]);

        flatten_module.to_bytes()
    }
}

pub trait FontIndice<'m> {
    fn get_font(&self, value: &FontRef) -> Option<&'m FontItem>;
}

pub trait ItemIndice<'m> {
    fn get_item(&self, value: &Fingerprint) -> Option<&'m VecItem>;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::hash::Fingerprint;
    use crate::vector::geom::{Axes, Scalar};

    use crate::vector::ir::{Image, ImageItem};

    /// Test image serialization.
    #[test]
    fn test_image_serialization() {
        let img = ImageItem {
            image: Arc::new(Image {
                data: vec![0, 1, 2, 3],
                format: "png".into(),
                size: Axes::new(10, 10),
                alt: None,
                hash: Fingerprint::from_pair(0xdeadbeef, 0),
            }),
            size: Axes::new(Scalar(10.0), Scalar(10.0)),
        };

        // Or you can customize your serialization for better performance
        // and compatibility with #![no_std] environments
        use rkyv::ser::{serializers::AllocSerializer, Serializer};

        let mut serializer = AllocSerializer::<0>::default();
        serializer.serialize_value(&img).unwrap();
        let bytes = serializer.into_serializer().into_inner();

        let ret = bytes.into_vec();
        assert_eq!("00010203706e6700f8ffffff04000000f4ffffff030000000a0000000a000000efbeadde000000000000000000000000000000000000000000000000000000000000204100002041c0ffffff", hex::encode(ret));
    }
}
