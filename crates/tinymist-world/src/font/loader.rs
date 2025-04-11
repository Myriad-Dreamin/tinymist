use tinymist_std::ReadAllOnce;
use typst::text::Font;

use crate::Bytes;

/// A FontLoader helps load a font from somewhere.
pub trait FontLoader {
    fn load(&mut self) -> Option<Font>;
}

/// Loads font from a buffer.
pub struct BufferFontLoader {
    pub buffer: Option<Bytes>,
    pub index: u32,
}

impl FontLoader for BufferFontLoader {
    fn load(&mut self) -> Option<Font> {
        Font::new(self.buffer.take().unwrap(), self.index)
    }
}

/// Loads font from a reader.
pub struct LazyBufferFontLoader<R> {
    pub read: Option<R>,
    pub index: u32,
}

impl<R: ReadAllOnce + Sized> LazyBufferFontLoader<R> {
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
