//! Package management tools.

mod library;
mod tidy;

use core::fmt::{self, Write};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use comemo::Track;
use ecow::{eco_vec, EcoString, EcoVec};
use indexmap::IndexSet;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tinymist_world::base::{EntryState, ShadowApi, TaskInputs};
use tinymist_world::LspWorld;
use typst::diag::{eco_format, StrResult};
use typst::engine::Route;
use typst::eval::Tracer;
use typst::foundations::{Bytes, Value};
use typst::syntax::package::{PackageManifest, PackageSpec};
use typst::syntax::{FileId, Span, VirtualPath};
use typst::World;

use self::tidy::*;
use crate::analysis::analyze_dyn_signature;
use crate::syntax::{find_docs_of, get_non_strict_def_target, IdentRef};
use crate::ty::Ty;
use crate::upstream::truncated_doc_repr;
use crate::AnalysisContext;

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

/// Docs about a symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Docs {
    /// Docs about a function.
    #[serde(rename = "func")]
    Function(TidyFuncDocs),
    /// Docs about a variable.
    #[serde(rename = "var")]
    Variable(TidyVarDocs),
    /// Docs about a module.
    #[serde(rename = "module")]
    Module(TidyModuleDocs),
    /// Other kinds of docs.
    #[serde(rename = "plain")]
    Plain(EcoString),
}

impl Docs {
    /// Get the markdown representation of the docs.
    pub fn docs(&self) -> &str {
        match self {
            Self::Function(docs) => docs.docs.as_str(),
            Self::Variable(docs) => docs.docs.as_str(),
            Self::Module(docs) => docs.docs.as_str(),
            Self::Plain(docs) => docs.as_str(),
        }
    }
}

/// Describes a primary function signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocSignature {
    /// The positional parameters.
    pub pos: Vec<DocParamSpec>,
    /// The named parameters.
    pub named: HashMap<String, DocParamSpec>,
    /// The rest parameter.
    pub rest: Option<DocParamSpec>,
    /// The return type.
    pub ret_ty: Option<(String, String)>,
}

/// Describes a function parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocParamSpec {
    /// The parameter's name.
    pub name: String,
    /// Documentation for the parameter.
    pub docs: String,
    /// Inferred type of the parameter.
    pub cano_type: Option<(String, String)>,
    /// The parameter's default name as type.
    pub type_repr: Option<EcoString>,
    /// The parameter's default name as value.
    pub expr: Option<EcoString>,
    /// Is the parameter positional?
    pub positional: bool,
    /// Is the parameter named?
    ///
    /// Can be true even if `positional` is true if the parameter can be given
    /// in both variants.
    pub named: bool,
    /// Can the parameter be given any number of times?
    pub variadic: bool,
    /// Is the parameter settable with a set rule?
    pub settable: bool,
}

/// Information about a symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInfoHead {
    /// The name of the symbol.
    pub name: EcoString,
    /// The kind of the symbol.
    pub kind: EcoString,
    /// The location (file, start, end) of the symbol.
    pub loc: Option<(usize, usize, usize)>,
    /// The raw documentation of the symbol.
    pub docs: Option<String>,
    /// The signature of the symbol.
    pub signature: Option<DocSignature>,
    /// The parsed documentation of the symbol.
    pub parsed_docs: Option<Docs>,
    /// The value of the symbol.
    #[serde(skip)]
    pub constant: Option<EcoString>,
    /// The span of the symbol.
    #[serde(skip)]
    pub span: Option<Span>,
    /// The name range of the symbol.
    #[serde(skip)]
    pub name_range: Option<Range<usize>>,
    /// The value of the symbol.
    #[serde(skip)]
    pub value: Option<Value>,
}

/// Information about a symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInfo {
    /// The primary information about the symbol.
    #[serde(flatten)]
    pub head: SymbolInfoHead,
    /// The children of the symbol.
    pub children: EcoVec<SymbolInfo>,
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

/// Parses the manifest of the package located at `package_path`.
pub fn get_manifest(world: &LspWorld, toml_id: FileId) -> StrResult<PackageManifest> {
    let toml_data = world
        .file(toml_id)
        .map_err(|err| eco_format!("failed to read package manifest ({})", err))?;

    let string = std::str::from_utf8(&toml_data)
        .map_err(|err| eco_format!("package manifest is not valid UTF-8 ({})", err))?;

    toml::from_str(string)
        .map_err(|err| eco_format!("package manifest is malformed ({})", err.message()))
}

/// List all symbols in a package.
pub fn list_symbols(world: &LspWorld, spec: &PackageInfo) -> StrResult<SymbolInfo> {
    let toml_id = FileId::new(
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
    Ok(symbol(world, Some(&for_spec), "root", &Value::Module(src)))
}

fn jbase64<T: Serialize>(s: &T) -> String {
    use base64::Engine;
    let content = serde_json::to_string(s).unwrap();
    base64::engine::general_purpose::STANDARD.encode(content)
}

// Unfortunately, we have only 65536 possible file ids and we cannot revoke
// them. So we share a global file id for all docs conversion.
static DOCS_CONVERT_ID: std::sync::LazyLock<Mutex<FileId>> = std::sync::LazyLock::new(|| {
    Mutex::new(FileId::new(None, VirtualPath::new("__tinymist_docs__.typ")))
});

fn convert_docs(world: &LspWorld, content: &str) -> StrResult<EcoString> {
    static DOCS_LIB: std::sync::LazyLock<Arc<typlite::scopes::Scopes<typlite::value::Value>>> =
        std::sync::LazyLock::new(library::lib);

    let conv_id = DOCS_CONVERT_ID.lock();
    let entry = EntryState::new_rootless(conv_id.vpath().as_rooted_path().into()).unwrap();
    let entry = entry.select_in_workspace(*conv_id);

    let mut w = world.task(TaskInputs {
        entry: Some(entry),
        inputs: None,
    });
    w.map_shadow_by_id(*conv_id, Bytes::from(content.as_bytes().to_owned()))?;
    // todo: bad performance
    w.source_db.take_state();

    let conv = typlite::Typlite::new(Arc::new(w))
        .with_library(DOCS_LIB.clone())
        .annotate_elements(true)
        .convert()
        .map_err(|e| eco_format!("failed to convert to markdown: {e}"))?;

    Ok(conv)
}

#[derive(Serialize, Deserialize)]
struct ConvertResult {
    errors: Vec<String>,
}

/// Generate full documents in markdown format
pub fn generate_md_docs(
    ctx: &mut AnalysisContext,
    world: &LspWorld,
    spec: &PackageInfo,
) -> StrResult<String> {
    log::info!("generate_md_docs {spec:?}");
    let toml_id = FileId::new(
        Some(PackageSpec {
            namespace: spec.namespace.clone(),
            name: spec.name.clone(),
            version: spec.version.parse()?,
        }),
        VirtualPath::new("typst.toml"),
    );

    let mut md = String::new();
    let sym = list_symbols(world, spec)?;

    let title = format!("@{}/{}:{}", spec.namespace, spec.name, spec.version);

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

    let mut key = 0;

    let mut modules_to_generate = vec![(EcoString::new(), sym.head.name.clone(), sym)];
    let mut generated_modules = HashSet::new();
    let mut file_ids = IndexSet::new();

    while !modules_to_generate.is_empty() {
        for (prefix, parent_ident, sym) in std::mem::take(&mut modules_to_generate) {
            // parent_ident, symbols
            let symbols = sym.children;
            if !prefix.is_empty() {
                let _ = writeln!(md, "---\n## Module: {prefix}");
            }

            let module_val = sym.head.value.as_ref().unwrap();
            let module = match module_val {
                Value::Module(m) => m,
                _ => todo!(),
            };

            let fid = module.file_id();
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
            }
            let m = jbase64(&ModuleInfo {
                prefix: prefix.clone(),
                name: sym.head.name.clone(),
                loc: persist_fid,
                parent_ident: parent_ident.clone(),
            });
            let _ = writeln!(md, "<!-- begin:module {parent_ident} {m} -->");

            for mut sym in symbols {
                let span = sym.head.span.and_then(|v| {
                    v.id().and_then(|e| {
                        let fid = file_ids.insert_full(e).0;
                        let src = world.source(e).ok()?;
                        let rng = src.range(v)?;
                        Some((fid, rng.start, rng.end))
                    })
                });
                sym.head.loc = span;

                let mut convert_err = None;
                if let Some(docs) = &sym.head.docs {
                    match convert_docs(world, docs) {
                        Ok(content) => {
                            let docs = match sym.head.kind.as_str() {
                                "function" => {
                                    let t = identify_tidy_func_docs(&content).ok();
                                    t.map(Docs::Function).unwrap_or(Docs::Plain(content))
                                }
                                "variable" => {
                                    let t = identify_tidy_var_docs(&content).ok();
                                    t.map(Docs::Variable).unwrap_or(Docs::Plain(content))
                                }
                                "module" => {
                                    let t = identify_tidy_module_docs(&content).ok();
                                    t.map(Docs::Module).unwrap_or(Docs::Plain(content))
                                }
                                _ => Docs::Plain(content),
                            };

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

                let signature =
                    match &sym.head.parsed_docs {
                        Some(Docs::Function(TidyFuncDocs {
                            params, return_ty, ..
                        })) => sym.head.value.clone().and_then(|e| {
                            let func = match e {
                                Value::Func(f) => f,
                                _ => return None,
                            };
                            let sig = analyze_dyn_signature(ctx, func.clone());
                            let type_sig = type_info.and_then(|(def_use, ty_chk)| {
                                let def_fid = func.span().id()?;
                                let def_ident = IdentRef {
                                    name: sym.head.name.clone(),
                                    range: sym.head.name_range.clone()?,
                                };
                                let (def_id, _) = def_use.get_def(def_fid, &def_ident)?;
                                ty_chk.type_of_def(def_id)
                            });
                            let type_sig = type_sig.and_then(|type_sig| type_sig.sig_repr(true));

                            let pos_in = sig.primary().pos.iter().enumerate().map(|(i, pos)| {
                                (pos, type_sig.as_ref().and_then(|sig| sig.pos(i)))
                            });
                            let named_in = sig
                                .primary()
                                .named
                                .iter()
                                .map(|x| (x, type_sig.as_ref().and_then(|sig| sig.named(x.0))));
                            let rest_in =
                                sig.primary().rest.as_ref().map(|x| {
                                    (x, type_sig.as_ref().and_then(|sig| sig.rest_param()))
                                });

                            let ret_in = type_sig
                                .as_ref()
                                .and_then(|sig| sig.body.as_ref())
                                .or_else(|| sig.primary().ret_ty.as_ref());

                            let doc_ty = |ty: Option<&Ty>| {
                                ty.and_then(|ty| ty.describe().map(|e| (e, format!("{ty:?}"))))
                            };

                            let _ = params;
                            let _ = return_ty;

                            let pos = pos_in
                                .map(|(param, ty)| DocParamSpec {
                                    name: param.name.as_ref().to_owned(),
                                    docs: param.docs.as_ref().to_owned(),
                                    cano_type: doc_ty(ty),
                                    type_repr: param.type_repr.clone(),
                                    expr: param.expr.clone(),
                                    positional: param.positional,
                                    named: param.named,
                                    variadic: param.variadic,
                                    settable: param.settable,
                                })
                                .collect();

                            let named = named_in
                                .map(|((name, param), ty)| {
                                    (
                                        name.as_ref().to_owned(),
                                        DocParamSpec {
                                            name: param.name.as_ref().to_owned(),
                                            docs: param.docs.as_ref().to_owned(),
                                            cano_type: doc_ty(ty),
                                            type_repr: param.type_repr.clone(),
                                            expr: param.expr.clone(),
                                            positional: param.positional,
                                            named: param.named,
                                            variadic: param.variadic,
                                            settable: param.settable,
                                        },
                                    )
                                })
                                .collect();

                            let rest = rest_in.map(|(param, ty)| DocParamSpec {
                                name: param.name.as_ref().to_owned(),
                                docs: param.docs.as_ref().to_owned(),
                                cano_type: doc_ty(ty),
                                type_repr: param.type_repr.clone(),
                                expr: param.expr.clone(),
                                positional: param.positional,
                                named: param.named,
                                variadic: param.variadic,
                                settable: param.settable,
                            });

                            let ret_ty = doc_ty(ret_in);

                            Some(DocSignature {
                                pos,
                                named,
                                rest,
                                ret_ty,
                            })
                        }),
                        _ => None,
                    };

                sym.head.signature = signature;

                let _ = writeln!(md, "### {}: {}", sym.head.kind, sym.head.name);

                let ident = eco_format!("symbol-{}-{}-{key}", sym.head.kind, sym.head.name);
                key += 1;
                let head = jbase64(&sym.head);
                let _ = writeln!(md, "<!-- begin:symbol {ident} {head} -->");

                if let Some(sig) = &sym.head.signature {
                    let _ = writeln!(md, "<!-- begin:sig -->");
                    let _ = writeln!(md, "```typc");
                    let _ = writeln!(
                        md,
                        "let {name}({params});",
                        name = sym.head.name,
                        params = ParamTooltip(sig)
                    );
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
                        if let Docs::Function(f) = docs {
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

                if let Some(docs) = &sym.head.docs {
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
                    full_path.push_str(&sym.head.name);
                    let link = format!("Module-{full_path}").replace(".", "-");
                    let _ = writeln!(md, "[Module Docs](#{link})\n");

                    if generated_modules.insert(full_path.clone()) {
                        modules_to_generate.push((full_path, ident.clone(), sym));
                    }
                }

                let _ = writeln!(md, "<!-- end:symbol {ident} -->");
            }

            let _ = writeln!(md, "<!-- end:module {parent_ident} -->");
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

fn symbol(world: &LspWorld, for_spec: Option<&PackageSpec>, key: &str, val: &Value) -> SymbolInfo {
    let children = match val {
        Value::Module(module) => {
            // only generate docs for the same package
            if module.file_id().map_or(true, |e| e.package() != for_spec) {
                eco_vec![]
            } else {
                let symbols = module.scope().iter();
                symbols
                    .map(|(k, v)| symbol(world, for_spec, k, v))
                    .collect()
            }
        }
        _ => eco_vec![],
    };
    SymbolInfo {
        head: create_head(world, key, val),
        children,
    }
}

fn create_head(world: &LspWorld, k: &str, v: &Value) -> SymbolInfoHead {
    let kind = kind_of(v);
    let (docs, name_range, span) = match v {
        Value::Func(f) => {
            let mut span = None;
            let mut name_range = None;
            let docs = None.or_else(|| {
                let source = world.source(f.span().id()?).ok()?;
                let node = source.find(f.span())?;
                log::debug!("node: {k} -> {:?}", node.parent());
                // use parent of params, todo: reliable way to get the def target
                let def = get_non_strict_def_target(node.parent()?.clone())?;
                span = Some(def.node().span());
                name_range = def.name_range();

                find_docs_of(&source, def)
            });

            (docs, name_range, span.or(Some(f.span())))
        }
        _ => (None, None, None),
    };

    SymbolInfoHead {
        name: k.to_string().into(),
        kind,
        loc: None,
        constant: None.or_else(|| match v {
            Value::Func(_) => None,
            t => Some(truncated_doc_repr(t)),
        }),
        signature: None,
        parsed_docs: None,
        docs,
        name_range,
        span,
        value: Some(v.clone()),
    }
}

// todo: hover with `with_stack`
struct ParamTooltip<'a>(&'a DocSignature);

impl<'a> fmt::Display for ParamTooltip<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut is_first = true;
        let mut write_sep = |f: &mut fmt::Formatter<'_>| {
            if is_first {
                is_first = false;
                return Ok(());
            }
            f.write_str(", ")
        };

        let primary_sig = self.0;

        for p in &primary_sig.pos {
            write_sep(f)?;
            write!(f, "{}", p.name)?;
        }
        if let Some(rest) = &primary_sig.rest {
            write_sep(f)?;
            write!(f, "{}", rest.name)?;
        }

        if !primary_sig.named.is_empty() {
            let mut name_prints = vec![];
            for v in primary_sig.named.values() {
                name_prints.push((v.name.clone(), v.type_repr.clone()))
            }
            name_prints.sort();
            for (k, v) in name_prints {
                write_sep(f)?;
                let v = v.as_deref().unwrap_or("any");
                let mut v = v.trim();
                if v.starts_with('{') && v.ends_with('}') && v.len() > 30 {
                    v = "{ ... }"
                }
                if v.starts_with('`') && v.ends_with('`') && v.len() > 30 {
                    v = "raw"
                }
                if v.starts_with('[') && v.ends_with(']') && v.len() > 30 {
                    v = "content"
                }
                write!(f, "{k}: {v}")?;
            }
        }

        Ok(())
    }
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
    use reflexo_typst::{
        package::{PackageRegistry, PackageSpec},
        TypstSystemUniverse,
    };

    use super::{generate_md_docs, PackageInfo};
    use crate::tests::*;

    fn test(pkg: PackageSpec) {
        run_with_sources("", |verse: &mut TypstSystemUniverse, p| {
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
