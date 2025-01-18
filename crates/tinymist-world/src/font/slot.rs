use core::fmt;
use std::sync::Arc;

use reflexo::debug_loc::DataSource;
use reflexo::QueryRef;
use typst::text::Font;

use crate::font::FontLoader;

type FontSlotInner = QueryRef<Option<Font>, (), Box<dyn FontLoader + Send>>;

/// Lazy Font Reference, load as needed.
pub struct FontSlot {
    inner: FontSlotInner,
    pub description: Option<Arc<DataSource>>,
}

impl FontSlot {
    pub fn with_value(f: Option<Font>) -> Self {
        Self {
            inner: FontSlotInner::with_value(f),
            description: None,
        }
    }

    pub fn new(f: Box<dyn FontLoader + Send>) -> Self {
        Self {
            inner: FontSlotInner::with_context(f),
            description: None,
        }
    }

    pub fn new_boxed<F: FontLoader + Send + 'static>(f: F) -> Self {
        Self::new(Box::new(f))
    }

    pub fn describe(self, desc: DataSource) -> Self {
        Self {
            inner: self.inner,
            description: Some(Arc::new(desc)),
        }
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

    /// Gets or make the font load result.
    pub fn get_or_init(&self) -> Option<Font> {
        let res = self.inner.compute_with_context(|mut c| Ok(c.load()));
        res.unwrap().clone()
    }
}

impl fmt::Debug for FontSlot {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("FontSlot")
            .field(&self.get_uninitialized())
            .finish()
    }
}
