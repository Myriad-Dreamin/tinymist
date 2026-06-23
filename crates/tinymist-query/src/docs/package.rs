use core::fmt::Write;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use ecow::{EcoString, EcoVec};
use indexmap::IndexSet;
use serde::{Deserialize, Serialize};
use tinymist_analysis::docs::tidy::remove_list_annotations;
use tinymist_std::path::unix_slash;
use tinymist_world::package::PackageSpec;
use typst::diag::{StrResult, eco_format};
use typst::syntax::package::PackageManifest;
use typst::syntax::{FileId, Span, VirtualRoot};
use typst_shim::syntax::{RootedPathExt, source_range};

use crate::LocalContext;
use crate::docs::{DefDocs, PackageDefInfo, SourceQuery, file_id_repr, module_docs};
use crate::index::{ScipPublicApi, ScipPublicModule};
use crate::package::{PackageInfo, get_manifest_id, package_entrypoint_id};
use crate::prelude::Definition;
use crate::syntax::DefKind;

/// Documentation Information about a package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageDoc {
    meta: PackageMeta,
    packages: Vec<PackageMeta>,
    files: Vec<FileMeta>,
    modules: Vec<(EcoString, crate::docs::DefInfo, ModuleInfo)>,
}

/// Documentation Information about a package module.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModuleInfo {
    prefix: EcoString,
    name: EcoString,
    parent_ident: EcoString,
    aka: EcoVec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<PathBuf>,
    #[serde(skip)]
    source: Option<EcoString>,
}

/// A generated Typst source file for bundle-mode package docs.
#[derive(Debug, Clone)]
pub struct PackageDocTypFile {
    /// The path of the generated file, relative to the bundle source root.
    pub path: PathBuf,
    /// The Typst source content.
    pub content: String,
}

struct BundleModulePath {
    module_source: PathBuf,
    module_source_import: String,
    module_common_import: String,
    module_output: String,
    module_func: String,
    symbol_paths: Vec<BundleSymbolPath>,
    source_source: Option<PathBuf>,
    source_source_import: Option<String>,
    source_common_import: Option<String>,
    source_output: Option<String>,
    source_func: String,
    source_path: Option<String>,
    source_text: Option<EcoString>,
}

struct BundleSymbolPath {
    section: &'static str,
    symbol_index: usize,
    source: PathBuf,
    source_import: String,
    common_import: String,
    output: String,
    func: String,
}

#[derive(Debug, Clone, Copy)]
enum BundleSection {
    Constants,
    Functions,
}

impl BundleSection {
    const ALL: [Self; 2] = [Self::Constants, Self::Functions];

    fn id(self) -> &'static str {
        match self {
            Self::Constants => "constants",
            Self::Functions => "functions",
        }
    }

    fn accepts(self, child: &crate::docs::DefInfo) -> bool {
        match self {
            Self::Constants => {
                matches!(child.kind, DefKind::Constant | DefKind::Variable)
                    && !child.name.as_str().starts_with('_')
            }
            Self::Functions => {
                matches!(child.kind, DefKind::Function) && !child.name.as_str().starts_with('_')
            }
        }
    }
}

/// Generate full documents in markdown format
pub fn package_docs(ctx: &mut LocalContext, spec: &PackageInfo) -> StrResult<PackageDoc> {
    log::info!("generate_md_docs {spec:?}");

    let toml_id = get_manifest_id(spec)?;
    let manifest = ctx.get_manifest(toml_id)?;

    let for_spec = toml_id
        .package_compat()
        .expect("package manifest must be in a package");
    let entry_point = package_entrypoint_id(toml_id, &manifest.package.entrypoint);

    ctx.preload_package(entry_point);

    let PackageDefInfo { root, module_uses } = module_docs(ctx, entry_point)?;

    crate::log_debug_ct!("module_uses: {module_uses:#?}");

    let manifest = ctx.get_manifest(toml_id)?;

    let meta = PackageMeta {
        namespace: spec.namespace.clone(),
        name: spec.name.clone(),
        version: spec.version.to_string(),
        manifest: Some(manifest),
    };

    let mut modules_to_generate = vec![(root.name.clone(), root)];
    let mut generated_modules = HashSet::new();
    let mut file_ids: IndexSet<FileId> = IndexSet::new();

    // let aka = module_uses[&file_id_repr(fid.unwrap())].clone();
    // let primary = &aka[0];
    let mut primary_aka_cache = HashMap::<FileId, EcoVec<String>>::new();
    let mut akas = |fid: FileId| {
        primary_aka_cache
            .entry(fid)
            .or_insert_with(|| {
                module_uses
                    .get(&file_id_repr(fid))
                    .unwrap_or_else(|| panic!("no module uses for {}", file_id_repr(fid)))
                    .clone()
            })
            .clone()
    };

    let mut modules = vec![];

    while !modules_to_generate.is_empty() {
        for (parent_ident, mut def) in std::mem::take(&mut modules_to_generate) {
            // parent_ident, symbols

            set_scip_symbol(ctx, &mut def);
            let module_val = def.decl.as_ref().unwrap();
            let fid = module_val.file_id();
            let aka = fid.map(&mut akas).unwrap_or_default();

            // It is (primary) known to safe as a part of HTML string, so we don't have to
            // do sanitization here.
            let primary = aka.first().cloned().unwrap_or_default();

            if let Some(fid) = fid {
                file_ids.insert_full(fid);
            }

            let module_info = ModuleInfo {
                prefix: primary.as_str().into(),
                name: def.name.clone(),
                parent_ident: parent_ident.clone(),
                aka,
                path: fid.map(|fid| fid.vpath().get_without_slash().to_owned().into()),
                source: fid.and_then(|fid| ctx.source_by_id(fid).ok().map(|src| src.text().into())),
            };

            for child in def.children.iter_mut() {
                set_scip_symbol(ctx, child);
                let span = child.decl.as_ref().map(|decl| decl.span());
                let fid_range = span.and_then(|v| {
                    v.id().and_then(|fid| {
                        let allocated = file_ids.insert_full(fid).0;
                        let src = ctx.source_by_id(fid).ok()?;
                        let rng = source_range(&src, v)?;
                        let start = ctx.to_lsp_range(rng.clone(), &src).start;
                        child.source = Some(SourceQuery {
                            file: allocated,
                            position: start,
                        });
                        Some((allocated, rng.start, rng.end))
                    })
                });
                let child_fid = child.decl.as_ref().and_then(|decl| decl.file_id());
                let child_fid = child_fid.or_else(|| span.and_then(Span::id)).or(fid);
                let span = fid_range.or_else(|| {
                    let fid = child_fid?;
                    Some((file_ids.insert_full(fid).0, 0, 0))
                });
                child.loc = span;

                if child.parsed_docs.is_some() {
                    child.docs = None;
                }

                let ident = if !primary.is_empty() {
                    eco_format!("symbol-{}-{primary}.{}", child.kind, child.name)
                } else {
                    eco_format!("symbol-{}-{}", child.kind, child.name)
                };

                if child.is_external
                    && let Some(fid) = child_fid
                {
                    let lnk = if matches!(fid.root(), VirtualRoot::Package(spec) if spec == for_spec)
                    {
                        let sub_aka = akas(fid);
                        let sub_primary = sub_aka.first().cloned().unwrap_or_default();
                        child.external_link = Some(format!(
                            "#symbol-{}-{sub_primary}.{}",
                            child.kind, child.name
                        ));
                        if matches!(child.kind, DefKind::Module) {
                            module_heading_anchor(&sub_primary)
                        } else {
                            format!("#{}-{}-in-{sub_primary}", child.kind, child.name)
                                .replace(".", "")
                        }
                    } else if let VirtualRoot::Package(spec) = fid.root() {
                        let lnk = format!(
                            "https://typst.app/universe/package/{}/{}",
                            spec.name, spec.version
                        );
                        child.external_link = Some(lnk.clone());
                        lnk
                    } else {
                        let lnk: String = "https://typst.app/docs".into();
                        child.external_link = Some(lnk.clone());
                        lnk
                    };
                    child.symbol_link = Some(lnk);
                }

                let child_children = std::mem::take(&mut child.children);
                if !child_children.is_empty() {
                    crate::log_debug_ct!("sub_fid: {child_fid:?}");
                    let lnk = match child_fid {
                        Some(fid) => {
                            let aka = akas(fid);
                            let primary = aka.first().cloned().unwrap_or_default();

                            if generated_modules.insert(fid) {
                                let mut child = child.clone();
                                child.children = child_children;
                                modules_to_generate.push((ident.clone(), child));
                            }

                            module_heading_anchor(&primary)
                        }
                        None => "builtin".to_owned(),
                    };

                    child.module_link = Some(lnk);
                }

                child.id = ident;
            }

            modules.push((parent_ident, def, module_info));
        }
    }

    let mut bundle_links = HashMap::new();
    for (idx, (parent_ident, _, info)) in modules.iter().enumerate() {
        let path = module_output_path(idx, info);
        bundle_links.insert(parent_ident.clone(), path.clone());
        for aka in &info.aka {
            bundle_links.insert(eco_format!("symbol-module-{aka}"), path.clone());
        }
    }
    for (idx, (_, def, info)) in modules.iter_mut().enumerate() {
        apply_bundle_links(def, &bundle_links);
        apply_symbol_bundle_links(idx, def, info);
    }

    let mut packages = IndexSet::new();

    let files = file_ids
        .into_iter()
        .map(|fid| {
            let pkg = fid
                .package_compat()
                .map(|spec| packages.insert_full(spec.clone()).0);

            FileMeta {
                package: pkg,
                path: fid.vpath().get_without_slash().to_owned().into(),
                uri: ctx.uri_for_id(fid).ok().map(|uri| uri.to_string()),
            }
        })
        .collect();

    let packages = packages
        .into_iter()
        .map(|spec| PackageMeta {
            namespace: spec.namespace.clone(),
            name: spec.name.clone(),
            version: spec.version.to_string(),
            manifest: None,
        })
        .collect();

    let doc = PackageDoc {
        meta,
        packages,
        files,
        modules,
    };

    Ok(doc)
}

impl PackageDoc {
    /// Gets the public API overlay for SCIP encoding.
    pub fn scip_public_api(&self) -> ScipPublicApi {
        ScipPublicApi {
            modules: self
                .modules
                .iter()
                .map(|(_, def, info)| ScipPublicModule {
                    file_path: info.path.as_ref().map(|path| unix_slash(path)),
                    module_symbol: def.symbol.clone(),
                    public_symbols: def
                        .children
                        .iter()
                        .filter(|child| is_public_api_symbol(child))
                        .filter_map(|child| child.symbol.clone())
                        .collect(),
                })
                .collect(),
        }
    }
}

fn is_public_api_symbol(def: &crate::docs::DefInfo) -> bool {
    !def.name.as_ref().starts_with('_')
}

fn set_scip_symbol(ctx: &LocalContext, def: &mut crate::docs::DefInfo) {
    let Some(decl) = def.decl.as_ref() else {
        return;
    };
    let definition = Definition::new(decl.clone(), None);
    def.symbol = crate::index::scip::scip_symbol(ctx.shared(), &definition).ok();
}

/// Generate full documents in markdown format
pub fn package_docs_typ(doc: &PackageDoc) -> StrResult<String> {
    let mut out = String::new();

    let _ = writeln!(out, "{}", include_str!("package-doc.typ"));

    let pi = &doc.meta;
    let _ = writeln!(
        out,
        "#package-doc(bytes(read(\"{}-{}-{}.json\")), scip: read(\"{}-{}-{}.scip\", encoding: none))",
        pi.namespace, pi.name, pi.version, pi.namespace, pi.name, pi.version,
    );

    Ok(out)
}

/// Generate Typst source files for bundle-mode package docs.
pub fn package_docs_bundle_typ(doc: &PackageDoc) -> StrResult<Vec<PackageDocTypFile>> {
    let pi = &doc.meta;
    let base = format!("{}-{}-{}", pi.namespace, pi.name, pi.version);

    let module_paths = doc
        .modules
        .iter()
        .enumerate()
        .map(|(idx, (_, def, info))| {
            let module_source = module_source_path(info);
            let source_source = info.source.as_ref().map(|_| module_source_page_path(info));
            let symbol_paths = BundleSection::ALL
                .into_iter()
                .flat_map(|section| {
                    def.children
                        .iter()
                        .filter(move |child| section.accepts(child))
                        .enumerate()
                        .map(move |(symbol_index, child)| {
                            let source = module_symbol_source_path(
                                idx,
                                info,
                                section.id(),
                                child.name.as_str(),
                            );
                            BundleSymbolPath {
                                section: section.id(),
                                symbol_index,
                                source_import: unix_slash(&source),
                                common_import: relative_import_to_common(&source),
                                output: module_symbol_output_path(
                                    idx,
                                    info,
                                    section.id(),
                                    child.name.as_str(),
                                ),
                                func: format!(
                                    "render-module-{idx}-{}-{symbol_index}",
                                    section.id()
                                ),
                                source,
                            }
                        })
                })
                .collect();
            BundleModulePath {
                module_source_import: unix_slash(&module_source),
                module_common_import: relative_import_to_common(&module_source),
                module_output: module_output_path(idx, info),
                module_func: format!("render-module-{idx}"),
                symbol_paths,
                source_source_import: source_source.as_ref().map(|path| unix_slash(path)),
                source_common_import: source_source
                    .as_ref()
                    .map(|path| relative_import_to_common(path)),
                source_output: info
                    .source
                    .as_ref()
                    .map(|_| module_source_output_path(info)),
                source_func: format!("render-source-{idx}"),
                source_path: info.path.as_ref().map(|path| unix_slash(path)),
                source_text: info.source.clone(),
                source_source,
                module_source,
            }
        })
        .collect::<Vec<_>>();

    let mut files = vec![];
    files.push(PackageDocTypFile {
        path: PathBuf::from("common.typ"),
        content: include_str!("package-doc.typ").to_owned(),
    });

    let mut entry = String::new();
    let _ = writeln!(
        entry,
        "#import \"/typ/packages/tinymist-index/lib.typ\": create_index"
    );
    for path in &module_paths {
        let _ = writeln!(
            entry,
            "#import {}: {}",
            typst_string(&path.module_source_import),
            path.module_func
        );
        for symbol in &path.symbol_paths {
            let _ = writeln!(
                entry,
                "#import {}: {}",
                typst_string(&symbol.source_import),
                symbol.func
            );
        }
        if let Some(source_import) = &path.source_source_import {
            let _ = writeln!(
                entry,
                "#import {}: {}",
                typst_string(source_import),
                path.source_func
            );
        }
    }
    let _ = writeln!(
        entry,
        "#let package-info = json(bytes(read(\"../{base}.json\")))"
    );
    let _ = writeln!(
        entry,
        "#let package-index = create_index(read(\"../{base}.scip\", encoding: none))"
    );
    for path in &module_paths {
        let _ = writeln!(entry, "#{}(package-info, package-index)", path.module_func);
        for symbol in &path.symbol_paths {
            let _ = writeln!(entry, "#{}(package-info, package-index)", symbol.func);
        }
    }
    for path in &module_paths {
        if path.source_text.is_some() {
            let _ = writeln!(entry, "#{}(package-info)", path.source_func);
        }
    }
    files.push(PackageDocTypFile {
        path: PathBuf::from("index.typ"),
        content: entry,
    });

    for (idx, path) in module_paths.into_iter().enumerate() {
        let mut content = String::new();
        let _ = writeln!(
            content,
            "#import {}: package-module-document",
            typst_string(&path.module_common_import)
        );
        let _ = writeln!(
            content,
            "#let {func}(package-info, package-index) = package-module-document(package-info, package-index, module-index: {idx}, path: {})",
            typst_string(&path.module_output),
            func = path.module_func,
        );
        files.push(PackageDocTypFile {
            path: path.module_source,
            content,
        });

        for symbol in path.symbol_paths {
            let mut content = String::new();
            let _ = writeln!(
                content,
                "#import {}: package-module-symbol-document",
                typst_string(&symbol.common_import)
            );
            let _ = writeln!(
                content,
                "#let {func}(package-info, package-index) = package-module-symbol-document(package-info, package-index, module-index: {idx}, section: {}, symbol-index: {}, path: {})",
                typst_string(symbol.section),
                symbol.symbol_index,
                typst_string(&symbol.output),
                func = symbol.func,
            );
            files.push(PackageDocTypFile {
                path: symbol.source,
                content,
            });
        }

        if let (
            Some(source_source),
            Some(source_common_import),
            Some(source_output),
            Some(source_path),
            Some(source_text),
        ) = (
            path.source_source,
            path.source_common_import,
            path.source_output,
            path.source_path,
            path.source_text,
        ) {
            let mut content = String::new();
            let _ = writeln!(
                content,
                "#import {}: package-source-document",
                typst_string(&source_common_import)
            );
            let _ = writeln!(
                content,
                "#let {func}(package-info) = package-source-document(package-info, module-index: {idx}, path: {}, source-path: {}, source: {})",
                typst_string(&source_output),
                typst_string(&source_path),
                typst_string(source_text.as_str()),
                func = path.source_func,
            );
            files.push(PackageDocTypFile {
                path: source_source,
                content,
            });
        }
    }

    Ok(files)
}

fn module_source_path(info: &ModuleInfo) -> PathBuf {
    PathBuf::from("modules").join(module_package_path(info))
}

fn module_source_page_path(info: &ModuleInfo) -> PathBuf {
    PathBuf::from("sources").join(module_package_path(info))
}

fn module_symbol_source_path(
    idx: usize,
    info: &ModuleInfo,
    section: &str,
    symbol: &str,
) -> PathBuf {
    let mut path = PathBuf::from("symbols");
    path.push(module_symbol_path(idx, info, section, symbol, "typ"));
    path
}

fn module_package_path(info: &ModuleInfo) -> PathBuf {
    info.path
        .clone()
        .unwrap_or_else(|| PathBuf::from(module_fallback_file_name(info)))
}

fn module_fallback_file_name(info: &ModuleInfo) -> String {
    let raw = if !info.parent_ident.is_empty() {
        info.parent_ident.as_str()
    } else if !info.prefix.is_empty() {
        info.prefix.as_str()
    } else {
        info.name.as_str()
    };
    let mut stem = String::new();
    let mut prev_dash = false;
    for ch in raw.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            stem.push(ch);
            prev_dash = false;
        } else if !prev_dash {
            stem.push('-');
            prev_dash = true;
        }
    }

    let stem = stem.trim_matches('-');
    if stem.is_empty() {
        "module.typ".to_owned()
    } else {
        format!("{stem}.typ")
    }
}

fn module_output_path(idx: usize, info: &ModuleInfo) -> String {
    if idx == 0 {
        "index.html".to_owned()
    } else {
        let mut output = module_package_path(info);
        output.set_extension("html");
        unix_slash(&output)
    }
}

fn module_source_output_path(info: &ModuleInfo) -> String {
    format!("{}.html", unix_slash(&module_package_path(info)))
}

fn module_symbol_output_path(idx: usize, info: &ModuleInfo, section: &str, symbol: &str) -> String {
    unix_slash(&module_symbol_path(idx, info, section, symbol, "html"))
}

fn module_symbol_path(
    idx: usize,
    info: &ModuleInfo,
    section: &str,
    symbol: &str,
    extension: &str,
) -> PathBuf {
    let file_name = format!("{}.{}", symbol_file_stem(symbol), extension);
    if idx == 0 {
        return PathBuf::from(section).join(file_name);
    }

    let path = module_package_path(info);
    let stem = path
        .file_stem()
        .map(|stem| stem.to_owned())
        .unwrap_or_else(|| info.name.as_str().into());
    let mut output = path.parent().map(Path::to_owned).unwrap_or_default();
    output.push(stem);
    output.push(section);
    output.push(file_name);
    output
}

fn symbol_file_stem(raw: &str) -> String {
    let mut stem = String::new();
    let mut prev_dash = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            stem.push(ch);
            prev_dash = false;
        } else if !prev_dash {
            stem.push('-');
            prev_dash = true;
        }
    }

    let stem = stem.trim_matches('-');
    if stem.is_empty() {
        "symbol".to_owned()
    } else {
        stem.to_owned()
    }
}

fn relative_import_to_common(source: &Path) -> String {
    let mut path = PathBuf::new();
    let depth = source
        .parent()
        .map(|parent| parent.components().count())
        .unwrap_or(0);
    for _ in 0..depth {
        path.push("..");
    }
    path.push("common.typ");
    unix_slash(&path)
}

fn typst_string(value: &str) -> String {
    serde_json::to_string(value).expect("Typst string serialization must succeed")
}

fn module_heading_anchor(primary: &str) -> String {
    let mut anchor = String::from("#");
    let mut prev_dash = false;

    for ch in format!("Module: {primary}").chars() {
        if ch.is_whitespace() || matches!(ch, ':' | '-') {
            if !prev_dash {
                anchor.push('-');
                prev_dash = true;
            }
        } else if matches!(ch, '.' | '(' | ')') {
            continue;
        } else {
            if ch == 'M' {
                anchor.push('m');
            } else {
                anchor.push(ch);
            }
            prev_dash = false;
        }
    }

    anchor
}

fn apply_bundle_links(def: &mut crate::docs::DefInfo, links: &HashMap<EcoString, String>) {
    for child in &mut def.children {
        if matches!(child.kind, DefKind::Module)
            && let Some(link) = links.get(&child.id)
        {
            child.bundle_link = Some(link.clone());
        }
    }
}

fn apply_symbol_bundle_links(idx: usize, def: &mut crate::docs::DefInfo, info: &ModuleInfo) {
    for child in &mut def.children {
        for section in BundleSection::ALL {
            if section.accepts(child) {
                child.bundle_link = Some(module_symbol_output_path(
                    idx,
                    info,
                    section.id(),
                    child.name.as_str(),
                ));
                break;
            }
        }
    }
}

/// Generate full documents in markdown format
pub fn package_docs_md(doc: &PackageDoc) -> StrResult<String> {
    let mut out = String::new();

    let title = doc.meta.spec().to_string();

    writeln!(out, "# {title}").unwrap();
    out.push('\n');
    writeln!(out, "This documentation is generated locally. Please submit issues to [tinymist](https://github.com/Myriad-Dreamin/tinymist/issues) if you see **incorrect** information in it.").unwrap();
    out.push('\n');
    out.push('\n');

    let package_meta = jbase64(&doc.meta);
    let _ = writeln!(out, "<!-- begin:package {package_meta} -->");

    let mut errors = vec![];
    for (parent_ident, def, module_info) in &doc.modules {
        // parent_ident, symbols
        let primary = &module_info.prefix;
        if !module_info.prefix.is_empty() {
            let _ = writeln!(out, "---\n## Module: {primary}");
        }

        crate::log_debug_ct!("module: {primary} -- {parent_ident}");
        let module_info = jbase64(&module_info);
        let _ = writeln!(out, "<!-- begin:module {primary} {module_info} -->");

        for child in &def.children {
            let convert_err = None::<EcoString>;

            let ident = if !primary.is_empty() {
                eco_format!("symbol-{}-{primary}.{}", child.kind, child.name)
            } else {
                eco_format!("symbol-{}-{}", child.kind, child.name)
            };
            let _ = writeln!(out, "### {}: {} in {primary}", child.kind, child.name);

            if let Some(lnk) = &child.symbol_link {
                let _ = writeln!(out, "[Symbol Docs]({lnk})\n");
            }

            let head = jbase64(&child);
            let _ = writeln!(out, "<!-- begin:symbol {ident} {head} -->");

            if let Some(DefDocs::Function(sig)) = &child.parsed_docs {
                let _ = writeln!(out, "<!-- begin:sig -->");
                let _ = writeln!(out, "```typc");
                let _ = write!(out, "let {}", child.name);
                let _ = sig.print(&mut out);
                let _ = writeln!(out, ";");
                let _ = writeln!(out, "```");
                let _ = writeln!(out, "<!-- end:sig -->");
            }

            let mut printed_docs = false;
            match (&child.parsed_docs, convert_err) {
                (_, Some(err)) => {
                    let err = format!("failed to convert docs in {title}: {err}").replace(
                        "-->", "—>", // avoid markdown comment
                    );
                    let _ = writeln!(out, "<!-- convert-error: {err} -->");
                    errors.push(err);
                }
                (Some(docs), _) if !child.is_external => {
                    let _ = writeln!(out, "{}", remove_list_annotations(docs.docs()));
                    printed_docs = true;
                    if let DefDocs::Function(docs) = docs {
                        for param in docs
                            .pos
                            .iter()
                            .chain(docs.named.values())
                            .chain(docs.rest.as_ref())
                        {
                            let _ = writeln!(out, "<!-- begin:param {} -->", param.name);
                            let ty = match &param.cano_type {
                                Some((short, _, _)) => short,
                                None => "unknown",
                            };
                            let _ = writeln!(
                                out,
                                "#### {} ({ty:?})\n<!-- begin:param-doc {} -->\n{}\n<!-- end:param-doc {} -->",
                                param.name, param.name, param.docs, param.name
                            );
                            let _ = writeln!(out, "<!-- end:param -->");
                        }
                    }
                }
                (_, None) => {}
            }

            if !printed_docs {
                let plain_docs = child.docs.as_deref();
                let plain_docs = plain_docs.or(child.oneliner.as_deref());

                if let Some(docs) = plain_docs {
                    let contains_code = docs.contains("```");
                    if contains_code {
                        let _ = writeln!(out, "`````typ");
                    }
                    let _ = writeln!(out, "{docs}");
                    if contains_code {
                        let _ = writeln!(out, "`````");
                    }
                }
            }

            if let Some(lnk) = &child.module_link {
                match lnk.as_str() {
                    "builtin" => {
                        let _ = writeln!(out, "A Builtin Module");
                    }
                    lnk => {
                        let _ = writeln!(out, "[Module Docs]({lnk})\n");
                    }
                }
            }

            let _ = writeln!(out, "<!-- end:symbol {ident} -->");
        }

        let _ = writeln!(out, "<!-- end:module {primary} -->");
    }

    let res = ConvertResult { errors };
    let err = jbase64(&res);
    let _ = writeln!(out, "<!-- begin:errors {err} -->");
    let _ = writeln!(out, "## Errors");
    for errs in res.errors {
        let _ = writeln!(out, "- {errs}");
    }
    let _ = writeln!(out, "<!-- end:errors -->");

    let meta = PackageMetaEnd {
        packages: doc.packages.clone(),
        files: doc.files.clone(),
    };
    let package_meta = jbase64(&meta);
    let _ = writeln!(out, "<!-- end:package {package_meta} -->");

    Ok(out)
}

fn jbase64<T: Serialize>(s: &T) -> String {
    use base64::Engine;
    let content = serde_json::to_string(s).unwrap();
    base64::engine::general_purpose::STANDARD.encode(content)
}

/// Information about a package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMeta {
    /// The namespace the package lives in.
    pub namespace: EcoString,
    /// The name of the package within its namespace.
    pub name: EcoString,
    /// The package's version.
    pub version: String,
    /// The package's manifest information.
    pub manifest: Option<PackageManifest>,
}

impl PackageMeta {
    /// Returns the package's full name, including namespace and version.
    pub fn spec(&self) -> PackageSpec {
        PackageSpec {
            namespace: self.namespace.clone(),
            name: self.name.clone(),
            version: self.version.parse().expect("Invalid version format"),
        }
    }
}

/// Information about a package.
#[derive(Debug, Serialize, Deserialize)]
pub struct PackageMetaEnd {
    packages: Vec<PackageMeta>,
    files: Vec<FileMeta>,
}

/// Information about a package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMeta {
    package: Option<usize>,
    path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    uri: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ConvertResult {
    errors: Vec<String>,
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tinymist_world::package::{PackageRegistry, PackageSpec, registry::PREVIEW_NS};

    use super::{
        PackageInfo, package_docs, package_docs_bundle_typ, package_docs_md, package_docs_typ,
    };
    use crate::analysis::Analysis;
    use crate::tests::*;

    fn test(pkg: PackageSpec) {
        run_with_sources("", |verse: &mut LspUniverse, path| {
            let pkg_root = verse.registry.resolve(&pkg).unwrap();
            let pi = PackageInfo {
                path: pkg_root.as_ref().to_owned(),
                namespace: pkg.namespace,
                name: pkg.name,
                version: pkg.version.to_string(),
            };
            let public_api = Arc::new(Mutex::new(None));
            let public_api_docs = public_api.clone();
            run_with_ctx(verse, path, &|a, _p| {
                let docs = package_docs(a, &pi).unwrap();
                *public_api_docs.lock().unwrap() = Some(docs.scip_public_api());
                let dest = format!(
                    "../../target/{}-{}-{}.json",
                    pi.namespace, pi.name, pi.version
                );
                std::fs::write(dest, serde_json::to_string_pretty(&docs).unwrap()).unwrap();
                let typ = package_docs_typ(&docs).unwrap();
                let dest = format!(
                    "../../target/{}-{}-{}.typ",
                    pi.namespace, pi.name, pi.version
                );
                std::fs::write(dest, typ).unwrap();
                let md = package_docs_md(&docs).unwrap();
                let dest = format!(
                    "../../target/{}-{}-{}.md",
                    pi.namespace, pi.name, pi.version
                );
                std::fs::write(dest, md).unwrap();
                let bundle = package_docs_bundle_typ(&docs).unwrap();
                let dest = std::path::PathBuf::from(format!(
                    "../../target/{}-{}-{}.bundle",
                    pi.namespace, pi.name, pi.version
                ));
                let _ = std::fs::remove_dir_all(&dest);
                for file in bundle {
                    let path = dest.join(file.path);
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent).unwrap();
                    }
                    std::fs::write(path, file.content).unwrap();
                }
            });
            let analysis = Arc::new(Analysis::default());
            let public_api_index = public_api.clone();
            let scip = analysis
                .query_snapshot(verse.computation(), None)
                .run_within_package(&pi, |a| {
                    let knowledge = crate::index::knowledge(a).unwrap();
                    let public_api = public_api_index.lock().unwrap();
                    let public_api = public_api
                        .as_ref()
                        .expect("package docs must build public API before SCIP");
                    knowledge
                        .bind(a.shared())
                        .to_scip_bytes_with_public_api(&public_api)
                })
                .unwrap();
            let dest = format!(
                "../../target/{}-{}-{}.scip",
                pi.namespace, pi.name, pi.version
            );
            std::fs::write(dest, scip).unwrap();
        })
    }

    #[test]
    fn tidy() {
        test(PackageSpec {
            namespace: PREVIEW_NS.into(),
            name: "tidy".into(),
            version: "0.3.0".parse().unwrap(),
        });
    }

    #[test]
    fn touying() {
        test(PackageSpec {
            namespace: PREVIEW_NS.into(),
            name: "touying".into(),
            version: "0.6.0".parse().unwrap(),
        });
    }

    #[test]
    fn fletcher() {
        test(PackageSpec {
            namespace: PREVIEW_NS.into(),
            name: "fletcher".into(),
            version: "0.5.8".parse().unwrap(),
        });
    }

    #[test]
    fn cetz() {
        test(PackageSpec {
            namespace: PREVIEW_NS.into(),
            name: "cetz".into(),
            version: "0.2.2".parse().unwrap(),
        });
    }
}
