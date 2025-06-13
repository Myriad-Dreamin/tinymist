use core::fmt::Write;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use ecow::{EcoString, EcoVec};
use indexmap::IndexSet;
use serde::{Deserialize, Serialize};
use tinymist_world::package::PackageSpec;
use typst::diag::{eco_format, StrResult};
use typst::syntax::package::PackageManifest;
use typst::syntax::{FileId, Span};

use crate::docs::{file_id_repr, module_docs, DefDocs, PackageDefInfo};
use crate::package::{get_manifest_id, PackageInfo};
use crate::LocalContext;

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
    loc: Option<usize>,
    parent_ident: EcoString,
    aka: EcoVec<String>,
}

/// Generate full documents in markdown format
pub fn package_docs(ctx: &mut LocalContext, spec: &PackageInfo) -> StrResult<PackageDoc> {
    log::info!("generate_md_docs {spec:?}");

    let toml_id = get_manifest_id(spec)?;
    let manifest = ctx.get_manifest(toml_id)?;

    let for_spec = toml_id.package().unwrap();
    let entry_point = toml_id.join(&manifest.package.entrypoint);

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

            let module_val = def.decl.as_ref().unwrap();
            let fid = module_val.file_id();
            let aka = fid.map(&mut akas).unwrap_or_default();

            // It is (primary) known to safe as a part of HTML string, so we don't have to
            // do sanitization here.
            let primary = aka.first().cloned().unwrap_or_default();

            let persist_fid = fid.map(|fid| file_ids.insert_full(fid).0);

            let module_info = ModuleInfo {
                prefix: primary.as_str().into(),
                name: def.name.clone(),
                loc: persist_fid,
                parent_ident: parent_ident.clone(),
                aka,
            };

            for child in def.children.iter_mut() {
                let span = child.decl.as_ref().map(|decl| decl.span());
                let fid_range = span.and_then(|v| {
                    v.id().and_then(|fid| {
                        let allocated = file_ids.insert_full(fid).0;
                        let src = ctx.source_by_id(fid).ok()?;
                        let rng = src.range(v)?;
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

                if child.is_external {
                    if let Some(fid) = child_fid {
                        let lnk = if fid.package() == Some(for_spec) {
                            let sub_aka = akas(fid);
                            let sub_primary = sub_aka.first().cloned().unwrap_or_default();
                            child.external_link = Some(format!(
                                "#symbol-{}-{sub_primary}.{}",
                                child.kind, child.name
                            ));
                            format!("#{}-{}-in-{sub_primary}", child.kind, child.name)
                                .replace(".", "")
                        } else if let Some(spec) = fid.package() {
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

                            let link = format!("module-{primary}").replace(".", "");
                            format!("#{link}")
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

    let mut packages = IndexSet::new();

    let files = file_ids
        .into_iter()
        .map(|fid| {
            let pkg = fid
                .package()
                .map(|spec| packages.insert_full(spec.clone()).0);

            FileMeta {
                package: pkg,
                path: fid.vpath().as_rootless_path().to_owned(),
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

/// Generate full documents in markdown format
pub fn package_docs_typ(doc: &PackageDoc) -> StrResult<String> {
    let mut out = String::new();

    let _ = writeln!(out, "{}", include_str!("package-doc.typ"));

    let pi = &doc.meta;
    let _ = writeln!(
        out,
        "#package-doc(bytes(read(\"{}-{}-{}.json\")))",
        pi.namespace, pi.name, pi.version,
    );

    Ok(out)
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
}

#[derive(Serialize, Deserialize)]
struct ConvertResult {
    errors: Vec<String>,
}

fn remove_list_annotations(s: &str) -> String {
    let s = s.to_string();
    static REG: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"<!-- typlite:(?:begin|end):[\w\-]+ \d+ -->").unwrap()
    });
    REG.replace_all(&s, "").to_string()
}

#[cfg(test)]
mod tests {
    use tinymist_world::package::{PackageRegistry, PackageSpec};

    use super::{package_docs, package_docs_md, package_docs_typ, PackageInfo};
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
            run_with_ctx(verse, path, &|a, _p| {
                let docs = package_docs(a, &pi).unwrap();
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
            })
        })
    }

    #[test]
    fn tidy() {
        test(PackageSpec {
            namespace: "preview".into(),
            name: "tidy".into(),
            version: "0.3.0".parse().unwrap(),
        });
    }

    #[test]
    fn touying() {
        test(PackageSpec {
            namespace: "preview".into(),
            name: "touying".into(),
            version: "0.6.0".parse().unwrap(),
        });
    }

    #[test]
    fn cetz() {
        test(PackageSpec {
            namespace: "preview".into(),
            name: "cetz".into(),
            version: "0.2.2".parse().unwrap(),
        });
    }
}
