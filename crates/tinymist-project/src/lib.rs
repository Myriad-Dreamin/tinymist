//! Project model of tinymist.

#![allow(missing_docs)]

mod args;
pub use args::*;

use core::fmt;
use std::{
    cmp::Ordering,
    io::{Read, Seek, SeekFrom, Write},
    num::NonZeroUsize,
    ops::RangeInclusive,
    path::Path,
    str::FromStr,
};

use anyhow::{bail, Context};
use clap::{ValueEnum, ValueHint};
use reflexo::path::unix_slash;

pub use anyhow::Result;

const LOCK_VERSION: &str = "0.1.0-beta0";

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case", tag = "version")]
pub enum LockFileCompat {
    #[serde(rename = "0.1.0-beta0")]
    Version010Beta0(LockFile),
    #[serde(untagged)]
    Other(serde_json::Value),
}

impl LockFileCompat {
    pub fn version(&self) -> anyhow::Result<&str> {
        match self {
            LockFileCompat::Version010Beta0(..) => Ok(LOCK_VERSION),
            LockFileCompat::Other(v) => v
                .get("version")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing version field")),
        }
    }

    pub fn migrate(self) -> anyhow::Result<LockFile> {
        match self {
            LockFileCompat::Version010Beta0(v) => Ok(v),
            this @ LockFileCompat::Other(..) => {
                bail!(
                    "cannot migrate from version: {}",
                    this.version().unwrap_or("unknown version")
                )
            }
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct LockFile {
    // The lock file version.
    // version: String,
    /// The project's document (input).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub document: Vec<ProjectInput>,
    /// The project's task (output).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub task: Vec<ProjectTask>,
    /// The project's task route.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub route: Vec<ProjectRoute>,
}

impl LockFile {
    pub fn replace_document(&mut self, input: ProjectInput) {
        let id = input.id.clone();
        let index = self.document.iter().position(|i| i.id == id);
        if let Some(index) = index {
            self.document[index] = input;
        } else {
            self.document.push(input);
        }
    }

    pub fn replace_task(&mut self, task: ProjectTask) {
        let id = task.id().clone();
        let index = self.task.iter().position(|i| *i.id() == id);
        if let Some(index) = index {
            self.task[index] = task;
        } else {
            self.task.push(task);
        }
    }

    pub fn sort(&mut self) {
        self.document.sort_by(|a, b| a.id.cmp(&b.id));
        self.task
            .sort_by(|a, b| a.doc_id().cmp(b.doc_id()).then_with(|| a.id().cmp(b.id())));
        // the route's order is important, so we don't sort them.
    }

    pub fn serialize_resolve(&self) -> String {
        let content = toml::Table::try_from(self).unwrap();

        let mut out = String::new();

        // At the start of the file we notify the reader that the file is generated.
        // Specifically Phabricator ignores files containing "@generated", so we use
        // that.
        let marker_line = "# This file is automatically @generated by tinymist.";
        let extra_line = "# It is not intended for manual editing.";

        out.push_str(marker_line);
        out.push('\n');
        out.push_str(extra_line);
        out.push('\n');

        out.push_str(&format!("version = {LOCK_VERSION:?}\n"));

        let document = content.get("document");
        if let Some(document) = document {
            for document in document.as_array().unwrap() {
                out.push('\n');
                out.push_str("[[document]]\n");
                emit_document(document, &mut out);
            }
        }

        let task = content.get("task");
        if let Some(task) = task {
            for task in task.as_array().unwrap() {
                out.push('\n');
                out.push_str("[[task]]\n");
                emit_output(task, &mut out);
            }
        }

        let route = content.get("route");
        if let Some(route) = route {
            for route in route.as_array().unwrap() {
                out.push('\n');
                out.push_str("[[route]]\n");
                emit_route(route, &mut out);
            }
        }

        return out;

        fn emit_document(input: &toml::Value, out: &mut String) {
            let table = input.as_table().unwrap();
            out.push_str(&table.to_string());
        }

        fn emit_output(output: &toml::Value, out: &mut String) {
            let mut table = output.clone();
            let table = table.as_table_mut().unwrap();
            // replace transform with task.transforms
            if let Some(transform) = table.remove("transform") {
                let mut task_table = toml::Table::new();
                task_table.insert("transform".to_string(), transform);

                table.insert("task".to_string(), task_table.into());
            }

            out.push_str(&table.to_string());
        }

        fn emit_route(route: &toml::Value, out: &mut String) {
            let table = route.as_table().unwrap();
            out.push_str(&table.to_string());
        }
    }

    pub fn update(path: &str, f: impl FnOnce(&mut Self) -> Result<()>) -> Result<()> {
        let cwd = Path::new(".").to_owned();
        let fs = tinymist_fs::flock::Filesystem::new(cwd);

        let mut lock_file = fs.open_rw_exclusive_create(path, "project commands")?;

        let mut data = vec![];
        lock_file.read_to_end(&mut data)?;

        let old_data =
            std::str::from_utf8(&data).context("tinymist.lock file is not valid utf-8")?;

        let mut state = if old_data.trim().is_empty() {
            LockFile {
                document: vec![],
                task: vec![],
                route: vec![],
            }
        } else {
            let old_state = toml::from_str::<LockFileCompat>(old_data)
                .context("tinymist.lock file is not a valid TOML file")?;

            let version = old_state.version()?;
            match Version(version).partial_cmp(&Version(LOCK_VERSION)) {
                Some(Ordering::Equal | Ordering::Less) => {}
                Some(Ordering::Greater) => {
                    bail!(
                    "trying to update lock file having a future version, current tinymist-cli supports {LOCK_VERSION}, the lock file is {version}",
                );
                }
                None => {
                    bail!(
                    "cannot compare version, are version strings in right format? current tinymist-cli supports {LOCK_VERSION}, the lock file is {version}",
                );
                }
            }

            old_state.migrate()?
        };

        f(&mut state)?;

        // todo: for read only operations, we don't have to compare it.
        state.sort();
        let new_data = state.serialize_resolve();

        // If the lock file contents haven't changed so don't rewrite it. This is
        // helpful on read-only filesystems.
        if old_data == new_data {
            return Ok(());
        }

        lock_file.file().set_len(0)?;
        lock_file.seek(SeekFrom::Start(0))?;
        lock_file.write_all(new_data.as_bytes())?;

        Ok(())
    }
}

/// A project ID.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub struct Id(String);

impl Id {
    pub fn new(s: String) -> Self {
        Id(s)
    }
}

/// The id of a document.
///
/// If an identifier is not provided, the document's path is used as the id.
#[derive(Debug, Clone, clap::Parser)]
pub struct DocIdArgs {
    /// Give a name to the document.
    #[clap(long = "name")]
    pub name: Option<String>,
    /// Path to input Typst file.
    #[clap(value_hint = ValueHint::FilePath)]
    pub input: String,
}

impl From<&DocIdArgs> for Id {
    fn from(args: &DocIdArgs) -> Self {
        if let Some(id) = &args.name {
            Id(id.clone())
        } else {
            let inp = Path::new(&args.input);
            Id(ResourcePath::from_user_sys(inp).to_string())
        }
    }
}

/// A resource path.
#[derive(Debug, Clone)]
pub struct ResourcePath(String, String);

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
            Ok(ResourcePath(scheme.to_string(), path.to_string()))
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
    pub fn from_user_sys(inp: &Path) -> Self {
        let rel = if inp.is_relative() {
            inp.to_path_buf()
        } else {
            let cwd = std::env::current_dir().unwrap();
            pathdiff::diff_paths(inp, &cwd).unwrap()
        };
        let rel = unix_slash(&rel);
        ResourcePath("file".to_string(), rel.to_string())
    }
}

/// A project input specifier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProjectInput {
    /// The project's ID.
    pub id: Id,
    /// The project's root directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<ResourcePath>,
    /// The project's font paths.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub font_paths: Vec<ResourcePath>,
    /// Whether to use system fonts.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub system_fonts: bool,
    /// The project's package path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_path: Option<ResourcePath>,
    /// The project's package cache path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_cache_path: Option<ResourcePath>,
}

/// A project task specifier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case", tag = "type")]
pub enum ProjectTask {
    /// A preview task.
    Preview(PreviewTask),
    /// An export PDF task.
    ExportPdf(ExportPdfTask),
    /// An export PNG task.
    ExportPng(ExportPngTask),
    /// An export SVG task.
    ExportSvg(ExportSvgTask),
    // todo: compatibility
    // An export task of another type.
    // Other(serde_json::Value),
}

impl ProjectTask {
    /// Returns the task's ID.
    pub fn doc_id(&self) -> &Id {
        match self {
            ProjectTask::Preview(task) => &task.doc_id,
            ProjectTask::ExportPdf(task) => &task.export.document,
            ProjectTask::ExportPng(task) => &task.export.document,
            ProjectTask::ExportSvg(task) => &task.export.document,
            // ProjectTask::Other(_) => return None,
        }
    }

    /// Returns the task's ID.
    pub fn id(&self) -> &Id {
        match self {
            ProjectTask::Preview(task) => &task.id,
            ProjectTask::ExportPdf(task) => &task.export.id,
            ProjectTask::ExportPng(task) => &task.export.id,
            ProjectTask::ExportSvg(task) => &task.export.id,
            // ProjectTask::Other(_) => return None,
        }
    }
}

/// An lsp task specifier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PreviewTask {
    /// The task's ID.
    pub id: Id,
    /// The doc's ID.
    pub doc_id: Id,
    /// When to run the task
    pub when: TaskWhen,
}

/// An export task specifier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExportTask {
    /// The task's ID.
    pub id: Id,
    /// The doc's ID.
    pub document: Id,
    /// When to run the task
    pub when: TaskWhen,
    /// The task's transforms.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub transform: Vec<ExportTransform>,
}

/// A project export transform specifier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportTransform {
    /// Only pick a subset of pages.
    Pages(Vec<Pages>),
}

/// An export pdf task specifier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExportPdfTask {
    /// The shared export arguments
    #[serde(flatten)]
    pub export: ExportTask,
    /// The pdf standards.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub pdf_standards: Vec<PdfStandard>,
}

/// An export png task specifier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExportPngTask {
    /// The shared export arguments
    #[serde(flatten)]
    pub export: ExportTask,
    /// The PPI (pixels per inch) to use for PNG export.
    pub ppi: f32,
}

/// An export png task specifier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExportSvgTask {
    /// The shared export arguments
    #[serde(flatten)]
    pub export: ExportTask,
}

/// A project route specifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProjectRoute {
    /// A project.
    pub id: Id,
    /// The priority of the project.
    pub priority: u32,
}

struct Version<'a>(&'a str);

impl PartialEq for Version<'_> {
    fn eq(&self, other: &Self) -> bool {
        semver::Version::parse(self.0)
            .ok()
            .and_then(|a| semver::Version::parse(other.0).ok().map(|b| a == b))
            .unwrap_or(false)
    }
}

impl PartialOrd for Version<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let lhs = semver::Version::parse(self.0).ok()?;
        let rhs = semver::Version::parse(other.0).ok()?;
        Some(lhs.cmp(&rhs))
    }
}
