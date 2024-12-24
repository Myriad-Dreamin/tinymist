use std::collections::BTreeMap;
use std::ops::Deref;

use ecow::{eco_format, EcoString};
use hashbrown::HashSet;
use lsp_types::{CompletionItem, CompletionTextEdit, InsertTextFormat, TextEdit};
use reflexo::path::unix_slash;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tinymist_derive::BindTyCtx;
use tinymist_world::LspWorld;
use typst::foundations::{
    fields_on, AutoValue, Func, Label, NoneValue, Scope, StyleChain, Type, Value,
};
use typst::syntax::ast::AstNode;
use typst::syntax::{ast, SyntaxKind, SyntaxNode};
use typst::visualize::Color;

use super::{Completion, CompletionContext, CompletionKind};
use crate::adt::interner::Interned;
use crate::analysis::{func_signature, BuiltinTy, PathPreference, Ty};
use crate::snippet::{ParsedSnippet, PostfixSnippet, PostfixSnippetScope, DEFAULT_POSTFIX_SNIPPET};
use crate::syntax::{
    interpret_mode_at, is_ident_like, previous_decls, surrounding_syntax, InterpretMode,
    PreviousDecl, SurroundingSyntax, SyntaxClass, SyntaxContext, VarClass,
};
use crate::ty::{
    DynTypeBounds, Iface, IfaceChecker, InsTy, SigTy, TyCtx, TypeInfo, TypeInterface, TypeVar,
};
use crate::upstream::complete::complete_code;
use crate::{completion_kind, prelude::*, LspCompletion};

/// Tinymist's completion features.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionFeat {
    /// Whether to trigger completions on arguments (placeholders) of snippets.
    #[serde(default)]
    pub trigger_on_snippet_placeholders: bool,
    /// Whether supports trigger suggest completion, a.k.a. auto-completion.
    #[serde(default)]
    pub trigger_suggest: bool,
    /// Whether supports trigger parameter hint, a.k.a. signature help.
    #[serde(default)]
    pub trigger_parameter_hints: bool,
    /// Whether supports trigger the command combining suggest and parameter
    /// hints.
    #[serde(default)]
    pub trigger_suggest_and_parameter_hints: bool,

    /// Whether to enable postfix completion.
    pub postfix: Option<bool>,
    /// Whether to enable ufcs completion.
    pub postfix_ufcs: Option<bool>,
    /// Whether to enable ufcs completion (left variant).
    pub postfix_ufcs_left: Option<bool>,
    /// Whether to enable ufcs completion (right variant).
    pub postfix_ufcs_right: Option<bool>,
    /// Postfix snippets.
    pub postfix_snippets: Option<EcoVec<PostfixSnippet>>,
}

impl CompletionFeat {
    pub(crate) fn any_ufcs(&self) -> bool {
        self.ufcs() || self.ufcs_left() || self.ufcs_right()
    }
    pub(crate) fn postfix(&self) -> bool {
        self.postfix.unwrap_or(true)
    }
    pub(crate) fn ufcs(&self) -> bool {
        self.postfix() && self.postfix_ufcs.unwrap_or(true)
    }
    pub(crate) fn ufcs_left(&self) -> bool {
        self.postfix() && self.postfix_ufcs_left.unwrap_or(true)
    }
    pub(crate) fn ufcs_right(&self) -> bool {
        self.postfix() && self.postfix_ufcs_right.unwrap_or(true)
    }

    pub(crate) fn postfix_snippets(&self) -> &[PostfixSnippet] {
        self.postfix_snippets
            .as_ref()
            .map_or(DEFAULT_POSTFIX_SNIPPET.deref(), |v| v.as_slice())
    }
}

impl CompletionContext<'_> {
    pub fn world(&self) -> &LspWorld {
        self.ctx.world()
    }

    fn seen_field(&mut self, field: Interned<str>) -> bool {
        !self.seen_fields.insert(field)
    }

    pub(crate) fn surrounding_syntax(&mut self) -> SurroundingSyntax {
        surrounding_syntax(&self.leaf)
    }

    fn scope_defs(&mut self) -> Option<(Source, Defines)> {
        let src = self.ctx.source_by_id(self.root.span().id()?).ok()?;

        let mut defines = Defines {
            types: self.ctx.type_check(&src),
            defines: Default::default(),
            docs: Default::default(),
        };

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
        defines.insert_scope(&scope);

        previous_decls(self.leaf.clone(), |node| -> Option<()> {
            match node {
                PreviousDecl::Ident(ident) => {
                    let ty = self.ctx.type_of_span(ident.span()).unwrap_or(Ty::Any);
                    defines.insert_ty(ty, ident.get());
                }
                PreviousDecl::ImportSource(src) => {
                    let ty = analyze_import_source(self.ctx, &defines.types, src)?;
                    let name = ty.name().as_ref().into();
                    defines.insert_ty(ty, &name);
                }
                // todo: cache completion items
                PreviousDecl::ImportAll(mi) => {
                    let ty = analyze_import_source(self.ctx, &defines.types, mi.source())?;
                    ty.iface_surface(
                        true,
                        &mut CompletionScopeChecker {
                            check_kind: ScopeCheckKind::Import,
                            defines: &mut defines,
                            ctx: self.ctx,
                        },
                    );
                }
            }
            None
        });

        Some((src, defines))
    }

    pub fn postfix_completions(&mut self, node: &LinkedNode, ty: Ty) -> Option<()> {
        if !self.ctx.analysis.completion_feat.postfix() {
            return None;
        }
        let src = self.ctx.source_by_id(self.root.span().id()?).ok()?;

        let _ = node;

        let surrounding_syntax = self.surrounding_syntax();
        if !matches!(surrounding_syntax, SurroundingSyntax::Regular) {
            return None;
        }

        let cursor_mode = interpret_mode_at(Some(node));
        let is_content = ty.is_content(&());
        crate::log_debug_ct!("post snippet is_content: {is_content}");

        let rng = node.range();
        for snippet in self.ctx.analysis.completion_feat.postfix_snippets() {
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
                label_detail: snippet.label_detail.clone(),
                detail: Some(snippet.description.clone()),
                // range: Some(range),
                ..Default::default()
            };
            if let Some(node_before_before_cursor) = &node_before_before_cursor {
                let node_content = node.get().clone().into_text();
                let before = TextEdit {
                    range: self.ctx.to_lsp_range(rng.start..self.from, &src),
                    new_text: String::new(),
                };

                self.completions.push(Completion {
                    apply: Some(eco_format!(
                        "{node_before_before_cursor}{node_before}{node_content}{node_after}"
                    )),
                    additional_text_edits: Some(vec![before]),
                    ..base
                });
            } else {
                let before = TextEdit {
                    range: self.ctx.to_lsp_range(rng.start..rng.start, &src),
                    new_text: node_before.as_ref().into(),
                };
                let after = TextEdit {
                    range: self.ctx.to_lsp_range(rng.end..self.from, &src),
                    new_text: "".into(),
                };
                self.completions.push(Completion {
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
        if !self.ctx.analysis.completion_feat.any_ufcs() {
            return;
        }

        let surrounding_syntax = self.surrounding_syntax();
        if !matches!(surrounding_syntax, SurroundingSyntax::Regular) {
            return;
        }

        let Some((src, defines)) = self.scope_defs() else {
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

            let label_detail = ty.describe().map(From::from).or_else(|| Some("any".into()));
            let base = Completion {
                kind: CompletionKind::Func,
                label_detail,
                apply: Some("".into()),
                // range: Some(range),
                command: self.ctx.analysis.trigger_on_snippet_with_param_hint(true),
                ..Default::default()
            };
            let fn_feat = FnCompletionFeat::default().check(kind_checker.functions.iter());

            crate::log_debug_ct!("fn_feat: {name} {ty:?} -> {fn_feat:?}");

            if fn_feat.min_pos() < 1 || !fn_feat.next_arg_is_content {
                continue;
            }
            crate::log_debug_ct!("checked ufcs: {ty:?}");
            if self.ctx.analysis.completion_feat.ufcs() && fn_feat.min_pos() == 1 {
                let before = TextEdit {
                    range: self.ctx.to_lsp_range(rng.start..rng.start, &src),
                    new_text: format!("{name}{lb}"),
                };
                let after = TextEdit {
                    range: self.ctx.to_lsp_range(rng.end..self.from, &src),
                    new_text: rb.into(),
                };

                self.completions.push(Completion {
                    label: name.clone(),
                    additional_text_edits: Some(vec![before, after]),
                    ..base.clone()
                });
            }
            let more_args = fn_feat.min_pos() > 1 || fn_feat.min_named() > 0;
            if self.ctx.analysis.completion_feat.ufcs_left() && more_args {
                let node_content = node.get().clone().into_text();
                let before = TextEdit {
                    range: self.ctx.to_lsp_range(rng.start..self.from, &src),
                    new_text: format!("{name}{lb}"),
                };
                self.completions.push(Completion {
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
            if self.ctx.analysis.completion_feat.ufcs_right() && more_args {
                let before = TextEdit {
                    range: self.ctx.to_lsp_range(rng.start..rng.start, &src),
                    new_text: format!("{name}("),
                };
                let after = TextEdit {
                    range: self.ctx.to_lsp_range(rng.end..self.from, &src),
                    new_text: "".into(),
                };
                self.completions.push(Completion {
                    apply: Some(eco_format!("${{}})")),
                    label: eco_format!("{name})"),
                    additional_text_edits: Some(vec![before, after]),
                    ..base
                });
            }
        }
    }

    /// Add completions for definitions that are available at the cursor.
    pub fn scope_completions(&mut self, parens: bool) {
        let Some((_, defines)) = self.scope_defs() else {
            return;
        };

        self.def_completions(defines, parens);
    }

    /// Add completions for definitions.
    fn def_completions(&mut self, defines: Defines, parens: bool) {
        let default_docs = defines.docs;
        let defines = defines.defines;

        let surrounding_syntax = self.surrounding_syntax();
        let mode = interpret_mode_at(Some(&self.leaf));

        let mut kind_checker = CompletionKindChecker {
            symbols: HashSet::default(),
            functions: HashSet::default(),
        };

        let filter = |checker: &CompletionKindChecker| {
            match surrounding_syntax {
                SurroundingSyntax::Regular => true,
                SurroundingSyntax::StringContent | SurroundingSyntax::ImportList => false,
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
                self.completions.push(Completion {
                    kind,
                    label: name,
                    label_detail: Some(symbol_label_detail(ch)),
                    detail: Some(symbol_detail(ch)),
                    ..Completion::default()
                });
                continue;
            }

            let docs = default_docs.get(&name).cloned();

            let label_detail = ty.describe().map(From::from).or_else(|| Some("any".into()));

            crate::log_debug_ct!("scope completions!: {name} {ty:?} {label_detail:?}");
            let detail = docs.or_else(|| label_detail.clone());

            if !kind_checker.functions.is_empty() {
                let base = Completion {
                    kind: CompletionKind::Func,
                    label_detail,
                    detail,
                    command: self.ctx.analysis.trigger_on_snippet_with_param_hint(true),
                    ..Default::default()
                };

                let fn_feat = FnCompletionFeat::default().check(kind_checker.functions.iter());

                crate::log_debug_ct!("fn_feat: {name} {ty:?} -> {fn_feat:?}");

                if matches!(surrounding_syntax, SurroundingSyntax::ShowTransform)
                    && (fn_feat.min_pos() > 0 || fn_feat.min_named() > 0)
                {
                    self.completions.push(Completion {
                        label: eco_format!("{}.with", name),
                        apply: Some(eco_format!("{}.with(${{}})", name)),
                        ..base.clone()
                    });
                }
                if fn_feat.is_element && matches!(surrounding_syntax, SurroundingSyntax::Selector) {
                    self.completions.push(Completion {
                        label: eco_format!("{}.where", name),
                        apply: Some(eco_format!("{}.where(${{}})", name)),
                        ..base.clone()
                    });
                }

                let bad_instantiate = matches!(
                    surrounding_syntax,
                    SurroundingSyntax::Selector | SurroundingSyntax::SetRule
                ) && !fn_feat.is_element;
                if !bad_instantiate {
                    if !parens || matches!(surrounding_syntax, SurroundingSyntax::Selector) {
                        self.completions.push(Completion {
                            label: name,
                            ..base
                        });
                    } else if fn_feat.min_pos() < 1 && !fn_feat.has_rest {
                        self.completions.push(Completion {
                            apply: Some(eco_format!("{}()${{}}", name)),
                            label: name,
                            ..base
                        });
                    } else {
                        let accept_content_arg = fn_feat.next_arg_is_content && !fn_feat.has_rest;
                        let scope_reject_content = matches!(mode, InterpretMode::Math)
                            || matches!(
                                surrounding_syntax,
                                SurroundingSyntax::Selector | SurroundingSyntax::SetRule
                            );
                        self.completions.push(Completion {
                            apply: Some(eco_format!("{name}(${{}})")),
                            label: name.clone(),
                            ..base.clone()
                        });
                        if !scope_reject_content && accept_content_arg {
                            self.completions.push(Completion {
                                apply: Some(eco_format!("{name}[${{}}]")),
                                label: eco_format!("{name}.bracket"),
                                ..base
                            });
                        };
                    }
                }
                continue;
            }

            let kind = ty_to_completion_kind(&ty);
            self.completions.push(Completion {
                kind,
                label: name,
                label_detail: label_detail.clone(),
                detail,
                ..Completion::default()
            });
        }
    }
    /// Add completions for all fields on a node.
    fn field_access_completions(&mut self, target: &LinkedNode) -> Option<()> {
        self.value_field_access_completions(target)
            .or_else(|| self.type_field_access_completions(target))
    }

    /// Add completions for all fields on a type.
    fn type_field_access_completions(&mut self, target: &LinkedNode) -> Option<()> {
        let ty = self
            .ctx
            .post_type_of_node(target.clone())
            .filter(|ty| !matches!(ty, Ty::Any));
        crate::log_debug_ct!("type_field_access_completions_on: {target:?} -> {ty:?}");

        let src = self.ctx.source_by_id(self.root.span().id()?).ok()?;
        let mut defines = Defines {
            types: self.ctx.type_check(&src),
            defines: Default::default(),
            docs: Default::default(),
        };
        ty?.iface_surface(
            true,
            &mut CompletionScopeChecker {
                check_kind: ScopeCheckKind::FieldAccess,
                defines: &mut defines,
                ctx: self.ctx,
            },
        );

        self.def_completions(defines, true);
        Some(())
    }

    /// Add completions for all fields on a value.
    fn value_field_access_completions(&mut self, target: &LinkedNode) -> Option<()> {
        let (value, styles) = self.ctx.analyze_expr(target).into_iter().next()?;
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
                        self.completions.push(Completion {
                            kind: CompletionKind::Symbol(modified.get()),
                            label: modifier.into(),
                            label_detail: Some(symbol_label_detail(modified.get())),
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
                    self.completions.push(Completion {
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

#[derive(BindTyCtx)]
#[bind(types)]
struct Defines {
    types: Arc<TypeInfo>,
    defines: BTreeMap<EcoString, Ty>,
    docs: BTreeMap<EcoString, EcoString>,
}

impl Defines {
    fn insert(&mut self, name: EcoString, item: Ty) {
        if name.is_empty() {
            return;
        }

        if let std::collections::btree_map::Entry::Vacant(entry) = self.defines.entry(name.clone())
        {
            entry.insert(item);
        }
    }

    fn insert_ty(&mut self, ty: Ty, name: &EcoString) {
        self.insert(name.clone(), ty);
    }

    fn insert_scope(&mut self, scope: &Scope) {
        // filter(Some(value)) &&
        for (name, value, _) in scope.iter() {
            if !self.defines.contains_key(name) {
                self.insert(name.clone(), Ty::Value(InsTy::new(value.clone())));
            }
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

enum ScopeCheckKind {
    Import,
    FieldAccess,
}

#[derive(BindTyCtx)]
#[bind(defines)]
struct CompletionScopeChecker<'a> {
    check_kind: ScopeCheckKind,
    defines: &'a mut Defines,
    ctx: &'a mut LocalContext,
}

impl CompletionScopeChecker<'_> {
    fn is_only_importable(&self) -> bool {
        matches!(self.check_kind, ScopeCheckKind::Import)
    }

    fn is_field_access(&self) -> bool {
        matches!(self.check_kind, ScopeCheckKind::FieldAccess)
    }

    fn type_methods(&mut self, ty: Type) {
        for name in fields_on(ty) {
            self.defines.insert((*name).into(), Ty::Any);
        }
        for (name, value, _) in ty.scope().iter() {
            let ty = Ty::Value(InsTy::new(value.clone()));
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
            Iface::Dict(..) | Iface::Value { .. } => {}
            Iface::Element { val, .. } if self.is_field_access() => {
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
            Iface::Type { val, .. } if self.is_field_access() => {
                self.type_methods(*val);
            }
            Iface::Func { .. } if self.is_field_access() => {
                self.type_methods(Type::of::<Func>());
            }
            Iface::Element { val, .. } => {
                self.defines.insert_scope(val.scope());
            }
            Iface::Type { val, .. } => {
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
        }
        None
    }
}

pub(crate) struct CompletionKindChecker {
    pub(crate) symbols: HashSet<char>,
    pub(crate) functions: HashSet<Ty>,
}
impl CompletionKindChecker {
    fn reset(&mut self) {
        self.symbols.clear();
        self.functions.clear();
    }

    fn check(&mut self, ty: &Ty) {
        self.reset();
        match ty {
            Ty::Value(val) => match &val.val {
                Value::Type(t) if t.constructor().is_ok() => {
                    self.functions.insert(ty.clone());
                }
                Value::Func(..) => {
                    self.functions.insert(ty.clone());
                }
                Value::Symbol(s) => {
                    self.symbols.insert(s.get());
                }
                _ => {}
            },
            Ty::Func(..) | Ty::With(..) => {
                self.functions.insert(ty.clone());
            }
            Ty::Builtin(BuiltinTy::TypeType(t)) if t.constructor().is_ok() => {
                self.functions.insert(ty.clone());
            }
            Ty::Builtin(BuiltinTy::Element(..)) => {
                self.functions.insert(ty.clone());
            }
            Ty::Let(bounds) => {
                for bound in bounds.ubs.iter().chain(bounds.lbs.iter()) {
                    self.check(bound);
                }
            }
            Ty::Any
            | Ty::Builtin(..)
            | Ty::Boolean(..)
            | Ty::Param(..)
            | Ty::Union(..)
            | Ty::Var(..)
            | Ty::Dict(..)
            | Ty::Array(..)
            | Ty::Tuple(..)
            | Ty::Args(..)
            | Ty::Pattern(..)
            | Ty::Select(..)
            | Ty::Unary(..)
            | Ty::Binary(..)
            | Ty::If(..) => {}
        }
    }
}

#[derive(Default, Debug)]
struct FnCompletionFeat {
    min_pos: Option<usize>,
    min_named: Option<usize>,
    has_rest: bool,
    next_arg_is_content: bool,
    is_element: bool,
}

impl FnCompletionFeat {
    fn check<'a>(mut self, fns: impl ExactSizeIterator<Item = &'a Ty>) -> Self {
        for ty in fns {
            self.check_one(ty, 0);
        }

        self
    }

    fn min_pos(&self) -> usize {
        self.min_pos.unwrap_or_default()
    }

    fn min_named(&self) -> usize {
        self.min_named.unwrap_or_default()
    }

    fn check_one(&mut self, ty: &Ty, pos: usize) {
        match ty {
            Ty::Value(val) => match &val.val {
                Value::Type(ty) => {
                    self.check_one(&Ty::Builtin(BuiltinTy::Type(*ty)), pos);
                }
                Value::Func(func) => {
                    if func.element().is_some() {
                        self.is_element = true;
                    }
                    let sig = func_signature(func.clone()).type_sig();
                    self.check_sig(&sig, pos);
                }
                Value::None
                | Value::Auto
                | Value::Bool(_)
                | Value::Int(_)
                | Value::Float(..)
                | Value::Length(..)
                | Value::Angle(..)
                | Value::Ratio(..)
                | Value::Relative(..)
                | Value::Fraction(..)
                | Value::Color(..)
                | Value::Gradient(..)
                | Value::Pattern(..)
                | Value::Symbol(..)
                | Value::Version(..)
                | Value::Str(..)
                | Value::Bytes(..)
                | Value::Label(..)
                | Value::Datetime(..)
                | Value::Decimal(..)
                | Value::Duration(..)
                | Value::Content(..)
                | Value::Styles(..)
                | Value::Array(..)
                | Value::Dict(..)
                | Value::Args(..)
                | Value::Module(..)
                | Value::Plugin(..)
                | Value::Dyn(..) => {}
            },
            Ty::Func(sig) => self.check_sig(sig, pos),
            Ty::With(w) => {
                self.check_one(&w.sig, pos + w.with.positional_params().len());
            }
            Ty::Builtin(b) => match b {
                BuiltinTy::Element(func) => {
                    self.is_element = true;
                    let func = (*func).into();
                    let sig = func_signature(func).type_sig();
                    self.check_sig(&sig, pos);
                }
                BuiltinTy::Type(ty) => {
                    let func = ty.constructor().ok();
                    if let Some(func) = func {
                        let sig = func_signature(func).type_sig();
                        self.check_sig(&sig, pos);
                    }
                }
                BuiltinTy::TypeType(..) => {}
                BuiltinTy::Clause
                | BuiltinTy::Undef
                | BuiltinTy::Content
                | BuiltinTy::Space
                | BuiltinTy::None
                | BuiltinTy::Break
                | BuiltinTy::Continue
                | BuiltinTy::Infer
                | BuiltinTy::FlowNone
                | BuiltinTy::Auto
                | BuiltinTy::Args
                | BuiltinTy::Color
                | BuiltinTy::TextSize
                | BuiltinTy::TextFont
                | BuiltinTy::TextLang
                | BuiltinTy::TextRegion
                | BuiltinTy::Label
                | BuiltinTy::CiteLabel
                | BuiltinTy::RefLabel
                | BuiltinTy::Dir
                | BuiltinTy::Length
                | BuiltinTy::Float
                | BuiltinTy::Stroke
                | BuiltinTy::Margin
                | BuiltinTy::Inset
                | BuiltinTy::Outset
                | BuiltinTy::Radius
                | BuiltinTy::Tag(..)
                | BuiltinTy::Module(..)
                | BuiltinTy::Path(..) => {}
            },
            Ty::Any
            | Ty::Boolean(..)
            | Ty::Param(..)
            | Ty::Union(..)
            | Ty::Let(..)
            | Ty::Var(..)
            | Ty::Dict(..)
            | Ty::Array(..)
            | Ty::Tuple(..)
            | Ty::Args(..)
            | Ty::Pattern(..)
            | Ty::Select(..)
            | Ty::Unary(..)
            | Ty::Binary(..)
            | Ty::If(..) => {}
        }
    }

    // todo: sig is element
    fn check_sig(&mut self, sig: &SigTy, idx: usize) {
        let pos_size = sig.positional_params().len();
        self.has_rest = self.has_rest || sig.rest_param().is_some();
        self.next_arg_is_content =
            self.next_arg_is_content || sig.pos(idx).map_or(false, |ty| ty.is_content(&()));
        let name_size = sig.named_params().len();
        let left_pos = pos_size.saturating_sub(idx);
        self.min_pos = self
            .min_pos
            .map_or(Some(left_pos), |v| Some(v.min(left_pos)));
        self.min_named = self
            .min_named
            .map_or(Some(name_size), |v| Some(v.min(name_size)));
    }
}

pub fn ty_to_completion_kind(ty: &Ty) -> CompletionKind {
    match ty {
        Ty::Value(ins_ty) => value_to_completion_kind(&ins_ty.val),
        Ty::Func(..) | Ty::With(..) => CompletionKind::Func,
        Ty::Any => CompletionKind::Variable,
        Ty::Builtin(b) => match b {
            BuiltinTy::Module(..) => CompletionKind::Module,
            BuiltinTy::Type(..) | BuiltinTy::TypeType(..) => CompletionKind::Type,
            _ => CompletionKind::Variable,
        },
        Ty::Let(bounds) => fold_ty_kind(bounds.ubs.iter().chain(bounds.lbs.iter())),
        Ty::Union(types) => fold_ty_kind(types.iter()),
        Ty::Boolean(..)
        | Ty::Param(..)
        | Ty::Var(..)
        | Ty::Dict(..)
        | Ty::Array(..)
        | Ty::Tuple(..)
        | Ty::Args(..)
        | Ty::Pattern(..)
        | Ty::Select(..)
        | Ty::Unary(..)
        | Ty::Binary(..)
        | Ty::If(..) => CompletionKind::Constant,
    }
}

fn fold_ty_kind<'a>(tys: impl Iterator<Item = &'a Ty>) -> CompletionKind {
    tys.fold(None, |acc, ty| match acc {
        Some(CompletionKind::Variable) => Some(CompletionKind::Variable),
        Some(acc) => {
            let kind = ty_to_completion_kind(ty);
            if acc == kind {
                Some(acc)
            } else {
                Some(CompletionKind::Variable)
            }
        }
        None => Some(ty_to_completion_kind(ty)),
    })
    .unwrap_or(CompletionKind::Variable)
}

pub fn value_to_completion_kind(value: &Value) -> CompletionKind {
    match value {
        Value::Func(..) => CompletionKind::Func,
        Value::Plugin(..) | Value::Module(..) => CompletionKind::Module,
        Value::Type(..) => CompletionKind::Type,
        Value::Symbol(s) => CompletionKind::Symbol(s.get()),
        Value::None
        | Value::Auto
        | Value::Bool(..)
        | Value::Int(..)
        | Value::Float(..)
        | Value::Length(..)
        | Value::Angle(..)
        | Value::Ratio(..)
        | Value::Relative(..)
        | Value::Fraction(..)
        | Value::Color(..)
        | Value::Gradient(..)
        | Value::Pattern(..)
        | Value::Version(..)
        | Value::Str(..)
        | Value::Bytes(..)
        | Value::Label(..)
        | Value::Datetime(..)
        | Value::Decimal(..)
        | Value::Duration(..)
        | Value::Content(..)
        | Value::Styles(..)
        | Value::Array(..)
        | Value::Dict(..)
        | Value::Args(..)
        | Value::Dyn(..) => CompletionKind::Variable,
    }
}

// if param.attrs.named {
//     match param.ty {
//         Ty::Builtin(BuiltinTy::TextSize) => {
//             for size_template in &[
//                 "10.5pt", "12pt", "9pt", "14pt", "8pt", "16pt", "18pt",
// "20pt", "22pt",                 "24pt", "28pt",
//             ] {
//                 let compl = compl.clone();
//                 ctx.completions.push(Completion {
//                     label: eco_format!("{}: {}", param.name, size_template),
//                     apply: None,
//                     ..compl
//                 });
//             }
//         }
//         Ty::Builtin(BuiltinTy::Dir) => {
//             for dir_template in &["ltr", "rtl", "ttb", "btt"] {
//                 let compl = compl.clone();
//                 ctx.completions.push(Completion {
//                     label: eco_format!("{}: {}", param.name, dir_template),
//                     apply: None,
//                     ..compl
//                 });
//             }
//         }
//         _ => {}
//     }
//     ctx.completions.push(compl);
// }

struct TypeCompletionContext<'a, 'b> {
    ctx: &'a mut CompletionContext<'b>,
    filter: &'a dyn Fn(&Ty) -> bool,
}

impl TypeCompletionContext<'_, '_> {
    fn snippet_completion(&mut self, label: &str, apply: &str, detail: &str) {
        if !(self.filter)(&Ty::Any) {
            return;
        }

        self.ctx.snippet_completion(label, apply, detail);
    }

    fn type_completion(&mut self, infer_type: &Ty, docs: Option<&str>) -> Option<()> {
        // Prevent duplicate completions from appearing.
        if !self.ctx.seen_types.insert(infer_type.clone()) {
            return Some(());
        }

        crate::log_debug_ct!("type_completion: {infer_type:?}");

        match infer_type {
            Ty::Any => return None,
            Ty::Pattern(_) => return None,
            Ty::Args(_) => return None,
            Ty::Func(_) => return None,
            Ty::With(_) => return None,
            Ty::Select(_) => return None,
            Ty::Var(_) => return None,
            Ty::Unary(_) => return None,
            Ty::Binary(_) => return None,
            Ty::If(_) => return None,
            Ty::Union(u) => {
                for info in u.as_ref() {
                    self.type_completion(info, docs);
                }
            }
            Ty::Let(bounds) => {
                for ut in bounds.ubs.iter() {
                    self.type_completion(ut, docs);
                }
                for lt in bounds.lbs.iter() {
                    self.type_completion(lt, docs);
                }
            }
            Ty::Tuple(..) | Ty::Array(..) => {
                if !(self.filter)(infer_type) {
                    return None;
                }
                self.snippet_completion("()", "(${})", "An array.");
            }
            Ty::Dict(..) => {
                if !(self.filter)(infer_type) {
                    return None;
                }
                self.snippet_completion("()", "(${})", "A dictionary.");
            }
            Ty::Boolean(_b) => {
                if !(self.filter)(infer_type) {
                    return None;
                }
                self.snippet_completion("false", "false", "No / Disabled.");
                self.snippet_completion("true", "true", "Yes / Enabled.");
            }
            Ty::Builtin(v) => {
                if !(self.filter)(infer_type) {
                    return None;
                }
                self.builtin_type_completion(v, docs);
            }
            Ty::Value(v) => {
                if !(self.filter)(infer_type) {
                    return None;
                }
                let docs = v.syntax.as_ref().map(|s| s.doc.as_ref()).or(docs);

                if let Value::Type(ty) = &v.val {
                    self.type_completion(&Ty::Builtin(BuiltinTy::Type(*ty)), docs);
                } else if v.val.ty() == Type::of::<NoneValue>() {
                    self.type_completion(&Ty::Builtin(BuiltinTy::None), docs);
                } else if v.val.ty() == Type::of::<AutoValue>() {
                    self.type_completion(&Ty::Builtin(BuiltinTy::Auto), docs);
                } else {
                    self.ctx.value_completion(None, &v.val, true, docs);
                }
            }
            Ty::Param(param) => {
                // todo: variadic

                let docs = docs.or_else(|| param.docs.as_deref());
                if param.attrs.positional {
                    self.type_completion(&param.ty, docs);
                }
                if !param.attrs.named {
                    return Some(());
                }

                let field = &param.name;
                if self.ctx.seen_field(field.clone()) {
                    return Some(());
                }
                if !(self.filter)(infer_type) {
                    return None;
                }

                let mut rev_stream = self.ctx.before.chars().rev();
                let ch = rev_stream.find(|ch| !typst::syntax::is_id_continue(*ch));
                // skip label/ref completion.
                // todo: more elegant way
                if matches!(ch, Some('<' | '@')) {
                    return Some(());
                }

                self.ctx.completions.push(Completion {
                    kind: CompletionKind::Field,
                    label: field.into(),
                    apply: Some(eco_format!("{}: ${{}}", field)),
                    label_detail: param.ty.describe(),
                    detail: docs.map(Into::into),
                    command: self
                        .ctx
                        .ctx
                        .analysis
                        .trigger_on_snippet_with_param_hint(true),
                    ..Completion::default()
                });
            }
        };

        Some(())
    }

    fn builtin_type_completion(&mut self, v: &BuiltinTy, docs: Option<&str>) -> Option<()> {
        match v {
            BuiltinTy::None => self.snippet_completion("none", "none", "Nothing."),
            BuiltinTy::Auto => {
                self.snippet_completion("auto", "auto", "A smart default.");
            }
            BuiltinTy::Clause => return None,
            BuiltinTy::Undef => return None,
            BuiltinTy::Space => return None,
            BuiltinTy::Break => return None,
            BuiltinTy::Continue => return None,
            BuiltinTy::Content => return None,
            BuiltinTy::Infer => return None,
            BuiltinTy::FlowNone => return None,
            BuiltinTy::Tag(..) => return None,
            BuiltinTy::Module(..) => return None,

            BuiltinTy::Path(preference) => {
                let source = self.ctx.ctx.source_by_id(self.ctx.root.span().id()?).ok()?;

                self.ctx.completions2.extend(
                    complete_path(
                        self.ctx.ctx,
                        Some(self.ctx.leaf.clone()),
                        &source,
                        self.ctx.cursor,
                        preference,
                    )
                    .into_iter()
                    .flatten(),
                );
            }
            BuiltinTy::Args => return None,
            BuiltinTy::Stroke => {
                self.snippet_completion("stroke()", "stroke(${})", "Stroke type.");
                self.snippet_completion("()", "(${})", "Stroke dictionary.");
                self.type_completion(&Ty::Builtin(BuiltinTy::Color), docs);
                self.type_completion(&Ty::Builtin(BuiltinTy::Length), docs);
            }
            BuiltinTy::Color => {
                self.snippet_completion("luma()", "luma(${v})", "A custom grayscale color.");
                self.snippet_completion(
                    "rgb()",
                    "rgb(${r}, ${g}, ${b}, ${a})",
                    "A custom RGBA color.",
                );
                self.snippet_completion(
                    "cmyk()",
                    "cmyk(${c}, ${m}, ${y}, ${k})",
                    "A custom CMYK color.",
                );
                self.snippet_completion(
                    "oklab()",
                    "oklab(${l}, ${a}, ${b}, ${alpha})",
                    "A custom Oklab color.",
                );
                self.snippet_completion(
                    "oklch()",
                    "oklch(${l}, ${chroma}, ${hue}, ${alpha})",
                    "A custom Oklch color.",
                );
                self.snippet_completion(
                    "color.linear-rgb()",
                    "color.linear-rgb(${r}, ${g}, ${b}, ${a})",
                    "A custom linear RGBA color.",
                );
                self.snippet_completion(
                    "color.hsv()",
                    "color.hsv(${h}, ${s}, ${v}, ${a})",
                    "A custom HSVA color.",
                );
                self.snippet_completion(
                    "color.hsl()",
                    "color.hsl(${h}, ${s}, ${l}, ${a})",
                    "A custom HSLA color.",
                );
            }
            BuiltinTy::TextSize => return None,
            BuiltinTy::TextLang => {
                for (&key, desc) in rust_iso639::ALL_MAP.entries() {
                    let detail = eco_format!("An ISO 639-1/2/3 language code, {}.", desc.name);
                    self.ctx.completions.push(Completion {
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
                    self.ctx.completions.push(Completion {
                        kind: CompletionKind::Syntax,
                        label: key.to_lowercase().into(),
                        apply: Some(eco_format!("\"{}\"", key.to_lowercase())),
                        detail: Some(detail),
                        label_detail: Some(desc.name.into()),
                        ..Completion::default()
                    });
                }
            }
            BuiltinTy::Dir => {}
            BuiltinTy::TextFont => {
                self.ctx.font_completions();
            }
            BuiltinTy::Margin => {
                self.snippet_completion("()", "(${})", "Margin dictionary.");
                self.type_completion(&Ty::Builtin(BuiltinTy::Length), docs);
            }
            BuiltinTy::Inset => {
                self.snippet_completion("()", "(${})", "Inset dictionary.");
                self.type_completion(&Ty::Builtin(BuiltinTy::Length), docs);
            }
            BuiltinTy::Outset => {
                self.snippet_completion("()", "(${})", "Outset dictionary.");
                self.type_completion(&Ty::Builtin(BuiltinTy::Length), docs);
            }
            BuiltinTy::Radius => {
                self.snippet_completion("()", "(${})", "Radius dictionary.");
                self.type_completion(&Ty::Builtin(BuiltinTy::Length), docs);
            }
            BuiltinTy::Length => {
                self.snippet_completion("pt", "${1}pt", "Point length unit.");
                self.snippet_completion("mm", "${1}mm", "Millimeter length unit.");
                self.snippet_completion("cm", "${1}cm", "Centimeter length unit.");
                self.snippet_completion("in", "${1}in", "Inch length unit.");
                self.snippet_completion("em", "${1}em", "Em length unit.");
                self.type_completion(&Ty::Builtin(BuiltinTy::Auto), docs);
            }
            BuiltinTy::Float => {
                self.snippet_completion(
                    "exponential notation",
                    "${1}e${0}",
                    "Exponential notation",
                );
            }
            BuiltinTy::Label => {
                self.ctx.label_completions(false);
            }
            BuiltinTy::CiteLabel => {
                self.ctx.label_completions(true);
            }
            BuiltinTy::RefLabel => {
                self.ctx.ref_completions();
            }
            BuiltinTy::TypeType(ty) | BuiltinTy::Type(ty) => {
                if *ty == Type::of::<NoneValue>() {
                    let docs = docs.or(Some("Nothing."));
                    self.type_completion(&Ty::Builtin(BuiltinTy::None), docs);
                } else if *ty == Type::of::<AutoValue>() {
                    let docs = docs.or(Some("A smart default."));
                    self.type_completion(&Ty::Builtin(BuiltinTy::Auto), docs);
                } else if *ty == Type::of::<bool>() {
                    self.snippet_completion("false", "false", "No / Disabled.");
                    self.snippet_completion("true", "true", "Yes / Enabled.");
                } else if *ty == Type::of::<Color>() {
                    self.type_completion(&Ty::Builtin(BuiltinTy::Color), docs);
                } else if *ty == Type::of::<Label>() {
                    self.ctx.label_completions(false)
                } else if *ty == Type::of::<Func>() {
                    self.snippet_completion(
                        "function",
                        "(${params}) => ${output}",
                        "A custom function.",
                    );
                } else {
                    self.ctx.completions.push(Completion {
                        kind: CompletionKind::Syntax,
                        label: ty.short_name().into(),
                        apply: Some(eco_format!("${{{ty}}}")),
                        detail: Some(eco_format!("A value of type {ty}.")),
                        ..Completion::default()
                    });
                }
            }
            BuiltinTy::Element(elem) => {
                self.ctx.value_completion(
                    Some(elem.name().into()),
                    &Value::Func((*elem).into()),
                    true,
                    docs,
                );
            }
        };

        Some(())
    }
}

// if ctx.before.ends_with(':') {
//     ctx.enrich(" ", "");
// }

/// Complete code by type or syntax.
pub(crate) fn complete_type_and_syntax(ctx: &mut CompletionContext) -> Option<()> {
    use crate::syntax::classify_context;
    use SurroundingSyntax::*;

    let syntax_context = classify_context(ctx.leaf.clone(), Some(ctx.cursor));
    let syntax = classify_syntax(ctx.leaf.clone(), ctx.cursor);
    crate::log_debug_ct!("complete_type: pos {:?} -> {syntax_context:#?}", ctx.leaf);
    let mut args_node = None;

    match syntax_context {
        Some(SyntaxContext::Element { container, .. }) => {
            if let Some(container) = container.cast::<ast::Dict>() {
                for named in container.items() {
                    if let ast::DictItem::Named(named) = named {
                        ctx.seen_field(named.name().into());
                    }
                }
            };
        }
        Some(SyntaxContext::Arg { args, .. }) => {
            let args = args.cast::<ast::Args>()?;
            for arg in args.items() {
                if let ast::Arg::Named(named) = arg {
                    ctx.seen_field(named.name().into());
                }
            }
            args_node = Some(args.to_untyped().clone());
        }
        // todo: complete field by types
        Some(SyntaxContext::VarAccess(
            var @ (VarClass::FieldAccess { .. } | VarClass::DotAccess { .. }),
        )) => {
            let target = var.accessed_node()?;
            let field = var.accessing_field()?;

            let offset = field.offset(&ctx.ctx.source_by_id(target.span().id()?).ok()?)?;
            ctx.from = offset;

            ctx.field_access_completions(&target);
            return Some(());
        }
        Some(SyntaxContext::ImportPath(path) | SyntaxContext::IncludePath(path)) => {
            let Some(ast::Expr::Str(str)) = path.cast() else {
                return None;
            };
            ctx.from = path.offset();
            let value = str.get();
            if value.starts_with('@') {
                let all_versions = value.contains(':');
                ctx.package_completions(all_versions);
                return Some(());
            } else {
                let source = ctx.ctx.source_by_id(ctx.root.span().id()?).ok()?;
                let paths = complete_path(
                    ctx.ctx,
                    Some(path),
                    &source,
                    ctx.cursor,
                    &crate::analysis::PathPreference::Source {
                        allow_package: true,
                    },
                );
                // todo: remove completions2
                ctx.completions2.extend(paths.unwrap_or_default());
            }

            return Some(());
        }
        Some(SyntaxContext::Normal(node))
            if (matches!(node.kind(), SyntaxKind::ContentBlock)
                && matches!(ctx.leaf.kind(), SyntaxKind::LeftBracket)) =>
        {
            args_node = node.parent().map(|s| s.get().clone());
        }
        Some(
            SyntaxContext::VarAccess(VarClass::Ident { .. })
            | SyntaxContext::Paren { .. }
            | SyntaxContext::Label { .. }
            | SyntaxContext::Normal(..),
        )
        | None => {}
    }

    crate::log_debug_ct!("ctx.leaf {:?}", ctx.leaf);

    let ty = ctx
        .ctx
        .post_type_of_node(ctx.leaf.clone())
        .filter(|ty| !matches!(ty, Ty::Any));

    let scope = ctx.surrounding_syntax();

    crate::log_debug_ct!("complete_type: {:?} -> ({scope:?}, {ty:#?})", ctx.leaf);
    if matches!((scope, &ty), (Regular | StringContent, None)) || matches!(scope, ImportList) {
        return None;
    }

    // adjust the completion position
    // todo: syntax class seems not being considering `is_ident_like`
    // todo: merge ident_content_offset and label_content_offset
    if is_ident_like(&ctx.leaf) {
        ctx.from = ctx.leaf.offset();
    } else if let Some(offset) = syntax.as_ref().and_then(SyntaxClass::complete_offset) {
        ctx.from = offset;
    }

    if let Some(ty) = ty {
        let filter = |ty: &Ty| match scope {
            SurroundingSyntax::StringContent => match ty {
                Ty::Builtin(BuiltinTy::Path(..) | BuiltinTy::TextFont) => true,
                Ty::Value(val) => matches!(val.val, Value::Str(..)),
                Ty::Builtin(BuiltinTy::Type(ty)) => *ty == Type::of::<typst::foundations::Str>(),
                _ => false,
            },
            _ => true,
        };
        let mut ctx = TypeCompletionContext {
            ctx,
            filter: &filter,
        };
        ctx.type_completion(&ty, None);
    }

    let mut completions = std::mem::take(&mut ctx.completions);
    let explicit = ctx.explicit;
    ctx.explicit = true;
    let ty = Some(Ty::from_types(ctx.seen_types.iter().cloned()));
    let from_ty = std::mem::replace(&mut ctx.from_ty, ty);
    complete_code(ctx, true);
    ctx.from_ty = from_ty;
    ctx.explicit = explicit;

    match scope {
        Regular | StringContent | ImportList | SetRule => {}
        Selector => {
            ctx.snippet_completion(
                "text selector",
                "\"${text}\"",
                "Replace occurrences of specific text.",
            );

            ctx.snippet_completion(
                "regex selector",
                "regex(\"${regex}\")",
                "Replace matches of a regular expression.",
            );
        }
        ShowTransform => {
            ctx.snippet_completion(
                "replacement",
                "[${content}]",
                "Replace the selected element with content.",
            );

            ctx.snippet_completion(
                "replacement (string)",
                "\"${text}\"",
                "Replace the selected element with a string of text.",
            );

            ctx.snippet_completion(
                "transformation",
                "element => [${content}]",
                "Transform the element with a function.",
            );
        }
    }

    // ctx.strict_scope_completions(false, |value| value.ty() == *ty);
    // let length_ty = Type::of::<Length>();
    // ctx.strict_scope_completions(false, |value| value.ty() == length_ty);
    // let color_ty = Type::of::<Color>();
    // ctx.strict_scope_completions(false, |value| value.ty() == color_ty);
    // let ty = Type::of::<Dir>();
    // ctx.strict_scope_completions(false, |value| value.ty() == ty);

    crate::log_debug_ct!(
        "sort_and_explicit_code_completion: {completions:#?} {:#?}",
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
    for (idx, compl) in ctx.completions2.iter_mut().enumerate() {
        compl.sort_text = Some(format!("{idx:03}"));
    }
    let sort_base = ctx.completions2.len();
    for (idx, compl) in (completions.iter_mut().chain(ctx.completions.iter_mut())).enumerate() {
        compl.sort_text = Some(eco_format!("{:03}", idx + sort_base));
    }

    crate::log_debug_ct!(
        "sort_and_explicit_code_completion after: {completions:#?} {:#?}",
        ctx.completions
    );

    ctx.completions.append(&mut completions);

    if let Some(node) = args_node {
        crate::log_debug_ct!("content block compl: args {node:?}");
        let is_unclosed = matches!(node.kind(), SyntaxKind::Args)
            && node.children().fold(0i32, |acc, node| match node.kind() {
                SyntaxKind::LeftParen => acc + 1,
                SyntaxKind::RightParen => acc - 1,
                SyntaxKind::Error if node.text() == "(" => acc + 1,
                SyntaxKind::Error if node.text() == ")" => acc - 1,
                _ => acc,
            }) > 0;
        if is_unclosed {
            ctx.enrich("", ")");
        }
    }

    if ctx.before.ends_with(',') || ctx.before.ends_with(':') {
        ctx.enrich(" ", "");
    }
    match scope {
        Regular | ImportList | ShowTransform | SetRule | StringContent => {}
        Selector => {
            ctx.enrich("", ": ${}");
        }
    }

    crate::log_debug_ct!("sort_and_explicit_code_completion: {:?}", ctx.completions);

    Some(())
}

fn complete_path(
    ctx: &LocalContext,
    node: Option<LinkedNode>,
    source: &Source,
    cursor: usize,
    preference: &PathPreference,
) -> Option<Vec<CompletionItem>> {
    let id = source.id();
    if id.package().is_some() {
        return None;
    }

    let is_in_text;
    let text;
    let rng;
    let node = node.filter(|v| v.kind() == SyntaxKind::Str);
    if let Some(str_node) = node {
        // todo: the non-str case
        str_node.cast::<ast::Str>()?;

        let vr = str_node.range();
        rng = vr.start + 1..vr.end - 1;
        crate::log_debug_ct!("path_of: {rng:?} {cursor}");
        if rng.start > rng.end || (cursor != rng.end && !rng.contains(&cursor)) {
            return None;
        }

        let mut w = EcoString::new();
        w.push('"');
        w.push_str(&source.text()[rng.start..cursor]);
        w.push('"');
        let partial_str = SyntaxNode::leaf(SyntaxKind::Str, w);
        crate::log_debug_ct!("path_of: {rng:?} {partial_str:?}");

        text = partial_str.cast::<ast::Str>()?.get();
        is_in_text = true;
    } else {
        text = EcoString::default();
        rng = cursor..cursor;
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
        log::warn!("absolute path completion is not supported for security consideration {path:?}");
        return None;
    }

    // find directory or files in the path
    let folder_completions = vec![];
    let mut module_completions = vec![];
    // todo: test it correctly
    for path in ctx.completion_files(preference) {
        crate::log_debug_ct!("compl_check_path: {path:?}");

        // Skip self smartly
        if *path == base {
            continue;
        }

        let label = if has_root {
            // diff with root
            unix_slash(path.vpath().as_rooted_path())
        } else {
            let base = base
                .vpath()
                .as_rooted_path()
                .parent()
                .unwrap_or(Path::new("/"));
            let path = path.vpath().as_rooted_path();
            let w = pathdiff::diff_paths(path, base)?;
            unix_slash(&w)
        };
        crate::log_debug_ct!("compl_label: {label:?}");

        module_completions.push((label, CompletionKind::File));

        // todo: looks like the folder completion is broken
        // if path.is_dir() {
        //     folder_completions.push((label, CompletionKind::Folder));
        // }
    }

    let replace_range = ctx.to_lsp_range(rng, source);

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

                crate::log_debug_ct!("compl_res: {res:?}");

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
