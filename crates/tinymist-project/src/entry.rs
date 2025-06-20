use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tinymist_l10n::DebugL10n;
use tinymist_std::error::prelude::*;
use tinymist_std::hash::FxDashMap;
use tinymist_std::ImmutPath;
use tinymist_world::EntryState;
use typst::syntax::VirtualPath;

/// The kind of project resolution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectResolutionKind {
    /// Manage typst documents like what we did in Markdown. Each single file is
    /// an individual document and no project resolution is needed.
    /// This is the default behavior.
    #[default]
    SingleFile,
    /// Manage typst documents like what we did in Rust. For each workspace,
    /// tinymist tracks your preview and compilation history, and stores the
    /// information in a lock file. Tinymist will automatically selects the main
    /// file to use according to the lock file. This also allows other tools
    /// push preview and export tasks to language server by updating the
    /// lock file.
    LockDatabase,
}

/// Entry resolver
#[derive(Debug, Default, Clone)]
pub struct EntryResolver {
    /// The kind of project resolution.
    pub project_resolution: ProjectResolutionKind,
    /// Specifies the root path of the project manually.
    pub root_path: Option<ImmutPath>,
    /// The workspace roots from initialization.
    pub roots: Vec<ImmutPath>,
    /// Default entry path from the configuration.
    pub entry: Option<ImmutPath>,
    /// The path to the typst.toml files.
    pub typst_toml_cache: Arc<FxDashMap<ImmutPath, Option<ImmutPath>>>,
}

impl EntryResolver {
    /// Resolves the root directory for the entry file.
    pub fn root(&self, entry: Option<&ImmutPath>) -> Option<ImmutPath> {
        if let Some(root) = &self.root_path {
            return Some(root.clone());
        }

        if let Some(entry) = entry {
            for root in self.roots.iter() {
                if entry.starts_with(root) {
                    return Some(root.clone());
                }
            }

            if !self.roots.is_empty() {
                log::warn!("entry is not in any set root directory");
            }

            let typst_toml_cache = &self.typst_toml_cache;

            match typst_toml_cache.get(entry).map(|r| r.clone()) {
                // In the case that the file is out of workspace, it is believed to not edited
                // frequently. When we check the package root of such files and didn't find it
                // previously, we quickly return None to avoid heavy IO frequently.
                //
                // todo: we avoid heavy io for the case when no root is set, but people should
                // restart the server to refresh the cache
                Some(None) => return None,
                Some(Some(cached)) => {
                    let cached = cached.clone();
                    if cached.join("typst.toml").exists() {
                        return Some(cached.clone());
                    }
                    typst_toml_cache.remove(entry);
                }
                None => {}
            };

            // cache miss, check the file system
            // todo: heavy io here?
            for ancestor in entry.ancestors() {
                let typst_toml = ancestor.join("typst.toml");
                if typst_toml.exists() {
                    let ancestor: ImmutPath = ancestor.into();
                    typst_toml_cache.insert(entry.clone(), Some(ancestor.clone()));
                    return Some(ancestor);
                }
            }
            typst_toml_cache.insert(entry.clone(), None);

            if let Some(parent) = entry.parent() {
                return Some(parent.into());
            }
        }

        if !self.roots.is_empty() {
            return Some(self.roots[0].clone());
        }

        None
    }

    /// Resolves the entry state.
    pub fn resolve(&self, entry: Option<ImmutPath>) -> EntryState {
        let root_dir = self.root(entry.as_ref());
        self.resolve_with_root(root_dir, entry)
    }

    /// Resolves the entry state.
    pub fn resolve_with_root(
        &self,
        root_dir: Option<ImmutPath>,
        entry: Option<ImmutPath>,
    ) -> EntryState {
        // todo: formalize untitled path
        // let is_untitled = entry.as_ref().is_some_and(|p| p.starts_with("/untitled"));
        // let root_dir = self.determine_root(if is_untitled { None } else {
        // entry.as_ref() });

        let entry = match (entry, root_dir) {
            // (Some(entry), Some(root)) if is_untitled => Some(EntryState::new_rooted(
            //     root,
            //     Some(FileId::new(None, VirtualPath::new(entry))),
            // )),
            (Some(entry), Some(root)) => match entry.strip_prefix(&root) {
                Ok(stripped) => Some(EntryState::new_rooted(
                    root,
                    Some(VirtualPath::new(stripped)),
                )),
                Err(err) => {
                    log::info!("Entry is not in root directory: err {err:?}: entry: {entry:?}, root: {root:?}");
                    EntryState::new_rooted_by_parent(entry)
                }
            },
            (Some(entry), None) => EntryState::new_rooted_by_parent(entry),
            (None, Some(root)) => Some(EntryState::new_workspace(root)),
            (None, None) => None,
        };

        entry.unwrap_or_else(|| match self.root(None) {
            Some(root) => EntryState::new_workspace(root),
            None => EntryState::new_detached(),
        })
    }

    /// Resolves the directory to store the lock file.
    pub fn resolve_lock(&self, entry: &EntryState) -> Option<ImmutPath> {
        match self.project_resolution {
            ProjectResolutionKind::LockDatabase if entry.is_in_package() => {
                log::info!("ProjectResolver: no lock for package: {entry:?}");
                None
            }
            ProjectResolutionKind::LockDatabase => {
                let root = entry.workspace_root();
                log::info!("ProjectResolver: lock for {entry:?} at {root:?}");

                root
            }
            ProjectResolutionKind::SingleFile => None,
        }
    }

    /// Resolves the default entry path.
    pub fn resolve_default(&self) -> Option<ImmutPath> {
        let entry = self.entry.as_ref();
        // todo: pre-compute this when updating config
        if let Some(entry) = entry {
            if entry.is_relative() {
                let root = self.root(None)?;
                return Some(root.join(entry).as_path().into());
            }
        }
        entry.cloned()
    }

    /// Validates the configuration.
    pub fn validate(&self) -> Result<()> {
        if let Some(root) = &self.root_path {
            if !root.is_absolute() {
                tinymist_l10n::bail!(
                    "tinymist-project.validate-error.root-path-not-absolute",
                    "rootPath or typstExtraArgs.root must be an absolute path: {root:?}",
                    root = root.debug_l10n()
                );
            }
        }

        Ok(())
    }
}

#[cfg(test)]
#[cfg(any(windows, unix, target_os = "macos"))]
mod entry_tests {
    use tinymist_world::vfs::WorkspaceResolver;

    use super::*;
    use std::path::Path;

    const ROOT: &str = if cfg!(windows) {
        "C:\\dummy-root"
    } else {
        "/dummy-root"
    };
    const ROOT2: &str = if cfg!(windows) {
        "C:\\dummy-root2"
    } else {
        "/dummy-root2"
    };

    #[test]
    fn test_entry_resolution() {
        let root_path = Path::new(ROOT);

        let entry = EntryResolver {
            root_path: Some(ImmutPath::from(root_path)),
            ..Default::default()
        };

        let entry = entry.resolve(Some(root_path.join("main.typ").into()));

        assert_eq!(entry.root(), Some(ImmutPath::from(root_path)));
        assert_eq!(
            entry.main(),
            Some(WorkspaceResolver::workspace_file(
                entry.root().as_ref(),
                VirtualPath::new("main.typ")
            ))
        );
    }

    #[test]
    fn test_entry_resolution_multi_root() {
        let root_path = Path::new(ROOT);
        let root2_path = Path::new(ROOT2);

        let entry = EntryResolver {
            root_path: Some(ImmutPath::from(root_path)),
            roots: vec![ImmutPath::from(root_path), ImmutPath::from(root2_path)],
            ..Default::default()
        };

        {
            let entry = entry.resolve(Some(root_path.join("main.typ").into()));

            assert_eq!(entry.root(), Some(ImmutPath::from(root_path)));
            assert_eq!(
                entry.main(),
                Some(WorkspaceResolver::workspace_file(
                    entry.root().as_ref(),
                    VirtualPath::new("main.typ")
                ))
            );
        }

        {
            let entry = entry.resolve(Some(root2_path.join("main.typ").into()));

            assert_eq!(entry.root(), Some(ImmutPath::from(root2_path)));
            assert_eq!(
                entry.main(),
                Some(WorkspaceResolver::workspace_file(
                    entry.root().as_ref(),
                    VirtualPath::new("main.typ")
                ))
            );
        }
    }

    #[test]
    fn test_entry_resolution_default_multi_root() {
        let root_path = Path::new(ROOT);
        let root2_path = Path::new(ROOT2);

        let mut entry = EntryResolver {
            root_path: Some(ImmutPath::from(root_path)),
            roots: vec![ImmutPath::from(root_path), ImmutPath::from(root2_path)],
            ..Default::default()
        };

        {
            entry.entry = Some(root_path.join("main.typ").into());

            let default_entry = entry.resolve_default();

            assert_eq!(default_entry, entry.entry);
        }

        {
            entry.entry = Some(Path::new("main.typ").into());

            let default_entry = entry.resolve_default();

            assert_eq!(default_entry, Some(root_path.join("main.typ").into()));
        }
    }
}
