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
            return Some(vec![]);
        };
        if resolve_path_from_id(id, full_path.to_str()?).is_err() {
            return Some(vec![]);
        }

        let resolved_path = resolved_path.vpath().as_rootless_path_compat();
        let compl_path = if path_text.ends_with('/') {
            resolved_path
        } else {
            resolved_path.parent().unwrap_or(Path::new(""))
        };
        crate::log_debug_ct!("compl_path: {path:?} -> {compl_path:?}");

        // List the entries in the current completion directory.
        let entries = self.directory_entries(compl_path, preference)?;
        let mut seen_entries = HashSet::new();
        let mut folder_completions = vec![];
        let mut module_completions = vec![];
        for (entry_path, entry_kind) in entries {
            if entry_path == base.vpath().as_rootless_path_compat() {
                continue;
            }

            let is_folder = matches!(entry_kind, CompletionKind::Folder);
            let label = completion_label(&entry_path, has_root, is_folder, base_dir)?;
            if !seen_entries.insert(label.clone()) {
                continue;
            }
            crate::log_debug_ct!("compl_label: {label:?}");

            if is_folder {
                folder_completions.push((label, entry_kind));
            } else {
                module_completions.push((label, entry_kind));
            }
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
                .map(|typst_completion| {
                    let lsp_snippet = &typst_completion.0;
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
                        ..Default::default()
                    }
                })
                .collect_vec(),
        )
    }

    fn directory_entries(
        &self,
        dir: &Path,
        preference: &PathKind,
    ) -> Option<Vec<(PathBuf, CompletionKind)>> {
        let root = self.worker.ctx.world().entry_state().workspace_root()?;
        let physical_dir = root.join(dir);
        let regexes = preference.ext_matcher();
        let mut entries = vec![];

        if let Ok(read_dir) = std::fs::read_dir(&physical_dir) {
            for entry in read_dir.flatten() {
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                let path = entry.path();
                let rootless = path.strip_prefix(root.as_ref()).ok()?.to_owned();
                if file_type.is_dir() {
                    entries.push((rootless, CompletionKind::Folder));
                } else if file_type.is_file()
                    && path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .is_some_and(|ext| regexes.is_match(ext))
                {
                    entries.push((rootless, CompletionKind::File));
                }
            }
        }

        for shadow_path in self.worker.world().shadow_paths() {
            let Some((entry_path, entry_kind)) =
                shadow_file_to_dir_entry(shadow_path.strip_prefix(root.as_ref()).ok()?, dir)
            else {
                continue;
            };
            if matches!(entry_kind, CompletionKind::Folder)
                || entry_path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| regexes.is_match(ext))
            {
                entries.push((entry_path, entry_kind));
            }
        }

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
    if file_path.parent().unwrap_or(Path::new("")) == current_dir {
        return Some((file_path.into(), CompletionKind::File));
    }

    let relative = if current_dir.as_os_str().is_empty() {
        file_path
    } else {
        file_path.strip_prefix(current_dir).ok()?
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
