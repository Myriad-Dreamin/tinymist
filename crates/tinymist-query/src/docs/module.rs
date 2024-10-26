//! Module documentation.

use std::collections::HashMap;
use std::ops::Range;

use ecow::{eco_vec, EcoString, EcoVec};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use typst::diag::{eco_format, StrResult};
use typst::foundations::{Module, Value};
use typst::syntax::package::PackageSpec;
use typst::syntax::{FileId, Span};

use crate::docs::file_id_repr;
use crate::syntax::{find_docs_of, get_non_strict_def_target};
use crate::upstream::truncated_doc_repr;
use crate::AnalysisContext;

use super::{get_manifest, get_manifest_id, kind_of, DocStringKind, PackageInfo, SymbolDocs};

/// Get documentation of symbols in a package.
pub fn package_module_docs(ctx: &mut AnalysisContext, pkg: &PackageInfo) -> StrResult<SymbolsInfo> {
    let toml_id = get_manifest_id(pkg)?;
    let manifest = get_manifest(ctx.world(), toml_id)?;

    let entry_point = toml_id.join(&manifest.package.entrypoint);
    module_docs(ctx, entry_point)
}

/// Get documentation of symbols in a module.
pub fn module_docs(ctx: &mut AnalysisContext, entry_point: FileId) -> StrResult<SymbolsInfo> {
    let mut aliases = HashMap::new();
    let mut extras = vec![];

    let mut scan_ctx = ScanSymbolCtx {
        ctx,
        root: entry_point,
        for_spec: entry_point.package(),
        aliases: &mut aliases,
        extras: &mut extras,
    };

    let src = scan_ctx
        .ctx
        .module_by_id(entry_point)
        .map_err(|e| eco_format!("failed to get module by id {entry_point:?}: {e:?}"))?;
    let mut symbols = scan_ctx.module_sym(eco_vec![], src);

    let module_uses = aliases
        .into_iter()
        .map(|(k, mut v)| {
            v.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
            (file_id_repr(k), v.into())
        })
        .collect();

    log::debug!("module_uses: {module_uses:#?}",);

    symbols.children.extend(extras);

    Ok(SymbolsInfo {
        root: symbols,
        module_uses,
    })
}

/// Information about a symbol.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SymbolInfoHead {
    /// The name of the symbol.
    pub name: EcoString,
    /// The kind of the symbol.
    pub kind: DocStringKind,
    /// The location (file, start, end) of the symbol.
    pub loc: Option<(usize, usize, usize)>,
    /// Is the symbol reexport
    pub export_again: bool,
    /// Is the symbol reexport
    pub external_link: Option<String>,
    /// The one-line documentation of the symbol.
    pub oneliner: Option<String>,
    /// The raw documentation of the symbol.
    pub docs: Option<String>,
    /// The parsed documentation of the symbol.
    pub parsed_docs: Option<SymbolDocs>,
    /// The value of the symbol.
    #[serde(skip)]
    pub constant: Option<EcoString>,
    /// The file owning the symbol.
    #[serde(skip)]
    pub fid: Option<FileId>,
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

/// Information about the symbols in a package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolsInfo {
    /// The root module information.
    #[serde(flatten)]
    pub root: SymbolInfo,
    /// The module accessible paths.
    pub module_uses: HashMap<String, EcoVec<String>>,
}

struct ScanSymbolCtx<'a, 'w> {
    ctx: &'a mut AnalysisContext<'w>,
    for_spec: Option<&'a PackageSpec>,
    aliases: &'a mut HashMap<FileId, Vec<String>>,
    extras: &'a mut Vec<SymbolInfo>,
    root: FileId,
}

impl ScanSymbolCtx<'_, '_> {
    fn module_sym(&mut self, path: EcoVec<&str>, module: Module) -> SymbolInfo {
        let key = module.name().to_owned();
        let site = Some(self.root);
        let p = path.clone();
        self.sym(&key, p, site.as_ref(), &Value::Module(module))
    }

    fn sym(
        &mut self,
        key: &str,
        path: EcoVec<&str>,
        site: Option<&FileId>,
        val: &Value,
    ) -> SymbolInfo {
        let mut head = create_head(self.ctx, key, val);

        if !matches!(&val, Value::Module(..)) {
            if let Some((span, mod_fid)) = head.span.and_then(Span::id).zip(site) {
                if span != *mod_fid {
                    head.export_again = true;
                    head.oneliner = head.docs.as_deref().map(oneliner).map(|e| e.to_owned());
                    head.docs = None;
                }
            }
        }

        let children = match val {
            Value::Module(module) => module.file_id().and_then(|fid| {
                // only generate docs for the same package
                if fid.package() != self.for_spec {
                    return None;
                }

                // !aliases.insert(fid)
                let aliases_vec = self.aliases.entry(fid).or_default();
                let is_fresh = aliases_vec.is_empty();
                aliases_vec.push(path.iter().join("."));

                if !is_fresh {
                    log::debug!("found module: {path:?} (reexport)");
                    return None;
                }

                log::debug!("found module: {path:?}");

                let symbols = module.scope().iter();
                let symbols = symbols
                    .map(|(k, v, _)| {
                        let mut path = path.clone();
                        path.push(k);
                        self.sym(k, path.clone(), Some(&fid), v)
                    })
                    .collect();
                Some(symbols)
            }),
            _ => None,
        };

        // Insert module that is not exported
        if let Some(fid) = head.fid {
            // only generate docs for the same package
            if fid.package() == self.for_spec {
                let av = self.aliases.entry(fid).or_default();
                if av.is_empty() {
                    let m = self.ctx.module_by_id(fid);
                    let mut path = path.clone();
                    path.push("-");
                    path.push(key);

                    log::debug!("found internal module: {path:?}");
                    if let Ok(m) = m {
                        let msym = self.module_sym(path, m);
                        self.extras.push(msym)
                    }
                }
            }
        }

        let children = children.unwrap_or_default();
        SymbolInfo { head, children }
    }
}

fn create_head(world: &mut AnalysisContext, k: &str, v: &Value) -> SymbolInfoHead {
    let kind = kind_of(v);
    let (docs, name_range, fid, span) = match v {
        Value::Func(f) => {
            let mut span = None;
            let mut name_range = None;
            let docs = None.or_else(|| {
                let source = world.source_by_id(f.span().id()?).ok()?;
                let node = source.find(f.span())?;
                log::debug!("node: {k} -> {:?}", node.parent());
                // use parent of params, todo: reliable way to get the def target
                let def = get_non_strict_def_target(node.parent()?.clone())?;
                span = Some(def.node().span());
                name_range = def.name_range();

                find_docs_of(&source, def)
            });

            let s = span.or(Some(f.span()));

            (docs, name_range, s.and_then(Span::id), s)
        }
        Value::Module(m) => (None, None, m.file_id(), None),
        _ => Default::default(),
    };

    SymbolInfoHead {
        name: k.to_string().into(),
        kind,
        constant: None.or_else(|| match v {
            Value::Func(_) => None,
            t => Some(truncated_doc_repr(t)),
        }),
        docs,
        name_range,
        fid,
        span,
        value: Some(v.clone()),
        ..Default::default()
    }
}

/// Extract the first line of documentation.
fn oneliner(docs: &str) -> &str {
    docs.lines().next().unwrap_or_default()
}
