use std::{
    borrow::Cow,
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
};

use fontdb::Database;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use sha2::{Digest, Sha256};
use tinymist_std::error::prelude::*;
use tinymist_vfs::system::LazyFile;
use typst::{
    diag::{FileError, FileResult},
    foundations::Bytes,
    text::{FontBook, FontInfo},
};

use super::{
    BufferFontLoader, FontProfile, FontProfileItem, FontResolverImpl, FontSlot,
    LazyBufferFontLoader,
};
use crate::debug_loc::{DataSource, FsDataSource, MemoryDataSource};
use crate::{build_info, config::CompileFontOpts};

#[derive(Debug, Default)]
struct FontProfileRebuilder {
    path_items: HashMap<PathBuf, FontProfileItem>,
    pub profile: FontProfile,
    can_profile: bool,
}

impl FontProfileRebuilder {
    /// Index the fonts in the file at the given path.
    #[allow(dead_code)]
    fn search_file(&mut self, path: impl AsRef<Path>) -> Option<&FontProfileItem> {
        let path = path.as_ref().canonicalize().unwrap();
        if let Some(item) = self.path_items.get(&path) {
            return Some(item);
        }

        if let Ok(mut file) = File::open(&path) {
            let hash = if self.can_profile {
                let mut hasher = Sha256::new();
                let _bytes_written = std::io::copy(&mut file, &mut hasher).unwrap();
                let hash = hasher.finalize();

                format!("sha256:{}", hex::encode(hash))
            } else {
                "".to_owned()
            };

            let mut profile_item = FontProfileItem::new("path", hash);
            profile_item.set_path(path.to_str().unwrap().to_owned());
            profile_item.set_mtime(file.metadata().unwrap().modified().unwrap());

            // eprintln!("searched font: {:?}", path);

            // if let Ok(mmap) = unsafe { Mmap::map(&file) } {
            //     for (i, info) in FontInfo::iter(&mmap).enumerate() {
            //         let coverage_hash = get_font_coverage_hash(&info.coverage);
            //         let mut ff = FontInfoItem::new(info);
            //         ff.set_coverage_hash(coverage_hash);
            //         if i != 0 {
            //             ff.set_index(i as u32);
            //         }
            //         profile_item.add_info(ff);
            //     }
            // }

            self.profile.items.push(profile_item);
            return self.profile.items.last();
        }

        None
    }
}

/// Searches for fonts.
#[derive(Debug)]
pub struct SystemFontSearcher {
    pub fonts: Vec<(FontInfo, FontSlot)>,
    pub font_paths: Vec<PathBuf>,
    profile_rebuilder: FontProfileRebuilder,
}

impl SystemFontSearcher {
    /// Create a new, empty system searcher.
    pub fn new() -> Self {
        let mut profile_rebuilder = FontProfileRebuilder::default();
        "v1beta".clone_into(&mut profile_rebuilder.profile.version);
        profile_rebuilder.profile.build_info = build_info::VERSION.to_string();

        Self {
            font_paths: vec![],
            fonts: vec![],
            profile_rebuilder,
        }
    }

    /// Resolve fonts from given options.
    pub fn resolve_opts(&mut self, opts: CompileFontOpts) -> Result<()> {
        if opts
            .font_profile_cache_path
            .to_str()
            .map(|e| !e.is_empty())
            .unwrap_or_default()
        {
            self.set_can_profile(true);
        }

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

        // Source3: add the fonts in memory.
        self.add_memory_fonts(opts.with_embedded_fonts.into_par_iter().map(|font_data| {
            match font_data {
                Cow::Borrowed(data) => Bytes::new(data),
                Cow::Owned(data) => Bytes::new(data),
            }
        }));

        Ok(())
    }

    pub fn set_can_profile(&mut self, can_profile: bool) {
        self.profile_rebuilder.can_profile = can_profile;
    }

    pub fn add_profile_by_path(&mut self, profile_path: &Path) {
        // let begin = std::time::Instant::now();
        // profile_path is in format of json.gz
        let profile_file = File::open(profile_path).unwrap();
        let profile_gunzip = flate2::read::GzDecoder::new(profile_file);
        let profile: FontProfile = serde_json::from_reader(profile_gunzip).unwrap();

        if self.profile_rebuilder.profile.version != profile.version
            || self.profile_rebuilder.profile.build_info != profile.build_info
        {
            return;
        }

        for item in profile.items {
            let path = match item.path() {
                Some(path) => path,
                None => continue,
            };
            let path = PathBuf::from(path);

            if let Ok(m) = std::fs::metadata(&path) {
                let modified = m.modified().ok();
                if !modified.map(|m| item.mtime_is_exact(m)).unwrap_or_default() {
                    continue;
                }
            }

            self.profile_rebuilder.path_items.insert(path, item.clone());
            self.profile_rebuilder.profile.items.push(item);
        }
        // let end = std::time::Instant::now();
        // eprintln!("profile_rebuilder init took {:?}", end - begin);
    }

    pub fn add_fonts_in_fontdb(&mut self, db: &Database) {
        use fontdb::Source;

        let face = db.faces().collect::<Vec<_>>();
        let info = face.into_par_iter().map(|face| {
            let path = match &face.source {
                Source::File(path) | Source::SharedFile(path, _) => path,
                // We never add binary sources to the database, so there
                // shouln't be any.
                Source::Binary(_) => unreachable!(),
            };

            let info = db
                .with_face_data(face.id, FontInfo::new)
                .expect("database must contain this font");

            info.map(|info| {
                let slot = FontSlot::new_boxed(LazyBufferFontLoader::new(
                    LazyFile::new(path.clone()),
                    face.index,
                ))
                .describe(DataSource::Fs(FsDataSource {
                    path: path.to_str().unwrap_or_default().to_owned(),
                }));

                (info, slot)
            })
        });

        for face in info.collect::<Vec<_>>() {
            if let Some((info, slot)) = face {
                self.fonts.push((info, slot));
            }
        }
    }

    /// Add an in-memory font.
    pub fn add_memory_font(&mut self, data: Bytes) {
        for (index, info) in FontInfo::iter(&data).enumerate() {
            self.fonts.push((
                info,
                FontSlot::new_boxed(BufferFontLoader {
                    buffer: Some(data.clone()),
                    index: index as u32,
                })
                .describe(DataSource::Memory(MemoryDataSource {
                    name: "<memory>".to_owned(),
                })),
            ));
        }
    }

    /// Adds in-memory fonts.
    pub fn add_memory_fonts(&mut self, data: impl ParallelIterator<Item = Bytes>) {
        let loaded = data.flat_map(|data| {
            FontInfo::iter(&data)
                .enumerate()
                .map(|(index, info)| {
                    (
                        info,
                        FontSlot::new_boxed(BufferFontLoader {
                            buffer: Some(data.clone()),
                            index: index as u32,
                        })
                        .describe(DataSource::Memory(MemoryDataSource {
                            name: "<memory>".to_owned(),
                        })),
                    )
                })
                .collect::<Vec<_>>()
        });

        for (info, slot) in loaded.collect::<Vec<_>>() {
            self.fonts.push((info, slot));
        }
    }

    pub fn with_fonts_mut(&mut self, func: impl FnOnce(&mut Vec<(FontInfo, FontSlot)>) -> ()) {
        func(&mut self.fonts);
    }

    pub fn search_system(&mut self) {
        let mut db = Database::new();
        db.load_system_fonts();

        self.add_fonts_in_fontdb(&db);
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

        let mut db = Database::new();
        db.load_fonts_dir(path);

        self.add_fonts_in_fontdb(&db);
    }

    /// Index the fonts in the file at the given path.
    pub fn search_file(&mut self, path: impl AsRef<Path>) -> FileResult<()> {
        self.record_path(path.as_ref());

        let mut db = Database::new();
        db.load_font_file(path.as_ref())
            .map_err(|e| FileError::from_io(e, path.as_ref()))?;

        self.add_fonts_in_fontdb(&db);

        Ok(())
    }
}

impl Default for SystemFontSearcher {
    fn default() -> Self {
        Self::new()
    }
}

impl From<SystemFontSearcher> for FontResolverImpl {
    fn from(searcher: SystemFontSearcher) -> Self {
        // let profile_item = match
        // self.profile_rebuilder.search_file(path.as_ref()) {
        //     Some(profile_item) => profile_item,
        //     None => return,
        // };

        // for info in profile_item.info.iter() {
        //     self.book.push(info.info.clone());
        //     self.fonts
        //         .push(FontSlot::new_boxed(LazyBufferFontLoader::new(
        //             LazyFile::new(path.as_ref().to_owned()),
        //             info.index().unwrap_or_default(),
        //         )));
        // }

        let (info, slots): (Vec<FontInfo>, Vec<FontSlot>) = searcher.fonts.into_iter().unzip();

        let book = FontBook::from_infos(info.into_iter());

        FontResolverImpl::new(
            searcher.font_paths,
            book,
            slots,
            searcher.profile_rebuilder.profile,
        )
    }
}

impl From<FontResolverImpl> for SystemFontSearcher {
    fn from(resolver: FontResolverImpl) -> Self {
        let slots = resolver.fonts;
        let book = resolver.book;

        let fonts = slots
            .into_iter()
            .enumerate()
            .map(|(idx, slot)| {
                (
                    book.info(idx).expect("font should be in font book").clone(),
                    slot,
                )
            })
            .collect();

        let mut profile_rebuilder = FontProfileRebuilder::default();
        profile_rebuilder.profile = resolver.profile;

        Self {
            fonts,
            font_paths: resolver.font_paths,
            profile_rebuilder,
        }
    }
}
