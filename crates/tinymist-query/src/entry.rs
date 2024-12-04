use anyhow::bail;
use reflexo_typst::{EntryState, ImmutPath, TypstFileId};
use typst::syntax::VirtualPath;

/// Entry resolver
#[derive(Debug, Default, Clone)]
pub struct EntryResolver {
    /// Specifies the root path of the project manually.
    pub root_path: Option<ImmutPath>,
    /// The workspace roots from initialization.
    pub roots: Vec<ImmutPath>,
    /// Default entry path from the configuration.
    pub entry: Option<ImmutPath>,
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
        // todo: formalize untitled path
        // let is_untitled = entry.as_ref().is_some_and(|p| p.starts_with("/untitled"));
        // let root_dir = self.determine_root(if is_untitled { None } else {
        // entry.as_ref() });
        let root_dir = self.root(entry.as_ref());

        let entry = match (entry, root_dir) {
            // (Some(entry), Some(root)) if is_untitled => Some(EntryState::new_rooted(
            //     root,
            //     Some(FileId::new(None, VirtualPath::new(entry))),
            // )),
            (Some(entry), Some(root)) => match entry.strip_prefix(&root) {
                Ok(stripped) => Some(EntryState::new_rooted(
                    root,
                    Some(TypstFileId::new(None, VirtualPath::new(stripped))),
                )),
                Err(err) => {
                    log::info!("Entry is not in root directory: err {err:?}: entry: {entry:?}, root: {root:?}");
                    EntryState::new_rootless(entry)
                }
            },
            (Some(entry), None) => EntryState::new_rootless(entry),
            (None, Some(root)) => Some(EntryState::new_workspace(root)),
            (None, None) => None,
        };

        entry.unwrap_or_else(|| match self.root(None) {
            Some(root) => EntryState::new_workspace(root),
            None => EntryState::new_detached(),
        })
    }

    /// Determines the default entry path.
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
    pub fn validate(&self) -> anyhow::Result<()> {
        if let Some(root) = &self.root_path {
            if !root.is_absolute() {
                bail!("rootPath or typstExtraArgs.root must be an absolute path: {root:?}");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
#[cfg(any(windows, unix, target_os = "macos"))]
mod entry_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_entry_resolution() {
        let root_path = Path::new(if cfg!(windows) { "C:\\root" } else { "/root" });

        let entry = EntryResolver {
            root_path: Some(ImmutPath::from(root_path)),
            ..Default::default()
        };

        let entry = entry.resolve(if cfg!(windows) {
            Some(Path::new("C:\\root\\main.typ").into())
        } else {
            Some(Path::new("/root/main.typ").into())
        });

        assert_eq!(entry.root(), Some(ImmutPath::from(root_path)));
        assert_eq!(
            entry.main(),
            Some(TypstFileId::new(None, VirtualPath::new("main.typ")))
        );
    }

    #[test]
    fn test_entry_resolution_multi_root() {
        let root_path = Path::new(if cfg!(windows) { "C:\\root" } else { "/root" });
        let root2_path = Path::new(if cfg!(windows) { "C:\\root2" } else { "/root2" });

        let entry = EntryResolver {
            root_path: Some(ImmutPath::from(root_path)),
            roots: vec![ImmutPath::from(root_path), ImmutPath::from(root2_path)],
            ..Default::default()
        };

        {
            let entry = entry.resolve(if cfg!(windows) {
                Some(Path::new("C:\\root\\main.typ").into())
            } else {
                Some(Path::new("/root/main.typ").into())
            });

            assert_eq!(entry.root(), Some(ImmutPath::from(root_path)));
            assert_eq!(
                entry.main(),
                Some(TypstFileId::new(None, VirtualPath::new("main.typ")))
            );
        }

        {
            let entry = entry.resolve(if cfg!(windows) {
                Some(Path::new("C:\\root2\\main.typ").into())
            } else {
                Some(Path::new("/root2/main.typ").into())
            });

            assert_eq!(entry.root(), Some(ImmutPath::from(root2_path)));
            assert_eq!(
                entry.main(),
                Some(TypstFileId::new(None, VirtualPath::new("main.typ")))
            );
        }
    }

    #[test]
    fn test_entry_resolution_default_multi_root() {
        let root_path = Path::new(if cfg!(windows) { "C:\\root" } else { "/root" });
        let root2_path = Path::new(if cfg!(windows) { "C:\\root2" } else { "/root2" });

        let mut entry = EntryResolver {
            root_path: Some(ImmutPath::from(root_path)),
            roots: vec![ImmutPath::from(root_path), ImmutPath::from(root2_path)],
            ..Default::default()
        };

        {
            entry.entry = if cfg!(windows) {
                Some(Path::new("C:\\root\\main.typ").into())
            } else {
                Some(Path::new("/root/main.typ").into())
            };

            let default_entry = entry.resolve_default();

            assert_eq!(default_entry, entry.entry);
        }

        {
            entry.entry = Some(Path::new("main.typ").into());

            let default_entry = entry.resolve_default();

            assert_eq!(
                default_entry,
                if cfg!(windows) {
                    Some(Path::new("C:\\root\\main.typ").into())
                } else {
                    Some(Path::new("/root/main.typ").into())
                }
            );
        }
    }
}
