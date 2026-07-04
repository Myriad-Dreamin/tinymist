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
            return None;
        }

        let (is_in_text, raw_text, text, full_text, rng) = if self.cursor.leaf.is::<ast::Str>() {
            let vr = self.cursor.leaf.range();
            let rng = vr.start + 1..vr.end - 1;
            if rng.start > rng.end
                || (self.cursor.cursor != rng.end && !rng.contains(&self.cursor.cursor))
            {
                return None;
            }

            let raw_text = &self.cursor.text[rng.start..self.cursor.cursor];
            let raw_full_text = &self.cursor.text[rng.clone()];
            (
                true,
                raw_text,
                string_prefix_lossy(raw_text),
                string_prefix_lossy(raw_full_text),
                rng,
            )
        } else {
            (
                false,
                "",
                EcoString::default(),
                EcoString::default(),
                self.cursor.cursor..self.cursor.cursor,
            )
        };
        crate::log_debug_ct!("complete_path: is_in_text: {is_in_text:?}");

        let path_text = text.as_str();
        let path = Path::new(path_text);
        let has_root = path.has_root();

        let base = id;
        let base_dir = base
            .vpath()
            .as_rooted_path_compat()
            .parent()
            .unwrap_or(Path::new("/"));
        let Ok(resolved_path) = resolve_path_from_id(id, path.to_str()?) else {
            return Some(vec![]);
        };
        if resolve_path_from_id(id, full_text.as_str()).is_err() {
            return Some(vec![]);
        }

        let resolved_path = resolved_path.vpath().as_rootless_path_compat();
        let path_is_dir_like = (path_text.is_empty() && full_text.is_empty())
            || path_text.ends_with('/')
            || path_text == "."
            || path_text.ends_with("/.");
        let compl_path = if path_is_dir_like {
            resolved_path
        } else {
            resolved_path.parent().unwrap_or(Path::new(""))
        };
        crate::log_debug_ct!("compl_path: {path:?} -> {compl_path:?}");

        let mut filter_prefix = completion_label(resolved_path, has_root, false, base_dir)?;
        if filter_prefix == "." {
            filter_prefix.clear();
        }
        if path_is_dir_like && !filter_prefix.is_empty() && !filter_prefix.ends_with('/') {
            filter_prefix.push('/');
        }

        let mut seen_entries = HashSet::new();
        let mut folder_completions = vec![];
        let mut module_completions = vec![];
        let base_path = normalize_rootless_path(base.vpath().as_rootless_path_compat());
        let mut push_completion = |entry_path: &Path, kind: CompletionKind| {
            if entry_path == base_path {
                return;
            }

            let is_folder = matches!(kind, CompletionKind::Folder);
            let Some(label) = completion_label(entry_path, has_root, is_folder, base_dir) else {
                return;
            };
            if !filter_prefix.is_empty() && !label.starts_with(filter_prefix.as_str()) {
                return;
            }
            if !seen_entries.insert(label.clone()) {
                return;
            }

            if is_folder {
                folder_completions.push((label, kind));
            } else {
                module_completions.push((label, kind));
            }
        };

        for (entry_path, kind) in self.directory_entries(compl_path) {
            push_completion(&entry_path, kind);
        }

        for file in self.worker.ctx.completion_files(preference) {
            crate::log_debug_ct!("compl_check_path: {file:?}");

            if *file == base {
                continue;
            }

            let file_path = normalize_rootless_path(file.vpath().as_rootless_path_compat());
            push_completion(&file_path, CompletionKind::File);
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
        Some(
            completions
                .map(|(label, kind)| {
                    let is_folder = matches!(kind, CompletionKind::Folder);
                    let lsp_snippet = label.clone();
                    let text_edit = EcoTextEdit::new(
                        replace_range,
                        if is_in_text {
                            lsp_snippet
                        } else {
                            eco_format!(r#""{lsp_snippet}""#)
                        },
                    );

                    let sort_text = eco_format!("{sorter:0>digits$}");
                    sorter += 1;

                    // todo: not all clients support label details
                    LspCompletion {
                        filter_text: completion_filter_text(&label, &filter_prefix, raw_text),
                        label,
                        kind,
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

    fn directory_entries(&self, dir: &Path) -> Vec<(PathBuf, CompletionKind)> {
        let Some(root) = self.worker.ctx.world().entry_state().workspace_root() else {
            return vec![];
        };

        let dir = normalize_rootless_path(dir);
        let mut entries = vec![];

        #[cfg(not(target_arch = "wasm32"))]
        if self
            .worker
            .ctx
            .analysis
            .completion_feat
            .path_completion_by_filesystem
        {
            if let Ok(read_dir) = std::fs::read_dir(root.join(&dir)) {
                entries.extend(read_dir.flatten().filter_map(|entry| {
                    let path = entry.path();
                    let rootless = path.strip_prefix(root.as_ref()).ok()?;
                    let kind = if entry.file_type().ok()?.is_dir() {
                        CompletionKind::Folder
                    } else {
                        CompletionKind::File
                    };
                    Some((normalize_rootless_path(rootless), kind))
                }));
            }
        }

        for shadow_path in self.worker.world().shadow_paths() {
            let Ok(rootless) = shadow_path.strip_prefix(root.as_ref()) else {
                continue;
            };
            if let Some(entry) = shadow_file_to_dir_entry(rootless, &dir) {
                entries.push(entry);
            }
        }

        entries
    }
}

fn string_prefix_lossy(raw: &str) -> EcoString {
    let mut w = EcoString::new();
    w.push('"');
    w.push_str(raw);
    w.push('"');
    let partial_str = SyntaxNode::leaf(SyntaxKind::Str, w);
    partial_str
        .cast::<ast::Str>()
        .map(|s| s.get())
        .unwrap_or_default()
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

fn completion_filter_text(label: &str, prefix: &str, raw_prefix: &str) -> Option<EcoString> {
    let filter_text = label
        .strip_prefix(prefix)
        .map(|suffix| eco_format!("{raw_prefix}{suffix}"))
        .unwrap_or_else(|| label.into());

    (filter_text.as_str() != label).then_some(filter_text)
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

#[cfg(test)]
mod tests {
    use super::string_prefix_lossy;

    #[test]
    fn string_prefix_lossy_normal_string() {
        assert_eq!(
            string_prefix_lossy("assets/logo.png").as_str(),
            "assets/logo.png"
        );
        assert_eq!(
            string_prefix_lossy(r#"assets/\u{6c}ogo.png"#).as_str(),
            "assets/logo.png"
        );
    }

    #[test]
    fn string_prefix_lossy_incomplete_escape() {
        assert_eq!(string_prefix_lossy(r#"assets\"#).as_str(), r#"assets\"#);
        assert_eq!(
            string_prefix_lossy(r#"assets/\u{"#).as_str(),
            r#"assets/\u{"#
        );
    }

    #[test]
    fn string_prefix_lossy_syntax_error_text() {
        assert_eq!(
            string_prefix_lossy(r#"assets/"bad"#).as_str(),
            r#"assets/"bad"#
        );
        assert_eq!(
            string_prefix_lossy(r#"assets/\u{zz}"#).as_str(),
            r#"assets/\u{zz}"#
        );
    }
}
