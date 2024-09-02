//! Package management tools.

use core::fmt::Write;
use std::collections::HashSet;
use std::path::PathBuf;

use comemo::Track;
use reflexo_typst::package::{http::HttpRegistry, PackageRegistry, PackageSpec};
use reflexo_typst::{typst::prelude::*, TypstFileId};
use serde::{Deserialize, Serialize};
use tinymist_query::syntax::{find_docs_of, get_non_strict_def_target};
use typst::diag::{eco_format, StrResult};
use typst::engine::Route;
use typst::eval::Tracer;
use typst::foundations::{Module, Value};
use typst::syntax::package::{PackageManifest, PackageVersion, VersionlessPackageSpec};
use typst::syntax::VirtualPath;
use typst::World;

use crate::LspWorld;

mod init;
pub use init::*;

/// Parses the manifest of the package located at `package_path`.
fn get_manifest(world: &LspWorld, toml_id: TypstFileId) -> StrResult<PackageManifest> {
    let toml_data = world
        .file(toml_id)
        .map_err(|err| eco_format!("failed to read package manifest ({})", err))?;

    let string = std::str::from_utf8(&toml_data)
        .map_err(|err| eco_format!("package manifest is not valid UTF-8 ({})", err))?;

    toml::from_str(string)
        .map_err(|err| eco_format!("package manifest is malformed ({})", err.message()))
}

/// Try to determine the latest version of a package.
pub fn determine_latest_version(
    world: &LspWorld,
    spec: &VersionlessPackageSpec,
) -> StrResult<PackageVersion> {
    if spec.namespace == "preview" {
        let packages = world.registry.packages();
        packages
            .iter()
            .filter(|(package, _)| package.namespace == "preview" && package.name == spec.name)
            .map(|(package, _)| package.version)
            .max()
            .ok_or_else(|| eco_format!("failed to find package {spec}"))
    } else {
        // For other namespaces, search locally. We only search in the data
        // directory and not the cache directory, because the latter is not
        // intended for storage of local packages.
        let subdir = format!("typst/packages/{}/{}", spec.namespace, spec.name);
        world
            .registry
            .local_path()
            .into_iter()
            .flat_map(|dir| std::fs::read_dir(dir.join(&subdir)).ok())
            .flatten()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter_map(|path| path.file_name()?.to_string_lossy().parse().ok())
            .max()
            .ok_or_else(|| eco_format!("please specify the desired version"))
    }
}

/// Information about a package.
#[derive(Debug, Serialize, Deserialize)]
pub struct PackageInfo {
    /// The path to the package if any.
    pub path: PathBuf,
    /// The namespace the package lives in.
    pub namespace: EcoString,
    /// The name of the package within its namespace.
    pub name: EcoString,
    /// The package's version.
    pub version: String,
}

impl From<(PathBuf, PackageSpec)> for PackageInfo {
    fn from((path, spec): (PathBuf, PackageSpec)) -> Self {
        Self {
            path,
            namespace: spec.namespace,
            name: spec.name,
            version: spec.version.to_string(),
        }
    }
}

/// Get the packages in namespaces and their descriptions.
pub fn list_package_by_namespace(
    registry: &HttpRegistry,
    ns: EcoString,
) -> EcoVec<(PathBuf, PackageSpec)> {
    // search packages locally. We only search in the data
    // directory and not the cache directory, because the latter is not
    // intended for storage of local packages.
    let mut packages = eco_vec![];

    log::info!(
        "searching for packages in namespace {ns} in paths {:?}",
        registry.paths()
    );
    for dir in registry.paths() {
        let local_path = dir.join(ns.as_str());
        if !local_path.exists() || !local_path.is_dir() {
            continue;
        }
        // namespace/package_name/version
        // 2. package_name
        let package_names = std::fs::read_dir(local_path).unwrap();
        for package in package_names {
            let package = package.unwrap();
            if !package.file_type().unwrap().is_dir() {
                continue;
            }
            if package.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            // 3. version
            let versions = std::fs::read_dir(package.path()).unwrap();
            for version in versions {
                let version = version.unwrap();
                if !version.file_type().unwrap().is_dir() {
                    continue;
                }
                if version.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                let path = version.path();
                let version = version.file_name().to_string_lossy().parse().unwrap();
                let spec = PackageSpec {
                    namespace: ns.clone(),
                    name: package.file_name().to_string_lossy().into(),
                    version,
                };
                packages.push((path, spec));
            }
        }
    }

    packages
}

/// Information about a symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInfo {
    /// The name of the symbol.
    pub name: EcoString,
    /// The kind of the symbol.
    pub kind: EcoString,
    /// The raw documentation of the symbol.
    pub docs: Option<String>,
    /// The children of the symbol.
    pub children: EcoVec<SymbolInfo>,
}

/// List all symbols in a package.
pub fn list_symbols(world: &LspWorld, spec: &PackageInfo) -> StrResult<EcoVec<SymbolInfo>> {
    let toml_id = TypstFileId::new(
        Some(PackageSpec {
            namespace: spec.namespace.clone(),
            name: spec.name.clone(),
            version: spec.version.parse()?,
        }),
        VirtualPath::new("typst.toml"),
    );
    let manifest = get_manifest(world, toml_id)?;

    let entry_point = toml_id.join(&manifest.package.entrypoint);
    let source = world.source(entry_point).map_err(|e| eco_format!("{e}"))?;
    let route = Route::default();
    let mut tracer = Tracer::default();
    let w: &dyn typst::World = world;

    let src = typst::eval::eval(w.track(), route.track(), tracer.track_mut(), &source)
        .map_err(|e| eco_format!("{e:?}"))?;

    let for_spec = PackageSpec {
        namespace: spec.namespace.clone(),
        name: spec.name.clone(),
        version: spec.version.parse()?,
    };
    Ok(module_symbols(world, Some(&for_spec), &src))
}

/// Generate full documents in markdown format
pub fn generate_md_docs(world: &LspWorld, spec: &PackageInfo) -> StrResult<String> {
    let mut md = String::new();
    let symbols = list_symbols(world, spec)?;

    let title = format!("@{}/{}:{}", spec.namespace, spec.name, spec.version);

    writeln!(md, "# {title}").unwrap();
    md.push('\n');
    writeln!(md, "This documentation is generated locally. Please submit issues to [tinymist](https://github.com/Myriad-Dreamin/tinymist/issues) if you see **incorrect** information in it.").unwrap();
    md.push('\n');
    md.push('\n');

    let mut modules_to_generate = vec![(EcoString::new(), symbols)];
    let mut generated_modules = HashSet::new();

    while !modules_to_generate.is_empty() {
        for (prefix, symbols) in std::mem::take(&mut modules_to_generate) {
            if !prefix.is_empty() {
                let _ = writeln!(md, "---\n## Module: {prefix}");
            }

            for sym in symbols {
                let _ = writeln!(md, "### {}: {}", sym.kind, sym.name);

                if let Some(docs) = sym.docs {
                    let contains_code = docs.contains("```");
                    if contains_code {
                        let _ = writeln!(md, "`````typ");
                    }
                    let _ = writeln!(md, "{docs}");
                    if contains_code {
                        let _ = writeln!(md, "`````");
                    }
                }

                if !sym.children.is_empty() {
                    let mut full_path = prefix.clone();
                    if !full_path.is_empty() {
                        full_path.push_str(".");
                    }
                    full_path.push_str(&sym.name);
                    let link = format!("Module-{full_path}").replace(".", "-");
                    let _ = writeln!(md, "[Module Docs](#{link})\n");

                    if generated_modules.insert(full_path.clone()) {
                        modules_to_generate.push((full_path, sym.children.clone()));
                    }
                }
            }
        }
    }

    Ok(md)
}

fn kind_of(val: &Value) -> EcoString {
    match val {
        Value::Module(_) => "module",
        Value::Type(_) => "struct",
        Value::Func(_) => "function",
        Value::Label(_) => "reference",
        _ => "constant",
    }
    .into()
}

fn module_symbols(
    world: &LspWorld,
    for_spec: Option<&PackageSpec>,
    val: &Module,
) -> EcoVec<SymbolInfo> {
    let symbols = val.scope().iter();

    symbols
        .map(|(k, v)| SymbolInfo {
            name: k.to_string().into(),
            kind: kind_of(v),
            docs: None.or_else(|| match v {
                Value::Func(f) => {
                    let source = world.source(f.span().id()?).ok()?;
                    let node = source.find(f.span())?;
                    log::debug!("node: {k} -> {:?}", node.parent());
                    // use parent of params, todo: reliable way to get the def target
                    let def = get_non_strict_def_target(node.parent()?.clone())?;
                    find_docs_of(&source, def)
                }
                _ => None,
            }),
            children: None
                .or_else(|| match v {
                    Value::Module(module) => {
                        // only generate docs for the same package
                        let fid = module.file_id()?;
                        if fid.package() != for_spec {
                            return None;
                        }

                        Some(module_symbols(world, for_spec, module))
                    }
                    _ => None,
                })
                .unwrap_or_default(),
        })
        .collect()
}
