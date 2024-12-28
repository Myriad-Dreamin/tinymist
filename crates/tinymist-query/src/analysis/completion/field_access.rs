use super::*;
impl CompletionPair<'_, '_, '_> {
    /// Add completions for all fields on a node.
    pub fn field_access_completions(&mut self, target: &LinkedNode) -> Option<()> {
        self.value_field_access_completions(target)
            .or_else(|| self.type_field_access_completions(target))
    }

    /// Add completions for all fields on a type.
    fn type_field_access_completions(&mut self, target: &LinkedNode) -> Option<()> {
        let ty = self
            .worker
            .ctx
            .post_type_of_node(target.clone())
            .filter(|ty| !matches!(ty, Ty::Any));
        crate::log_debug_ct!("type_field_access_completions_on: {target:?} -> {ty:?}");
        let mut defines = Defines {
            types: self.worker.ctx.type_check(&self.cursor.source),
            defines: Default::default(),
            docs: Default::default(),
        };
        ty?.iface_surface(
            true,
            &mut CompletionScopeChecker {
                check_kind: ScopeCheckKind::FieldAccess,
                defines: &mut defines,
                ctx: self.worker.ctx,
            },
        );

        self.def_completions(defines, true);
        Some(())
    }

    /// Add completions for all fields on a value.
    fn value_field_access_completions(&mut self, target: &LinkedNode) -> Option<()> {
        let (value, styles) = self.worker.ctx.analyze_expr(target).into_iter().next()?;
        for (name, value, _) in value.ty().scope().iter() {
            self.value_completion(Some(name.clone()), value, true, None);
        }

        if let Some(scope) = value.scope() {
            for (name, value, _) in scope.iter() {
                self.value_completion(Some(name.clone()), value, true, None);
            }
        }

        for &field in fields_on(value.ty()) {
            // Complete the field name along with its value. Notes:
            // 1. No parentheses since function fields cannot currently be called
            // with method syntax;
            // 2. We can unwrap the field's value since it's a field belonging to
            // this value's type, so accessing it should not fail.
            self.value_completion(
                Some(field.into()),
                &value.field(field).unwrap(),
                false,
                None,
            );
        }

        self.postfix_completions(target, Ty::Value(InsTy::new(value.clone())));

        match value {
            Value::Symbol(symbol) => {
                for modifier in symbol.modifiers() {
                    if let Ok(modified) = symbol.clone().modified(modifier) {
                        self.push_completion(Completion {
                            kind: CompletionKind::Symbol(modified.get()),
                            label: modifier.into(),
                            label_details: Some(symbol_label_detail(modified.get())),
                            ..Completion::default()
                        });
                    }
                }

                self.ufcs_completions(target);
            }
            Value::Content(content) => {
                for (name, value) in content.fields() {
                    self.value_completion(Some(name.into()), &value, false, None);
                }

                self.ufcs_completions(target);
            }
            Value::Dict(dict) => {
                for (name, value) in dict.iter() {
                    self.value_completion(Some(name.clone().into()), value, false, None);
                }
            }
            Value::Func(func) => {
                // Autocomplete get rules.
                if let Some((elem, styles)) = func.element().zip(styles.as_ref()) {
                    for param in elem.params().iter().filter(|param| !param.required) {
                        if let Some(value) = elem
                            .field_id(param.name)
                            .map(|id| elem.field_from_styles(id, StyleChain::new(styles)))
                        {
                            self.value_completion(
                                Some(param.name.into()),
                                &value.unwrap(),
                                false,
                                None,
                            );
                        }
                    }
                }
            }
            Value::Plugin(plugin) => {
                for name in plugin.iter() {
                    self.push_completion(Completion {
                        kind: CompletionKind::Func,
                        label: name.clone(),
                        ..Completion::default()
                    })
                }
            }
            _ => {}
        }

        Some(())
    }
}
