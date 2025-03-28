//! World implementation of typst for tinymist.

#![allow(missing_docs)]

pub mod args;
pub mod config;
pub mod debug_loc;
pub mod entry;
pub mod font;
pub mod package;
pub mod parser;
pub mod source;
pub mod world;

pub use compute::*;
pub use entry::*;
pub use snapshot::*;
pub use world::*;

pub use tinymist_vfs as vfs;

mod compute;
mod snapshot;

/// Run the compiler in the system environment.
#[cfg(feature = "system")]
pub mod system;
#[cfg(feature = "system")]
pub use system::{print_diagnostics, SystemCompilerFeat, TypstSystemUniverse, TypstSystemWorld};

/// Run the compiler in the browser environment.
#[cfg(feature = "browser")]
pub(crate) mod browser;
#[cfg(feature = "browser")]
pub use browser::{BrowserCompilerFeat, TypstBrowserUniverse, TypstBrowserWorld};

use std::{path::Path, sync::Arc};

use ecow::EcoVec;
use tinymist_vfs::PathAccessModel as VfsAccessModel;
use typst::diag::{At, FileResult, SourceResult};
use typst::foundations::Bytes;
use typst::syntax::{FileId, Span};

use font::FontResolver;
use package::PackageRegistry;

/// Latest version of the shadow api, which is in beta.
pub trait ShadowApi {
    /// Get the shadow files.
    fn shadow_paths(&self) -> Vec<Arc<Path>>;
    /// Get the shadow files by file id.
    fn shadow_ids(&self) -> Vec<FileId>;

    /// Reset the shadow files.
    fn reset_shadow(&mut self) {
        for path in self.shadow_paths() {
            self.unmap_shadow(&path).unwrap();
        }
    }

    /// Add a shadow file to the driver.
    fn map_shadow(&mut self, path: &Path, content: Bytes) -> FileResult<()>;

    /// Add a shadow file to the driver.
    fn unmap_shadow(&mut self, path: &Path) -> FileResult<()>;

    /// Add a shadow file to the driver by file id.
    /// Note: If a *path* is both shadowed by id and by path, the shadow by id
    /// will be used.
    fn map_shadow_by_id(&mut self, file_id: FileId, content: Bytes) -> FileResult<()>;

    /// Add a shadow file to the driver by file id.
    /// Note: If a *path* is both shadowed by id and by path, the shadow by id
    /// will be used.
    fn unmap_shadow_by_id(&mut self, file_id: FileId) -> FileResult<()>;
}

pub trait ShadowApiExt {
    /// Wrap the driver with a given shadow file and run the inner function.
    fn with_shadow_file<T>(
        &mut self,
        file_path: &Path,
        content: Bytes,
        f: impl FnOnce(&mut Self) -> SourceResult<T>,
    ) -> SourceResult<T>;

    /// Wrap the driver with a given shadow file and run the inner function by
    /// file id.
    /// Note: to enable this function, `ShadowApi` must implement
    /// `_shadow_map_id`.
    fn with_shadow_file_by_id<T>(
        &mut self,
        file_id: FileId,
        content: Bytes,
        f: impl FnOnce(&mut Self) -> SourceResult<T>,
    ) -> SourceResult<T>;
}

impl<C: ShadowApi> ShadowApiExt for C {
    /// Wrap the driver with a given shadow file and run the inner function.
    fn with_shadow_file<T>(
        &mut self,
        file_path: &Path,
        content: Bytes,
        f: impl FnOnce(&mut Self) -> SourceResult<T>,
    ) -> SourceResult<T> {
        self.map_shadow(file_path, content).at(Span::detached())?;
        let res: Result<T, EcoVec<typst::diag::SourceDiagnostic>> = f(self);
        self.unmap_shadow(file_path).at(Span::detached())?;
        res
    }

    /// Wrap the driver with a given shadow file and run the inner function by
    /// file id.
    /// Note: to enable this function, `ShadowApi` must implement
    /// `_shadow_map_id`.
    fn with_shadow_file_by_id<T>(
        &mut self,
        file_id: FileId,
        content: Bytes,
        f: impl FnOnce(&mut Self) -> SourceResult<T>,
    ) -> SourceResult<T> {
        self.map_shadow_by_id(file_id, content)
            .at(Span::detached())?;
        let res: Result<T, EcoVec<typst::diag::SourceDiagnostic>> = f(self);
        self.unmap_shadow_by_id(file_id).at(Span::detached())?;
        res
    }
}

/// Latest version of the world dependencies api, which is in beta.
pub trait WorldDeps {
    fn iter_dependencies(&self, f: &mut dyn FnMut(FileId));
}

/// type trait interface of [`CompilerWorld`].
pub trait CompilerFeat: Send + Sync + 'static {
    /// Specify the font resolver for typst compiler.
    type FontResolver: FontResolver + Send + Sync + Sized;
    /// Specify the access model for VFS.
    type AccessModel: VfsAccessModel + Clone + Send + Sync + Sized;
    /// Specify the package registry.
    type Registry: PackageRegistry + Send + Sync + Sized;
}

/// Which format to use for diagnostics.
#[derive(Debug, Copy, Clone, Default, Eq, PartialEq, Ord, PartialOrd)]
pub enum DiagnosticFormat {
    #[default]
    Human,
    Short,
}

pub mod build_info {
    /// The version of the reflexo-world crate.
    pub static VERSION: &str = env!("CARGO_PKG_VERSION");
}
