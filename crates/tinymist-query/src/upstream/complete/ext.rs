use std::collections::BTreeMap;

use ecow::{eco_format, EcoString};
use lsp_types::{CompletionItem, CompletionTextEdit, InsertTextFormat, TextEdit};
use once_cell::sync::OnceCell;
use reflexo::path::{unix_slash, PathClean};
use typst::foundations::{AutoValue, Func, Label, NoneValue, Repr, Type, Value};
use typst::layout::{Dir, Length};
use typst::syntax::ast::AstNode;
use typst::syntax::{ast, Span, SyntaxKind, SyntaxNode};
use typst::visualize::Color;

use super::{Completion, CompletionContext, CompletionKind};
use crate::adt::interner::Interned;
use crate::analysis::{analyze_dyn_signature, resolve_call_target, BuiltinTy, PathPreference, Ty};
use crate::syntax::{param_index_at_leaf, CheckTarget};
use crate::upstream::complete::complete_code;
use crate::upstream::plain_docs_sentence;

use crate::{completion_kind, prelude::*, LspCompletion};

impl<'a, 'w> CompletionContext<'a, 'w> {
    pub fn world(&self) -> &'w dyn typst::World {
        self.ctx.world()
    }

    pub fn scope_completions(&mut self, parens: bool, filter: impl Fn(&Value) -> bool) {
        self.scope_completions_(parens, |v| v.map_or(true, &filter));
    }

    pub fn strict_scope_completions(&mut self, parens: bool, filter: impl Fn(&Value) -> bool) {
        self.scope_completions_(parens, |v| v.map_or(false, &filter));
    }

    fn seen_field(&mut self, field: Interned<str>) -> bool {
        !self.seen_fields.insert(field)
    }

    /// Add completions for definitions that are available at the cursor.
    ///
    /// Filters the global/math scope with the given filter.
    pub fn scope_completions_(&mut self, parens: bool, filter: impl Fn(Option<&Value>) -> bool) {
        let mut defined = BTreeMap::new();

        #[derive(Debug, Clone)]
        enum DefKind {
            Syntax(Span),
            Instance(Span, Value),
        }

        let mut try_insert = |name: EcoString, kind: (CompletionKind, DefKind)| {
            if name.is_empty() {
                return;
            }

            if let std::collections::btree_map::Entry::Vacant(entry) = defined.entry(name) {
                entry.insert(kind);
            }
        };

        let types = (|| {
            let id = self.root.span().id()?;
            let src = self.ctx.source_by_id(id).ok()?;
            self.ctx.type_check(src)
        })();
        let types = types.as_ref();

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
                        try_insert(
                            ident.get().clone(),
                            (kind.clone(), DefKind::Syntax(ident.span())),
                        );
                    }
                }

                // todo: cache
                if let Some(v) = node.cast::<ast::ModuleImport>() {
                    let imports = v.imports();
                    let anaylyze = node.children().find(|child| child.is::<ast::Expr>());
                    let analyzed = anaylyze
                        .as_ref()
                        .and_then(|source| self.ctx.analyze_import(source));
                    if analyzed.is_none() {
                        log::debug!("failed to analyze import: {:?}", anaylyze);
                    }

                    // import it self
                    if imports.is_none() || v.new_name().is_some() {
                        // todo: name of import syntactically

                        let name = (|| {
                            if let Some(new_name) = v.new_name() {
                                return Some(new_name.get().clone());
                            }
                            if let Some(module_ins) = &analyzed {
                                return module_ins.name().map(From::from);
                            }

                            // todo: name of import syntactically
                            None
                        })();

                        let def_kind = analyzed.clone().map(|module_ins| {
                            DefKind::Instance(
                                v.new_name()
                                    .map(|n| n.span())
                                    .unwrap_or_else(Span::detached),
                                module_ins,
                            )
                        });

                        if let Some((name, def_kind)) = name.zip(def_kind) {
                            try_insert(name, (CompletionKind::Module, def_kind));
                        }
                    }

                    // import items
                    match (imports, analyzed) {
                        (Some(..), None) => {
                            // todo: name of import syntactically
                        }
                        (Some(e), Some(module_ins)) => {
                            let import_filter = match e {
                                ast::Imports::Wildcard => None,
                                ast::Imports::Items(e) => {
                                    let mut filter = HashMap::new();
                                    for item in e.iter() {
                                        match item {
                                            ast::ImportItem::Simple(n) => {
                                                filter.insert(
                                                    n.get().clone(),
                                                    DefKind::Syntax(n.span()),
                                                );
                                            }
                                            ast::ImportItem::Renamed(n) => {
                                                filter.insert(
                                                    n.new_name().get().clone(),
                                                    DefKind::Syntax(n.span()),
                                                );
                                            }
                                        }
                                    }
                                    Some(filter)
                                }
                            };

                            if let Some(scope) = module_ins.scope() {
                                for (name, v) in scope.iter() {
                                    let kind = value_to_completion_kind(v);
                                    let def_kind = match &import_filter {
                                        Some(import_filter) => {
                                            let w = import_filter.get(name);
                                            match w {
                                                Some(DefKind::Syntax(span)) => {
                                                    Some(DefKind::Instance(*span, v.clone()))
                                                }
                                                Some(DefKind::Instance(span, v)) => {
                                                    Some(DefKind::Instance(*span, v.clone()))
                                                }
                                                None => None,
                                            }
                                        }
                                        None => {
                                            Some(DefKind::Instance(Span::detached(), v.clone()))
                                        }
                                    };
                                    if let Some(def_kind) = def_kind {
                                        try_insert(name.clone(), (kind, def_kind));
                                    }
                                }
                            } else if let Some(filter) = import_filter {
                                for (name, def_kind) in filter {
                                    try_insert(name, (CompletionKind::Variable, def_kind));
                                }
                            }
                        }
                        _ => {}
                    }
                }

                sibling = node.prev_sibling();
            }

            if let Some(parent) = node.parent() {
                if let Some(v) = parent.cast::<ast::ForLoop>() {
                    if node.prev_sibling_kind() != Some(SyntaxKind::In) {
                        let pattern = v.pattern();
                        for ident in pattern.bindings() {
                            try_insert(
                                ident.get().clone(),
                                (CompletionKind::Variable, DefKind::Syntax(ident.span())),
                            );
                        }
                    }
                }
                if let Some(v) = node.cast::<ast::Closure>() {
                    for param in v.params().children() {
                        match param {
                            ast::Param::Pos(pattern) => {
                                for ident in pattern.bindings() {
                                    try_insert(
                                        ident.get().clone(),
                                        (CompletionKind::Variable, DefKind::Syntax(ident.span())),
                                    );
                                }
                            }
                            ast::Param::Named(n) => try_insert(
                                n.name().get().clone(),
                                (CompletionKind::Variable, DefKind::Syntax(n.name().span())),
                            ),
                            ast::Param::Spread(s) => {
                                if let Some(sink_ident) = s.sink_ident() {
                                    try_insert(
                                        sink_ident.get().clone(),
                                        (CompletionKind::Variable, DefKind::Syntax(s.span())),
                                    )
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
            if filter(Some(value)) && !defined.contains_key(name) {
                defined.insert(
                    name.clone(),
                    (
                        value_to_completion_kind(value),
                        DefKind::Instance(Span::detached(), value.clone()),
                    ),
                );
            }
        }

        enum SurroundingSyntax {
            Regular,
            Selector,
            SetRule,
        }

        let surrounding_syntax = check_surrounding_syntax(&self.leaf)
            .or_else(|| check_previous_syntax(&self.leaf))
            .unwrap_or(SurroundingSyntax::Regular);

        for (name, (kind, def_kind)) in defined {
            if !filter(None) || name.is_empty() {
                continue;
            }
            let span = match def_kind {
                DefKind::Syntax(span) => span,
                DefKind::Instance(span, _) => span,
            };
            // we don't check literal type here for faster completion
            let ty_detail = if let CompletionKind::Symbol(c) = &kind {
                Some(symbol_label_detail(*c))
            } else {
                types
                    .and_then(|types| {
                        let ty = types.type_of_span(span)?;
                        let ty = types.simplify(ty, false);
                        types.describe(&ty).map(From::from)
                    })
                    .or_else(|| {
                        if let DefKind::Instance(_, v) = &def_kind {
                            Some(describe_value(self.ctx, v))
                        } else {
                            None
                        }
                    })
                    .or_else(|| Some("any".into()))
            };
            let detail = if let CompletionKind::Symbol(c) = &kind {
                Some(symbol_detail(*c))
            } else {
                ty_detail.clone()
            };

            if kind == CompletionKind::Func {
                let base = Completion {
                    kind: kind.clone(),
                    label_detail: ty_detail,
                    // todo: only vscode and neovim (0.9.1) support this
                    command: Some("editor.action.triggerParameterHints"),
                    ..Default::default()
                };

                let zero_args = match &def_kind {
                    DefKind::Instance(_, Value::Func(func)) => func
                        .params()
                        .is_some_and(|params| params.iter().all(|param| param.name == "self")),
                    _ => false,
                };
                let is_element = match &def_kind {
                    DefKind::Instance(_, Value::Func(func)) => func.element().is_some(),
                    _ => false,
                };
                log::debug!("is_element: {} {:?} -> {:?}", name, def_kind, is_element);

                if !zero_args && matches!(surrounding_syntax, SurroundingSyntax::Regular) {
                    self.completions.push(Completion {
                        label: eco_format!("{}.with", name),
                        apply: Some(eco_format!("{}.with(${{}})", name)),
                        ..base.clone()
                    });
                }
                if is_element && !matches!(surrounding_syntax, SurroundingSyntax::SetRule) {
                    self.completions.push(Completion {
                        label: eco_format!("{}.where", name),
                        apply: Some(eco_format!("{}.where(${{}})", name)),
                        ..base.clone()
                    });
                }

                let bad_instantiate = matches!(
                    surrounding_syntax,
                    SurroundingSyntax::Selector | SurroundingSyntax::SetRule
                ) && !is_element;
                if !bad_instantiate {
                    if !parens {
                        self.completions.push(Completion {
                            label: name,
                            ..base
                        });
                    } else if zero_args {
                        self.completions.push(Completion {
                            apply: Some(eco_format!("{}()${{}}", name)),
                            label: name,
                            ..base
                        });
                    } else {
                        self.completions.push(Completion {
                            apply: Some(eco_format!("{}(${{}})", name)),
                            label: name,
                            ..base
                        });
                    }
                }
            } else if let DefKind::Instance(_, v) = def_kind {
                let bad_instantiate = matches!(
                    surrounding_syntax,
                    SurroundingSyntax::Selector | SurroundingSyntax::SetRule
                ) && !matches!(&v, Value::Func(func) if func.element().is_some());
                if !bad_instantiate {
                    self.value_completion_(
                        Some(name),
                        &v,
                        parens,
                        ty_detail.clone(),
                        detail.as_deref(),
                    );
                }
            } else {
                self.completions.push(Completion {
                    kind,
                    label: name,
                    label_detail: ty_detail.clone(),
                    detail,
                    ..Completion::default()
                });
            }
        }

        fn check_surrounding_syntax(mut leaf: &LinkedNode) -> Option<SurroundingSyntax> {
            use SurroundingSyntax::*;
            let mut met_args = false;
            while let Some(parent) = leaf.parent() {
                log::debug!(
                    "check_surrounding_syntax: {:?}::{:?}",
                    parent.kind(),
                    leaf.kind()
                );
                match parent.kind() {
                    SyntaxKind::CodeBlock | SyntaxKind::ContentBlock | SyntaxKind::Equation => {
                        return Some(Regular);
                    }
                    SyntaxKind::Named => {
                        return Some(Regular);
                    }
                    SyntaxKind::Args => {
                        met_args = true;
                    }
                    SyntaxKind::SetRule => {
                        let rule = parent.get().cast::<ast::SetRule>()?;
                        if met_args || encolsed_by(parent, rule.condition().map(|s| s.span()), leaf)
                        {
                            return Some(Regular);
                        } else {
                            return Some(SetRule);
                        }
                    }
                    SyntaxKind::ShowRule => {
                        let rule = parent.get().cast::<ast::ShowRule>()?;
                        if encolsed_by(parent, Some(rule.transform().span()), leaf) {
                            return Some(Regular);
                        } else {
                            return Some(Selector); // query's first argument
                        }
                    }
                    _ => {}
                }

                leaf = parent;
            }

            None
        }

        fn check_previous_syntax(leaf: &LinkedNode) -> Option<SurroundingSyntax> {
            let mut leaf = leaf.clone();
            if leaf.kind().is_trivia() {
                leaf = leaf.prev_sibling()?;
            }
            if matches!(leaf.kind(), SyntaxKind::ShowRule | SyntaxKind::SetRule) {
                return check_surrounding_syntax(&leaf.rightmost_leaf()?);
            }

            if matches!(leaf.kind(), SyntaxKind::Show) {
                return Some(SurroundingSyntax::Selector);
            }
            if matches!(leaf.kind(), SyntaxKind::Set) {
                return Some(SurroundingSyntax::SetRule);
            }

            None
        }
    }
}

fn describe_value(ctx: &mut AnalysisContext, v: &Value) -> EcoString {
    match v {
        Value::Func(f) => {
            let mut f = f;
            while let typst::foundations::func::Repr::With(with_f) = f.inner() {
                f = &with_f.0;
            }

            let sig = analyze_dyn_signature(ctx, f.clone());
            sig.primary()
                .ty()
                .describe()
                .unwrap_or_else(|| "function".into())
                .into()
        }
        Value::Module(m) => {
            if let Some(fid) = m.file_id() {
                let package = fid.package();
                let path = unix_slash(fid.vpath().as_rootless_path());
                if let Some(package) = package {
                    return eco_format!("{package}:{path}");
                }
                return path.into();
            }

            "module".into()
        }
        _ => v.ty().repr(),
    }
}

fn encolsed_by(parent: &LinkedNode, s: Option<Span>, leaf: &LinkedNode) -> bool {
    s.and_then(|s| parent.find(s)?.find(leaf.span())).is_some()
}

fn sort_and_explicit_code_completion(ctx: &mut CompletionContext) {
    let mut completions = std::mem::take(&mut ctx.completions);
    let explict = ctx.explicit;
    ctx.explicit = true;
    complete_code(ctx, true);
    ctx.explicit = explict;

    log::debug!(
        "sort_and_explicit_code_completion: {:#?} {:#?}",
        completions,
        ctx.completions
    );

    completions.sort_by(|a, b| {
        a.sort_text
            .as_ref()
            .cmp(&b.sort_text.as_ref())
            .then_with(|| a.label.cmp(&b.label))
    });
    ctx.completions.sort_by(|a, b| {
        a.sort_text
            .as_ref()
            .cmp(&b.sort_text.as_ref())
            .then_with(|| a.label.cmp(&b.label))
    });

    // todo: this is a bit messy, we can refactor for improving maintainability
    // The messy code will finally gone, but to help us go over the mess stage, I
    // drop some comment here.
    //
    // currently, there are only path completions in ctx.completions2
    // and type/named param/positional param completions in completions
    // and all rest less relevant completions inctx.completions
    for (i, compl) in ctx.completions2.iter_mut().enumerate() {
        compl.sort_text = Some(format!("{i:03}"));
    }
    let sort_base = ctx.completions2.len();
    for (i, compl) in (completions.iter_mut().chain(ctx.completions.iter_mut())).enumerate() {
        compl.sort_text = Some(eco_format!("{i:03}", i = i + sort_base));
    }

    log::debug!(
        "sort_and_explicit_code_completion after: {:#?} {:#?}",
        completions,
        ctx.completions
    );

    ctx.completions.append(&mut completions);

    log::debug!("sort_and_explicit_code_completion: {:?}", ctx.completions);
}

pub fn value_to_completion_kind(value: &Value) -> CompletionKind {
    match value {
        Value::Func(..) => CompletionKind::Func,
        Value::Module(..) => CompletionKind::Module,
        Value::Type(..) => CompletionKind::Type,
        Value::Symbol(s) => CompletionKind::Symbol(s.get()),
        _ => CompletionKind::Constant,
    }
}

/// Add completions for the parameters of a function.
pub fn param_completions<'a>(
    ctx: &mut CompletionContext<'a, '_>,
    callee: ast::Expr<'a>,
    set: bool,
    args: ast::Args<'a>,
) {
    let Some(cc) = ctx
        .root
        .find(callee.span())
        .and_then(|callee| resolve_call_target(ctx.ctx, &callee))
    else {
        return;
    };
    // todo: regards call convention
    let this = cc.method_this().cloned();
    let func = cc.callee();

    use typst::foundations::func::Repr;
    let mut func = func;
    while let Repr::With(f) = func.inner() {
        // todo: complete with positional arguments
        // with_args.push(ArgValue::Instance(f.1.clone()));
        func = f.0.clone();
    }

    let pos_index =
        param_index_at_leaf(&ctx.leaf, &func, args).map(|i| if this.is_some() { i + 1 } else { i });

    let signature = analyze_dyn_signature(ctx.ctx, func.clone());

    let leaf_type = ctx.ctx.literal_type_of_node(ctx.leaf.clone());
    log::debug!("pos_param_completion_by_type: {:?}", leaf_type);

    for arg in args.items() {
        if let ast::Arg::Named(named) = arg {
            ctx.seen_field(named.name().into());
        }
    }

    let primary_sig = signature.primary();

    'pos_check: {
        let mut doc = None;

        if let Some(pos_index) = pos_index {
            let pos = primary_sig.pos.get(pos_index);
            log::debug!("pos_param_completion_to: {:?}", pos);

            if let Some(pos) = pos {
                if set && !pos.settable {
                    break 'pos_check;
                }

                doc = Some(plain_docs_sentence(&pos.docs));

                if pos.positional {
                    type_completion(ctx, &pos.base_type, doc.as_deref());
                }
            }
        }

        if let Some(leaf_type) = leaf_type {
            type_completion(ctx, &leaf_type, doc.as_deref());
        }
    }

    for (name, param) in &primary_sig.named {
        if ctx.seen_field(name.as_ref().into()) {
            continue;
        }
        log::debug!(
            "pos_named_param_completion_to({set:?}): {name:?} {:?}",
            param.settable
        );

        if set && !param.settable {
            continue;
        }

        let _d = OnceCell::new();
        let docs = || Some(_d.get_or_init(|| plain_docs_sentence(&param.docs)).clone());

        if param.named {
            let compl = Completion {
                kind: CompletionKind::Field,
                label: param.name.as_ref().into(),
                apply: Some(eco_format!("{}: ${{}}", param.name)),
                detail: docs(),
                label_detail: None,
                command: Some("tinymist.triggerNamedCompletion"),
                ..Completion::default()
            };
            match param.base_type {
                Ty::Builtin(BuiltinTy::TextSize) => {
                    for size_template in &[
                        "10.5pt", "12pt", "9pt", "14pt", "8pt", "16pt", "18pt", "20pt", "22pt",
                        "24pt", "28pt",
                    ] {
                        let compl = compl.clone();
                        ctx.completions.push(Completion {
                            label: eco_format!("{}: {}", param.name, size_template),
                            apply: None,
                            ..compl
                        });
                    }
                }
                Ty::Builtin(BuiltinTy::Dir) => {
                    for dir_template in &["ltr", "rtl", "ttb", "btt"] {
                        let compl = compl.clone();
                        ctx.completions.push(Completion {
                            label: eco_format!("{}: {}", param.name, dir_template),
                            apply: None,
                            ..compl
                        });
                    }
                }
                _ => {}
            }
            ctx.completions.push(compl);
        }

        if param.positional {
            type_completion(ctx, &param.base_type, docs().as_deref());
        }
    }

    sort_and_explicit_code_completion(ctx);
    if ctx.before.ends_with(',') {
        ctx.enrich(" ", "");
    }
}

fn type_completion(
    ctx: &mut CompletionContext<'_, '_>,
    infer_type: &Ty,
    docs: Option<&str>,
) -> Option<()> {
    // Prevent duplicate completions from appearing.
    if !ctx.seen_types.insert(infer_type.clone()) {
        return Some(());
    }

    log::debug!("type_completion: {infer_type:?}");

    match infer_type {
        Ty::Any => return None,
        Ty::Tuple(..) | Ty::Array(..) => {
            ctx.snippet_completion("()", "(${})", "An array.");
        }
        Ty::Dict(..) => {
            ctx.snippet_completion("()", "(${})", "A dictionary.");
        }
        Ty::Boolean(_b) => {
            ctx.snippet_completion("false", "false", "No / Disabled.");
            ctx.snippet_completion("true", "true", "Yes / Enabled.");
        }
        Ty::Field(f) => {
            let f = &f.name;
            if ctx.seen_field(f.clone()) {
                return Some(());
            }

            let mut rev_stream = ctx.before.chars().rev();
            let ch = rev_stream.find(|c| !typst::syntax::is_id_continue(*c));
            // skip label/ref completion.
            // todo: more elegant way
            if matches!(ch, Some('<' | '@')) {
                return Some(());
            }

            ctx.completions.push(Completion {
                kind: CompletionKind::Field,
                label: f.into(),
                apply: Some(eco_format!("{}: ${{}}", f)),
                detail: docs.map(Into::into),
                command: Some("tinymist.triggerNamedCompletion"),
                ..Completion::default()
            });
        }
        Ty::Builtin(v) => match v {
            BuiltinTy::None => ctx.snippet_completion("none", "none", "Nothing."),
            BuiltinTy::Auto => {
                ctx.snippet_completion("auto", "auto", "A smart default.");
            }
            BuiltinTy::Clause => return None,
            BuiltinTy::Undef => return None,
            BuiltinTy::Space => return None,
            BuiltinTy::Content => return None,
            BuiltinTy::Infer => return None,
            BuiltinTy::FlowNone => return None,

            BuiltinTy::Path(p) => {
                let source = ctx.ctx.source_by_id(ctx.root.span().id()?).ok()?;

                ctx.completions2.extend(
                    complete_path(ctx.ctx, Some(ctx.leaf.clone()), &source, ctx.cursor, p)
                        .into_iter()
                        .flatten(),
                );
            }
            BuiltinTy::Args => return None,
            BuiltinTy::Stroke => {
                ctx.snippet_completion("stroke()", "stroke(${})", "Stroke type.");
                ctx.snippet_completion("()", "(${})", "Stroke dictionary.");
                type_completion(ctx, &Ty::Builtin(BuiltinTy::Color), docs);
                type_completion(ctx, &Ty::Builtin(BuiltinTy::Length), docs);
            }
            BuiltinTy::Color => {
                ctx.snippet_completion("luma()", "luma(${v})", "A custom grayscale color.");
                ctx.snippet_completion(
                    "rgb()",
                    "rgb(${r}, ${g}, ${b}, ${a})",
                    "A custom RGBA color.",
                );
                ctx.snippet_completion(
                    "cmyk()",
                    "cmyk(${c}, ${m}, ${y}, ${k})",
                    "A custom CMYK color.",
                );
                ctx.snippet_completion(
                    "oklab()",
                    "oklab(${l}, ${a}, ${b}, ${alpha})",
                    "A custom Oklab color.",
                );
                ctx.snippet_completion(
                    "oklch()",
                    "oklch(${l}, ${chroma}, ${hue}, ${alpha})",
                    "A custom Oklch color.",
                );
                ctx.snippet_completion(
                    "color.linear-rgb()",
                    "color.linear-rgb(${r}, ${g}, ${b}, ${a})",
                    "A custom linear RGBA color.",
                );
                ctx.snippet_completion(
                    "color.hsv()",
                    "color.hsv(${h}, ${s}, ${v}, ${a})",
                    "A custom HSVA color.",
                );
                ctx.snippet_completion(
                    "color.hsl()",
                    "color.hsl(${h}, ${s}, ${l}, ${a})",
                    "A custom HSLA color.",
                );
                let color_ty = Type::of::<Color>();
                ctx.strict_scope_completions(false, |value| value.ty() == color_ty);
            }
            BuiltinTy::TextSize => return None,
            BuiltinTy::TextLang => {
                for (&key, desc) in rust_iso639::ALL_MAP.entries() {
                    let detail = eco_format!("An ISO 639-1/2/3 language code, {}.", desc.name);
                    ctx.completions.push(Completion {
                        kind: CompletionKind::Syntax,
                        label: key.to_lowercase().into(),
                        apply: Some(eco_format!("\"{}\"", key.to_lowercase())),
                        detail: Some(detail),
                        label_detail: Some(desc.name.into()),
                        ..Completion::default()
                    });
                }
            }
            BuiltinTy::TextRegion => {
                for (&key, desc) in rust_iso3166::ALPHA2_MAP.entries() {
                    let detail = eco_format!("An ISO 3166-1 alpha-2 region code, {}.", desc.name);
                    ctx.completions.push(Completion {
                        kind: CompletionKind::Syntax,
                        label: key.to_lowercase().into(),
                        apply: Some(eco_format!("\"{}\"", key.to_lowercase())),
                        detail: Some(detail),
                        label_detail: Some(desc.name.into()),
                        ..Completion::default()
                    });
                }
            }
            BuiltinTy::Dir => {
                let ty = Type::of::<Dir>();
                ctx.strict_scope_completions(false, |value| value.ty() == ty);
            }
            BuiltinTy::TextFont => {
                ctx.font_completions();
            }
            BuiltinTy::Margin => {
                ctx.snippet_completion("()", "(${})", "Margin dictionary.");
                type_completion(ctx, &Ty::Builtin(BuiltinTy::Length), docs);
            }
            BuiltinTy::Inset => {
                ctx.snippet_completion("()", "(${})", "Inset dictionary.");
                type_completion(ctx, &Ty::Builtin(BuiltinTy::Length), docs);
            }
            BuiltinTy::Outset => {
                ctx.snippet_completion("()", "(${})", "Outset dictionary.");
                type_completion(ctx, &Ty::Builtin(BuiltinTy::Length), docs);
            }
            BuiltinTy::Radius => {
                ctx.snippet_completion("()", "(${})", "Radius dictionary.");
                type_completion(ctx, &Ty::Builtin(BuiltinTy::Length), docs);
            }
            BuiltinTy::Length => {
                ctx.snippet_completion("pt", "${1}pt", "Point length unit.");
                ctx.snippet_completion("mm", "${1}mm", "Millimeter length unit.");
                ctx.snippet_completion("cm", "${1}cm", "Centimeter length unit.");
                ctx.snippet_completion("in", "${1}in", "Inch length unit.");
                ctx.snippet_completion("em", "${1}em", "Em length unit.");
                let length_ty = Type::of::<Length>();
                ctx.strict_scope_completions(false, |value| value.ty() == length_ty);
                type_completion(ctx, &Ty::Builtin(BuiltinTy::Auto), docs);
            }
            BuiltinTy::Float => {
                ctx.snippet_completion("exponential notation", "${1}e${0}", "Exponential notation");
            }
            BuiltinTy::CiteLabel => {
                ctx.label_completions(true);
            }
            BuiltinTy::RefLabel => {
                ctx.ref_completions();
            }
            BuiltinTy::Type(ty) => {
                if *ty == Type::of::<NoneValue>() {
                    let docs = docs.or(Some("Nothing."));
                    type_completion(ctx, &Ty::Builtin(BuiltinTy::None), docs);
                } else if *ty == Type::of::<AutoValue>() {
                    let docs = docs.or(Some("A smart default."));
                    type_completion(ctx, &Ty::Builtin(BuiltinTy::Auto), docs);
                } else if *ty == Type::of::<bool>() {
                    ctx.snippet_completion("false", "false", "No / Disabled.");
                    ctx.snippet_completion("true", "true", "Yes / Enabled.");
                } else if *ty == Type::of::<Color>() {
                    type_completion(ctx, &Ty::Builtin(BuiltinTy::Color), docs);
                } else if *ty == Type::of::<Label>() {
                    ctx.label_completions(false)
                } else if *ty == Type::of::<Func>() {
                    ctx.snippet_completion(
                        "function",
                        "(${params}) => ${output}",
                        "A custom function.",
                    );
                } else {
                    ctx.completions.push(Completion {
                        kind: CompletionKind::Syntax,
                        label: ty.long_name().into(),
                        apply: Some(eco_format!("${{{ty}}}")),
                        detail: Some(eco_format!("A value of type {ty}.")),
                        ..Completion::default()
                    });
                    ctx.strict_scope_completions(false, |value| value.ty() == *ty);
                }
            }
            BuiltinTy::Element(e) => {
                ctx.value_completion(Some(e.name().into()), &Value::Func((*e).into()), true, docs);
            }
        },
        Ty::Args(_) => return None,
        Ty::Func(_) => return None,
        Ty::With(_) => return None,
        Ty::Select(_) => return None,
        Ty::Union(u) => {
            for info in u.as_ref() {
                type_completion(ctx, info, docs);
            }
        }
        Ty::Let(e) => {
            for ut in e.ubs.iter() {
                type_completion(ctx, ut, docs);
            }
            for lt in e.lbs.iter() {
                type_completion(ctx, lt, docs);
            }
        }
        Ty::Var(_) => return None,
        Ty::Unary(_) => return None,
        Ty::Binary(_) => return None,
        Ty::If(_) => return None,
        Ty::Value(v) => {
            let docs = v.syntax.as_ref().map(|s| s.doc.as_ref()).or(docs);

            if let Value::Type(ty) = &v.val {
                type_completion(ctx, &Ty::Builtin(BuiltinTy::Type(*ty)), docs);
            } else if v.val.ty() == Type::of::<NoneValue>() {
                type_completion(ctx, &Ty::Builtin(BuiltinTy::None), docs);
            } else if v.val.ty() == Type::of::<AutoValue>() {
                type_completion(ctx, &Ty::Builtin(BuiltinTy::Auto), docs);
            } else {
                ctx.value_completion(None, &v.val, true, docs);
            }
        }
    };

    Some(())
}

/// Add completions for the values of a named function parameter.
pub fn named_param_value_completions<'a>(
    ctx: &mut CompletionContext<'a, '_>,
    callee: ast::Expr<'a>,
    name: &Interned<str>,
    ty: Option<&Ty>,
) {
    let Some(cc) = ctx
        .root
        .find(callee.span())
        .and_then(|callee| resolve_call_target(ctx.ctx, &callee))
    else {
        // static analysis
        if let Some(ty) = ty {
            type_completion(ctx, ty, None);
        }

        return;
    };
    // todo: regards call convention
    let func = cc.callee();

    let leaf_type = ctx.ctx.literal_type_of_node(ctx.leaf.clone());
    log::debug!(
        "named_param_completion_by_type: {:?} -> {:?}",
        ctx.leaf.kind(),
        leaf_type
    );

    use typst::foundations::func::Repr;
    let mut func = func;
    while let Repr::With(f) = func.inner() {
        // todo: complete with positional arguments
        // with_args.push(ArgValue::Instance(f.1.clone()));
        func = f.0.clone();
    }

    let signature = analyze_dyn_signature(ctx.ctx, func.clone());

    let primary_sig = signature.primary();

    let Some(param) = primary_sig.named.get(name) else {
        return;
    };
    if !param.named {
        return;
    }

    let doc = Some(plain_docs_sentence(&param.docs));

    // static analysis
    if let Some(ty) = ty {
        type_completion(ctx, ty, doc.as_deref());
    }

    let mut completed = false;
    if let Some(type_sig) = leaf_type {
        log::debug!("named_param_completion by type: {:?}", param);
        type_completion(ctx, &type_sig, doc.as_deref());
        completed = true;
    }

    if !matches!(param.base_type, Ty::Any) {
        type_completion(ctx, &param.base_type, doc.as_deref());
        completed = true;
    }

    if !completed {
        if let Some(expr) = &param.expr {
            ctx.completions.push(Completion {
                kind: CompletionKind::Constant,
                label: expr.clone(),
                apply: None,
                detail: doc.map(Into::into),
                ..Completion::default()
            });
        }
    }

    sort_and_explicit_code_completion(ctx);
    if ctx.before.ends_with(':') {
        ctx.enrich(" ", "");
    }
}

/// Complete call and set rule parameters.
pub(crate) fn complete_type(ctx: &mut CompletionContext) -> Option<()> {
    use crate::syntax::get_check_target;

    let check_target = get_check_target(ctx.leaf.clone());
    log::debug!("complete_type: pos {:?} -> {check_target:#?}", ctx.leaf);

    match check_target {
        Some(CheckTarget::Element { container, .. }) => {
            if let Some(container) = container.cast::<ast::Dict>() {
                for named in container.items() {
                    if let ast::DictItem::Named(named) = named {
                        ctx.seen_field(named.name().into());
                    }
                }
            };
        }
        Some(CheckTarget::Param { args, .. }) => {
            let args = args.cast::<ast::Args>()?;
            for arg in args.items() {
                if let ast::Arg::Named(named) = arg {
                    ctx.seen_field(named.name().into());
                }
            }
        }
        Some(CheckTarget::Normal(e)) if matches!(e.kind(), SyntaxKind::Label | SyntaxKind::Ref) => {
        }
        Some(CheckTarget::Paren { .. }) => {}
        Some(CheckTarget::Normal(..)) => return None,
        None => return None,
    }

    let ty = ctx
        .ctx
        .literal_type_of_node(ctx.leaf.clone())
        .filter(|ty| !matches!(ty, Ty::Any))?;

    log::debug!("complete_type: ty  {:?} -> {ty:#?}", ctx.leaf);

    type_completion(ctx, &ty, None);
    if ctx.before.ends_with(',') || ctx.before.ends_with(':') {
        ctx.enrich(" ", "");
    }

    sort_and_explicit_code_completion(ctx);
    Some(())
}

pub fn complete_path(
    ctx: &AnalysisContext,
    v: Option<LinkedNode>,
    source: &Source,
    cursor: usize,
    p: &PathPreference,
) -> Option<Vec<CompletionItem>> {
    let id = source.id();
    if id.package().is_some() {
        return None;
    }

    let is_in_text;
    let text;
    let rng;
    let v = v.filter(|v| v.kind() == SyntaxKind::Str);
    if let Some(v) = v {
        // todo: the non-str case
        v.cast::<ast::Str>()?;

        let vr = v.range();
        rng = vr.start + 1..vr.end - 1;
        log::debug!("path_of: {rng:?} {cursor}");
        if rng.start > rng.end || (cursor != rng.end && !rng.contains(&cursor)) {
            return None;
        }

        let mut w = EcoString::new();
        w.push('"');
        w.push_str(&source.text()[rng.start..cursor]);
        w.push('"');
        let partial_str = SyntaxNode::leaf(SyntaxKind::Str, w);
        log::debug!("path_of: {rng:?} {partial_str:?}");

        text = partial_str.cast::<ast::Str>()?.get();
        is_in_text = true;
    } else {
        text = EcoString::default();
        rng = cursor..cursor;
        is_in_text = false;
    }
    log::debug!("complete_path: is_in_text: {is_in_text:?}");
    let path = Path::new(text.as_str());
    let has_root = path.has_root();

    let src_path = id.vpath();
    let base = src_path.resolve(&ctx.root)?;
    let dst_path = src_path.join(path);
    let mut compl_path = dst_path.as_rootless_path();
    if !compl_path.is_dir() {
        compl_path = compl_path.parent().unwrap_or(Path::new(""));
    }
    log::debug!("compl_path: {src_path:?} + {path:?} -> {compl_path:?}");

    if compl_path.is_absolute() {
        log::warn!("absolute path completion is not supported for security consideration {path:?}");
        return None;
    }

    let dirs = ctx.root.clone();
    log::debug!("compl_dirs: {dirs:?}");
    // find directory or files in the path
    let mut folder_completions = vec![];
    let mut module_completions = vec![];
    // todo: test it correctly
    for path in ctx.completion_files(p) {
        log::debug!("compl_check_path: {path:?}");

        // diff with root
        let path = dirs.join(path);

        // Skip self smartly
        if path.clean() == base.clean() {
            continue;
        }

        let label = if has_root {
            // diff with root
            let w = path.strip_prefix(&ctx.root).ok()?;
            eco_format!("/{}", unix_slash(w))
        } else {
            let base = base.parent()?;
            let w = pathdiff::diff_paths(&path, base)?;
            unix_slash(&w).into()
        };
        log::debug!("compl_label: {label:?}");

        if path.is_dir() {
            folder_completions.push((label, CompletionKind::Folder));
        } else {
            module_completions.push((label, CompletionKind::File));
        }
    }

    let replace_range = ctx.to_lsp_range(rng, source);

    let path_priority_cmp = |a: &str, b: &str| {
        // files are more important than dot started paths
        if a.starts_with('.') || b.starts_with('.') {
            // compare consecutive dots and slashes
            let a_prefix = a.chars().take_while(|c| *c == '.' || *c == '/').count();
            let b_prefix = b.chars().take_while(|c| *c == '.' || *c == '/').count();
            if a_prefix != b_prefix {
                return a_prefix.cmp(&b_prefix);
            }
        }
        a.cmp(b)
    };

    module_completions.sort_by(|a, b| path_priority_cmp(&a.0, &b.0));
    folder_completions.sort_by(|a, b| path_priority_cmp(&a.0, &b.0));

    let mut sorter = 0;
    let digits = (module_completions.len() + folder_completions.len())
        .to_string()
        .len();
    let completions = module_completions.into_iter().chain(folder_completions);
    Some(
        completions
            .map(|typst_completion| {
                let lsp_snippet = &typst_completion.0;
                let text_edit = CompletionTextEdit::Edit(TextEdit::new(
                    replace_range,
                    if is_in_text {
                        lsp_snippet.to_string()
                    } else {
                        format!(r#""{lsp_snippet}""#)
                    },
                ));

                let sort_text = format!("{sorter:0>digits$}");
                sorter += 1;

                // todo: no all clients support label details
                let res = LspCompletion {
                    label: typst_completion.0.to_string(),
                    kind: Some(completion_kind(typst_completion.1.clone())),
                    detail: None,
                    text_edit: Some(text_edit),
                    // don't sort me
                    sort_text: Some(sort_text),
                    filter_text: Some("".to_owned()),
                    insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                    ..Default::default()
                };

                log::debug!("compl_res: {res:?}");

                res
            })
            .collect_vec(),
    )
}

/// If is printable, return the symbol itself.
/// Otherwise, return the symbol's unicode detailed description.
pub fn symbol_detail(ch: char) -> EcoString {
    let ld = symbol_label_detail(ch);
    if ld.starts_with("\\u") {
        return ld;
    }
    format!("{}, unicode: `\\u{{{:04x}}}`", ld, ch as u32).into()
}

/// If is printable, return the symbol itself.
/// Otherwise, return the symbol's unicode description.
pub fn symbol_label_detail(ch: char) -> EcoString {
    if !ch.is_whitespace() && !ch.is_control() {
        return ch.into();
    }
    match ch {
        ' ' => "space".into(),
        '\t' => "tab".into(),
        '\n' => "newline".into(),
        '\r' => "carriage return".into(),
        // replacer
        '\u{200D}' => "zero width joiner".into(),
        '\u{200C}' => "zero width non-joiner".into(),
        '\u{200B}' => "zero width space".into(),
        '\u{2060}' => "word joiner".into(),
        // spaces
        '\u{00A0}' => "non-breaking space".into(),
        '\u{202F}' => "narrow no-break space".into(),
        '\u{2002}' => "en space".into(),
        '\u{2003}' => "em space".into(),
        '\u{2004}' => "three-per-em space".into(),
        '\u{2005}' => "four-per-em space".into(),
        '\u{2006}' => "six-per-em space".into(),
        '\u{2007}' => "figure space".into(),
        '\u{205f}' => "medium mathematical space".into(),
        '\u{2008}' => "punctuation space".into(),
        '\u{2009}' => "thin space".into(),
        '\u{200A}' => "hair space".into(),
        _ => format!("\\u{{{:04x}}}", ch as u32).into(),
    }
}

#[cfg(test)]

mod tests {
    use crate::upstream::complete::slice_at;

    #[test]
    fn test_before() {
        const TEST_UTF8_STR: &str = "我们";
        for i in 0..=TEST_UTF8_STR.len() {
            for j in 0..=TEST_UTF8_STR.len() {
                let _s = std::hint::black_box(slice_at(TEST_UTF8_STR, i..j));
            }
        }
    }
}

// todo: doesn't complete parameter now, which is not good.
