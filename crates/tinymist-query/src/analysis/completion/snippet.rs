//! Snippet completions.
//!
//! A prefix snippet is a snippet that completes non-existing items. For example
//! `RR` is completed as `ℝ`.
//!
//! A postfix snippet is a snippet that modifies existing items by the dot
//! accessor syntax. For example `$ RR.abs| $` is completed as `$ abs(RR) $`.

use super::*;

impl CompletionPair<'_, '_, '_> {
    /// Add a (prefix) snippet completion.
    pub fn snippet_completion(&mut self, label: &str, snippet: &str, docs: &str) {
        self.push_completion(Completion {
            kind: CompletionKind::Syntax,
            label: label.into(),
            apply: Some(snippet.into()),
            detail: Some(docs.into()),
            command: self
                .worker
                .ctx
                .analysis
                .trigger_on_snippet(snippet.contains("${"))
                .map(From::from),
            ..Completion::default()
        });
    }

    pub fn snippet_completions(
        &mut self,
        mode: Option<InterpretMode>,
        surrounding_syntax: Option<SurroundingSyntax>,
    ) {
        let mut keys = vec![CompletionContextKey::new(mode, surrounding_syntax)];
        if mode.is_some() {
            keys.push(CompletionContextKey::new(None, surrounding_syntax));
        }
        if surrounding_syntax.is_some() {
            keys.push(CompletionContextKey::new(mode, None));
            if mode.is_some() {
                keys.push(CompletionContextKey::new(None, None));
            }
        }
        let applies_to = |snippet: &PrefixSnippet| keys.iter().any(|key| snippet.applies_to(key));

        for snippet in DEFAULT_PREFIX_SNIPPET.iter() {
            if !applies_to(snippet) {
                continue;
            }

            let analysis = &self.worker.ctx.analysis;
            let command = match snippet.command {
                Some(CompletionCommand::TriggerSuggest) => analysis.trigger_suggest(true),
                None => analysis.trigger_on_snippet(snippet.snippet.contains("${")),
            };

            self.push_completion(Completion {
                kind: CompletionKind::Syntax,
                label: snippet.label.as_ref().into(),
                apply: Some(snippet.snippet.as_ref().into()),
                detail: Some(snippet.description.as_ref().into()),
                command: command.map(From::from),
                ..Completion::default()
            });
        }
    }

    pub fn postfix_completions(&mut self, node: &LinkedNode, ty: Ty) -> Option<()> {
        if !self.worker.ctx.analysis.completion_feat.postfix() {
            return None;
        }

        let _ = node;

        if !matches!(self.cursor.surrounding_syntax, SurroundingSyntax::Regular) {
            return None;
        }

        let cursor_mode = interpret_mode_at(Some(node));
        let is_content = ty.is_content(&());
        crate::log_debug_ct!("post snippet is_content: {is_content}");

        let rng = node.range();
        for snippet in self
            .worker
            .ctx
            .analysis
            .completion_feat
            .postfix_snippets()
            .clone()
        {
            if !snippet.mode.contains(&cursor_mode) {
                continue;
            }

            let scope = match snippet.scope {
                PostfixSnippetScope::Value => true,
                PostfixSnippetScope::Content => is_content,
            };
            if !scope {
                continue;
            }
            crate::log_debug_ct!("post snippet: {}", snippet.label);

            static TYPST_SNIPPET_PLACEHOLDER_RE: LazyLock<Regex> =
                LazyLock::new(|| Regex::new(r"\$\{(.*?)\}").unwrap());

            let parsed_snippet = snippet.parsed_snippet.get_or_init(|| {
                let split = TYPST_SNIPPET_PLACEHOLDER_RE
                    .find_iter(&snippet.snippet)
                    .map(|s| (&s.as_str()[2..s.as_str().len() - 1], s.start(), s.end()))
                    .collect::<Vec<_>>();
                if split.len() > 2 {
                    return None;
                }

                let split0 = split[0];
                let split1 = split.get(1);

                if split0.0.contains("node") {
                    Some(ParsedSnippet {
                        node_before: snippet.snippet[..split0.1].into(),
                        node_before_before_cursor: None,
                        node_after: snippet.snippet[split0.2..].into(),
                    })
                } else {
                    split1.map(|split1| ParsedSnippet {
                        node_before_before_cursor: Some(snippet.snippet[..split0.1].into()),
                        node_before: snippet.snippet[split0.1..split1.1].into(),
                        node_after: snippet.snippet[split1.2..].into(),
                    })
                }
            });
            crate::log_debug_ct!("post snippet: {} on {:?}", snippet.label, parsed_snippet);
            let Some(ParsedSnippet {
                node_before,
                node_before_before_cursor,
                node_after,
            }) = parsed_snippet
            else {
                continue;
            };

            let base = Completion {
                kind: CompletionKind::Syntax,
                apply: Some("".into()),
                label: snippet.label.clone(),
                label_details: snippet.label_detail.clone(),
                detail: Some(snippet.description.clone()),
                // range: Some(range),
                ..Default::default()
            };
            if let Some(node_before_before_cursor) = &node_before_before_cursor {
                let node_content = node.get().clone().into_text();
                let before = EcoTextEdit {
                    range: self.cursor.lsp_range_of(rng.start..self.cursor.from),
                    new_text: EcoString::new(),
                };

                self.push_completion(Completion {
                    apply: Some(eco_format!(
                        "{node_before_before_cursor}{node_before}{node_content}{node_after}"
                    )),
                    additional_text_edits: Some(vec![before]),
                    ..base
                });
            } else {
                let before = EcoTextEdit {
                    range: self.cursor.lsp_range_of(rng.start..rng.start),
                    new_text: node_before.clone(),
                };
                let after = EcoTextEdit {
                    range: self.cursor.lsp_range_of(rng.end..self.cursor.from),
                    new_text: "".into(),
                };
                self.push_completion(Completion {
                    apply: Some(node_after.clone()),
                    additional_text_edits: Some(vec![before, after]),
                    ..base
                });
            }
        }

        Some(())
    }

    /// Make ufcs-style completions. Note: you must check that node is a content
    /// before calling this. Todo: ufcs completions for other types.
    pub fn ufcs_completions(&mut self, node: &LinkedNode) {
        if !self.worker.ctx.analysis.completion_feat.any_ufcs() {
            return;
        }

        if !matches!(self.cursor.surrounding_syntax, SurroundingSyntax::Regular) {
            return;
        }

        let Some(defines) = self.scope_defs() else {
            return;
        };

        crate::log_debug_ct!("defines: {:?}", defines.defines.len());
        let mut kind_checker = CompletionKindChecker {
            symbols: HashSet::default(),
            functions: HashSet::default(),
        };

        let rng = node.range();

        let is_content_block = node.kind() == SyntaxKind::ContentBlock;

        let lb = if is_content_block { "" } else { "(" };
        let rb = if is_content_block { "" } else { ")" };

        // we don't check literal type here for faster completion
        for (name, ty) in defines.defines {
            // todo: filter ty
            if name.is_empty() {
                continue;
            }

            kind_checker.check(&ty);

            if kind_checker.symbols.iter().min().copied().is_some() {
                continue;
            }
            if kind_checker.functions.is_empty() {
                continue;
            }

            let label_details = ty.describe().or_else(|| Some("any".into()));
            let base = Completion {
                kind: CompletionKind::Func,
                label_details,
                apply: Some("".into()),
                // range: Some(range),
                command: self
                    .worker
                    .ctx
                    .analysis
                    .trigger_on_snippet_with_param_hint(true)
                    .map(From::from),
                ..Default::default()
            };
            let fn_feat = FnCompletionFeat::default().check(kind_checker.functions.iter());

            crate::log_debug_ct!("fn_feat: {name} {ty:?} -> {fn_feat:?}");

            if fn_feat.min_pos() < 1 || !fn_feat.next_arg_is_content {
                continue;
            }
            crate::log_debug_ct!("checked ufcs: {ty:?}");
            if self.worker.ctx.analysis.completion_feat.ufcs() && fn_feat.min_pos() == 1 {
                let before = EcoTextEdit {
                    range: self.cursor.lsp_range_of(rng.start..rng.start),
                    new_text: eco_format!("{name}{lb}"),
                };
                let after = EcoTextEdit {
                    range: self.cursor.lsp_range_of(rng.end..self.cursor.from),
                    new_text: rb.into(),
                };

                self.push_completion(Completion {
                    label: name.clone(),
                    additional_text_edits: Some(vec![before, after]),
                    ..base.clone()
                });
            }
            let more_args = fn_feat.min_pos() > 1 || fn_feat.min_named() > 0;
            if self.worker.ctx.analysis.completion_feat.ufcs_left() && more_args {
                let node_content = node.get().clone().into_text();
                let before = EcoTextEdit {
                    range: self.cursor.lsp_range_of(rng.start..self.cursor.from),
                    new_text: eco_format!("{name}{lb}"),
                };
                self.push_completion(Completion {
                    apply: if is_content_block {
                        Some(eco_format!("(${{}}){node_content}"))
                    } else {
                        Some(eco_format!("${{}}, {node_content})"))
                    },
                    label: eco_format!("{name}("),
                    additional_text_edits: Some(vec![before]),
                    ..base.clone()
                });
            }
            if self.worker.ctx.analysis.completion_feat.ufcs_right() && more_args {
                let before = EcoTextEdit {
                    range: self.cursor.lsp_range_of(rng.start..rng.start),
                    new_text: eco_format!("{name}("),
                };
                let after = EcoTextEdit {
                    range: self.cursor.lsp_range_of(rng.end..self.cursor.from),
                    new_text: "".into(),
                };
                self.push_completion(Completion {
                    apply: Some(eco_format!("${{}})")),
                    label: eco_format!("{name})"),
                    additional_text_edits: Some(vec![before, after]),
                    ..base
                });
            }
        }
    }
}
