use std::sync::Once;

use once_cell::sync::Lazy;
use regex::RegexSet;

use crate::prelude::*;

/// The dependency information of a module (file).
#[derive(Debug, Clone)]
pub struct ModuleDependency {
    /// The dependencies of this module.
    pub dependencies: EcoVec<TypstFileId>,
    /// The dependents of this module.
    pub dependents: EcoVec<TypstFileId>,
}

/// Construct the module dependencies of the given context.
///
/// It will scan all the files in the context, using
/// [`AnalysisContext::source_files`], and find the dependencies and dependents
/// of each file.
pub fn construct_module_dependencies(
    ctx: &mut LocalContext,
) -> HashMap<TypstFileId, ModuleDependency> {
    let mut dependencies = HashMap::new();
    let mut dependents = HashMap::new();

    for file_id in ctx.source_files().clone() {
        let source = match ctx.shared.source_by_id(file_id) {
            Ok(source) => source,
            Err(err) => {
                static WARN_ONCE: Once = Once::new();
                WARN_ONCE.call_once(|| {
                    log::warn!("construct_module_dependencies: {err:?}", err = err);
                });
                continue;
            }
        };

        let file_id = source.id();
        let ei = ctx.shared.expr_stage(&source);

        dependencies
            .entry(file_id)
            .or_insert_with(|| ModuleDependency {
                dependencies: ei.imports.iter().cloned().collect(),
                dependents: EcoVec::default(),
            });
        for dep in ei.imports.clone() {
            dependents
                .entry(dep)
                .or_insert_with(EcoVec::new)
                .push(file_id);
        }
    }

    for (file_id, dependents) in dependents {
        if let Some(dep) = dependencies.get_mut(&file_id) {
            dep.dependents = dependents;
        }
    }

    dependencies
}

fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with('.'))
        .unwrap_or(false)
}

/// Scan the files in the workspace and return the file ids.
///
/// Note: this function will touch the physical file system.
pub(crate) fn scan_workspace_files<T>(
    root: &Path,
    ext: &RegexSet,
    f: impl Fn(&Path) -> T,
) -> Vec<T> {
    let mut res = vec![];
    let mut it = walkdir::WalkDir::new(root).follow_links(false).into_iter();
    loop {
        let de = match it.next() {
            None => break,
            Some(Err(_err)) => continue,
            Some(Ok(entry)) => entry,
        };
        if is_hidden(&de) {
            if de.file_type().is_dir() {
                it.skip_current_dir();
            }
            continue;
        }

        /// this is a temporary solution to ignore some common build directories
        static IGNORE_REGEX: Lazy<RegexSet> = Lazy::new(|| {
            RegexSet::new([
                r#"^build$"#,
                r#"^target$"#,
                r#"^node_modules$"#,
                r#"^out$"#,
                r#"^dist$"#,
            ])
            .unwrap()
        });
        if de
            .path()
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| IGNORE_REGEX.is_match(s))
        {
            if de.file_type().is_dir() {
                it.skip_current_dir();
            }
            continue;
        }

        if !de.file_type().is_file() {
            continue;
        }
        if !de
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| ext.is_match(e))
        {
            continue;
        }

        let path = de.path();
        let relative_path = match path.strip_prefix(root) {
            Ok(p) => p,
            Err(err) => {
                log::warn!("failed to strip prefix, path: {path:?}, root: {root:?}: {err}");
                continue;
            }
        };

        res.push(f(relative_path));

        // two times of max number of typst file ids
        if res.len() >= (u16::MAX as usize) {
            break;
        }
    }

    res
}
