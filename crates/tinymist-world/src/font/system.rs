use std::{
    borrow::Cow,
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use fontdb::Database;
use sha2::{Digest, Sha256};
use tinymist_std::debug_loc::{DataSource, MemoryDataSource};
use tinymist_std::error::prelude::*;
use tinymist_vfs::system::LazyFile;
use typst::{
    diag::{FileError, FileResult},
    foundations::Bytes,
    text::{FontBook, FontInfo},
};

use super::{
    BufferFontLoader, FontProfile, FontProfileItem, FontResolverImpl, FontSlot,
    LazyBufferFontLoader, PartialFontBook,
};
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
    db: Database,

    pub book: FontBook,
    pub fonts: Vec<FontSlot>,
    pub font_paths: Vec<PathBuf>,
    profile_rebuilder: FontProfileRebuilder,
}

impl SystemFontSearcher {
    /// Create a new, empty system searcher.
    pub fn new() -> Self {
        let mut profile_rebuilder = FontProfileRebuilder::default();
        "v1beta".clone_into(&mut profile_rebuilder.profile.version);
        profile_rebuilder.profile.build_info = build_info::VERSION.to_string();
        let db = Database::new();

        Self {
            font_paths: vec![],
            db,
            book: FontBook::new(),
            fonts: vec![],
            profile_rebuilder,
        }
    }

    /// Resolve fonts from given options.
    pub fn resolve_opts(&mut self, opts: CompileFontOpts) -> ZResult<()> {
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

        // flush source1 and source2 before adding source3
        self.flush();

        // Source3: add the fonts in memory.
        for font_data in opts.with_embedded_fonts {
            self.add_memory_font(match font_data {
                Cow::Borrowed(data) => Bytes::from_static(data),
                Cow::Owned(data) => Bytes::from(data),
            });
        }

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

    #[cfg(feature = "lazy-fontdb")]
    pub fn flush(&mut self) {
        use rayon::prelude::*;
        self.db
            .lazy_faces()
            .enumerate()
            .par_bridge()
            .flat_map(|(_idx, face)| {
                let path = match face.path() {
                    Some(path) => path,
                    None => return None,
                };

                #[derive(std::hash::Hash)]
                struct CacheStateKey {
                    path: PathBuf,
                    index: u32,
                }

                #[derive(serde::Serialize, serde::Deserialize)]
                struct CacheStateValue {
                    info: Option<FontInfo>,
                    mtime: u64,
                }

                let cache_state_key = CacheStateKey {
                    path: path.to_owned(),
                    index: face.index(),
                };
                let cache_state_key = reflexo::hash::hash128(&cache_state_key);
                let cache_state_path = dirs::cache_dir()
                    .unwrap_or_else(std::env::temp_dir)
                    .join("typst")
                    .join("fonts/v1")
                    .join(format!("{:x}.json", cache_state_key));
                // eprintln!("cache_state: {:?}", cache_state_path);
                let cache_state = std::fs::read_to_string(&cache_state_path).ok();
                let cache_state: Option<CacheStateValue> = cache_state
                    .as_ref()
                    .and_then(|s| serde_json::from_str(s).ok());

                let mtime = path
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .map(|m| m.duration_since(std::time::UNIX_EPOCH).unwrap().as_micros() as u64)
                    .unwrap_or_default();

                let cache_state = cache_state.filter(|cache_state| cache_state.mtime == mtime);

                let info = match cache_state {
                    Some(cache_state) => cache_state.info,
                    None => {
                        let info = face
                            .with_data(|data| FontInfo::new(data, face.index()))
                            .expect("database must contain this font");
                        std::fs::create_dir_all(cache_state_path.parent().unwrap()).unwrap();

                        let info = CacheStateValue { info, mtime };

                        std::fs::write(&cache_state_path, serde_json::to_string(&info).unwrap())
                            .unwrap();
                        info.info
                    }
                };

                // eprintln!("searched font: {idx} {:?}", path);

                Some((
                    info?,
                    FontSlot::new_boxed(LazyBufferFontLoader::new(
                        LazyFile::new(path.to_owned()),
                        face.index(),
                    )),
                ))
            })
            .collect::<Vec<_>>()
            .into_iter()
            .for_each(|(info, font)| {
                self.book.push(info);
                self.fonts.push(font);
            });

        self.db = Database::new();
    }

    #[cfg(not(feature = "lazy-fontdb"))]
    pub fn flush(&mut self) {
        use fontdb::Source;
        use tinymist_std::debug_loc::FsDataSource;

        for face in self.db.faces() {
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

            // eprintln!("searched font: {idx} {:?}", path);

            if let Some(info) = info {
                self.book.push(info);
                self.fonts.push(
                    FontSlot::new_boxed(LazyBufferFontLoader::new(
                        LazyFile::new(path.clone()),
                        face.index,
                    ))
                    .describe(DataSource::Fs(FsDataSource {
                        path: path.to_str().unwrap_or_default().to_owned(),
                    })),
                );
            }
        }

        self.db = Database::new();
    }

    /// Add an in-memory font.
    pub fn add_memory_font(&mut self, data: Bytes) {
        if !self.db.is_empty() {
            panic!("dirty font search state, please flush the searcher before adding memory fonts");
        }

        for (index, info) in FontInfo::iter(&data).enumerate() {
            self.book.push(info.clone());
            self.fonts.push(
                FontSlot::new_boxed(BufferFontLoader {
                    buffer: Some(data.clone()),
                    index: index as u32,
                })
                .describe(DataSource::Memory(MemoryDataSource {
                    name: "<memory>".to_owned(),
                })),
            );
        }
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
        FontResolverImpl::new(
            searcher.font_paths,
            searcher.book,
            Arc::new(Mutex::new(PartialFontBook::default())),
            searcher.fonts,
            searcher.profile_rebuilder.profile,
        )
    }
}
