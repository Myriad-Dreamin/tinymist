//! Completion of definitions in scope.

use typst::foundations::{Array, Dict};

use crate::ty::SigWithTy;

use super::*;

#[derive(BindTyCtx)]
#[bind(types)]
pub(crate) struct Defines {
    pub types: Arc<TypeInfo>,
    pub defines: BTreeMap<EcoString, Ty>,
    pub docs: BTreeMap<EcoString, EcoString>,
}

impl Defines {
    pub fn insert(&mut self, name: EcoString, item: Ty) {
        if name.is_empty() {
            return;
        }

        if let std::collections::btree_map::Entry::Vacant(entry) = self.defines.entry(name.clone())
        {
            entry.insert(item);
        }
    }

    pub fn insert_ty(&mut self, ty: Ty, name: &EcoString) {
        self.insert(name.clone(), ty);
    }

    pub fn insert_scope(&mut self, scope: &Scope) {
        // filter(Some(value)) &&
        for (name, bind) in scope.iter() {
            if !self.defines.contains_key(name) {
                self.insert(name.clone(), Ty::Value(InsTy::new(bind.read().clone())));
            }
        }
    }
}

impl CompletionPair<'_, '_, '_> {
    /// Add completions for definitions that are available at the cursor.
    pub fn scope_completions(&mut self, parens: bool) {
        let Some(defines) = self.scope_defs() else {
            return;
        };

        self.def_completions(defines, parens);
    }

    pub fn scope_defs(&mut self) -> Option<Defines> {
        let mut defines = Defines {
            types: self.worker.ctx.type_check(&self.cursor.source),
            defines: Default::default(),
            docs: Default::default(),
        };

        let mode = interpret_mode_at(Some(&self.cursor.leaf));

        previous_decls(self.cursor.leaf.clone(), |node| -> Option<()> {
            match node {
                PreviousDecl::Ident(ident) => {
                    let ty = self
                        .worker
                        .ctx
                        .type_of_span(ident.span())
                        .unwrap_or(Ty::Any);
                    defines.insert_ty(ty, ident.get());
                }
                PreviousDecl::ImportSource(src) => {
                    let ty = analyze_import_source(self.worker.ctx, &defines.types, src)?;
                    let name = ty.name().as_ref().into();
                    defines.insert_ty(ty, &name);
                }
                // todo: cache completion items
                PreviousDecl::ImportAll(mi) => {
                    let ty = analyze_import_source(self.worker.ctx, &defines.types, mi.source())?;
                    ty.iface_surface(
                        true,
                        &mut CompletionScopeChecker {
                            check_kind: ScopeCheckKind::Import,
                            defines: &mut defines,
                            ctx: self.worker.ctx,
                        },
                    );
                }
            }
            None
        });

        let in_math = matches!(mode, InterpretMode::Math);

        let lib = self.worker.world().library();
        let scope = if in_math { &lib.math } else { &lib.global }
            .scope()
            .clone();
        defines.insert_scope(&scope);

        Some(defines)
    }

    /// Add completions for definitions.
    pub fn def_completions(&mut self, defines: Defines, parens: bool) {
        let default_docs = defines.docs;
        let defines = defines.defines;

        let mode = interpret_mode_at(Some(&self.cursor.leaf));
        let surrounding_syntax = self.cursor.surrounding_syntax;

        let mut kind_checker = CompletionKindChecker {
            symbols: HashSet::default(),
            functions: HashSet::default(),
        };

        let filter = |checker: &CompletionKindChecker| {
            match surrounding_syntax {
                SurroundingSyntax::Regular => true,
                SurroundingSyntax::StringContent => false,
                SurroundingSyntax::ImportList | SurroundingSyntax::ParamList => false,
                SurroundingSyntax::Selector => 'selector: {
                    for func in &checker.functions {
                        if func.element().is_some() {
                            break 'selector true;
                        }
                    }

                    false
                }
                SurroundingSyntax::ShowTransform => !checker.functions.is_empty(),
                SurroundingSyntax::SetRule => 'set_rule: {
                    // todo: user defined elements
                    for func in &checker.functions {
                        if let Some(elem) = func.element() {
                            if elem.params().iter().any(|param| param.settable) {
                                break 'set_rule true;
                            }
                        }
                    }

                    false
                }
            }
        };

        // we don't check literal type here for faster completion
        for (name, ty) in defines {
            if name.is_empty() {
                continue;
            }

            kind_checker.check(&ty);
            if !filter(&kind_checker) {
                continue;
            }

            if let Some(ch) = kind_checker.symbols.iter().min().copied() {
                // todo: describe all chars
                let kind = CompletionKind::Symbol(ch);
                self.push_completion(Completion {
                    kind,
                    label: name,
                    label_details: Some(symbol_label_detail(ch)),
                    detail: Some(symbol_detail(ch)),
                    ..Completion::default()
                });
                continue;
            }

            let docs = default_docs.get(&name).cloned();

            let label_details = ty.describe().or_else(|| Some("any".into()));

            crate::log_debug_ct!("scope completions!: {name} {ty:?} {label_details:?}");
            let detail = docs.or_else(|| label_details.clone());

            if !kind_checker.functions.is_empty() {
                let fn_feat = FnCompletionFeat::default().check(kind_checker.functions.iter());
                crate::log_debug_ct!("fn_feat: {name} {ty:?} -> {fn_feat:?}");
                self.func_completion(mode, fn_feat, name, label_details, detail, parens);
                continue;
            }

            let kind = type_to_completion_kind(&ty);
            self.push_completion(Completion {
                kind,
                label: name,
                label_details,
                detail,
                ..Completion::default()
            });
        }
    }
}

fn analyze_import_source(ctx: &LocalContext, types: &TypeInfo, s: ast::Expr) -> Option<Ty> {
    if let Some(res) = types.type_of_span(s.span()) {
        if !matches!(res.value(), Some(Value::Str(..))) {
            return Some(types.simplify(res, false));
        }
    }

    let m = ctx.module_by_syntax(s.to_untyped())?;
    Some(Ty::Value(InsTy::new_at(m, s.span())))
}

pub(crate) enum ScopeCheckKind {
    Import,
    FieldAccess,
}

#[derive(BindTyCtx)]
#[bind(defines)]
pub(crate) struct CompletionScopeChecker<'a> {
    pub check_kind: ScopeCheckKind,
    pub defines: &'a mut Defines,
    pub ctx: &'a mut LocalContext,
}

impl CompletionScopeChecker<'_> {
    fn is_only_importable(&self) -> bool {
        matches!(self.check_kind, ScopeCheckKind::Import)
    }

    fn is_field_access(&self) -> bool {
        matches!(self.check_kind, ScopeCheckKind::FieldAccess)
    }

    fn type_methods(&mut self, bound_self: Option<Ty>, ty: Type) {
        for name in fields_on(ty) {
            self.defines.insert((*name).into(), Ty::Any);
        }
        let bound_self = bound_self.map(|this| SigTy::unary(this, Ty::Any));
        for (name, bind) in ty.scope().iter() {
            let val = bind.read().clone();
            let has_self = bound_self.is_some()
                && (if let Value::Func(func) = &val {
                    let first_pos = func
                        .params()
                        .and_then(|params| params.iter().find(|p| p.required));
                    first_pos.is_some_and(|p| p.name == "self")
                } else {
                    false
                });
            let ty = Ty::Value(InsTy::new(val));
            let ty = if has_self {
                if let Some(bound_self) = bound_self.as_ref() {
                    Ty::With(SigWithTy::new(ty.into(), bound_self.clone()))
                } else {
                    ty
                }
            } else {
                ty
            };

            self.defines.insert(name.into(), ty);
        }
    }
}

impl IfaceChecker for CompletionScopeChecker<'_> {
    fn check(
        &mut self,
        iface: Iface,
        _ctx: &mut crate::ty::IfaceCheckContext,
        _pol: bool,
    ) -> Option<()> {
        match iface {
            // dict is not importable
            Iface::Dict(d) if !self.is_only_importable() => {
                for (name, term) in d.interface() {
                    self.defines.insert(name.as_ref().into(), term.clone());
                }
            }
            Iface::Value { val, .. } if !self.is_only_importable() => {
                for (name, value) in val.iter() {
                    let term = Ty::Value(InsTy::new(value.clone()));
                    self.defines.insert(name.clone().into(), term);
                }
            }
            Iface::Content { val, .. } if self.is_field_access() => {
                // 255 is the magic "label"
                let styles = StyleChain::default();
                for field_id in 0u8..254u8 {
                    let Some(field_name) = val.field_name(field_id) else {
                        continue;
                    };
                    let param_info = val.params().iter().find(|p| p.name == field_name);
                    let param_docs = param_info.map(|p| p.docs.into());
                    let ty_from_param = param_info.map(|f| Ty::from_cast_info(&f.input));

                    let ty_from_style = val
                        .field_from_styles(field_id, styles)
                        .ok()
                        .map(|v| Ty::Builtin(BuiltinTy::Type(v.ty())));

                    let field_ty = match (ty_from_param, ty_from_style) {
                        (Some(param), None) => Some(param),
                        (Some(opt), Some(_)) | (None, Some(opt)) => Some(Ty::from_types(
                            [opt, Ty::Builtin(BuiltinTy::None)].into_iter(),
                        )),
                        (None, None) => None,
                    };

                    self.defines
                        .insert(field_name.into(), field_ty.unwrap_or(Ty::Any));

                    if let Some(docs) = param_docs {
                        self.defines.docs.insert(field_name.into(), docs);
                    }
                }
            }
            Iface::Type { val, at } if self.is_field_access() => {
                self.type_methods(Some(at.clone()), *val);
            }
            Iface::TypeType { val, .. } if self.is_field_access() => {
                self.type_methods(None, *val);
            }
            Iface::Func { .. } if self.is_field_access() => {
                self.type_methods(Some(iface.to_type()), Type::of::<Func>());
            }
            Iface::Array { .. } | Iface::Tuple { .. } if self.is_field_access() => {
                self.type_methods(Some(iface.to_type()), Type::of::<Array>());
            }
            Iface::Dict { .. } if self.is_field_access() => {
                self.type_methods(Some(iface.to_type()), Type::of::<Dict>());
            }
            Iface::Content { val, .. } => {
                self.defines.insert_scope(val.scope());
            }
            // todo: distingusish TypeType and Type
            Iface::TypeType { val, .. } | Iface::Type { val, .. } => {
                self.defines.insert_scope(val.scope());
            }
            Iface::Func { val, .. } => {
                if let Some(s) = val.scope() {
                    self.defines.insert_scope(s);
                }
            }
            Iface::Module { val, .. } => {
                let ti = self.ctx.type_check_by_id(val);
                if !ti.valid {
                    self.defines
                        .insert_scope(self.ctx.module_by_id(val).ok()?.scope());
                } else {
                    for (name, ty) in ti.exports.iter() {
                        // todo: Interned -> EcoString here
                        let ty = ti.simplify(ty.clone(), false);
                        self.defines.insert(name.as_ref().into(), ty);
                    }
                }
            }
            Iface::ModuleVal { val, .. } => {
                self.defines.insert_scope(val.scope());
            }
            Iface::Array { .. } | Iface::Tuple { .. } | Iface::Dict(..) | Iface::Value { .. } => {}
        }
        None
    }
}
