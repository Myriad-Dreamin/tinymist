//! Static analysis for def-use relations.

use reflexo::hash::hash128;

use super::{prelude::*, ImportInfo};
use crate::adt::snapshot_map::SnapshotMap;

/// The type namespace of def-use relations
///
/// The symbols from different namespaces are not visible to each other.
enum Ns {
    /// Def-use for labels
    Label,
    /// Def-use for values
    Value,
}

type ExternalRefMap = HashMap<(TypstFileId, Option<EcoString>), Vec<(Option<DefId>, IdentRef)>>;

/// The def-use information of a source file.
#[derive(Default)]
pub struct DefUseInfo {
    /// The definitions of symbols.
    pub ident_defs: indexmap::IndexMap<(TypstFileId, IdentRef), IdentDef>,
    external_refs: ExternalRefMap,
    /// The references to defined symbols.
    pub ident_refs: HashMap<IdentRef, DefId>,
    /// The references of labels.
    pub label_refs: HashMap<EcoString, Vec<Range<usize>>>,
    /// The references to undefined symbols.
    pub undefined_refs: Vec<IdentRef>,
    exports_refs: Vec<DefId>,
    exports_defs: HashMap<EcoString, DefId>,

    self_id: Option<TypstFileId>,
    self_hash: u128,
    all_hash: once_cell::sync::OnceCell<u128>,
}

impl Hash for DefUseInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.dep_hash(self.self_id.unwrap()).hash(state);
    }
}

impl DefUseInfo {
    /// Get the estimated memory usage of the def-use information.
    pub fn estimated_memory(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.ident_defs.capacity()
                * (std::mem::size_of::<IdentDef>() + std::mem::size_of::<IdentRef>() + 32)
            + self.external_refs.capacity()
                * (std::mem::size_of::<(TypstFileId, Option<String>)>()
                    + std::mem::size_of::<Vec<(Option<DefId>, IdentRef)>>()
                    + 32)
            + self.ident_refs.capacity()
                * (std::mem::size_of::<IdentRef>() + std::mem::size_of::<DefId>() + 32)
            + self.label_refs.capacity() * (std::mem::size_of::<Range<usize>>() + 32)
            + self.undefined_refs.capacity() * (std::mem::size_of::<IdentRef>() + 32)
            + self.exports_refs.capacity() * (std::mem::size_of::<DefId>() + 32)
            + self.exports_defs.capacity()
                * (std::mem::size_of::<String>() + std::mem::size_of::<DefId>() + 32)
    }

    /// Get the definition id of a symbol by its name reference.
    pub fn get_ref(&self, ident: &IdentRef) -> Option<DefId> {
        self.ident_refs.get(ident).copied()
    }

    /// Get the definition of a symbol by its unique id.
    pub fn get_def_by_id(&self, id: DefId) -> Option<(TypstFileId, &IdentDef)> {
        let ((fid, _), def) = self.ident_defs.get_index(id.0 as usize)?;
        Some((*fid, def))
    }

    /// Get the definition of a symbol by its name reference.
    pub fn get_def(&self, fid: TypstFileId, ident: &IdentRef) -> Option<(DefId, &IdentDef)> {
        let (id, _, def) = self.ident_defs.get_full(&(fid, ident.clone()))?;
        Some((DefId(id as u64), def))
    }

    /// Get the references of a symbol by its unique id.
    pub fn get_refs(&self, id: DefId) -> impl Iterator<Item = &IdentRef> {
        self.ident_refs
            .iter()
            .filter_map(move |(k, v)| if *v == id { Some(k) } else { None })
    }

    /// Get external references of a symbol by its name reference.
    pub fn get_external_refs(
        &self,
        ext_id: TypstFileId,
        ext_name: Option<EcoString>,
    ) -> impl Iterator<Item = &(Option<DefId>, IdentRef)> {
        self.external_refs
            .get(&(ext_id, ext_name))
            .into_iter()
            .flatten()
    }

    /// Check if a symbol is exported.
    pub fn is_exported(&self, id: DefId) -> bool {
        self.exports_refs.contains(&id)
    }

    /// Get the definition id of an exported symbol by its name.
    pub fn dep_hash(&self, fid: TypstFileId) -> u128 {
        *self.all_hash.get_or_init(|| {
            use siphasher::sip128::Hasher128;
            let mut hasher = reflexo::hash::FingerprintSipHasherBase::default();
            self.self_hash.hash(&mut hasher);
            for (dep_fid, def) in self.ident_defs.keys() {
                if fid == *dep_fid {
                    continue;
                }
                fid.hash(&mut hasher);
                def.hash(&mut hasher);
            }

            hasher.finish128().into()
        })
    }
}

pub(super) fn get_def_use_inner(
    ctx: &mut SearchCtx,
    source: Source,
    e: EcoVec<LexicalHierarchy>,
    import: Arc<ImportInfo>,
) -> Option<Arc<DefUseInfo>> {
    let current_id = source.id();

    let info = DefUseInfo {
        self_hash: hash128(&source),
        self_id: Some(current_id),

        ..Default::default()
    };

    let mut collector = DefUseCollector {
        ctx,
        info,
        id_scope: SnapshotMap::default(),
        label_scope: SnapshotMap::default(),
        import,

        current_id,
        ext_src: None,
    };

    collector.scan(&e);
    collector.calc_exports();

    Some(Arc::new(collector.info))
}

struct DefUseCollector<'a, 'b, 'w> {
    ctx: &'a mut SearchCtx<'b, 'w>,
    info: DefUseInfo,
    label_scope: SnapshotMap<EcoString, DefId>,
    id_scope: SnapshotMap<EcoString, DefId>,
    import: Arc<ImportInfo>,

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
        self.info.exports_defs = self
            .id_scope
            .entries()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
    }

    fn import_name(&mut self, name: &str) -> Option<()> {
        let source = self.ext_src.as_ref()?;

        log::debug!("import for def use: {:?}, name: {name}", source.id());
        let (_, external_info) =
            Some(source.id()).zip(AnalysisContext::def_use_(self.ctx, source.clone()))?;

        let ext_id = external_info.exports_defs.get(name)?;
        self.import_from(&external_info, *ext_id);

        Some(())
    }

    fn import_from(&mut self, external_info: &DefUseInfo, v: DefId) {
        // Use FileId in ident_defs map should lose stacked import
        // information, but it is currently
        // not a problem.
        let ((ext_id, _), ext_sym) = external_info.ident_defs.get_index(v.0 as usize).unwrap();

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

    fn scan(&mut self, e: &'a [LexicalHierarchy]) -> Option<()> {
        for e in e {
            match &e.info.kind {
                LexicalKind::Var(LexicalVarKind::BibKey) | LexicalKind::Heading(..) => {
                    unreachable!()
                }
                LexicalKind::Var(LexicalVarKind::Label) => {
                    self.insert(Ns::Label, e);
                }
                LexicalKind::Var(LexicalVarKind::LabelRef) => self.insert_label_ref(e),
                LexicalKind::Var(LexicalVarKind::Function)
                | LexicalKind::Var(LexicalVarKind::Variable) => {
                    self.insert(Ns::Value, e);
                }
                LexicalKind::Var(LexicalVarKind::ValRef) => self.insert_value_ref(e),
                LexicalKind::Block => {
                    if let Some(e) = &e.children {
                        self.enter(|this| this.scan(e.as_slice()))?;
                    }
                }

                LexicalKind::Mod(LexicalModKind::Module(..)) => {
                    let mut src = self.import.imports.get(&e.info.range)?.clone();
                    log::debug!("check import: {info:?} => {src:?}", info = e.info);
                    std::mem::swap(&mut self.ext_src, &mut src);

                    // todo: process import star
                    if let Some(e) = &e.children {
                        self.scan(e.as_slice())?;
                    }

                    std::mem::swap(&mut self.ext_src, &mut src);
                }
                LexicalKind::Mod(LexicalModKind::Star) => {
                    if let Some(source) = &self.ext_src {
                        log::debug!("diving source for def use: {:?}", source.id());
                        let (_, external_info) = Some(source.id())
                            .zip(AnalysisContext::def_use_(self.ctx, source.clone()))?;

                        for ext_id in &external_info.exports_refs {
                            self.import_from(&external_info, *ext_id);
                        }
                    }
                }
                LexicalKind::Mod(LexicalModKind::PathInclude) => {}
                LexicalKind::Mod(LexicalModKind::PathVar)
                | LexicalKind::Mod(LexicalModKind::ModuleAlias) => self.insert_module(Ns::Value, e),
                LexicalKind::Mod(LexicalModKind::Ident) => match self.import_name(&e.info.name) {
                    Some(()) => {
                        self.insert_value_ref(e);
                    }
                    None => {
                        let def_id = self.insert(Ns::Value, e);
                        self.insert_extern(e.info.name.clone(), e.info.range.clone(), Some(def_id));
                    }
                },
                LexicalKind::Mod(LexicalModKind::Alias { target }) => {
                    match self.import_name(&target.name) {
                        Some(()) => {
                            self.insert_value_ref_(IdentRef {
                                name: target.name.clone(),
                                range: target.range.clone(),
                            });
                            self.insert(Ns::Value, e);
                        }
                        None => {
                            let def_id = self.insert(Ns::Value, e);
                            self.insert_extern(
                                target.name.clone(),
                                target.range.clone(),
                                Some(def_id),
                            );
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
                vec![(
                    None,
                    IdentRef {
                        name: e.info.name.clone(),
                        range: e.info.range.clone(),
                    },
                )],
            );
        }
    }

    fn insert_extern(&mut self, name: EcoString, range: Range<usize>, redefine_id: Option<DefId>) {
        if let Some(src) = &self.ext_src {
            self.info.external_refs.insert(
                (src.id(), Some(name.clone())),
                vec![(redefine_id, IdentRef { name, range })],
            );
        }
    }

    fn insert(&mut self, label: Ns, e: &LexicalHierarchy) -> DefId {
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
        id
    }

    fn insert_value_ref_(&mut self, id_ref: IdentRef) {
        match self.id_scope.get(&id_ref.name) {
            Some(id) => {
                self.info.ident_refs.insert(id_ref, *id);
            }
            None => {
                self.info.undefined_refs.push(id_ref);
            }
        }
    }

    fn insert_value_ref(&mut self, e: &LexicalHierarchy) {
        self.insert_value_ref_(IdentRef {
            name: e.info.name.clone(),
            range: e.info.range.clone(),
        });
    }

    fn insert_label_ref(&mut self, e: &LexicalHierarchy) {
        let refs = self.info.label_refs.entry(e.info.name.clone()).or_default();
        refs.push(e.info.range.clone());
    }
}
