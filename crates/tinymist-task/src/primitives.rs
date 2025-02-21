use core::fmt;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::{path::Path, str::FromStr};

use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use tinymist_std::error::prelude::*;
use tinymist_std::path::{unix_slash, PathClean};
use tinymist_std::ImmutPath;
use tinymist_world::vfs::WorkspaceResolver;
use tinymist_world::{CompilerFeat, CompilerWorld, EntryReader, EntryState};
use typst::diag::EcoString;
use typst::syntax::FileId;

/// A scalar that is not NaN.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Scalar(f32);

impl TryFrom<f32> for Scalar {
    type Error = &'static str;

    fn try_from(value: f32) -> Result<Self, Self::Error> {
        if value.is_nan() {
            Err("NaN is not a valid scalar value")
        } else {
            Ok(Scalar(value))
        }
    }
}

impl Scalar {
    /// Converts the scalar to an f32.
    pub fn to_f32(self) -> f32 {
        self.0
    }
}

impl PartialEq for Scalar {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Scalar {}

impl Hash for Scalar {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

impl PartialOrd for Scalar {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Scalar {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.partial_cmp(&other.0).unwrap()
    }
}

/// A project ID.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Id(String);

impl Id {
    /// Creates a new project Id.
    pub fn new(s: String) -> Self {
        Id(s)
    }

    /// Creates a new project Id from a world.
    pub fn from_world<F: CompilerFeat>(world: &CompilerWorld<F>) -> Option<Self> {
        let entry = world.entry_state();
        let id = unix_slash(entry.main()?.vpath().as_rootless_path());

        let path = &ResourcePath::from_user_sys(Path::new(&id));
        Some(path.into())
    }
}

impl From<&ResourcePath> for Id {
    fn from(value: &ResourcePath) -> Self {
        Id::new(value.to_string())
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

macro_rules! display_possible_values {
    ($ty:ty) => {
        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.to_possible_value()
                    .expect("no values are skipped")
                    .get_name()
                    .fmt(f)
            }
        }
    };
}

/// When to export an output file.
///
/// By default, a `tinymist compile` only provides input information and
/// doesn't change the `when` field. However, you can still specify a `when`
/// argument to override the default behavior for specific tasks.
///
/// ## Examples
///
/// ```bash
/// tinymist compile --when onSave main.typ
/// alias typst="tinymist compile --when=onSave"
/// typst compile main.typ
/// ```
#[derive(Debug, Copy, Clone, Eq, PartialEq, Default, Hash, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[clap(rename_all = "camelCase")]
pub enum TaskWhen {
    /// Never watch to run task.
    #[default]
    Never,
    /// Run task on saving the document, i.e. on `textDocument/didSave` events.
    OnSave,
    /// Run task on typing, i.e. on `textDocument/didChange` events.
    OnType,
    /// *DEPRECATED* Run task when a document has a title and on saved, which is
    /// useful to filter out template files.
    ///
    /// Note: this is deprecating.
    OnDocumentHasTitle,
}

impl TaskWhen {
    /// Returns `true` if the task should never be run automatically.
    pub fn is_never(&self) -> bool {
        matches!(self, TaskWhen::Never)
    }
}

display_possible_values!(TaskWhen);

/// Which format to use for the generated output file.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, ValueEnum)]
pub enum OutputFormat {
    /// Export to PDF.
    Pdf,
    /// Export to PNG.
    Png,
    /// Export to SVG.
    Svg,
    /// Export to HTML.
    Html,
}

display_possible_values!(OutputFormat);

/// The path pattern that could be substituted.
///
/// # Examples
/// - `$root` is the root of the project.
/// - `$root/$dir` is the parent directory of the input (main) file.
/// - `$root/main` will help store pdf file to `$root/main.pdf` constantly.
/// - (default) `$root/$dir/$name` will help store pdf file along with the input
///   file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PathPattern(pub String);

impl fmt::Display for PathPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl PathPattern {
    /// Creates a new path pattern.
    pub fn new(pattern: &str) -> Self {
        Self(pattern.to_owned())
    }

    /// Substitutes the path pattern with `$root`, and `$dir/$name`.
    pub fn substitute(&self, entry: &EntryState) -> Option<ImmutPath> {
        self.substitute_impl(entry.root(), entry.main())
    }

    #[comemo::memoize]
    fn substitute_impl(&self, root: Option<ImmutPath>, main: Option<FileId>) -> Option<ImmutPath> {
        log::info!("Check path {main:?} and root {root:?} with output directory {self:?}");

        let (root, main) = root.zip(main)?;

        // Files in packages are not exported
        if WorkspaceResolver::is_package_file(main) {
            return None;
        }
        // Files without a path are not exported
        let path = main.vpath().resolve(&root)?;

        // todo: handle untitled path
        if let Ok(path) = path.strip_prefix("/untitled") {
            let tmp = std::env::temp_dir();
            let path = tmp.join("typst").join(path);
            return Some(path.as_path().into());
        }

        if self.0.is_empty() {
            return Some(path.to_path_buf().clean().into());
        }

        let path = path.strip_prefix(&root).ok()?;
        let dir = path.parent();
        let file_name = path.file_name().unwrap_or_default();

        let w = root.to_string_lossy();
        let f = file_name.to_string_lossy();

        // replace all $root
        let mut path = self.0.replace("$root", &w);
        if let Some(dir) = dir {
            let d = dir.to_string_lossy();
            path = path.replace("$dir", &d);
        }
        path = path.replace("$name", &f);

        Some(PathBuf::from(path).clean().into())
    }
}

/// A PDF standard that Typst can enforce conformance with.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, ValueEnum, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub enum PdfStandard {
    /// PDF 1.7.
    #[value(name = "1.7")]
    #[serde(rename = "1.7")]
    V_1_7,
    /// PDF/A-2b.
    #[value(name = "a-2b")]
    #[serde(rename = "a-2b")]
    A_2b,
}

display_possible_values!(PdfStandard);

/// Implements parsing of page ranges (`1-3`, `4`, `5-`, `-2`), used by the
/// `CompileCommand.pages` argument, through the `FromStr` trait instead of a
/// value parser, in order to generate better errors.
///
/// See also: <https://github.com/clap-rs/clap/issues/5065>
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Pages(pub RangeInclusive<Option<NonZeroUsize>>);

impl Pages {
    /// Selects the first page.
    pub const FIRST: Pages = Pages(NonZeroUsize::new(1)..=None);
}

impl FromStr for Pages {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value
            .split('-')
            .map(str::trim)
            .collect::<Vec<_>>()
            .as_slice()
        {
            [] | [""] => Err("page export range must not be empty"),
            [single_page] => {
                let page_number = parse_page_number(single_page)?;
                Ok(Pages(Some(page_number)..=Some(page_number)))
            }
            ["", ""] => Err("page export range must have start or end"),
            [start, ""] => Ok(Pages(Some(parse_page_number(start)?)..=None)),
            ["", end] => Ok(Pages(None..=Some(parse_page_number(end)?))),
            [start, end] => {
                let start = parse_page_number(start)?;
                let end = parse_page_number(end)?;
                if start > end {
                    Err("page export range must end at a page after the start")
                } else {
                    Ok(Pages(Some(start)..=Some(end)))
                }
            }
            [_, _, _, ..] => Err("page export range must have a single hyphen"),
        }
    }
}

impl fmt::Display for Pages {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let start = match self.0.start() {
            Some(start) => start.to_string(),
            None => String::from(""),
        };
        let end = match self.0.end() {
            Some(end) => end.to_string(),
            None => String::from(""),
        };
        write!(f, "{start}-{end}")
    }
}

impl serde::Serialize for Pages {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for Pages {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

/// Parses a single page number.
fn parse_page_number(value: &str) -> Result<NonZeroUsize, &'static str> {
    if value == "0" {
        Err("page numbers start at one")
    } else {
        NonZeroUsize::from_str(value).map_err(|_| "not a valid page number")
    }
}

/// A resource path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResourcePath(EcoString, String);

impl fmt::Display for ResourcePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.0, self.1)
    }
}

impl FromStr for ResourcePath {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut parts = value.split(':');
        let scheme = parts.next().ok_or("missing scheme")?;
        let path = parts.next().ok_or("missing path")?;
        if parts.next().is_some() {
            Err("too many colons")
        } else {
            Ok(ResourcePath(scheme.into(), path.to_string()))
        }
    }
}

impl serde::Serialize for ResourcePath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for ResourcePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

impl ResourcePath {
    /// Creates a new resource path from a user passing system path.
    pub fn from_user_sys(inp: &Path) -> Self {
        let rel = if inp.is_relative() {
            inp.to_path_buf()
        } else {
            let cwd = std::env::current_dir().unwrap();
            pathdiff::diff_paths(inp, &cwd).unwrap()
        };
        let rel = unix_slash(&rel);
        ResourcePath("file".into(), rel.to_string())
    }
    /// Creates a new resource path from a file id.
    pub fn from_file_id(id: FileId) -> Self {
        let package = id.package();
        match package {
            Some(package) => ResourcePath(
                "file_id".into(),
                format!("{package}{}", unix_slash(id.vpath().as_rooted_path())),
            ),
            None => ResourcePath(
                "file_id".into(),
                format!("$root{}", unix_slash(id.vpath().as_rooted_path())),
            ),
        }
    }

    /// Converts the resource path to a path relative to the `base` (usually the
    /// directory storing the lockfile).
    pub fn to_rel_path(&self, base: &Path) -> Option<PathBuf> {
        if self.0 == "file" {
            let path = Path::new(&self.1);
            if path.is_absolute() {
                Some(pathdiff::diff_paths(path, base).unwrap_or_else(|| path.to_owned()))
            } else {
                Some(path.to_owned())
            }
        } else {
            None
        }
    }

    /// Converts the resource path to an absolute file system path.
    pub fn to_abs_path(&self, base: &Path) -> Option<PathBuf> {
        if self.0 == "file" {
            let path = Path::new(&self.1);
            if path.is_absolute() {
                Some(path.to_owned())
            } else {
                Some(base.join(path))
            }
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst::syntax::VirtualPath;

    #[test]
    fn test_substitute_path() {
        let root = Path::new("/root");
        let entry =
            EntryState::new_rooted(root.into(), Some(VirtualPath::new("/dir1/dir2/file.txt")));

        assert_eq!(
            PathPattern::new("/substitute/$dir/$name").substitute(&entry),
            Some(PathBuf::from("/substitute/dir1/dir2/file.txt").into())
        );
        assert_eq!(
            PathPattern::new("/substitute/$dir/../$name").substitute(&entry),
            Some(PathBuf::from("/substitute/dir1/file.txt").into())
        );
        assert_eq!(
            PathPattern::new("/substitute/$name").substitute(&entry),
            Some(PathBuf::from("/substitute/file.txt").into())
        );
        assert_eq!(
            PathPattern::new("/substitute/target/$dir/$name").substitute(&entry),
            Some(PathBuf::from("/substitute/target/dir1/dir2/file.txt").into())
        );
    }
}
