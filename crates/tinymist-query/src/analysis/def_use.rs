use core::fmt;
use std::{collections::HashMap, ops::Range};

use serde::Serialize;
use typst::syntax::Source;

use crate::adt::snapshot_map::SnapshotMap;

use super::{
    get_lexical_hierarchy, LexicalHierarchy, LexicalKind, LexicalScopeKind, LexicalVarKind,
};

pub use typst_ts_core::vector::ir::DefId;

enum Ns {
    Label,
    Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IdentRef {
    name: String,
    range: Range<usize>,
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

#[derive(Serialize)]
pub struct IdentDef {
    name: String,
    kind: LexicalKind,
    range: Range<usize>,
}

#[derive(Default)]
pub struct DefUseInfo {
    ident_defs: indexmap::IndexMap<IdentRef, IdentDef>,
    ident_refs: HashMap<IdentRef, DefId>,
    undefined_refs: Vec<IdentRef>,
}

pub struct DefUseSnapshot<'a>(pub &'a DefUseInfo);

impl<'a> Serialize for DefUseSnapshot<'a> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        // HashMap<IdentRef, DefId>
        let references: HashMap<DefId, Vec<IdentRef>> = {
            let mut map = HashMap::new();
            for (k, v) in &self.0.ident_refs {
                map.entry(*v).or_insert_with(Vec::new).push(k.clone());
            }
            map
        };

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

            state.serialize_entry(&ident_ref.to_string(), &entry)?;
        }

        if !self.0.undefined_refs.is_empty() {
            let entry = DefUseEntry {
                def: &IdentDef {
                    name: "<nil>".to_string(),
                    kind: LexicalKind::Block,
                    range: 0..0,
                },
                refs: &self.0.undefined_refs,
            };
            state.serialize_entry("<nil>", &entry)?;
        }

        state.end()
    }
}

pub fn get_def_use(source: Source) -> Option<DefUseInfo> {
    let e = get_lexical_hierarchy(source, LexicalScopeKind::DefUse)?;

    let mut collector = DefUseCollector {
        info: DefUseInfo::default(),
        id_scope: SnapshotMap::default(),
        label_scope: SnapshotMap::default(),
    };

    collector.scan(&e);
    Some(collector.info)
}

struct DefUseCollector {
    info: DefUseInfo,
    label_scope: SnapshotMap<String, DefId>,
    id_scope: SnapshotMap<String, DefId>,
}

impl DefUseCollector {
    fn enter<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let id_snap = self.id_scope.snapshot();
        let res = f(self);
        self.id_scope.rollback_to(id_snap);
        res
    }

    fn scan(&mut self, e: &[LexicalHierarchy]) -> Option<()> {
        for e in e {
            match &e.info.kind {
                LexicalKind::Heading(..) => unreachable!(),
                LexicalKind::Var(LexicalVarKind::Label) => self.insert(Ns::Label, e),
                LexicalKind::Var(LexicalVarKind::LabelRef) => self.insert_ref(Ns::Label, e),
                LexicalKind::Var(LexicalVarKind::Function)
                | LexicalKind::Var(LexicalVarKind::Variable) => self.insert(Ns::Value, e),
                LexicalKind::Var(LexicalVarKind::ValRef) => self.insert_ref(Ns::Value, e),
                LexicalKind::Block => {
                    if let Some(e) = &e.children {
                        self.enter(|this| this.scan(e.as_slice()))?;
                    }
                }
                LexicalKind::Mod(..) => {}
            }
        }

        Some(())
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
        let (id, old_def) = self.info.ident_defs.insert_full(
            id_ref.clone(),
            IdentDef {
                name: e.info.name.clone(),
                kind: e.info.kind.clone(),
                range: e.info.range.clone(),
            },
        );
        if let Some(old_def) = old_def {
            assert_eq!(old_def.kind, e.info.kind);
        }

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
