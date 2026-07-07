//! Module documentation.

use std::collections::HashMap;
use std::sync::Arc;

use ecow::{EcoString, EcoVec, eco_vec};
use itertools::Itertools;
use lsp_types::Position;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use typst::diag::StrResult;
use typst::syntax::FileId;
use typst::syntax::VirtualRoot;
use typst::syntax::package::PackageSpec;
use typst_shim::syntax::RootedPathExt;

use crate::LocalContext;
use crate::adt::interner::Interned;
use crate::analysis::{Definition, SharedQueryCache};
use crate::docs::file_id_repr;
use crate::package::{PackageInfo, get_manifest_id, package_entrypoint_id};
use crate::syntax::{Decl, DefKind, Expr, ExprInfo};

use super::DefDocs;

/// Get documentation of definitions in a package.
pub fn package_module_docs(ctx: &mut LocalContext, pkg: &PackageInfo) -> StrResult<PackageDefInfo> {
    let toml_id = get_manifest_id(pkg)?;
    let manifest = ctx.get_manifest(toml_id)?;

    let entry_point = package_entrypoint_id(toml_id, &manifest.package.entrypoint);
    module_docs(ctx, entry_point)
}

/// Get documentation of definitions in a module.
pub fn module_docs(ctx: &mut LocalContext, entry_point: FileId) -> StrResult<PackageDefInfo> {
    let mut aliases = HashMap::new();
    let mut extras = vec![];
    let shared = ctx.shared().clone();

    let mut scan_ctx = ScanDefCtx {
        ctx,
        root: entry_point,
        for_spec: entry_point.package_compat(),
        aliases: &mut aliases,
        extras: &mut extras,
    };

    let ei = scan_ctx
        .ctx
        .expr_stage_by_id(entry_point)
        .ok_or("entry point not found")?;
    let docs_cache = SharedQueryCache::<Definition, Option<DefDocs>>::default();
    let mut defs = enrich_def_docs_parallel(
        shared.clone(),
        docs_cache.clone(),
        scan_ctx.defs(eco_vec![], ei),
    );

    let module_uses = aliases
        .into_iter()
        .map(|(fid, mut v)| {
            v.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
            (file_id_repr(fid), v.into())
        })
        .collect();

    crate::log_debug_ct!("module_uses: {module_uses:#?}",);

    defs.children.extend(
        extras
            .into_par_iter()
            .map(|extra| enrich_def_docs_parallel(shared.clone(), docs_cache.clone(), extra))
            .collect::<Vec<_>>(),
    );

    Ok(PackageDefInfo {
        root: defs,
        module_uses,
    })
}

fn enrich_def_docs_parallel(
    shared: Arc<crate::analysis::SharedContext>,
    docs_cache: SharedQueryCache<Definition, Option<DefDocs>>,
    mut head: DefInfo,
) -> DefInfo {
    head.children = head
        .children
        .into_par_iter()
        .map(|child| enrich_def_docs_parallel(shared.clone(), docs_cache.clone(), child))
        .collect();

    let def_docs = head
        .decl
        .as_ref()
        .and_then(definition_for_docs)
        .and_then(|definition| {
            docs_cache.get_or_init(definition.clone(), || shared.def_docs(&definition))
        });
    head.docs = def_docs.as_ref().map(|docs| docs.docs().clone());
    head.parsed_docs = def_docs;

    if head.is_external {
        head.oneliner = head.docs.as_ref().map(|docs| oneliner(docs).to_owned());
        head.docs = None;
    }

    head
}

fn definition_for_docs(decl: &Interned<Decl>) -> Option<Definition> {
    match decl.as_ref() {
        Decl::Func(..) => Some(Definition::new(decl.clone(), None)),
        _ => None,
    }
}

/// Information about a definition.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DefInfo {
    /// The raw documentation of the definition.
    pub id: EcoString,
    /// The name of the definition.
    pub name: EcoString,
    /// The kind of the definition.
    pub kind: DefKind,
    /// The SCIP symbol for index-backed queries.
    #[serde(skip)]
    pub symbol: Option<String>,
    /// The location (file, start, end) of the definition.
    #[serde(skip)]
    pub loc: Option<(usize, usize, usize)>,
    /// The source position for index-backed queries.
    #[serde(skip)]
    pub source: Option<SourceQuery>,
    /// Whether the definition external to the module.
    pub is_external: bool,
    /// The module link to the definition
    pub module_link: Option<String>,
    /// The bundle-mode link to the definition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_link: Option<String>,
    /// The symbol link to the definition
    pub symbol_link: Option<String>,
    /// The link to the definition if it is external.
    pub external_link: Option<String>,
    /// The one-line documentation of the definition.
    #[serde(skip_serializing)]
    pub oneliner: Option<String>,
    /// The raw documentation of the definition.
    #[serde(skip_serializing)]
    pub docs: Option<EcoString>,
    /// The parsed documentation of the definition.
    #[serde(skip_serializing)]
    pub parsed_docs: Option<DefDocs>,
    /// The value of the definition.
    #[serde(skip)]
    pub constant: Option<EcoString>,
    /// The name range of the definition.
    /// The value of the definition.
    #[serde(skip)]
    pub decl: Option<Interned<Decl>>,
    /// The children of the definition.
    pub children: Vec<DefInfo>,
}

/// A source position that can be used to query a package index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceQuery {
    /// The source file index in the package document file table.
    pub file: usize,
    /// The source position for a `textDocument/definition` query.
    pub position: Position,
}

/// Information about the definitions in a package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageDefInfo {
    /// The root module information.
    #[serde(flatten)]
    pub root: DefInfo,
    /// The module accessible paths.
    pub module_uses: HashMap<String, EcoVec<String>>,
}

struct ScanDefCtx<'a> {
    ctx: &'a mut LocalContext,
    for_spec: Option<&'a PackageSpec>,
    aliases: &'a mut HashMap<FileId, Vec<String>>,
    extras: &'a mut Vec<DefInfo>,
    root: FileId,
}

impl ScanDefCtx<'_> {
    fn defs(&mut self, paths: EcoVec<&str>, ei: ExprInfo) -> DefInfo {
        let module_decl = Decl::module(ei.fid);
        let key = module_decl.name().clone();
        let site = Some(self.root);
        let paths = paths.clone();
        self.def(&key, paths, site.as_ref(), &module_decl.into(), None)
    }

    fn expr(
        &mut self,
        key: &str,
        path: EcoVec<&str>,
        site: Option<&FileId>,
        val: &Expr,
    ) -> DefInfo {
        match val {
            Expr::Decl(decl) => self.def(key, path, site, decl, Some(val)),
            Expr::Ref(r) if r.root.is_some() => {
                self.expr(key, path, site, r.root.as_ref().unwrap())
            }
            // todo: select
            Expr::Select(..) => {
                let mut path = path.clone();
                path.push(key);
                DefInfo {
                    name: key.to_string().into(),
                    kind: DefKind::Module,
                    ..Default::default()
                }
            }
            // v => panic!("unexpected export: {key} -> {v}"),
            _ => {
                let mut path = path.clone();
                path.push(key);
                DefInfo {
                    name: key.to_string().into(),
                    kind: DefKind::Constant,
                    ..Default::default()
                }
            }
        }
    }

    fn def(
        &mut self,
        key: &str,
        path: EcoVec<&str>,
        site: Option<&FileId>,
        decl: &Interned<Decl>,
        expr: Option<&Expr>,
    ) -> DefInfo {
        let children = match decl.as_ref() {
            Decl::Module(..) => decl.file_id().and_then(|fid| {
                // only generate docs for the same package
                if !matches!(fid.root(), VirtualRoot::Package(package) if Some(package) == self.for_spec) {
                    return None;
                }

                // !aliases.insert(fid)
                let aliases_vec = self.aliases.entry(fid).or_default();
                let is_fresh = aliases_vec.is_empty();
                aliases_vec.push(path.iter().join("."));

                if !is_fresh {
                    crate::log_debug_ct!("found module: {path:?} (reexport)");
                    return None;
                }

                crate::log_debug_ct!("found module: {path:?}");

                let ei = self.ctx.expr_stage_by_id(fid)?;

                let symbols = ei
                    .exports
                    .iter()
                    .map(|(name, val)| {
                        let mut path = path.clone();
                        path.push(name);
                        self.expr(name, path.clone(), Some(&fid), val)
                    })
                    .collect();
                Some(symbols)
            }),
            _ => None,
        };

        let mut head = DefInfo {
            id: EcoString::new(),
            name: key.to_string().into(),
            kind: decl.kind(),
            constant: expr.map(|expr| expr.repr()),
            docs: None,
            parsed_docs: None,
            decl: Some(decl.clone()),
            children: children.unwrap_or_default(),
            symbol: None,
            loc: None,
            source: None,
            is_external: false,
            module_link: None,
            bundle_link: None,
            symbol_link: None,
            external_link: None,
            oneliner: None,
        };

        if let Some((span, mod_fid)) = head.decl.as_ref().and_then(|decl| decl.file_id()).zip(site)
            && span != *mod_fid
        {
            head.is_external = true;
        }

        // Insert module that is not exported
        if let Some(fid) = head.decl.as_ref().and_then(|del| del.file_id()) {
            // only generate docs for the same package
            if matches!(fid.root(), VirtualRoot::Package(package) if Some(package) == self.for_spec)
            {
                let av = self.aliases.entry(fid).or_default();
                if av.is_empty() {
                    let src = self.ctx.expr_stage_by_id(fid);
                    let mut path = path.clone();
                    path.push("-");
                    path.push(key);

                    crate::log_debug_ct!("found internal module: {path:?}");
                    if let Some(m) = src {
                        let msym = self.defs(path, m);
                        self.extras.push(msym)
                    }
                }
            }
        }

        head
    }
}

/// Extract the first line of documentation.
fn oneliner(docs: &str) -> &str {
    docs.lines().next().unwrap_or_default()
}
