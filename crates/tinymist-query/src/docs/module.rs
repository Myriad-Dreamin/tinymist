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
use crate::package::{get_manifest_id, PackageInfo};
use crate::syntax::{Decl, DefKind, Expr, ExprInfo};
use crate::ty::Interned;
use crate::LocalContext;

use super::DefDocs;

/// Get documentation of definitions in a package.
pub fn package_module_docs(ctx: &mut LocalContext, pkg: &PackageInfo) -> StrResult<PackageDefInfo> {
    let toml_id = get_manifest_id(pkg)?;
    let manifest = ctx.get_manifest(toml_id)?;

    let entry_point = toml_id.join(&manifest.package.entrypoint);
    module_docs(ctx, entry_point)
}

/// Get documentation of definitions in a module.
pub fn module_docs(ctx: &mut LocalContext, entry_point: FileId) -> StrResult<PackageDefInfo> {
    let mut aliases = HashMap::new();
    let mut extras = vec![];

    let mut scan_ctx = ScanDefCtx {
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
    let mut defs = scan_ctx.defs(eco_vec![], ei);

    let module_uses = aliases
        .into_iter()
        .map(|(k, mut v)| {
            v.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
            (file_id_repr(k), v.into())
        })
        .collect();

    log::debug!("module_uses: {module_uses:#?}",);

    defs.children.extend(extras);

    Ok(PackageDefInfo {
        root: defs,
        module_uses,
    })
}

/// Information about a definition.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DefInfo {
    /// The name of the definition.
    pub name: EcoString,
    /// The kind of the definition.
    pub kind: DefKind,
    /// The location (file, start, end) of the definition.
    pub loc: Option<(usize, usize, usize)>,
    /// Whether the definition external to the module.
    pub is_external: bool,
    /// The link to the definition if it is external.
    pub external_link: Option<String>,
    /// The one-line documentation of the definition.
    pub oneliner: Option<String>,
    /// The raw documentation of the definition.
    pub docs: Option<EcoString>,
    /// The parsed documentation of the definition.
    pub parsed_docs: Option<DefDocs>,
    /// The value of the definition.
    #[serde(skip)]
    pub constant: Option<EcoString>,
    /// The name range of the definition.
    /// The value of the definition.
    #[serde(skip)]
    pub decl: Option<Interned<Decl>>,
    /// The children of the definition.
    pub children: EcoVec<DefInfo>,
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
    fn defs(&mut self, path: EcoVec<&str>, ei: Arc<ExprInfo>) -> DefInfo {
        let name = {
            let stem = ei.fid.vpath().as_rooted_path().file_stem();
            stem.and_then(|s| Some(Interned::new_str(s.to_str()?)))
                .unwrap_or_default()
        };
        let module_decl = Decl::module(name.clone(), ei.fid).into();
        let site = Some(self.root);
        let p = path.clone();
        self.def(&name, p, site.as_ref(), &module_decl, None)
    }

    fn expr(
        &mut self,
        key: &str,
        path: EcoVec<&str>,
        site: Option<&FileId>,
        val: &Expr,
    ) -> DefInfo {
        match val {
            Expr::Decl(d) => self.def(key, path, site, d, Some(val)),
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
        let def = self.ctx.def_of_decl(decl);
        let def_docs = def.and_then(|def| self.ctx.def_docs(&def));
        let docs = def_docs.as_ref().map(|d| d.docs().clone());
        let children = match decl.as_ref() {
            Decl::Module(..) => decl.file_id().and_then(|fid| {
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

        let mut head = DefInfo {
            name: key.to_string().into(),
            kind: decl.kind(),
            constant: expr.map(|e| e.repr()),
            docs,
            parsed_docs: def_docs,
            decl: Some(decl.clone()),
            children: children.unwrap_or_default(),
            loc: None,
            is_external: false,
            external_link: None,
            oneliner: None,
        };

        if let Some((span, mod_fid)) = head.decl.as_ref().and_then(|d| d.file_id()).zip(site) {
            if span != *mod_fid {
                head.is_external = true;
                head.oneliner = head.docs.as_deref().map(oneliner).map(|e| e.to_owned());
                head.docs = None;
            }
        }

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
