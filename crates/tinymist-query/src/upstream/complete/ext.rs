use super::{Completion, CompletionContext, CompletionKind};
use std::collections::BTreeMap;

use ecow::{eco_format, EcoString};
use typst::foundations::{Func, Value};
use typst::syntax::ast::AstNode;
use typst::syntax::{ast, SyntaxKind};

use crate::analysis::{analyze_import, analyze_signature};
use crate::find_definition;
use crate::prelude::analyze_expr;
use crate::syntax::{get_deref_target, LexicalKind, LexicalVarKind};
use crate::upstream::plain_docs_sentence;

impl<'a, 'w> CompletionContext<'a, 'w> {
    pub fn world(&self) -> &'w dyn typst::World {
        self.ctx.world()
    }

    /// Add completions for definitions that are available at the cursor.
    ///
    /// Filters the global/math scope with the given filter.
    pub fn scope_completions_(&mut self, parens: bool, filter: impl Fn(&Value) -> bool) {
        let mut defined = BTreeMap::new();
        let mut try_insert = |name: EcoString, kind: CompletionKind| {
            if name.is_empty() {
                return;
            }

            if let std::collections::btree_map::Entry::Vacant(entry) = defined.entry(name) {
                entry.insert(kind);
            }
        };

        let mut ancestor = Some(self.leaf.clone());
        while let Some(node) = &ancestor {
            let mut sibling = Some(node.clone());
            while let Some(node) = &sibling {
                if let Some(v) = node.cast::<ast::LetBinding>() {
                    let kind = match v.kind() {
                        ast::LetBindingKind::Closure(..) => CompletionKind::Func,
                        ast::LetBindingKind::Normal(..) => CompletionKind::Variable,
                    };
                    for ident in v.kind().bindings() {
                        try_insert(ident.get().clone(), kind.clone());
                    }
                }

                // todo: cache
                if let Some(v) = node.cast::<ast::ModuleImport>() {
                    let imports = v.imports();
                    let anaylyze = node.children().find(|child| child.is::<ast::Expr>());
                    let analyzed = anaylyze
                        .as_ref()
                        .and_then(|source| analyze_import(self.world(), source));
                    if analyzed.is_none() {
                        log::info!("failed to analyze import: {:?}", anaylyze);
                    }
                    if let Some(value) = analyzed {
                        if imports.is_none() {
                            if let Some(name) = value.name() {
                                try_insert(name.into(), CompletionKind::Module);
                            }
                        } else if let Some(scope) = value.scope() {
                            for (name, v) in scope.iter() {
                                let kind = match v {
                                    Value::Func(..) => CompletionKind::Func,
                                    Value::Module(..) => CompletionKind::Module,
                                    Value::Type(..) => CompletionKind::Type,
                                    _ => CompletionKind::Constant,
                                };
                                try_insert(name.clone(), kind);
                            }
                        }
                    }
                }

                sibling = node.prev_sibling();
            }

            if let Some(parent) = node.parent() {
                if let Some(v) = parent.cast::<ast::ForLoop>() {
                    if node.prev_sibling_kind() != Some(SyntaxKind::In) {
                        let pattern = v.pattern();
                        for ident in pattern.bindings() {
                            try_insert(ident.get().clone(), CompletionKind::Variable);
                        }
                    }
                }
                if let Some(v) = node.cast::<ast::Closure>() {
                    for param in v.params().children() {
                        match param {
                            ast::Param::Pos(pattern) => {
                                for ident in pattern.bindings() {
                                    try_insert(ident.get().clone(), CompletionKind::Variable);
                                }
                            }
                            ast::Param::Named(n) => {
                                try_insert(n.name().get().clone(), CompletionKind::Variable)
                            }
                            ast::Param::Spread(s) => {
                                if let Some(sink_ident) = s.sink_ident() {
                                    try_insert(sink_ident.get().clone(), CompletionKind::Variable)
                                }
                            }
                        }
                    }
                }

                ancestor = Some(parent.clone());
                continue;
            }

            break;
        }

        let in_math = matches!(
            self.leaf.parent_kind(),
            Some(SyntaxKind::Equation)
                | Some(SyntaxKind::Math)
                | Some(SyntaxKind::MathFrac)
                | Some(SyntaxKind::MathAttach)
        );

        let lib = self.world().library();
        let scope = if in_math { &lib.math } else { &lib.global }
            .scope()
            .clone();
        for (name, value) in scope.iter() {
            if filter(value) && !defined.contains_key(name) {
                self.value_completion(Some(name.clone()), value, parens, None);
            }
        }

        for (name, kind) in defined {
            if !name.is_empty() {
                if kind == CompletionKind::Func {
                    // todo: check arguments, if empty, jump to after the parens
                    let apply = eco_format!("{}(${{}})", name);
                    self.completions.push(Completion {
                        kind,
                        label: name,
                        apply: Some(apply),
                        detail: None,
                        // todo: only vscode and neovim (0.9.1) support this
                        command: Some("editor.action.triggerSuggest"),
                    });
                } else {
                    self.completions.push(Completion {
                        kind,
                        label: name,
                        apply: None,
                        detail: None,
                        command: None,
                    });
                }
            }
        }
    }
}

/// Add completions for the parameters of a function.
pub fn param_completions<'a>(
    ctx: &mut CompletionContext<'a, '_>,
    callee: ast::Expr<'a>,
    set: bool,
    args: ast::Args<'a>,
) {
    let Some(func) = resolve_callee(ctx, callee) else {
        return;
    };

    use typst::foundations::func::Repr;
    let mut func = func;
    while let Repr::With(f) = func.inner() {
        // todo: complete with positional arguments
        // with_args.push(ArgValue::Instance(f.1.clone()));
        func = f.0.clone();
    }

    let signature = analyze_signature(func.clone());

    // Exclude named arguments which are already present.
    let exclude: Vec<_> = args
        .items()
        .filter_map(|arg| match arg {
            ast::Arg::Named(named) => Some(named.name()),
            _ => None,
        })
        .collect();

    for (name, param) in &signature.named {
        if exclude.iter().any(|ident| ident.as_str() == name) {
            continue;
        }

        if set && !param.settable {
            continue;
        }

        if param.named {
            ctx.completions.push(Completion {
                kind: CompletionKind::Param,
                label: param.name.clone().into(),
                apply: Some(eco_format!("{}: ${{}}", param.name)),
                detail: Some(plain_docs_sentence(&param.docs)),
                // todo: only vscode and neovim (0.9.1) support this
                //
                // VS Code doesn't do that... Auto triggering suggestion only happens on typing
                // (word starts or trigger characters). However, you can use
                // editor.action.triggerSuggest as command on a suggestion to
                // "manually" retrigger suggest after inserting one
                command: Some("editor.action.triggerSuggest"),
            });
        }

        if param.positional {
            ctx.cast_completions(&param.input);
        }
    }

    if ctx.before.ends_with(',') {
        ctx.enrich(" ", "");
    }
}

/// Add completions for the values of a named function parameter.
pub fn named_param_value_completions<'a>(
    ctx: &mut CompletionContext<'a, '_>,
    callee: ast::Expr<'a>,
    name: &str,
) {
    let Some(func) = resolve_callee(ctx, callee) else {
        return;
    };

    use typst::foundations::func::Repr;
    let mut func = func;
    while let Repr::With(f) = func.inner() {
        // todo: complete with positional arguments
        // with_args.push(ArgValue::Instance(f.1.clone()));
        func = f.0.clone();
    }

    let signature = analyze_signature(func.clone());

    let Some(param) = signature.named.get(name) else {
        return;
    };
    if !param.named {
        return;
    }

    if let Some(expr) = &param.type_repr {
        ctx.completions.push(Completion {
            kind: CompletionKind::Constant,
            label: expr.clone(),
            apply: None,
            detail: Some(plain_docs_sentence(&param.docs)),
            command: None,
        });
    }

    ctx.cast_completions(&param.input);
    if name == "font" {
        ctx.font_completions();
    }

    if ctx.before.ends_with(':') {
        ctx.enrich(" ", "");
    }
}

/// Resolve a callee expression to a function.
// todo: fallback to static analysis if we can't resolve the callee
pub fn resolve_callee<'a>(
    ctx: &mut CompletionContext<'a, '_>,
    callee: ast::Expr<'a>,
) -> Option<Func> {
    resolve_global_dyn_callee(ctx, callee)
        .or_else(|| {
            let source = ctx.ctx.source_by_id(callee.span().id()?).ok()?;
            let node = source.find(callee.span())?;
            let cursor = node.offset();
            let deref_target = get_deref_target(node, cursor)?;
            let def = find_definition(ctx.ctx, source.clone(), deref_target)?;
            match def.kind {
                LexicalKind::Var(LexicalVarKind::Function) => match def.value {
                    Some(Value::Func(f)) => Some(f),
                    _ => None,
                },
                _ => None,
            }
        })
        .or_else(|| {
            let lib = ctx.world().library();
            let value = match callee {
                ast::Expr::Ident(ident) => lib.global.scope().get(&ident)?,
                ast::Expr::FieldAccess(access) => match access.target() {
                    ast::Expr::Ident(target) => match lib.global.scope().get(&target)? {
                        Value::Module(module) => module.field(&access.field()).ok()?,
                        Value::Func(func) => func.field(&access.field()).ok()?,
                        _ => return None,
                    },
                    _ => return None,
                },
                _ => return None,
            };

            match value {
                Value::Func(func) => Some(func.clone()),
                _ => None,
            }
        })
}

/// Resolve a callee expression to a dynamic function.
// todo: fallback to static analysis if we can't resolve the callee
fn resolve_global_dyn_callee<'a>(
    ctx: &CompletionContext<'a, '_>,
    callee: ast::Expr<'a>,
) -> Option<Func> {
    let values = analyze_expr(ctx.world(), &ctx.root.find(callee.span())?);

    values.into_iter().find_map(|v| match v.0 {
        Value::Func(f) => Some(f),
        _ => None,
    })
}
