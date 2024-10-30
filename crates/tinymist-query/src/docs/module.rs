//! Module documentation.

use std::collections::HashMap;
use std::sync::Arc;

use ecow::{eco_vec, EcoString, EcoVec};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use typst::diag::StrResult;
use typst::syntax::package::PackageSpec;
use typst::syntax::FileId;

use crate::docs::file_id_repr;
use crate::syntax::{Decl, DefKind, Expr, ExprInfo};
use crate::ty::Interned;
use crate::AnalysisContext;

use super::{get_manifest_id, DefDocs, PackageInfo};

/// Get documentation of symbols in a package.
pub fn package_module_docs(ctx: &mut AnalysisContext, pkg: &PackageInfo) -> StrResult<SymbolsInfo> {
    let toml_id = get_manifest_id(pkg)?;
    let manifest = ctx.get_manifest(toml_id)?;

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

    let ei = scan_ctx
        .ctx
        .expr_stage_by_id(entry_point)
        .ok_or("entry point not found")?;
    let mut symbols = scan_ctx.module_sym(eco_vec![], ei);

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
    pub kind: DefKind,
    /// The location (file, start, end) of the symbol.
    pub loc: Option<(usize, usize, usize)>,
    /// Is the symbol reexport
    pub export_again: bool,
    /// Is the symbol reexport
    pub external_link: Option<String>,
    /// The one-line documentation of the symbol.
    pub oneliner: Option<String>,
    /// The raw documentation of the symbol.
    pub docs: Option<EcoString>,
    /// The parsed documentation of the symbol.
    pub parsed_docs: Option<DefDocs>,
    /// The value of the symbol.
    #[serde(skip)]
    pub constant: Option<EcoString>,
    /// The name range of the symbol.
    /// The value of the symbol.
    #[serde(skip)]
    pub decl: Option<Interned<Decl>>,
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
    fn module_sym(&mut self, path: EcoVec<&str>, ei: Arc<ExprInfo>) -> SymbolInfo {
        let name = {
            let stem = ei.fid.vpath().as_rooted_path().file_stem();
            stem.and_then(|s| Some(Interned::new_str(s.to_str()?)))
                .unwrap_or_default()
        };
        let module_decl = Decl::module(name.clone(), ei.fid).into();
        let site = Some(self.root);
        let p = path.clone();
        self.sym(&name, p, site.as_ref(), &module_decl, None)
    }

    fn expr(
        &mut self,
        key: &str,
        path: EcoVec<&str>,
        site: Option<&FileId>,
        val: &Expr,
    ) -> SymbolInfo {
        match val {
            Expr::Decl(d) => self.sym(key, path, site, d, Some(val)),
            Expr::Ref(r) if r.root.is_some() => {
                self.expr(key, path, site, r.root.as_ref().unwrap())
            }
            // todo: select
            Expr::Select(..) => {
                let mut path = path.clone();
                path.push(key);
                let head = SymbolInfoHead {
                    name: key.to_string().into(),
                    kind: DefKind::Module,
                    ..Default::default()
                };
                SymbolInfo {
                    head,
                    children: eco_vec![],
                }
            }
            // v => panic!("unexpected export: {key} -> {v}"),
            _ => {
                let mut path = path.clone();
                path.push(key);
                let head = SymbolInfoHead {
                    name: key.to_string().into(),
                    kind: DefKind::Constant,
                    ..Default::default()
                };
                SymbolInfo {
                    head,
                    children: eco_vec![],
                }
            }
        }
    }

    fn sym(
        &mut self,
        key: &str,
        path: EcoVec<&str>,
        site: Option<&FileId>,
        val: &Interned<Decl>,
        expr: Option<&Expr>,
    ) -> SymbolInfo {
        let mut head = create_head(self.ctx, key, val, expr);

        if !matches!(val.as_ref(), Decl::Module(..)) {
            if let Some((span, mod_fid)) = head.decl.as_ref().and_then(|d| d.file_id()).zip(site) {
                if span != *mod_fid {
                    head.export_again = true;
                    head.oneliner = head.docs.as_deref().map(oneliner).map(|e| e.to_owned());
                    head.docs = None;
                }
            }
        }

        let children = match val.as_ref() {
            Decl::Module(..) => val.file_id().and_then(|fid| {
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

                let ei = self.ctx.expr_stage_by_id(fid)?;

                let symbols = ei
                    .exports
                    .iter()
                    .map(|(k, v)| {
                        let mut path = path.clone();
                        path.push(k);
                        self.expr(k, path.clone(), Some(&fid), v)
                    })
                    .collect();
                Some(symbols)
            }),
            _ => None,
        };

        // Insert module that is not exported
        if let Some(fid) = head.decl.as_ref().and_then(|d| d.file_id()) {
            // only generate docs for the same package
            if fid.package() == self.for_spec {
                let av = self.aliases.entry(fid).or_default();
                if av.is_empty() {
                    let src = self.ctx.expr_stage_by_id(fid);
                    let mut path = path.clone();
                    path.push("-");
                    path.push(key);

                    log::debug!("found internal module: {path:?}");
                    if let Some(m) = src {
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

fn create_head(
    ctx: &mut AnalysisContext,
    k: &str,
    decl: &Interned<Decl>,
    expr: Option<&Expr>,
) -> SymbolInfoHead {
    let kind = decl.kind();

    let parsed_docs = ctx.def_of_decl(decl).and_then(|def| ctx.def_docs(&def));
    let docs = parsed_docs.as_ref().map(|d| d.docs().clone());

    SymbolInfoHead {
        name: k.to_string().into(),
        kind,
        constant: expr.map(|e| e.repr()),
        docs,
        parsed_docs,
        decl: Some(decl.clone()),
        ..Default::default()
    }
}

/// Extract the first line of documentation.
fn oneliner(docs: &str) -> &str {
    docs.lines().next().unwrap_or_default()
}
