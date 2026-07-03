//! Completion of paths (string literal).

use std::collections::HashSet;
use std::path::Component;

use tinymist_world::vfs::WorkspaceResolver;

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
            let Some(ast::Expr::Str(str)) = self.cursor.leaf.cast() else {
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
            full_text = str.get();
            is_in_text = true;
        } else {
            text = EcoString::default();
            full_text = EcoString::default();
            rng = self.cursor.cursor..self.cursor.cursor;
            is_in_text = false;
        }
        crate::log_debug_ct!("complete_path: is_in_text: {is_in_text:?}");
        let path = Path::new(text.as_str());
        let full_path = Path::new(full_text.as_str());
        let has_root = path.has_root();

        let src_path = id.vpath();
        let base = id;
        let dst_path = src_path.join(path.to_str()?).ok()?;
        let base_dir = base
            .vpath()
            .as_rooted_path_compat()
            .parent()
            .unwrap_or(Path::new("/"));
        // Check both the complete string and the cursor prefix: parent dirs are fine
        // while they remain within the workspace root, but completion must not offer
        // filesystem paths once either form walks past that root.
        if path_leaves_root(base_dir, full_path) || path_leaves_root(base_dir, path) {
            return Some(vec![]);
        }

        let mut compl_path = dst_path.as_rootless_path_compat();
        if !compl_path.is_dir() {
            compl_path = compl_path.parent().unwrap_or(Path::new(""));
        }
        crate::log_debug_ct!("compl_path: {src_path:?} + {path:?} -> {compl_path:?}");

        if compl_path.is_absolute() {
            log::warn!(
                "absolute path completion is not supported for security consideration {path:?}"
            );
            return None;
        }

        // find directory or files in the path
        let mut seen_folders = HashSet::new();
        let mut folder_completions = vec![];
        let mut module_completions = vec![];
        // todo: test it correctly
        for path in self.worker.ctx.completion_files(preference) {
            crate::log_debug_ct!("compl_check_path: {path:?}");

            // Skip self smartly
            if *path == base {
                continue;
            }

            let label: EcoString = if has_root {
                // diff with root
                unix_slash(path.vpath().as_rooted_path_compat()).into()
            } else {
                let path = path.vpath().as_rooted_path_compat();
                let w = tinymist_std::path::diff(path, base_dir)?;
                unix_slash(&w).into()
            };
            crate::log_debug_ct!("compl_label: {label:?}");

            module_completions.push((label.clone(), CompletionKind::File));
            push_parent_folder_completions(&mut folder_completions, &mut seen_folders, &label);
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
}

fn string_prefix_lossy(raw: &str) -> EcoString {
    let mut w = EcoString::new();
    w.push('"');
    w.push_str(raw);
    w.push('"');
    let partial_str = SyntaxNode::leaf(SyntaxKind::Str, w);
    if let Some(text) = partial_str.cast::<ast::Str>().map(|s| s.get()) {
        return text;
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

fn push_parent_folder_completions(
    folder_completions: &mut Vec<(EcoString, CompletionKind)>,
    seen_folders: &mut HashSet<EcoString>,
    label: &EcoString,
) {
    let mut end = 0;
    for (idx, ch) in label.char_indices() {
        if ch != '/' {
            continue;
        }

        end = idx + ch.len_utf8();
        let folder: EcoString = label[..end].into();
        if seen_folders.insert(folder.clone()) {
            folder_completions.push((folder, CompletionKind::Folder));
        }
    }

    if end == label.len() && seen_folders.insert(label.clone()) {
        folder_completions.push((label.clone(), CompletionKind::Folder));
    }
}

fn path_leaves_root(base_dir: &Path, typed: &Path) -> bool {
    let mut depth = if typed.has_root() {
        0
    } else {
        base_dir
            .components()
            .filter(|component| matches!(component, Component::Normal(_)))
            .count()
    };

    for component in typed.components() {
        match component {
            Component::ParentDir if depth == 0 => return true,
            Component::ParentDir => depth -= 1,
            Component::Normal(_) => depth += 1,
            Component::Prefix(_) | Component::RootDir | Component::CurDir => {}
        }
    }

    false
}
