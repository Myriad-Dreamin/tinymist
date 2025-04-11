use core::fmt;
use std::sync::Arc;

use tinymist_std::QueryRef;
use typst::text::Font;

use crate::debug_loc::DataSource;
use crate::font::FontLoader;

type FontSlotInner = QueryRef<Option<Font>, (), Box<dyn FontLoader + Send>>;

/// A font slot holds a reference to a font resource. It can be created from
/// - either a callback to load the font lazily, using [`Self::new`] or
///   [`Self::new_boxed`],
/// - or a loaded font, using [`Self::new_loaded`].
#[derive(Clone)]
pub struct FontSlot {
    inner: Arc<FontSlotInner>,
    pub description: Option<Arc<DataSource>>,
}

impl FontSlot {
    /// Creates a font slot to load.
    pub fn new<F: FontLoader + Send + 'static>(f: F) -> Self {
        Self::new_boxed(Box::new(f))
    }

    /// Creates a font slot from a boxed font loader trait object.
    pub fn new_boxed(f: Box<dyn FontLoader + Send>) -> Self {
        Self {
            inner: Arc::new(FontSlotInner::with_context(f)),
            description: None,
        }
    }

    /// Creates a font slot with a loaded font.
    pub fn new_loaded(f: Option<Font>) -> Self {
        Self {
            inner: Arc::new(FontSlotInner::with_value(f)),
            description: None,
        }
    }

    /// Attaches a description to the font slot.
    pub fn with_describe(self, desc: DataSource) -> Self {
        self.with_describe_arc(Arc::new(desc))
    }

    /// Attaches a description to the font slot.
    pub fn with_describe_arc(self, desc: Arc<DataSource>) -> Self {
        Self {
            inner: self.inner,
            description: Some(desc),
        }
    }

    /// Gets or make the font load result.
    pub fn get_or_init(&self) -> Option<Font> {
        let res = self.inner.compute_with_context(|mut c| Ok(c.load()));
        res.unwrap().clone()
    }

    /// Gets the reference to the font load result (possible uninitialized).
    ///
    /// Returns `None` if the cell is empty, or being initialized. This
    /// method never blocks.
    pub fn get_uninitialized(&self) -> Option<Option<Font>> {
        self.inner
            .get_uninitialized()
            .cloned()
            .map(|e| e.ok().flatten())
    }
}

impl fmt::Debug for FontSlot {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("FontSlot")
            .field(&self.get_uninitialized())
            .finish()
    }
}
