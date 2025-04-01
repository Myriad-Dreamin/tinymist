//! A bundle that is modifiable

use core::fmt;
use std::fmt::Display;
use std::io::{self, Read};
use std::path::Path;
use std::sync::Arc;

use ecow::{eco_format, EcoVec};
use tinymist_std::{ImmutBytes, ImmutPath};
use typst::diag::{PackageError, PackageResult};
use typst::syntax::package::{PackageSpec, VersionlessPackageSpec};

mod fs;
mod gitcl;
mod http;
mod memory;
mod ops;
mod release;
mod tarball;
mod universe;

pub use fs::*;
pub use gitcl::*;
pub use http::*;
pub use memory::*;
pub use ops::*;
pub use release::*;
pub use tarball::*;
pub use universe::*;

/// The pack file is the knownn file type in the package.
pub enum PackFile<'a> {
    /// A single file in the memory.
    Data(io::Cursor<ImmutBytes>),
    /// A file in the package.
    Read(Box<dyn Read + 'a>),
}

impl io::Read for PackFile<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            PackFile::Data(data) => data.read(buf),
            PackFile::Read(reader) => reader.read(buf),
        }
    }
}

/// The pack file is the knownn file type in the package.
pub enum PackEntries<'a> {
    /// A single file in the memory.
    Data(EcoVec<ImmutPath>),
    /// A file in the package.
    Read(Box<dyn Iterator<Item = Path> + 'a>),
}

/// The pack trait is used for read/write files in a package.
pub trait PackFs: fmt::Debug {
    /// Read files from the package.
    fn read_all(
        &mut self,
        f: &mut (dyn FnMut(&str, PackFile) -> PackageResult<()> + Send + Sync),
    ) -> PackageResult<()>;
    /// Read a file from the package.
    fn read(&self, _path: &str) -> io::Result<PackFile> {
        Err(unsupported())
    }
    /// Read entries from the package.
    fn entries(&self) -> io::Result<PackEntries> {
        Err(unsupported())
    }
}

/// The specifier is used to identify a package.
pub enum PackSpecifier {
    /// A package with a version.
    Versioned(PackageSpec),
    /// A package without a version.
    Versionless(VersionlessPackageSpec),
}

/// The pack trait is used to hold a package.
pub trait Pack: PackFs {}

/// The pack trait extension.
pub trait PackExt: Pack {
    /// Filter the package files to read by a function.
    fn filter(&mut self, f: impl Fn(&str) -> bool + Send + Sync) -> impl Pack
    where
        Self: std::marker::Sized,
    {
        FilterPack { src: self, f }
    }
}

/// The pack trait is used to hold a package.
pub trait CloneIntoPack: fmt::Debug {
    /// Clones the pack into a new pack.
    fn clone_into_pack(&mut self, pack: &mut impl PackFs) -> std::io::Result<()>;
}

/// The package is a trait that can be used to create a package.
#[derive(Debug, Clone)]
pub struct Package {
    /// The underlying pack.
    pub pack: Arc<dyn Pack + Send + Sync>,
}

fn unsupported() -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, "unsupported operation")
}

fn malform(e: io::Error) -> PackageError {
    PackageError::MalformedArchive(Some(eco_format!("{e:?}")))
}

fn other_io(e: impl Display) -> io::Error {
    io::Error::new(io::ErrorKind::Other, e.to_string())
}

fn other(e: impl Display) -> PackageError {
    PackageError::Other(Some(eco_format!("{e}")))
}
