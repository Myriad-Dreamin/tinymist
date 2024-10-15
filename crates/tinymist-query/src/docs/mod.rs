//! Documentation generation utilities.

mod docstring;
mod library;
mod module;
mod package;
mod signature;
mod tidy;

use core::fmt::Write;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use ecow::{EcoString, EcoVec};
use indexmap::IndexSet;
use reflexo::path::unix_slash;
use serde::{Deserialize, Serialize};
use tinymist_world::LspWorld;
use typst::diag::{eco_format, StrResult};
use typst::foundations::Value;
use typst::syntax::package::{PackageManifest, PackageSpec};
use typst::syntax::{FileId, Span};
use typst::World;

use crate::syntax::IdentRef;
use crate::ty::Ty;
use crate::AnalysisContext;
pub use docstring::*;
pub use module::*;
pub use package::*;
pub use signature::*;
pub(crate) use tidy::*;

fn jbase64<T: Serialize>(s: &T) -> String {
    use base64::Engine;
    let content = serde_json::to_string(s).unwrap();
    base64::engine::general_purpose::STANDARD.encode(content)
}

/// Generate full documents in markdown format
pub fn generate_md_docs(
    ctx: &mut AnalysisContext,
    world: &LspWorld,
    spec: &PackageInfo,
) -> StrResult<String> {
    log::info!("generate_md_docs {spec:?}");
    let toml_id = get_manifest_id(spec)?;

    let for_spec = PackageSpec {
        namespace: spec.namespace.clone(),
        name: spec.name.clone(),
        version: spec.version.parse()?,
    };

    let mut md = String::new();
    let SymbolsInfo { root, module_uses } = list_symbols(ctx, spec)?;

    log::debug!("module_uses: {module_uses:#?}");

    let title = for_spec.to_string();

    let mut errors = vec![];

    writeln!(md, "# {title}").unwrap();
    md.push('\n');
    writeln!(md, "This documentation is generated locally. Please submit issues to [tinymist](https://github.com/Myriad-Dreamin/tinymist/issues) if you see **incorrect** information in it.").unwrap();
    md.push('\n');
    md.push('\n');

    let manifest = get_manifest(world, toml_id)?;

    let meta = PackageMeta {
        namespace: spec.namespace.clone(),
        name: spec.name.clone(),
        version: spec.version.to_string(),
        manifest: Some(manifest),
    };
    let package_meta = jbase64(&meta);
    let _ = writeln!(md, "<!-- begin:package {package_meta} -->");

    let mut modules_to_generate = vec![(root.head.name.clone(), root)];
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

    // todo: extend this cache idea for all crate?
    #[allow(clippy::mutable_key_type)]
    let mut describe_cache = HashMap::<Ty, String>::new();
    let mut doc_ty = |ty: Option<&Ty>| {
        let ty = ty?;
        let short = {
            describe_cache
                .entry(ty.clone())
                .or_insert_with(|| ty.describe().unwrap_or_else(|| "unknown".to_string()))
                .clone()
        };

        Some((short, format!("{ty:?}")))
    };

    while !modules_to_generate.is_empty() {
        for (parent_ident, sym) in std::mem::take(&mut modules_to_generate) {
            // parent_ident, symbols
            let symbols = sym.children;

            let module_val = sym.head.value.as_ref().unwrap();
            let module = match module_val {
                Value::Module(m) => m,
                _ => todo!(),
            };
            let fid = module.file_id();
            let aka = fid.map(&mut akas).unwrap_or_default();

            // It is (primary) known to safe as a part of HTML string, so we don't have to
            // do sanitization here.
            let primary = aka.first().cloned().unwrap_or_default();
            if !primary.is_empty() {
                let _ = writeln!(md, "---\n## Module: {primary}");
            }

            log::debug!("module: {primary} -- {parent_ident}");

            let type_info = None.or_else(|| {
                let file_id = fid?;
                let src = world.source(file_id).ok()?;
                let def_use = ctx.def_use(src.clone())?;
                let ty_chck = ctx.type_check(src)?;
                Some((def_use, ty_chck))
            });
            let type_info = type_info.as_ref();

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
                name: sym.head.name.clone(),
                loc: persist_fid,
                parent_ident: parent_ident.clone(),
                aka,
            });
            let _ = writeln!(md, "<!-- begin:module {primary} {m} -->");

            for mut sym in symbols {
                let span = sym.head.span.and_then(|v| {
                    v.id().and_then(|e| {
                        let fid = file_ids.insert_full(e).0;
                        let src = world.source(e).ok()?;
                        let rng = src.range(v)?;
                        Some((fid, rng.start, rng.end))
                    })
                });
                let sym_fid = sym.head.fid;
                let sym_fid = sym_fid.or_else(|| sym.head.span.and_then(Span::id)).or(fid);
                let span = span.or_else(|| {
                    let fid = sym_fid?;
                    Some((file_ids.insert_full(fid).0, 0, 0))
                });
                sym.head.loc = span;

                let sym_value = sym.head.value.clone();
                let signature = sym_value.and_then(|e| {
                    let def_ident = IdentRef {
                        name: sym.head.name.clone(),
                        range: sym.head.name_range.clone()?,
                    };
                    docs_signature(ctx, type_info, Some(&def_ident), &e, Some(&mut doc_ty))
                });
                sym.head.signature = signature;

                let mut convert_err = None;
                if let Some(docs) = &sym.head.docs {
                    match convert_docs(world, docs) {
                        Ok(content) => {
                            let docs = identify_docs(sym.head.kind.as_str(), &content)
                                .unwrap_or(RawDocs::Plain(content));

                            sym.head.parsed_docs = Some(docs.clone());
                            sym.head.docs = None;
                        }
                        Err(e) => {
                            let err = format!("failed to convert docs in {title}: {e}").replace(
                                "-->", "—>", // avoid markdown comment
                            );
                            log::error!("{err}");
                            convert_err = Some(err);
                        }
                    }
                }

                let ident = if !primary.is_empty() {
                    eco_format!("symbol-{}-{primary}.{}", sym.head.kind, sym.head.name)
                } else {
                    eco_format!("symbol-{}-{}", sym.head.kind, sym.head.name)
                };
                let _ = writeln!(md, "### {}: {} in {primary}", sym.head.kind, sym.head.name);

                if sym.head.export_again {
                    let sub_fid = sym.head.fid;
                    if let Some(fid) = sub_fid {
                        let lnk = if fid.package() == Some(&for_spec) {
                            let sub_aka = akas(fid);
                            let sub_primary = sub_aka.first().cloned().unwrap_or_default();
                            sym.head.external_link = Some(format!(
                                "#symbol-{}-{sub_primary}.{}",
                                sym.head.kind, sym.head.name
                            ));
                            format!("#{}-{}-in-{sub_primary}", sym.head.kind, sym.head.name)
                                .replace(".", "")
                        } else if let Some(spec) = fid.package() {
                            let lnk = format!(
                                "https://typst.app/universe/package/{}/{}",
                                spec.name, spec.version
                            );
                            sym.head.external_link = Some(lnk.clone());
                            lnk
                        } else {
                            let lnk: String = "https://typst.app/docs".into();
                            sym.head.external_link = Some(lnk.clone());
                            lnk
                        };
                        let _ = writeln!(md, "[Symbol Docs]({lnk})\n");
                    }
                }

                let head = jbase64(&sym.head);
                let _ = writeln!(md, "<!-- begin:symbol {ident} {head} -->");

                if let Some(sig) = &sym.head.signature {
                    let _ = writeln!(md, "<!-- begin:sig -->");
                    let _ = writeln!(md, "```typc");
                    let _ = writeln!(md, "let {name}({sig});", name = sym.head.name);
                    let _ = writeln!(md, "```");
                    let _ = writeln!(md, "<!-- end:sig -->");
                }

                match (&sym.head.parsed_docs, convert_err) {
                    (_, Some(err)) => {
                        let err = format!("failed to convert docs in {title}: {err}").replace(
                            "-->", "—>", // avoid markdown comment
                        );
                        let _ = writeln!(md, "<!-- convert-error: {err} -->");
                        errors.push(err);
                    }
                    (Some(docs), _) => {
                        let _ = writeln!(md, "{}", remove_list_annotations(docs.docs()));
                        if let RawDocs::Function(f) = docs {
                            for param in &f.params {
                                let _ = writeln!(md, "<!-- begin:param {} -->", param.name);
                                let _ = writeln!(
                                    md,
                                    "#### {} ({})\n<!-- begin:param-doc {} -->\n{}\n<!-- end:param-doc {} -->",
                                    param.name, param.types, param.name, param.docs, param.name
                                );
                                let _ = writeln!(md, "<!-- end:param -->");
                            }
                        }
                    }
                    (None, None) => {}
                }

                let plain_docs = sym.head.docs.as_deref();
                let plain_docs = plain_docs.or(sym.head.oneliner.as_deref());

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

                if !sym.children.is_empty() {
                    let sub_fid = sym.head.fid;
                    log::debug!("sub_fid: {sub_fid:?}");
                    match sub_fid {
                        Some(fid) => {
                            let aka = akas(fid);
                            let primary = aka.first().cloned().unwrap_or_default();
                            let link = format!("module-{primary}").replace(".", "");
                            let _ = writeln!(md, "[Module Docs](#{link})\n");

                            if generated_modules.insert(fid) {
                                modules_to_generate.push((ident.clone(), sym));
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
    for e in res.errors {
        let _ = writeln!(md, "- {e}");
    }
    let _ = writeln!(md, "<!-- end:errors -->");

    let mut packages = IndexSet::new();

    let files = file_ids
        .into_iter()
        .map(|e| {
            let pkg = e.package().map(|e| packages.insert_full(e.clone()).0);

            FileMeta {
                package: pkg,
                path: e.vpath().as_rootless_path().to_owned(),
            }
        })
        .collect();

    let packages = packages
        .into_iter()
        .map(|e| PackageMeta {
            namespace: e.namespace.clone(),
            name: e.name.clone(),
            version: e.version.to_string(),
            manifest: None,
        })
        .collect();

    let meta = PackageMetaEnd { packages, files };
    let package_meta = jbase64(&meta);
    let _ = writeln!(md, "<!-- end:package {package_meta} -->");

    Ok(md)
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

fn file_id_repr(k: FileId) -> String {
    if let Some(p) = k.package() {
        format!("{p}{}", unix_slash(k.vpath().as_rooted_path()))
    } else {
        unix_slash(k.vpath().as_rooted_path())
    }
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

    use super::{generate_md_docs, PackageInfo};
    use crate::tests::*;

    fn test(pkg: PackageSpec) {
        run_with_sources("", |verse: &mut LspUniverse, p| {
            let w = verse.snapshot();
            let path = verse.registry.resolve(&pkg).unwrap();
            let pi = PackageInfo {
                path: path.as_ref().to_owned(),
                namespace: pkg.namespace,
                name: pkg.name,
                version: pkg.version.to_string(),
            };
            run_with_ctx(verse, p, &|a, _p| {
                let d = generate_md_docs(a, &w, &pi).unwrap();
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
