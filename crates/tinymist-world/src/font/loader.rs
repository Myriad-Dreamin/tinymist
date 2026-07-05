use tinymist_std::ReadAllOnce;
use typst::text::Font;

use crate::Bytes;

/// A FontLoader helps load a font from somewhere.
pub trait FontLoader {
    /// Loads a font.
    fn load(&mut self) -> Option<Font>;
}

/// Loads a font from a buffer.
pub struct BufferFontLoader {
    /// The buffer to load the font from.
    pub buffer: Option<Bytes>,
    /// The index in a font file.
    pub index: u32,
}

impl FontLoader for BufferFontLoader {
    fn load(&mut self) -> Option<Font> {
        Font::new(self.buffer.take().unwrap(), self.index)
    }
}

/// Loads a font from a reader.
pub struct LazyBufferFontLoader<R> {
    /// The reader to load the font from.
    pub read: Option<R>,
    /// The index in a font file.
    pub index: u32,
}

impl<R: ReadAllOnce + Sized> LazyBufferFontLoader<R> {
    /// Creates a new lazy buffer font loader.
    pub fn new(read: R, index: u32) -> Self {
        Self {
            read: Some(read),
            index,
        }
    }
}

impl<R: ReadAllOnce + Sized> FontLoader for LazyBufferFontLoader<R> {
    fn load(&mut self) -> Option<Font> {
        let mut buf = vec![];
        self.read.take().unwrap().read_all(&mut buf).ok()?;
        Font::new(Bytes::new(buf), self.index)
    }
}
