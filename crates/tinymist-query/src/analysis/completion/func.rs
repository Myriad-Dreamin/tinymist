//! Completion for functions on nodes.

use super::*;

impl CompletionPair<'_, '_, '_> {
    pub fn func_completion(
        &mut self,
        mode: InterpretMode,
        fn_feat: FnCompletionFeat,
        name: EcoString,
        label_details: Option<EcoString>,
        detail: Option<EcoString>,
        parens: bool,
    ) {
        let base = Completion {
            kind: CompletionKind::Func,
            label_details,
            detail,
            command: self
                .worker
                .ctx
                .analysis
                .trigger_on_snippet_with_param_hint(true)
                .map(From::from),
            ..Default::default()
        };

        if matches!(
            self.cursor.surrounding_syntax,
            SurroundingSyntax::ShowTransform
        ) && (fn_feat.min_pos() > 0 || fn_feat.min_named() > 0)
        {
            self.push_completion(Completion {
                label: eco_format!("{name}.with"),
                apply: Some(eco_format!("{name}.with(${{}})")),
                ..base.clone()
            });
        }
        if fn_feat.is_element
            && matches!(self.cursor.surrounding_syntax, SurroundingSyntax::Selector)
        {
            self.push_completion(Completion {
                label: eco_format!("{name}.where"),
                apply: Some(eco_format!("{name}.where(${{}})")),
                ..base.clone()
            });
        }

        let bad_instantiate = match self.cursor.surrounding_syntax {
            // todo: filter invalid function as a selector.
            SurroundingSyntax::Selector => name == "with",
            SurroundingSyntax::SetRule => !fn_feat.is_element,
            _ => false,
        };
        if !bad_instantiate {
            let only_parens = !parens
                || (matches!(self.cursor.surrounding_syntax, SurroundingSyntax::Selector)
                    && fn_feat.is_element);

            if !only_parens && fn_feat.has_static_member {
                self.push_completion(Completion {
                    label: name.clone(),
                    ..base.clone()
                });
            }

            if only_parens {
                self.push_completion(Completion {
                    label: name,
                    ..base
                });
            } else if (fn_feat.min_pos() < 1 || fn_feat.has_only_self()) && !fn_feat.has_rest {
                self.push_completion(Completion {
                    apply: Some(eco_format!("{}()${{}}", name)),
                    label: paren_label(&name, &fn_feat),
                    command: None,
                    ..base
                });
            } else {
                let accept_content_arg = fn_feat.next_arg_is_content && !fn_feat.has_rest;
                let scope_reject_content = matches!(mode, InterpretMode::Math)
                    || matches!(
                        self.cursor.surrounding_syntax,
                        SurroundingSyntax::Selector | SurroundingSyntax::SetRule
                    );
                self.push_completion(Completion {
                    apply: Some(eco_format!("{name}(${{}})")),
                    label: paren_label(&name, &fn_feat),
                    ..base.clone()
                });
                if !scope_reject_content && accept_content_arg {
                    self.push_completion(Completion {
                        apply: Some(eco_format!("{name}[${{}}]")),
                        label: eco_format!("{name}.bracket"),
                        ..base
                    });
                };
            }
        }

        fn paren_label(name: &EcoString, fn_feat: &FnCompletionFeat) -> EcoString {
            if fn_feat.has_static_member {
                eco_format!("{name}.paren")
            } else {
                name.clone()
            }
        }
    }
}
