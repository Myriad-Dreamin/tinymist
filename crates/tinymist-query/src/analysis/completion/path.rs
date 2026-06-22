//! Completion of paths (string literal).

use tinymist_world::vfs::WorkspaceResolver;

use super::*;
impl CompletionPair<'_, '_, '_> {
    fn unique_const_string(ty: &Ty) -> Option<EcoString> {
        fn merge_unique<'a>(mut values: impl Iterator<Item = &'a Ty>) -> Option<EcoString> {
            let first = unique_const_string(values.next()?)?;
            values.try_fold(first, |acc, ty| {
                let value = unique_const_string(ty)?;
                (value == acc).then_some(acc)
            })
        }

        fn unique_bound_const<'a>(values: impl Iterator<Item = &'a Ty>) -> Option<EcoString> {
            let mut found = None;
            for ty in values {
                let Some(value) = unique_const_string(ty) else {
                    continue;
                };
                if found.as_ref().is_some_and(|prev| prev != &value) {
                    return None;
                }
                found = Some(value);
            }
            found
        }

        fn unique_const_string(ty: &Ty) -> Option<EcoString> {
            match ty {
                Ty::Value(ins) => match &ins.val {
                    Value::Str(s) => Some(s.as_str().into()),
                    _ => None,
                },
                Ty::Param(p) => unique_const_string(&p.ty),
                Ty::Let(bounds) => unique_bound_const(bounds.lbs.iter()),
                Ty::Union(types) => unique_bound_const(types.iter()),
                Ty::Binary(binary) if binary.op == ast::BinOp::Add => {
                    let [lhs, rhs] = binary.operands();
                    Some(eco_format!(
                        "{}{}",
                        unique_const_string(lhs)?,
                        unique_const_string(rhs)?
                    ))
                }
                Ty::If(if_) => merge_unique([if_.then.as_ref(), if_.else_.as_ref()].into_iter()),
                _ => None,
            }
        }

        unique_const_string(ty)
    }

    fn const_string_expr(&mut self, node: &LinkedNode) -> Option<EcoString> {
        if let Some(str) = node.cast::<ast::Str>() {
            return Some(str.get());
        }

        if let Some(paren) = node.cast::<ast::Parenthesized>() {
            let expr = paren.expr();
            let expr_node = node.find(expr.span())?;
            return self.const_string_expr(&expr_node);
        }

        if let Some(binary) = node.cast::<ast::Binary>()
            && binary.op() == ast::BinOp::Add
        {
            let lhs = binary.lhs();
            let rhs = binary.rhs();
            let lhs_node = node.find(lhs.span())?;
            let rhs_node = node.find(rhs.span())?;
            let lhs = self.const_string_expr(&lhs_node)?;
            let rhs = self.const_string_expr(&rhs_node)?;
            return Some(eco_format!("{lhs}{rhs}"));
        }

        // Resolve constant string values through type checking info (e.g. `#let dir = "dir/"`).
        if let Some(ty) = self.worker.ctx.post_type_of_node(node.clone())
            && let Some(s) = Self::unique_const_string(&ty)
        {
            return Some(s);
        }

        None
    }

    fn concat_string_affixes(&mut self) -> (EcoString, EcoString) {
        let mut prefix = EcoString::new();
        let mut suffix = EcoString::new();
        let mut focus = self.cursor.leaf.clone();

        loop {
            let Some(parent) = focus.parent() else {
                break;
            };
            let parent = (*parent).clone();

            if let Some(paren) = parent.cast::<ast::Parenthesized>() {
                let _ = paren;
                focus = parent;
                continue;
            }

            let Some(binary) = parent.cast::<ast::Binary>() else {
                break;
            };
            if binary.op() != ast::BinOp::Add {
                break;
            }

            let lhs = binary.lhs();
            let rhs = binary.rhs();
            let lhs_node = parent.find(lhs.span());
            let rhs_node = parent.find(rhs.span());
            let (Some(lhs_node), Some(rhs_node)) = (lhs_node, rhs_node) else {
                break;
            };

            if lhs_node.find(focus.span()).is_some() {
                if let Some(rhs) = self.const_string_expr(&rhs_node) {
                    suffix.push_str(rhs.as_str());
                }
                focus = parent;
                continue;
            }

            if rhs_node.find(focus.span()).is_some() {
                if let Some(lhs) = self.const_string_expr(&lhs_node) {
                    prefix = lhs + prefix;
                }
                focus = parent;
                continue;
            }

            break;
        }

        (prefix, suffix)
    }

    pub fn complete_path(&mut self, preference: &PathKind) -> Option<Vec<CompletionItem>> {
        let id = self.cursor.source.id();
        if WorkspaceResolver::is_package_file(id) {
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
        let dst_path = src_path.join(path.to_str()?).ok()?;
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
        let folder_completions: Vec<(EcoString, EcoString, CompletionKind)> = vec![];
        let mut module_completions: Vec<(EcoString, EcoString, CompletionKind)> = vec![];

        let (concat_prefix, concat_suffix) = if is_in_text {
            self.concat_string_affixes()
        } else {
            (EcoString::new(), EcoString::new())
        };

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
                let base = base
                    .vpath()
                    .as_rooted_path_compat()
                    .parent()
                    .unwrap_or(Path::new("/"));
                let path = path.vpath().as_rooted_path_compat();
                let w = tinymist_std::path::diff(path, base)?;
                unix_slash(&w).into()
            };
            crate::log_debug_ct!("compl_label: {label:?}");

            let insert = {
                let label_str = label.as_str();
                if !concat_prefix.is_empty() && !label_str.starts_with(concat_prefix.as_str()) {
                    continue;
                }
                if !concat_suffix.is_empty() && !label_str.ends_with(concat_suffix.as_str()) {
                    continue;
                }

                let mut insert_str = label_str;
                if !concat_prefix.is_empty() {
                    let Some(stripped) = insert_str.strip_prefix(concat_prefix.as_str()) else {
                        continue;
                    };
                    insert_str = stripped;
                }
                if !concat_suffix.is_empty() {
                    let Some(stripped) = insert_str.strip_suffix(concat_suffix.as_str()) else {
                        continue;
                    };
                    insert_str = stripped;
                }

                EcoString::from(insert_str)
            };

            module_completions.push((label, insert, CompletionKind::File));

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
                    let lsp_snippet = &typst_completion.1;
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
                        kind: typst_completion.2,
                        detail: None,
                        text_edit: Some(text_edit.into()),
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
