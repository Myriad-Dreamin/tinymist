//! Actions for initializing a new project from a template.

use std::io::Write;
use std::path::{Path, PathBuf};

use reflexo_typst::{Bytes, ImmutPath, TypstFileId};
use tinymist_query::package::get_manifest;
use typst::diag::{bail, eco_format, FileError, FileResult, StrResult};
use typst::syntax::package::{PackageSpec, TemplateInfo};
use typst::syntax::VirtualPath;
use typst::World;

use crate::project::LspWorld;

/// The source of a template.
#[derive(Debug, Clone)]
pub enum TemplateSource {
    /// A package from the registry.
    Package(PackageSpec),
}

/// The task to initialize a new project.
pub struct InitTask {
    /// The template to use.
    pub tmpl: TemplateSource,
    /// The directory at which to create the project.
    pub dir: Option<ImmutPath>,
}

/// Get content of the entry file of a template.
pub fn get_entry(world: &LspWorld, tmpl: TemplateSource) -> StrResult<Bytes> {
    let TemplateSource::Package(spec) = tmpl;

    let toml_id = TypstFileId::new(Some(spec.clone()), VirtualPath::new("typst.toml"));
    let manifest = get_manifest(world, toml_id)?;

    // Ensure that it is indeed a template.
    let Some(tmpl_info) = &manifest.template else {
        bail!("package {spec} is not a template");
    };

    let entry_point = toml_id
        .join(&(tmpl_info.path.to_string() + "/main.typ"))
        .join(&tmpl_info.entrypoint);

    world.file(entry_point).map_err(|e| eco_format!("{e}"))
}

/// Execute an initialization command.
pub fn init(world: &LspWorld, task: InitTask) -> StrResult<PathBuf> {
    let TemplateSource::Package(spec) = task.tmpl;
    let project_dir = task
        .dir
        .unwrap_or_else(|| Path::new(spec.name.as_str()).into());

    let toml_id = TypstFileId::new(Some(spec.clone()), VirtualPath::new("typst.toml"));
    let manifest = get_manifest(world, toml_id)?;

    // Ensure that it is indeed a template.
    let Some(template) = &manifest.template else {
        bail!("package {spec} is not a template");
    };

    let entry_point = Path::new(template.entrypoint.as_str()).to_owned();

    // Determine the directory at which we will create the project.
    // let project_dir =
    // Path::new(command.dir.as_deref().unwrap_or(&manifest.package.name));

    // Set up the project.
    scaffold_project(world, template, toml_id, &project_dir)?;

    Ok(entry_point)
}

/// Creates the project directory with the template's contents and returns the
/// path at which it was created.
fn scaffold_project(
    world: &LspWorld,
    tmpl_info: &TemplateInfo,
    toml_id: TypstFileId,
    project_dir: &Path,
) -> StrResult<()> {
    if project_dir.exists() {
        if !project_dir.is_dir() {
            bail!(
                "project directory already exists as a file (at {})",
                project_dir.display()
            );
        }
        // empty_dir(project_dir)?;
        let mut entries = std::fs::read_dir(project_dir)
            .map_err(|e| FileError::from_io(e, project_dir))?
            .peekable();
        if entries.peek().is_some() {
            bail!(
                "project directory already exists and is not empty (at {})",
                project_dir.display()
            );
        }
    }

    let package_root = world.path_for_id(toml_id)?.as_path().to_owned();
    let package_root = package_root
        .parent()
        .ok_or_else(|| eco_format!("package root is not a directory (at {:?})", toml_id))?;

    let template_dir = toml_id.join(tmpl_info.path.as_str());
    // todo: template in memory
    let real_template_dir = world.path_for_id(template_dir)?.to_err()?;
    if !real_template_dir.exists() {
        bail!(
            "template directory does not exist (at {})",
            real_template_dir.display()
        );
    }

    let files = scan_package_files(toml_id.package().cloned(), package_root, &real_template_dir)?;

    // res.insert(id, world.file(id)?);
    for id in files {
        let f = world.file(id)?;
        let template_dir = template_dir.vpath().as_rooted_path();
        let file_path = id.vpath().as_rooted_path();
        let relative_path = file_path.strip_prefix(template_dir).map_err(|err| {
            eco_format!(
                "failed to strip prefix, path: {file_path:?}, root: {template_dir:?}: {err}"
            )
        })?;
        let file_path = project_dir.join(relative_path);
        let file_dir = file_path.parent().unwrap();
        std::fs::create_dir_all(file_dir).map_err(|e| FileError::from_io(e, file_dir))?;
        let mut file =
            std::fs::File::create(&file_path).map_err(|e| FileError::from_io(e, &file_path))?;
        file.write_all(f.as_slice())
            .map_err(|e| FileError::from_io(e, &file_path))?
    }

    Ok(())
}

fn scan_package_files(
    package: Option<PackageSpec>,
    root: &Path,
    tmpl_root: &Path,
) -> FileResult<Vec<TypstFileId>> {
    let mut res = Vec::new();
    for path in walkdir::WalkDir::new(tmpl_root)
        .follow_links(false)
        .into_iter()
    {
        let Ok(de) = path else {
            continue;
        };
        if !de.file_type().is_file() {
            continue;
        }

        let path = de.path();
        let relative_path = match path.strip_prefix(root) {
            Ok(p) => p,
            Err(err) => {
                log::warn!("failed to strip prefix, path: {path:?}, root: {root:?}: {err}");
                continue;
            }
        };

        let id = TypstFileId::new(package.clone(), VirtualPath::new(relative_path));
        res.push(id);
    }

    Ok(res)
}
