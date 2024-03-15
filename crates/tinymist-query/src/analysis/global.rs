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

use super::{get_def_use_inner, DefUseInfo};
use crate::syntax::{construct_module_dependencies, scan_workspace_files, ModuleDependency};

/// A cache for module-level analysis results of a module.
///
/// You should not holds across requests, because source code may change.
pub struct ModuleAnalysisCache {
    source: OnceCell<FileResult<Source>>,
    def_use: OnceCell<Option<Arc<DefUseInfo>>>,
}

impl ModuleAnalysisCache {
    /// Get the source of a file.
    pub fn source(&self, ctx: &AnalysisContext, file_id: TypstFileId) -> FileResult<Source> {
        self.source
            .get_or_init(|| ctx.world.source(file_id))
            .clone()
    }

    /// Try to get the def-use information of a file.
    pub fn def_use(&self) -> Option<Arc<DefUseInfo>> {
        self.def_use.get().cloned().flatten()
    }

    /// Compute the def-use information of a file.
    pub(crate) fn compute_def_use(
        &self,
        f: impl FnOnce() -> Option<Arc<DefUseInfo>>,
    ) -> Option<Arc<DefUseInfo>> {
        self.def_use.get_or_init(f).clone()
    }
}

/// The analysis data holds globally.
pub struct Analysis {
    /// The root of the workspace.
    /// This means that the analysis result won't be valid if the root directory
    /// changes.
    pub root: ImmutPath,
}

/// A cache for all level of analysis results of a module.
pub struct AnalysisCaches {
    modules: HashMap<TypstFileId, ModuleAnalysisCache>,
    root_files: OnceCell<Vec<TypstFileId>>,
    module_deps: OnceCell<HashMap<TypstFileId, ModuleDependency>>,
}

/// The context for analyzers.
pub struct AnalysisContext<'a> {
    /// The world surface for Typst compiler
    pub world: &'a TypstSystemWorld,
    /// The analysis data
    pub analysis: CowMut<'a, Analysis>,
    caches: AnalysisCaches,
}

impl<'w> AnalysisContext<'w> {
    /// Create a new analysis context.
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

    /// Get all the files in the workspace.
    pub fn files(&mut self) -> &Vec<TypstFileId> {
        self.caches
            .root_files
            .get_or_init(|| scan_workspace_files(&self.analysis.root))
    }

    /// Get the module dependencies of the workspace.
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

    /// Get the source of a file by file id.
    pub fn source_by_id(&mut self, id: TypstFileId) -> FileResult<Source> {
        self.get_mut(id);
        self.get(id).unwrap().source(self, id)
    }

    /// Get the source of a file by file path.
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

    /// Get the module-level analysis cache of a file.
    pub fn get(&self, file_id: TypstFileId) -> Option<&ModuleAnalysisCache> {
        self.caches.modules.get(&file_id)
    }

    /// Get the module-level analysis cache of a file.
    pub fn get_mut(&mut self, file_id: TypstFileId) -> &ModuleAnalysisCache {
        self.caches.modules.entry(file_id).or_insert_with(|| {
            let source = OnceCell::new();
            let def_use = OnceCell::new();
            ModuleAnalysisCache { source, def_use }
        })
    }

    /// Get the def-use information of a source file.
    pub fn def_use(&mut self, source: Source) -> Option<Arc<DefUseInfo>> {
        get_def_use_inner(&mut self.fork_for_search(), source)
    }

    /// Fork a new context for searching in the workspace.
    pub fn fork_for_search<'s>(&'s mut self) -> SearchCtx<'s, 'w> {
        SearchCtx {
            ctx: self,
            searched: Default::default(),
            worklist: Default::default(),
        }
    }
}

/// The context for searching in the workspace.
pub struct SearchCtx<'b, 'w> {
    /// The inner analysis context.
    pub ctx: &'b mut AnalysisContext<'w>,
    /// The set of files that have been searched.
    pub searched: HashSet<TypstFileId>,
    /// The files that need to be searched.
    pub worklist: Vec<TypstFileId>,
}

impl SearchCtx<'_, '_> {
    /// Push a file to the worklist.
    pub fn push(&mut self, id: TypstFileId) -> bool {
        if self.searched.insert(id) {
            self.worklist.push(id);
            true
        } else {
            false
        }
    }

    /// Push the dependents of a file to the worklist.
    pub fn push_dependents(&mut self, id: TypstFileId) {
        let deps = self.ctx.module_dependencies().get(&id);
        let dependents = deps.map(|e| e.dependents.clone()).into_iter().flatten();
        for dep in dependents {
            self.push(dep);
        }
    }
}
