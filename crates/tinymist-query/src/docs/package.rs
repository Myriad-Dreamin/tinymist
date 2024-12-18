use core::fmt::Write;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use ecow::{EcoString, EcoVec};
use indexmap::IndexSet;
use serde::{Deserialize, Serialize};
use typst::diag::{eco_format, StrResult};
use typst::syntax::package::PackageManifest;
use typst::syntax::{FileId, Span};

use crate::docs::{file_id_repr, module_docs, DefDocs, PackageDefInfo};
use crate::package::{get_manifest_id, PackageInfo};
use crate::LocalContext;

/// Generate full documents in markdown format
pub fn package_docs(ctx: &mut LocalContext, spec: &PackageInfo) -> StrResult<String> {
    log::info!("generate_md_docs {spec:?}");

    let mut md = String::new();
    let toml_id = get_manifest_id(spec)?;
    let manifest = ctx.get_manifest(toml_id)?;

    let for_spec = toml_id.package().unwrap();
    let entry_point = toml_id.join(&manifest.package.entrypoint);

    ctx.preload_package(entry_point);

    let PackageDefInfo { root, module_uses } = module_docs(ctx, entry_point)?;

    crate::log_debug_ct!("module_uses: {module_uses:#?}");

    let title = for_spec.to_string();

    let mut errors = vec![];

    writeln!(md, "# {title}").unwrap();
    md.push('\n');
    writeln!(md, "This documentation is generated locally. Please submit issues to [tinymist](https://github.com/Myriad-Dreamin/tinymist/issues) if you see **incorrect** information in it.").unwrap();
    md.push('\n');
    md.push('\n');

    let manifest = ctx.get_manifest(toml_id)?;

    let meta = PackageMeta {
        namespace: spec.namespace.clone(),
        name: spec.name.clone(),
        version: spec.version.to_string(),
        manifest: Some(manifest),
    };
    let package_meta = jbase64(&meta);
    let _ = writeln!(md, "<!-- begin:package {package_meta} -->");

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

    while !modules_to_generate.is_empty() {
        for (parent_ident, def) in std::mem::take(&mut modules_to_generate) {
            // parent_ident, symbols
            let children = def.children;

            let module_val = def.decl.as_ref().unwrap();
            let fid = module_val.file_id();
            let aka = fid.map(&mut akas).unwrap_or_default();

            // It is (primary) known to safe as a part of HTML string, so we don't have to
            // do sanitization here.
            let primary = aka.first().cloned().unwrap_or_default();
            if !primary.is_empty() {
                let _ = writeln!(md, "---\n## Module: {primary}");
            }

            crate::log_debug_ct!("module: {primary} -- {parent_ident}");

            let persist_fid = fid.map(|f| file_ids.insert_full(f).0);

            #[derive(Serialize)]
            struct ModuleInfo {
                prefix: EcoString,
                name: EcoString,
                loc: Option<usize>,
                parent_ident: EcoString,
                aka: EcoVec<String>,
            }
            let m = jbase64(&ModuleInfo {
                prefix: primary.as_str().into(),
                name: def.name.clone(),
                loc: persist_fid,
                parent_ident: parent_ident.clone(),
                aka,
            });
            let _ = writeln!(md, "<!-- begin:module {primary} {m} -->");

            for mut child in children {
                let span = child.decl.as_ref().map(|d| d.span());
                let fid_range = span.and_then(|v| {
                    v.id().and_then(|fid| {
                        let allocated = file_ids.insert_full(fid).0;
                        let src = ctx.source_by_id(fid).ok()?;
                        let rng = src.range(v)?;
                        Some((allocated, rng.start, rng.end))
                    })
                });
                let child_fid = child.decl.as_ref().and_then(|d| d.file_id());
                let child_fid = child_fid.or_else(|| span.and_then(Span::id)).or(fid);
                let span = fid_range.or_else(|| {
                    let fid = child_fid?;
                    Some((file_ids.insert_full(fid).0, 0, 0))
                });
                child.loc = span;

                let convert_err = None::<EcoString>;
                if let Some(docs) = &child.parsed_docs {
                    child.parsed_docs = Some(docs.clone());
                    child.docs = None;
                }

                let ident = if !primary.is_empty() {
                    eco_format!("symbol-{}-{primary}.{}", child.kind, child.name)
                } else {
                    eco_format!("symbol-{}-{}", child.kind, child.name)
                };
                let _ = writeln!(md, "### {}: {} in {primary}", child.kind, child.name);

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
                        let _ = writeln!(md, "[Symbol Docs]({lnk})\n");
                    }
                }

                let child_children = std::mem::take(&mut child.children);
                let head = jbase64(&child);
                let _ = writeln!(md, "<!-- begin:symbol {ident} {head} -->");

                if let Some(DefDocs::Function(sig)) = &child.parsed_docs {
                    let _ = writeln!(md, "<!-- begin:sig -->");
                    let _ = writeln!(md, "```typc");
                    let _ = write!(md, "let {}", child.name);
                    let _ = sig.print(&mut md);
                    let _ = writeln!(md, ";");
                    let _ = writeln!(md, "```");
                    let _ = writeln!(md, "<!-- end:sig -->");
                }

                let mut printed_docs = false;
                match (&child.parsed_docs, convert_err) {
                    (_, Some(err)) => {
                        let err = format!("failed to convert docs in {title}: {err}").replace(
                            "-->", "—>", // avoid markdown comment
                        );
                        let _ = writeln!(md, "<!-- convert-error: {err} -->");
                        errors.push(err);
                    }
                    (Some(docs), _) if !child.is_external => {
                        let _ = writeln!(md, "{}", remove_list_annotations(docs.docs()));
                        printed_docs = true;
                        if let DefDocs::Function(f) = docs {
                            for param in f.pos.iter().chain(f.named.values()).chain(f.rest.as_ref())
                            {
                                let _ = writeln!(md, "<!-- begin:param {} -->", param.name);
                                let ty = match &param.cano_type {
                                    Some((short, _, _)) => short,
                                    None => "unknown",
                                };
                                let _ = writeln!(
                                    md,
                                    "#### {} ({ty:?})\n<!-- begin:param-doc {} -->\n{}\n<!-- end:param-doc {} -->",
                                    param.name, param.name, param.docs, param.name
                                );
                                let _ = writeln!(md, "<!-- end:param -->");
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
                            let _ = writeln!(md, "`````typ");
                        }
                        let _ = writeln!(md, "{docs}");
                        if contains_code {
                            let _ = writeln!(md, "`````");
                        }
                    }
                }

                if !child_children.is_empty() {
                    crate::log_debug_ct!("sub_fid: {child_fid:?}");
                    match child_fid {
                        Some(fid) => {
                            let aka = akas(fid);
                            let primary = aka.first().cloned().unwrap_or_default();
                            let link = format!("module-{primary}").replace(".", "");
                            let _ = writeln!(md, "[Module Docs](#{link})\n");

                            if generated_modules.insert(fid) {
                                child.children = child_children;
                                modules_to_generate.push((ident.clone(), child));
                            }
                        }
                        None => {
                            let _ = writeln!(md, "A Builtin Module");
                        }
                    }
                }

                let _ = writeln!(md, "<!-- end:symbol {ident} -->");
            }

            let _ = writeln!(md, "<!-- end:module {primary} -->");
        }
    }

    let res = ConvertResult { errors };
    let err = jbase64(&res);
    let _ = writeln!(md, "<!-- begin:errors {err} -->");
    let _ = writeln!(md, "## Errors");
    for errs in res.errors {
        let _ = writeln!(md, "- {errs}");
    }
    let _ = writeln!(md, "<!-- end:errors -->");

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

    let meta = PackageMetaEnd { packages, files };
    let package_meta = jbase64(&meta);
    let _ = writeln!(md, "<!-- end:package {package_meta} -->");

    Ok(md)
}

fn jbase64<T: Serialize>(s: &T) -> String {
    use base64::Engine;
    let content = serde_json::to_string(s).unwrap();
    base64::engine::general_purpose::STANDARD.encode(content)
}

/// Information about a package.
#[derive(Debug, Serialize, Deserialize)]
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

/// Information about a package.
#[derive(Debug, Serialize, Deserialize)]
pub struct PackageMetaEnd {
    packages: Vec<PackageMeta>,
    files: Vec<FileMeta>,
}

/// Information about a package.
#[derive(Debug, Serialize, Deserialize)]
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
    use reflexo_typst::package::{PackageRegistry, PackageSpec};

    use super::{package_docs, PackageInfo};
    use crate::tests::*;

    fn test(pkg: PackageSpec) {
        run_with_sources("", |verse: &mut LspUniverse, p| {
            let path = verse.registry.resolve(&pkg).unwrap();
            let pi = PackageInfo {
                path: path.as_ref().to_owned(),
                namespace: pkg.namespace,
                name: pkg.name,
                version: pkg.version.to_string(),
            };
            run_with_ctx(verse, p, &|a, _p| {
                let d = package_docs(a, &pi).unwrap();
                let dest = format!(
                    "../../target/{}-{}-{}.md",
                    pi.namespace, pi.name, pi.version
                );
                std::fs::write(dest, d).unwrap();
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
            version: "0.5.2".parse().unwrap(),
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
