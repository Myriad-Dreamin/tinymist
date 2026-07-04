//! Completion of paths (string literal).

use std::collections::HashSet;
use std::path::Component;

use tinymist_project::EntryReader;
use tinymist_world::{ShadowApi, vfs::WorkspaceResolver};

use super::*;
impl CompletionPair<'_, '_, '_> {
    pub fn complete_path(&mut self, preference: &PathKind) -> Option<Vec<CompletionItem>> {
        let id = self.cursor.source.id();
        if WorkspaceResolver::is_package_file(id) {
            log::info!("completion.path.skip: package file id={id:?}");
            return None;
        }

        let is_in_text;
        let text;
        let full_text;
        let rng;
        // todo: the non-str case
        if self.cursor.leaf.is::<ast::Str>() {
            let Some(ast::Expr::Str(_)) = self.cursor.leaf.cast() else {
                return None;
            };
            let vr = self.cursor.leaf.range();
            rng = vr.start + 1..vr.end - 1;
            if rng.start > rng.end
                || (self.cursor.cursor != rng.end && !rng.contains(&self.cursor.cursor))
            {
                return None;
            }

            text = string_prefix_lossy(&self.cursor.text[rng.start..self.cursor.cursor]);
            full_text = string_prefix_lossy(&self.cursor.text[rng.clone()]);
            is_in_text = true;
        } else {
            text = EcoString::default();
            full_text = EcoString::default();
            rng = self.cursor.cursor..self.cursor.cursor;
            is_in_text = false;
        }
        crate::log_debug_ct!("complete_path: is_in_text: {is_in_text:?}");
        log::info!(
            "completion.path.start: id={id:?}, cursor={}, is_in_text={is_in_text}, raw_prefix={:?}, raw_full={:?}, decoded_prefix={text:?}, decoded_full={full_text:?}, range={rng:?}",
            self.cursor.cursor,
            &self.cursor.text[rng.start..self.cursor.cursor],
            &self.cursor.text[rng.clone()],
        );
        let full_path = Path::new(full_text.as_str());
        let path_text = text.as_str();
        let path = Path::new(path_text);
        let has_root = path.has_root();

        let base = id;
        let base_dir = base
            .vpath()
            .as_rooted_path_compat()
            .parent()
            .unwrap_or(Path::new("/"));
        // Reuse Typst's path semantics for root escape checks. Parent dirs are
        // fine while Typst resolves them within the workspace root.
        let Ok(resolved_path) = resolve_path_from_id(id, path.to_str()?) else {
            log::info!("completion.path.resolve_prefix_failed: id={id:?}, path={path:?}");
            return Some(vec![]);
        };
        if resolve_path_from_id(id, full_path.to_str()?).is_err() {
            log::info!("completion.path.resolve_full_failed: id={id:?}, full_path={full_path:?}");
            return Some(vec![]);
        }

        let resolved_path = resolved_path.vpath().as_rootless_path_compat();
        let compl_path =
            if (path_text.is_empty() && full_text.is_empty()) || path_text.ends_with('/') {
                resolved_path
            } else {
                resolved_path.parent().unwrap_or(Path::new(""))
            };
        crate::log_debug_ct!("compl_path: {path:?} -> {compl_path:?}");
        log::info!(
            "completion.path.resolved: path={path:?}, full_path={full_path:?}, resolved={resolved_path:?}, compl_path={compl_path:?}, has_root={has_root}, base_dir={base_dir:?}"
        );

        // List entries in the current completion directory.
        let dir_entries = self.directory_entries(compl_path)?;
        log::info!(
            "completion.path.dir_entries: compl_path={compl_path:?}, entries={}",
            dir_entries.len()
        );
        let mut seen_entries = HashSet::new();
        let mut folder_completions = vec![];
        let mut module_completions = vec![];
        let base_path = normalize_rootless_path(base.vpath().as_rootless_path_compat());
        for (entry_path, entry_kind) in dir_entries {
            if entry_path == base_path {
                continue;
            }

            let is_folder = matches!(entry_kind, CompletionKind::Folder);
            let Some(label) = completion_label(&entry_path, has_root, is_folder, base_dir) else {
                log::info!(
                    "completion.path.label_failed: entry_path={entry_path:?}, has_root={has_root}, is_folder={is_folder}, base_dir={base_dir:?}"
                );
                continue;
            };
            if seen_entries.insert(label.clone()) {
                crate::log_debug_ct!("compl_dir_label: {label:?}");
                if is_folder {
                    folder_completions.push((label, entry_kind));
                } else {
                    module_completions.push((label, entry_kind));
                }
            }
        }

        for file in self.worker.ctx.completion_files(preference) {
            crate::log_debug_ct!("compl_check_path: {file:?}");

            // Skip self smartly
            if *file == base {
                continue;
            }

            let file_path = normalize_rootless_path(file.vpath().as_rootless_path_compat());
            if file_path == base_path {
                continue;
            }

            let Some(label) = completion_label(&file_path, has_root, false, base_dir) else {
                log::info!(
                    "completion.path.label_failed: file_path={file_path:?}, has_root={has_root}, is_folder=false, base_dir={base_dir:?}"
                );
                continue;
            };
            if !seen_entries.insert(label.clone()) {
                continue;
            }
            crate::log_debug_ct!("compl_label: {label:?}");
            module_completions.push((label, CompletionKind::File));
        }

        let replace_range = self.cursor.lsp_range_of(rng);

        fn is_dot_or_slash(ch: &char) -> bool {
            matches!(*ch, '.' | '/')
        }

        let path_priority_cmp = |lhs: &str, rhs: &str| {
            // files are more important than dot started paths
            if lhs.starts_with('.') || rhs.starts_with('.') {
                // compare consecutive dots and slashes
                let a_prefix = lhs.chars().take_while(is_dot_or_slash).count();
                let b_prefix = rhs.chars().take_while(is_dot_or_slash).count();
                if a_prefix != b_prefix {
                    return a_prefix.cmp(&b_prefix);
                }
            }
            lhs.cmp(rhs)
        };

        module_completions.sort_by(|a, b| path_priority_cmp(&a.0, &b.0));
        folder_completions.sort_by(|a, b| path_priority_cmp(&a.0, &b.0));

        let mut sorter = 0;
        let digits = (module_completions.len() + folder_completions.len())
            .to_string()
            .len();
        let completions = folder_completions.into_iter().chain(module_completions);
        log::info!(
            "completion.path.done: folders={}, files={}",
            completions
                .clone()
                .filter(|(_, kind)| matches!(kind, CompletionKind::Folder))
                .count(),
            completions
                .clone()
                .filter(|(_, kind)| matches!(kind, CompletionKind::File))
                .count()
        );
        Some(
            completions
                .map(|typst_completion| {
                    let lsp_snippet = &typst_completion.0;
                    let is_folder = matches!(typst_completion.1, CompletionKind::Folder);
                    let text_edit = EcoTextEdit::new(
                        replace_range,
                        if is_in_text {
                            lsp_snippet.clone()
                        } else {
                            eco_format!(r#""{lsp_snippet}""#)
                        },
                    );

                    let sort_text = eco_format!("{sorter:0>digits$}");
                    sorter += 1;

                    // todo: not all clients support label details
                    LspCompletion {
                        label: typst_completion.0,
                        kind: typst_completion.1,
                        detail: None,
                        text_edit: Some(text_edit.into()),
                        // don't sort me
                        sort_text: Some(sort_text),
                        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                        command: self
                            .worker
                            .ctx
                            .analysis
                            .trigger_suggest(is_folder)
                            .map(From::from),
                        ..Default::default()
                    }
                })
                .collect_vec(),
        )
    }

    fn directory_entries(&self, dir: &Path) -> Option<Vec<(PathBuf, CompletionKind)>> {
        let Some(root) = self.worker.ctx.world().entry_state().workspace_root() else {
            log::info!("completion.path.directory_entries: no workspace root, dir={dir:?}");
            return None;
        };
        let dir = normalize_rootless_path(dir);
        let physical_dir = root.join(&dir);
        let mut entries = vec![];
        let fs_enabled = self
            .worker
            .ctx
            .analysis
            .completion_feat
            .path_completion_by_filesystem;
        log::info!(
            "completion.path.directory_entries: root={:?}, dir={dir:?}, physical_dir={physical_dir:?}, fs_enabled={fs_enabled}",
            root.as_ref()
        );

        #[cfg(not(target_arch = "wasm32"))]
        if fs_enabled {
            match std::fs::read_dir(&physical_dir) {
                Ok(read_dir) => {
                    let before = entries.len();
                    for entry in read_dir.flatten() {
                        let Ok(file_type) = entry.file_type() else {
                            continue;
                        };
                        let path = entry.path();
                        let Ok(rootless) = path.strip_prefix(root.as_ref()) else {
                            log::info!(
                                "completion.path.directory_entries.fs_skip_outside_root: path={path:?}, root={:?}",
                                root.as_ref()
                            );
                            continue;
                        };
                        let rootless = normalize_rootless_path(rootless);
                        if file_type.is_dir() {
                            entries.push((rootless, CompletionKind::Folder));
                        } else if file_type.is_file() {
                            entries.push((rootless, CompletionKind::File));
                        }
                    }
                    log::info!(
                        "completion.path.directory_entries.fs: physical_dir={physical_dir:?}, entries={}",
                        entries.len() - before
                    );
                }
                Err(err) => {
                    log::info!(
                        "completion.path.directory_entries.fs_failed: physical_dir={physical_dir:?}, error={err}"
                    );
                }
            }
        }

        let before_shadow = entries.len();
        for shadow_path in self.worker.world().shadow_paths() {
            let Ok(rootless_shadow_path) = shadow_path.strip_prefix(root.as_ref()) else {
                log::info!(
                    "completion.path.directory_entries.shadow_skip_outside_root: path={:?}, root={:?}",
                    shadow_path.as_ref(),
                    root.as_ref()
                );
                continue;
            };
            if let Some(entry) = shadow_file_to_dir_entry(rootless_shadow_path, &dir) {
                entries.push(entry);
            }
        }
        log::info!(
            "completion.path.directory_entries.shadow: entries={}",
            entries.len() - before_shadow
        );

        Some(entries)
    }
}

fn string_prefix_lossy(raw: &str) -> EcoString {
    if !raw.contains('\\') {
        let mut w = EcoString::new();
        w.push('"');
        w.push_str(raw);
        w.push('"');
        let partial_str = SyntaxNode::leaf(SyntaxKind::Str, w);
        if let Some(text) = partial_str.cast::<ast::Str>().map(|s| s.get()) {
            return text;
        }
    }

    let mut decoded = EcoString::new();
    let mut chars = raw.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            decoded.push(ch);
            continue;
        }

        match chars.next() {
            Some('"') => decoded.push('"'),
            Some('\\') => decoded.push('\\'),
            Some('/') => decoded.push('/'),
            Some('n') => decoded.push('\n'),
            Some('r') => decoded.push('\r'),
            Some('t') => decoded.push('\t'),
            Some(ch) => decoded.push(ch),
            None => break,
        }
    }

    decoded
}

fn shadow_file_to_dir_entry(
    file_path: &Path,
    current_dir: &Path,
) -> Option<(PathBuf, CompletionKind)> {
    let file_path = normalize_rootless_path(file_path);
    let current_dir = normalize_rootless_path(current_dir);
    if file_path.parent().unwrap_or(Path::new("")) == current_dir {
        return Some((file_path, CompletionKind::File));
    }

    let relative = if current_dir.as_os_str().is_empty() {
        file_path.as_path()
    } else {
        file_path.strip_prefix(&current_dir).ok()?
    };
    let mut components = relative.components();
    let Component::Normal(first) = components.next()? else {
        return None;
    };
    components.next()?;

    Some((current_dir.join(first), CompletionKind::Folder))
}

fn completion_label(
    rootless_path: &Path,
    has_root: bool,
    is_folder: bool,
    base_dir: &Path,
) -> Option<EcoString> {
    let rooted_path = Path::new("/").join(rootless_path);
    let label_path = if has_root {
        rooted_path
    } else {
        tinymist_std::path::diff(&rooted_path, base_dir)?
    };
    let mut label: EcoString = unix_slash(&label_path).into();
    if is_folder && !label.ends_with('/') {
        label.push('/');
    }
    Some(label)
}

fn normalize_rootless_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(component) => normalized.push(component),
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(_) | Component::RootDir => {}
        }
    }
    normalized
}
