use super::*;
impl CompletionPair<'_, '_, '_> {
    pub fn complete_path(&mut self, preference: &PathPreference) -> Option<Vec<CompletionItem>> {
        let id = self.cursor.source.id();
        if id.package().is_some() {
            return None;
        }

        let is_in_text;
        let text;
        let rng;
        // todo: the non-str case
        if self.cursor.leaf.is::<ast::Str>() {
            let vr = self.cursor.leaf.range();
            rng = vr.start + 1..vr.end - 1;
            if rng.start > rng.end
                || (self.cursor.cursor != rng.end && !rng.contains(&self.cursor.cursor))
            {
                return None;
            }

            let mut w = EcoString::new();
            w.push('"');
            w.push_str(&self.cursor.text[rng.start..self.cursor.cursor]);
            w.push('"');
            let partial_str = SyntaxNode::leaf(SyntaxKind::Str, w);

            text = partial_str.cast::<ast::Str>()?.get();
            is_in_text = true;
        } else {
            text = EcoString::default();
            rng = self.cursor.cursor..self.cursor.cursor;
            is_in_text = false;
        }
        crate::log_debug_ct!("complete_path: is_in_text: {is_in_text:?}");
        let path = Path::new(text.as_str());
        let has_root = path.has_root();

        let src_path = id.vpath();
        let base = id;
        let dst_path = src_path.join(path);
        let mut compl_path = dst_path.as_rootless_path();
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
        let folder_completions = vec![];
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
                unix_slash(path.vpath().as_rooted_path()).into()
            } else {
                let base = base
                    .vpath()
                    .as_rooted_path()
                    .parent()
                    .unwrap_or(Path::new("/"));
                let path = path.vpath().as_rooted_path();
                let w = pathdiff::diff_paths(path, base)?;
                unix_slash(&w).into()
            };
            crate::log_debug_ct!("compl_label: {label:?}");

            module_completions.push((label, CompletionKind::File));

            // todo: looks like the folder completion is broken
            // if path.is_dir() {
            //     folder_completions.push((label, CompletionKind::Folder));
            // }
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
        // folder_completions.sort_by(|a, b| path_priority_cmp(&a.0, &b.0));

        let mut sorter = 0;
        let digits = (module_completions.len() + folder_completions.len())
            .to_string()
            .len();
        let completions = module_completions.into_iter().chain(folder_completions);
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

                    // todo: no all clients support label details
                    LspCompletion {
                        label: typst_completion.0,
                        kind: typst_completion.1,
                        detail: None,
                        text_edit: Some(text_edit),
                        // don't sort me
                        sort_text: Some(sort_text),
                        filter_text: Some("".into()),
                        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                        ..Default::default()
                    }
                })
                .collect_vec(),
        )
    }
}
