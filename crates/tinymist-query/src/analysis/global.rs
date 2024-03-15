use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
};

use once_cell::sync::OnceCell;
use typst::{
    diag::{eco_format, FileError, FileResult},
    syntax::{Source, VirtualPath},
    World,
};
use typst_ts_compiler::{service::WorkspaceProvider, TypstSystemWorld};
use typst_ts_core::{cow_mut::CowMut, ImmutPath, TypstFileId};

use super::{construct_module_dependencies, DefUseInfo, ModuleDependency};

pub struct ModuleAnalysisCache {
    source: OnceCell<FileResult<Source>>,
    def_use: OnceCell<Option<Arc<DefUseInfo>>>,
}

impl ModuleAnalysisCache {
    pub fn source(&self, ctx: &AnalysisContext, file_id: TypstFileId) -> FileResult<Source> {
        self.source
            .get_or_init(|| ctx.world.source(file_id))
            .clone()
    }

    pub fn def_use(&self) -> Option<Arc<DefUseInfo>> {
        self.def_use.get().cloned().flatten()
    }

    pub fn compute_def_use(
        &self,
        f: impl FnOnce() -> Option<Arc<DefUseInfo>>,
    ) -> Option<Arc<DefUseInfo>> {
        self.def_use.get_or_init(f).clone()
    }
}

pub struct Analysis {
    pub root: ImmutPath,
}

pub struct AnalysisCaches {
    modules: HashMap<TypstFileId, ModuleAnalysisCache>,
    root_files: OnceCell<Vec<TypstFileId>>,
    module_deps: OnceCell<HashMap<TypstFileId, ModuleDependency>>,
}

pub struct AnalysisContext<'a> {
    pub world: &'a TypstSystemWorld,
    pub analysis: CowMut<'a, Analysis>,
    caches: AnalysisCaches,
}

impl<'w> AnalysisContext<'w> {
    pub fn new(world: &'w TypstSystemWorld) -> Self {
        Self {
            world,
            analysis: CowMut::Owned(Analysis {
                root: world.workspace_root(),
            }),
            caches: AnalysisCaches {
                modules: HashMap::new(),
                root_files: OnceCell::new(),
                module_deps: OnceCell::new(),
            },
        }
    }

    #[cfg(test)]
    pub fn test_files(&mut self, f: impl FnOnce() -> Vec<TypstFileId>) -> &Vec<TypstFileId> {
        self.caches.root_files.get_or_init(f)
    }

    pub fn files(&mut self) -> &Vec<TypstFileId> {
        self.caches.root_files.get_or_init(|| self.search_files())
    }

    pub fn module_dependencies(&mut self) -> &HashMap<TypstFileId, ModuleDependency> {
        if self.caches.module_deps.get().is_some() {
            return self.caches.module_deps.get().unwrap();
        } else {
            // may cause multiple times to calculate, but it is okay because we have mutable
            // reference to self.
            let deps = construct_module_dependencies(self);
            self.caches.module_deps.get_or_init(|| deps)
        }
    }

    pub fn fork_for_search<'s>(&'s mut self) -> SearchCtx<'s, 'w> {
        SearchCtx {
            ctx: self,
            searched: Default::default(),
            worklist: Default::default(),
        }
    }

    pub fn get_mut(&mut self, file_id: TypstFileId) -> &ModuleAnalysisCache {
        self.caches.modules.entry(file_id).or_insert_with(|| {
            let source = OnceCell::new();
            let def_use = OnceCell::new();
            ModuleAnalysisCache { source, def_use }
        })
    }

    pub fn get(&self, file_id: TypstFileId) -> Option<&ModuleAnalysisCache> {
        self.caches.modules.get(&file_id)
    }

    pub fn source_by_id(&mut self, id: TypstFileId) -> FileResult<Source> {
        self.get_mut(id);
        self.get(id).unwrap().source(self, id)
    }

    pub fn source_by_path(&mut self, p: &Path) -> FileResult<Source> {
        // todo: source in packages
        let relative_path = p.strip_prefix(&self.analysis.root).map_err(|_| {
            FileError::Other(Some(eco_format!(
                "not in root, path is {p:?}, root is {:?}",
                self.analysis.root
            )))
        })?;

        let id = TypstFileId::new(None, VirtualPath::new(relative_path));
        self.source_by_id(id)
    }

    fn search_files(&self) -> Vec<TypstFileId> {
        let root = self.analysis.root.clone();

        let mut res = vec![];
        for path in walkdir::WalkDir::new(&root).follow_links(false).into_iter() {
            let Ok(de) = path else {
                continue;
            };
            if !de.file_type().is_file() {
                continue;
            }
            if !de
                .path()
                .extension()
                .is_some_and(|e| e == "typ" || e == "typc")
            {
                continue;
            }

            let path = de.path();
            let relative_path = match path.strip_prefix(&root) {
                Ok(p) => p,
                Err(err) => {
                    log::warn!("failed to strip prefix, path: {path:?}, root: {root:?}: {err}");
                    continue;
                }
            };

            res.push(TypstFileId::new(None, VirtualPath::new(relative_path)));
        }

        res
    }
}

pub struct SearchCtx<'b, 'w> {
    pub ctx: &'b mut AnalysisContext<'w>,
    pub searched: HashSet<TypstFileId>,
    pub worklist: Vec<TypstFileId>,
}

impl SearchCtx<'_, '_> {
    pub fn push(&mut self, id: TypstFileId) -> bool {
        if self.searched.insert(id) {
            self.worklist.push(id);
            true
        } else {
            false
        }
    }

    pub fn push_dependents(&mut self, id: TypstFileId) {
        let deps = self.ctx.module_dependencies().get(&id);
        let dependents = deps.map(|e| e.dependents.clone()).into_iter().flatten();
        for dep in dependents {
            self.push(dep);
        }
    }
}
