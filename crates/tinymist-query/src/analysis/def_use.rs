use core::fmt;
use std::{
    collections::HashMap,
    ops::{Deref, Range},
    sync::Arc,
};

use log::info;
use serde::Serialize;
use typst::syntax::Source;
use typst_ts_core::{path::unix_slash, TypstFileId};

use crate::{adt::snapshot_map::SnapshotMap, analysis::find_source_by_import_path};

use super::{
    get_lexical_hierarchy, AnalysisContext, LexicalHierarchy, LexicalKind, LexicalScopeKind,
    LexicalVarKind, ModSrc, SearchCtx,
};

pub use typst_ts_core::vector::ir::DefId;

enum Ns {
    Label,
    Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IdentRef {
    pub name: String,
    pub range: Range<usize>,
}

impl PartialOrd for IdentRef {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for IdentRef {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name
            .cmp(&other.name)
            .then_with(|| self.range.start.cmp(&other.range.start))
    }
}

impl fmt::Display for IdentRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{:?}", self.name, self.range)
    }
}

impl Serialize for IdentRef {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let s = self.to_string();
        serializer.serialize_str(&s)
    }
}

#[derive(Serialize, Clone)]
pub struct IdentDef {
    pub name: String,
    pub kind: LexicalKind,
    pub range: Range<usize>,
}

#[derive(Default)]
pub struct DefUseInfo {
    ident_defs: indexmap::IndexMap<(TypstFileId, IdentRef), IdentDef>,
    external_refs: HashMap<(TypstFileId, Option<String>), Vec<IdentRef>>,
    ident_refs: HashMap<IdentRef, DefId>,
    undefined_refs: Vec<IdentRef>,
    exports_refs: Vec<DefId>,
}

impl DefUseInfo {
    pub fn get_ref(&self, ident: &IdentRef) -> Option<DefId> {
        self.ident_refs.get(ident).copied()
    }

    pub fn get_def_by_id(&self, id: DefId) -> Option<(TypstFileId, &IdentDef)> {
        let ((fid, _), def) = self.ident_defs.get_index(id.0 as usize)?;
        Some((*fid, def))
    }

    pub fn get_def(&self, fid: TypstFileId, ident: &IdentRef) -> Option<(DefId, &IdentDef)> {
        let (id, _, def) = self.ident_defs.get_full(&(fid, ident.clone()))?;
        Some((DefId(id as u64), def))
    }

    pub fn get_refs(&self, id: DefId) -> impl Iterator<Item = &IdentRef> {
        self.ident_refs
            .iter()
            .filter_map(move |(k, v)| if *v == id { Some(k) } else { None })
    }

    pub fn get_external_refs(
        &self,
        ext_id: TypstFileId,
        ext_name: Option<String>,
    ) -> impl Iterator<Item = &IdentRef> {
        self.external_refs
            .get(&(ext_id, ext_name))
            .into_iter()
            .flatten()
    }

    pub fn is_exported(&self, id: DefId) -> bool {
        self.exports_refs.contains(&id)
    }
}

pub fn get_def_use(ctx: &mut AnalysisContext, source: Source) -> Option<Arc<DefUseInfo>> {
    get_def_use_inner(&mut ctx.fork_for_search(), source)
}

fn get_def_use_inner(ctx: &mut SearchCtx, source: Source) -> Option<Arc<DefUseInfo>> {
    let current_id = source.id();
    if !ctx.searched.insert(current_id) {
        return None;
    }

    ctx.ctx.get_mut(current_id);
    let c = ctx.ctx.get(current_id).unwrap();

    if let Some(info) = c.def_use() {
        return Some(info);
    }

    let e = get_lexical_hierarchy(source, LexicalScopeKind::DefUse)?;

    let mut collector = DefUseCollector {
        ctx,
        info: DefUseInfo::default(),
        id_scope: SnapshotMap::default(),
        label_scope: SnapshotMap::default(),

        current_id,
        ext_src: None,
    };

    collector.scan(&e);
    collector.calc_exports();
    let res = Some(Arc::new(collector.info));

    let c = ctx.ctx.get(current_id).unwrap();
    // todo: cyclic import cause no any information
    c.compute_def_use(|| res.clone());
    res
}

struct DefUseCollector<'a, 'b, 'w> {
    ctx: &'a mut SearchCtx<'b, 'w>,
    info: DefUseInfo,
    label_scope: SnapshotMap<String, DefId>,
    id_scope: SnapshotMap<String, DefId>,

    current_id: TypstFileId,
    ext_src: Option<Source>,
}

impl<'a, 'b, 'w> DefUseCollector<'a, 'b, 'w> {
    fn enter<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let id_snap = self.id_scope.snapshot();
        let res = f(self);
        self.id_scope.rollback_to(id_snap);
        res
    }

    fn calc_exports(&mut self) {
        self.info.exports_refs = self.id_scope.values().copied().collect();
    }

    fn scan(&mut self, e: &'a [LexicalHierarchy]) -> Option<()> {
        for e in e {
            match &e.info.kind {
                LexicalKind::Heading(..) => unreachable!(),
                LexicalKind::Var(LexicalVarKind::Label) => self.insert(Ns::Label, e),
                LexicalKind::Var(LexicalVarKind::LabelRef) => self.insert_ref(Ns::Label, e),
                LexicalKind::Var(LexicalVarKind::Function)
                | LexicalKind::Var(LexicalVarKind::Variable) => self.insert(Ns::Value, e),
                LexicalKind::Mod(super::LexicalModKind::PathVar)
                | LexicalKind::Mod(super::LexicalModKind::ModuleAlias) => {
                    self.insert_module(Ns::Value, e)
                }
                LexicalKind::Mod(super::LexicalModKind::Ident) => {
                    self.insert(Ns::Value, e);
                    self.insert_extern(e.info.name.clone(), e.info.range.clone());
                }
                LexicalKind::Mod(super::LexicalModKind::Alias { target }) => {
                    self.insert(Ns::Value, e);
                    self.insert_extern(target.name.clone(), target.range.clone());
                }
                LexicalKind::Var(LexicalVarKind::ValRef) => self.insert_ref(Ns::Value, e),
                LexicalKind::Block => {
                    if let Some(e) = &e.children {
                        self.enter(|this| this.scan(e.as_slice()))?;
                    }
                }
                LexicalKind::Mod(super::LexicalModKind::Module(p)) => {
                    match p {
                        ModSrc::Expr(_) => {}
                        ModSrc::Path(p) => {
                            let src = find_source_by_import_path(
                                self.ctx.ctx.world,
                                self.current_id,
                                p.deref(),
                            );
                            self.ext_src = src;
                        }
                    }

                    // todo: process import star
                    if let Some(e) = &e.children {
                        self.scan(e.as_slice())?;
                    }

                    self.ext_src = None;
                }
                LexicalKind::Mod(super::LexicalModKind::Star) => {
                    if let Some(source) = &self.ext_src {
                        info!("diving source for def use: {:?}", source.id());
                        let (_, external_info) =
                            Some(source.id()).zip(get_def_use_inner(self.ctx, source.clone()))?;

                        for v in &external_info.exports_refs {
                            // Use FileId in ident_defs map should lose stacked import
                            // information, but it is currently
                            // not a problem.
                            let ((ext_id, _), ext_sym) =
                                external_info.ident_defs.get_index(v.0 as usize).unwrap();

                            let name = ext_sym.name.clone();

                            let ext_ref = IdentRef {
                                name: name.clone(),
                                range: ext_sym.range.clone(),
                            };

                            let (id, ..) = self
                                .info
                                .ident_defs
                                .insert_full((*ext_id, ext_ref), ext_sym.clone());

                            let id = DefId(id as u64);
                            self.id_scope.insert(name, id);
                        }
                    }
                }
            }
        }

        Some(())
    }

    fn insert_module(&mut self, label: Ns, e: &LexicalHierarchy) {
        self.insert(label, e);
        if let Some(src) = &self.ext_src {
            self.info.external_refs.insert(
                (src.id(), None),
                vec![IdentRef {
                    name: e.info.name.clone(),
                    range: e.info.range.clone(),
                }],
            );
        }
    }

    fn insert_extern(&mut self, name: String, range: Range<usize>) {
        if let Some(src) = &self.ext_src {
            self.info.external_refs.insert(
                (src.id(), Some(name.clone())),
                vec![IdentRef { name, range }],
            );
        }
    }

    fn insert(&mut self, label: Ns, e: &LexicalHierarchy) {
        let snap = match label {
            Ns::Label => &mut self.label_scope,
            Ns::Value => &mut self.id_scope,
        };

        let id_ref = IdentRef {
            name: e.info.name.clone(),
            range: e.info.range.clone(),
        };
        let (id, ..) = self.info.ident_defs.insert_full(
            (self.current_id, id_ref.clone()),
            IdentDef {
                name: e.info.name.clone(),
                kind: e.info.kind.clone(),
                range: e.info.range.clone(),
            },
        );

        let id = DefId(id as u64);
        snap.insert(e.info.name.clone(), id);
    }

    fn insert_ref(&mut self, label: Ns, e: &LexicalHierarchy) {
        let snap = match label {
            Ns::Label => &mut self.label_scope,
            Ns::Value => &mut self.id_scope,
        };

        let id_ref = IdentRef {
            name: e.info.name.clone(),
            range: e.info.range.clone(),
        };

        match snap.get(&e.info.name) {
            Some(id) => {
                self.info.ident_refs.insert(id_ref, *id);
            }
            None => {
                self.info.undefined_refs.push(id_ref);
            }
        }
    }
}

pub struct DefUseSnapshot<'a>(pub &'a DefUseInfo);

impl<'a> Serialize for DefUseSnapshot<'a> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        // HashMap<IdentRef, DefId>
        let mut references: HashMap<DefId, Vec<IdentRef>> = {
            let mut map = HashMap::new();
            for (k, v) in &self.0.ident_refs {
                map.entry(*v).or_insert_with(Vec::new).push(k.clone());
            }
            map
        };
        // sort
        for (_, v) in references.iter_mut() {
            v.sort();
        }

        #[derive(Serialize)]
        struct DefUseEntry<'a> {
            def: &'a IdentDef,
            refs: &'a Vec<IdentRef>,
        }

        let mut state = serializer.serialize_map(None)?;
        for (k, (ident_ref, ident_def)) in self.0.ident_defs.as_slice().iter().enumerate() {
            let id = DefId(k as u64);

            let empty_ref = Vec::new();
            let entry = DefUseEntry {
                def: ident_def,
                refs: references.get(&id).unwrap_or(&empty_ref),
            };

            state.serialize_entry(
                &format!(
                    "{}@{}",
                    ident_ref.1,
                    unix_slash(ident_ref.0.vpath().as_rootless_path())
                ),
                &entry,
            )?;
        }

        if !self.0.undefined_refs.is_empty() {
            let mut undefined_refs = self.0.undefined_refs.clone();
            undefined_refs.sort();
            let entry = DefUseEntry {
                def: &IdentDef {
                    name: "<nil>".to_string(),
                    kind: LexicalKind::Block,
                    range: 0..0,
                },
                refs: &undefined_refs,
            };
            state.serialize_entry("<nil>", &entry)?;
        }

        state.end()
    }
}
