use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

use fontdb::Database;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use tinymist_std::error::prelude::*;
use tinymist_vfs::system::LazyFile;
use typst::{
    diag::{FileError, FileResult},
    foundations::Bytes,
    text::{FontBook, FontInfo},
};

use super::{BufferFontLoader, FontResolverImpl, FontSlot, LazyBufferFontLoader};
use crate::config::CompileFontOpts;
use crate::debug_loc::{DataSource, FsDataSource, MemoryDataSource};

/// Searches for fonts in system.
#[derive(Debug)]
pub struct SystemFontSearcher {
    /// Stores `FontInfo` and `FontSlot` in order.
    pub fonts: Vec<(FontInfo, FontSlot)>,
    /// Records user-specific font path when loading from directory or file for
    /// debug.
    pub font_paths: Vec<PathBuf>,
    /// Stores font data loaded from file
    db: Database,
}

impl SystemFontSearcher {
    /// Creates a system searcher.
    pub fn new() -> Self {
        Self {
            fonts: vec![],
            font_paths: vec![],
            db: Database::new(),
        }
    }

    /// Creates a new system searcher with fonts in a FontResolverImpl.
    pub fn from_resolver(resolver: FontResolverImpl) -> Self {
        let fonts = resolver
            .slots
            .into_iter()
            .enumerate()
            .map(|(idx, slot)| {
                (
                    resolver
                        .book
                        .info(idx)
                        .expect("font should be in font book")
                        .clone(),
                    slot,
                )
            })
            .collect();

        Self {
            fonts,
            font_paths: resolver.font_paths,
            db: Database::new(),
        }
    }

    /// Create a new system searcher with fonts cloned from a FontResolverImpl.
    /// Since FontSlot only holds QueryRef to font data, cloning is cheap.
    pub fn new_with_resolver(resolver: &FontResolverImpl) -> Self {
        let fonts = resolver
            .slots
            .iter()
            .enumerate()
            .map(|(idx, slot)| {
                (
                    resolver
                        .book
                        .info(idx)
                        .expect("font should be in font book")
                        .clone(),
                    slot.clone(),
                )
            })
            .collect();

        Self {
            fonts,
            font_paths: resolver.font_paths.clone(),
            db: Database::new(),
        }
    }

    /// Build a FontResolverImpl.
    pub fn build(self) -> FontResolverImpl {
        let (info, slots): (Vec<FontInfo>, Vec<FontSlot>) = self.fonts.into_iter().unzip();

        let book = FontBook::from_infos(info);

        FontResolverImpl::new(self.font_paths, book, slots)
    }
}

impl SystemFontSearcher {
    /// Resolve fonts from given options.
    pub fn resolve_opts(&mut self, opts: CompileFontOpts) -> Result<()> {
        // Note: the order of adding fonts is important.
        // See: https://github.com/typst/typst/blob/9c7f31870b4e1bf37df79ebbe1df9a56df83d878/src/font/book.rs#L151-L154
        // Source1: add the fonts specified by the user.
        for path in opts.font_paths {
            if path.is_dir() {
                self.search_dir(&path);
            } else {
                let _ = self.search_file(&path);
            }
        }

        // Source2: add the fonts from system paths.
        if !opts.no_system_fonts {
            self.search_system();
        }

        // Flush font db before adding fonts in memory
        self.flush();

        // Source3: add the fonts in memory.
        self.add_memory_fonts(opts.with_embedded_fonts.into_par_iter().map(|font_data| {
            match font_data {
                Cow::Borrowed(data) => Bytes::new(data),
                Cow::Owned(data) => Bytes::new(data),
            }
        }));

        Ok(())
    }

    pub fn flush(&mut self) {
        use fontdb::Source;

        let face = self.db.faces().collect::<Vec<_>>();
        let info = face.into_par_iter().map(|face| {
            let path = match &face.source {
                Source::File(path) | Source::SharedFile(path, _) => path,
                // We never add binary sources to the database, so there
                // shouln't be any.
                Source::Binary(_) => unreachable!(),
            };

            let info = self
                .db
                .with_face_data(face.id, FontInfo::new)
                .expect("database must contain this font");

            info.map(|info| {
                let slot = FontSlot::new(LazyBufferFontLoader::new(
                    LazyFile::new(path.clone()),
                    face.index,
                ))
                .with_describe(DataSource::Fs(FsDataSource {
                    path: path.to_str().unwrap_or_default().to_owned(),
                }));

                (info, slot)
            })
        });

        // todo: we can simplify it?
        self.fonts
            .extend(info.collect::<Vec<_>>().into_iter().flatten());
        self.db = Database::new();
    }

    /// Add an in-memory font.
    pub fn add_memory_font(&mut self, data: Bytes) {
        if !self.db.is_empty() {
            panic!("dirty font search state, please flush the searcher before adding memory fonts");
        }

        for (index, info) in FontInfo::iter(&data).enumerate() {
            self.fonts.push((
                info,
                FontSlot::new(BufferFontLoader {
                    buffer: Some(data.clone()),
                    index: index as u32,
                })
                .with_describe(DataSource::Memory(MemoryDataSource {
                    name: "<memory>".to_owned(),
                })),
            ));
        }
    }

    /// Adds in-memory fonts.
    pub fn add_memory_fonts(&mut self, data: impl ParallelIterator<Item = Bytes>) {
        if !self.db.is_empty() {
            panic!("dirty font search state, please flush the searcher before adding memory fonts");
        }

        let loaded = data.flat_map(|data| {
            FontInfo::iter(&data)
                .enumerate()
                .map(|(index, info)| {
                    (
                        info,
                        FontSlot::new(BufferFontLoader {
                            buffer: Some(data.clone()),
                            index: index as u32,
                        })
                        .with_describe(DataSource::Memory(
                            MemoryDataSource {
                                name: "<memory>".to_owned(),
                            },
                        )),
                    )
                })
                .collect::<Vec<_>>()
        });

        for (info, slot) in loaded.collect::<Vec<_>>() {
            self.fonts.push((info, slot));
        }
    }

    pub fn with_fonts_mut(&mut self, func: impl FnOnce(&mut Vec<(FontInfo, FontSlot)>)) {
        func(&mut self.fonts);
    }

    pub fn search_system(&mut self) {
        self.db.load_system_fonts();
    }

    fn record_path(&mut self, path: &Path) {
        self.font_paths.push(if !path.is_relative() {
            path.to_owned()
        } else {
            let current_dir = std::env::current_dir();
            match current_dir {
                Ok(current_dir) => current_dir.join(path),
                Err(_) => path.to_owned(),
            }
        });
    }

    /// Search for all fonts in a directory recursively.
    pub fn search_dir(&mut self, path: impl AsRef<Path>) {
        self.record_path(path.as_ref());

        self.db.load_fonts_dir(path);
    }

    /// Index the fonts in the file at the given path.
    pub fn search_file(&mut self, path: impl AsRef<Path>) -> FileResult<()> {
        self.record_path(path.as_ref());

        self.db
            .load_font_file(path.as_ref())
            .map_err(|e| FileError::from_io(e, path.as_ref()))
    }
}

impl Default for SystemFontSearcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn edit_fonts() {
        use clap::Parser as _;

        use crate::args::CompileOnceArgs;

        let args = CompileOnceArgs::parse_from(["tinymist", "main.typ"]);
        let mut verse = args
            .resolve_system()
            .expect("failed to resolve system universe");

        let fonts: Vec<_> = verse.font_resolver.fonts().collect();

        let new_resolver = FontResolverImpl::new_with_fonts(
            vec![],
            fonts
                .into_iter()
                .map(|(info, slot)| (info.clone(), slot.clone())),
        );
        verse.increment_revision(|verse| verse.set_fonts(Arc::new(new_resolver)));
    }
}
