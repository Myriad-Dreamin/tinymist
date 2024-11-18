use std::collections::BTreeMap;

use ecow::{eco_format, EcoString};
use hashbrown::HashSet;
use lsp_types::{CompletionItem, CompletionTextEdit, InsertTextFormat, TextEdit};
use reflexo::path::unix_slash;
use tinymist_derive::BindTyCtx;
use tinymist_world::LspWorld;
use typst::foundations::{AutoValue, Func, Label, NoneValue, Scope, Type, Value};
use typst::syntax::ast::AstNode;
use typst::syntax::{ast, Span, SyntaxKind, SyntaxNode};
use typst::visualize::Color;

use super::{Completion, CompletionContext, CompletionKind};
use crate::adt::interner::Interned;
use crate::analysis::{BuiltinTy, PathPreference, Ty};
use crate::syntax::{descending_decls, is_ident_like, CheckTarget, DescentDecl};
use crate::ty::{Iface, IfaceChecker, InsTy, SigTy, TyCtx, TypeBounds, TypeScheme, TypeVar};
use crate::upstream::complete::complete_code;

use crate::{completion_kind, prelude::*, LspCompletion};

impl<'a> CompletionContext<'a> {
    pub fn world(&self) -> &LspWorld {
        self.ctx.world()
    }

    pub fn scope_completions(&mut self, parens: bool, filter: impl Fn(&Value) -> bool) {
        self.scope_completions_(parens, |v| v.map_or(true, &filter));
    }

    fn seen_field(&mut self, field: Interned<str>) -> bool {
        !self.seen_fields.insert(field)
    }

    /// Add completions for definitions that are available at the cursor.
    ///
    /// Filters the global/math scope with the given filter.
    pub fn scope_completions_(&mut self, parens: bool, filter: impl Fn(Option<&Value>) -> bool) {
        println!("scope completions {:?}", self.from_ty);

        let Some(fid) = self.root.span().id() else {
            return;
        };
        let Ok(src) = self.ctx.source_by_id(fid) else {
            return;
        };

        let mut defines = Defines {
            types: self.ctx.type_check(&src),
            defines: Default::default(),
        };

        descending_decls(self.leaf.clone(), |node| -> Option<()> {
            match node {
                DescentDecl::Ident(ident) => {
                    let ty = self.ctx.type_of_span(ident.span()).unwrap_or(Ty::Any);
                    defines.insert_ty(ty, ident.get());
                }
                DescentDecl::ImportSource(src) => {
                    println!("scope completions import source: {src:?}");
                    let ty = analyze_import_source(self.ctx, &defines.types, src)?;
                    let name = ty.name().as_ref().into();
                    defines.insert_ty(ty, &name);
                }
                // todo: cache completion items
                DescentDecl::ImportAll(mi) => {
                    let ty = analyze_import_source(self.ctx, &defines.types, mi.source())?;
                    ty.iface_surface(true, &mut ScopeChecker(&mut defines, self.ctx));
                }
            }
            None
        });

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

        enum SurroundingSyntax {
            Regular,
            Selector,
            SetRule,
        }

        let defines = defines.defines;

        let surrounding_syntax = check_surrounding_syntax(&self.leaf)
            .or_else(|| check_previous_syntax(&self.leaf))
            .unwrap_or(SurroundingSyntax::Regular);

        let mut kind_checker = CompletionKindChecker {
            symbols: HashSet::default(),
            functions: HashSet::default(),
        };

        // we don't check literal type here for faster completion
        for (name, ty) in defines {
            // todo: filter ty
            if !filter(None) || name.is_empty() {
                continue;
            }

            kind_checker.check(&ty);

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

            let label_detail = ty.describe().map(From::from).or_else(|| Some("any".into()));

            log::debug!("scope completions!: {name} {ty:?} {label_detail:?}");
            let detail = label_detail.clone();

            if !kind_checker.functions.is_empty() {
                let base = Completion {
                    kind: CompletionKind::Func,
                    label_detail,
                    command: self
                        .trigger_parameter_hints
                        .then_some("editor.action.triggerParameterHints"),
                    ..Default::default()
                };

                let fn_feat = FnCompletionFeat::default().check(kind_checker.functions.iter());

                log::debug!("fn_feat: {name} {ty:?} -> {fn_feat:?}");

                if !fn_feat.zero_args && matches!(surrounding_syntax, SurroundingSyntax::Regular) {
                    self.completions.push(Completion {
                        label: eco_format!("{}.with", name),
                        apply: Some(eco_format!("{}.with(${{}})", name)),
                        ..base.clone()
                    });
                }
                if fn_feat.is_element && !matches!(surrounding_syntax, SurroundingSyntax::SetRule) {
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
                    if !parens {
                        self.completions.push(Completion {
                            label: name,
                            ..base
                        });
                    } else if fn_feat.zero_args {
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

#[derive(BindTyCtx)]
#[bind(types)]
struct Defines {
    types: Arc<TypeScheme>,
    defines: BTreeMap<EcoString, Ty>,
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

fn analyze_import_source(ctx: &LocalContext, types: &TypeScheme, s: ast::Expr) -> Option<Ty> {
    if let Some(res) = types.type_of_span(s.span()) {
        if !matches!(res.value(), Some(Value::Str(..))) {
            let res = types.simplify(res, false);
            println!("analyze_import_source: {res:?}");
            return Some(res);
        }
    }

    let m = ctx.module_by_syntax(s.to_untyped())?;
    Some(Ty::Value(InsTy::new_at(m, s.span())))
}

#[derive(BindTyCtx)]
#[bind(0)]
struct ScopeChecker<'a>(&'a mut Defines, &'a mut LocalContext);

impl<'a> IfaceChecker for ScopeChecker<'a> {
    fn check(
        &mut self,
        sig: Iface,
        _args: &mut crate::ty::IfaceCheckContext,
        _pol: bool,
    ) -> Option<()> {
        match sig {
            // dict is not importable
            Iface::Dict(..) | Iface::Value { .. } => {}
            Iface::Element { val, .. } => {
                self.0.insert_scope(val.scope());
            }
            Iface::Type { val, .. } => {
                self.0.insert_scope(val.scope());
            }
            Iface::Module { val, .. } => {
                let ti = self.1.type_check_by_id(val);
                if !ti.valid {
                    self.0.insert_scope(self.1.module_by_id(val).ok()?.scope());
                } else {
                    for (name, ty) in ti.exports.iter() {
                        // todo: Interned -> EcoString here
                        let ty = ti.simplify(ty.clone(), false);
                        self.0.insert(name.as_ref().into(), ty);
                    }
                }
            }
            Iface::ModuleVal { val, .. } => {
                self.0.insert_scope(val.scope());
            }
        }
        None
    }
}

struct CompletionKindChecker {
    symbols: HashSet<char>,
    functions: HashSet<Ty>,
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
            Ty::Let(l) => {
                for ty in l.ubs.iter().chain(l.lbs.iter()) {
                    self.check(ty);
                }
            }
            Ty::Any | Ty::Builtin(..) => {}
            _ => panic!("check kind {ty:?}"),
        }
    }
}

#[derive(Default, Debug)]
struct FnCompletionFeat {
    zero_args: bool,
    is_element: bool,
}

impl FnCompletionFeat {
    fn check<'a>(mut self, fns: impl ExactSizeIterator<Item = &'a Ty>) -> Self {
        for ty in fns {
            self.check_one(ty, 0);
        }

        self
    }

    fn check_one(&mut self, ty: &Ty, pos: usize) {
        match ty {
            Ty::Value(val) => match &val.val {
                Value::Type(..) => {}
                Value::Func(sig) => {
                    if sig.element().is_some() {
                        self.is_element = true;
                    }
                    let ps = sig.params().into_iter().flatten();
                    let pos_size = ps.filter(|s| s.positional).count();
                    if pos_size <= pos {
                        self.zero_args = true;
                    }
                }
                _ => panic!("FnCompletionFeat check_one {val:?}"),
            },
            Ty::Func(sig) => self.check_sig(sig, pos),
            Ty::With(w) => {
                self.check_one(&w.sig, pos + w.with.positional_params().len());
            }
            Ty::Builtin(BuiltinTy::Element(..)) => {
                self.is_element = true;
            }
            Ty::Builtin(BuiltinTy::TypeType(..)) => {}
            _ => panic!("FnCompletionFeat check_one {ty:?}"),
        }
    }

    fn check_sig(&mut self, sig: &SigTy, pos: usize) {
        if pos >= sig.positional_params().len() {
            self.zero_args = true;
        }
    }
}

fn encolsed_by(parent: &LinkedNode, s: Option<Span>, leaf: &LinkedNode) -> bool {
    s.and_then(|s| parent.find(s)?.find(leaf.span())).is_some()
}

fn sort_and_explicit_code_completion(ctx: &mut CompletionContext) {
    let mut completions = std::mem::take(&mut ctx.completions);
    let explict = ctx.explicit;
    ctx.explicit = true;
    let ty = Some(Ty::from_types(ctx.seen_types.iter().cloned()));
    let from_ty = std::mem::replace(&mut ctx.from_ty, ty);
    complete_code(ctx, true);
    ctx.from_ty = from_ty;
    ctx.explicit = explict;

    // ctx.strict_scope_completions(false, |value| value.ty() == *ty);
    // let length_ty = Type::of::<Length>();
    // ctx.strict_scope_completions(false, |value| value.ty() == length_ty);
    // let color_ty = Type::of::<Color>();
    // ctx.strict_scope_completions(false, |value| value.ty() == color_ty);
    // let ty = Type::of::<Dir>();
    // ctx.strict_scope_completions(false, |value| value.ty() == ty);

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

pub fn ty_to_completion_kind(ty: &Ty) -> CompletionKind {
    match ty {
        Ty::Value(ty) => value_to_completion_kind(&ty.val),
        Ty::Func(..) | Ty::With(..) => CompletionKind::Func,
        Ty::Any => CompletionKind::Variable,
        Ty::Builtin(BuiltinTy::Module(..)) => CompletionKind::Module,
        Ty::Builtin(BuiltinTy::TypeType(..)) => CompletionKind::Type,
        Ty::Builtin(..) => CompletionKind::Variable,
        Ty::Let(l) => l
            .ubs
            .iter()
            .chain(l.lbs.iter())
            .fold(None, |acc, ty| match acc {
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
            .unwrap_or(CompletionKind::Variable),
        _ => panic!("ty_to_completion_kind {ty:?}"),
    }
}

pub fn value_to_completion_kind(value: &Value) -> CompletionKind {
    match value {
        Value::Func(..) => CompletionKind::Func,
        Value::Module(..) => CompletionKind::Module,
        Value::Type(..) => CompletionKind::Type,
        Value::Symbol(s) => CompletionKind::Symbol(s.get()),
        _ => CompletionKind::Variable,
    }
}

// if ctx.before.ends_with(',') {
//     ctx.enrich(" ", "");
// }

// if param.attrs.named {
//     let compl = Completion {
//         kind: CompletionKind::Field,
//         label: param.name.as_ref().into(),
//         apply: Some(eco_format!("{}: ${{}}", param.name)),
//         detail: docs(),
//         label_detail: None,
//         command: ctx
//             .trigger_named_completion
//             .then_some("tinymist.triggerNamedCompletion"),
//         ..Completion::default()
//     };
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

fn type_completion(
    ctx: &mut CompletionContext<'_>,
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
        Ty::Param(p) => {
            // todo: variadic

            if p.attrs.positional {
                type_completion(ctx, &p.ty, docs);
            }
            if !p.attrs.named {
                return Some(());
            }

            let f = &p.name;
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

            // todo: label details
            let docs = docs.or_else(|| p.docs.as_deref());
            ctx.completions.push(Completion {
                kind: CompletionKind::Field,
                label: f.into(),
                apply: Some(eco_format!("{}: ${{}}", f)),
                detail: docs.map(Into::into),
                command: ctx
                    .trigger_named_completion
                    .then_some("tinymist.triggerNamedCompletion"),
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
            BuiltinTy::Break => return None,
            BuiltinTy::Continue => return None,
            BuiltinTy::Content => return None,
            BuiltinTy::Infer => return None,
            BuiltinTy::FlowNone => return None,
            BuiltinTy::Tag(..) => return None,
            BuiltinTy::Module(..) => return None,

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
            BuiltinTy::Dir => {}
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
                type_completion(ctx, &Ty::Builtin(BuiltinTy::Auto), docs);
            }
            BuiltinTy::Float => {
                ctx.snippet_completion("exponential notation", "${1}e${0}", "Exponential notation");
            }
            BuiltinTy::Label => {
                ctx.label_completions(false);
            }
            BuiltinTy::CiteLabel => {
                ctx.label_completions(true);
            }
            BuiltinTy::RefLabel => {
                ctx.ref_completions();
            }
            BuiltinTy::TypeType(ty) | BuiltinTy::Type(ty) => {
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
                        label: ty.short_name().into(),
                        apply: Some(eco_format!("${{{ty}}}")),
                        detail: Some(eco_format!("A value of type {ty}.")),
                        ..Completion::default()
                    });
                }
            }
            BuiltinTy::Element(e) => {
                ctx.value_completion(Some(e.name().into()), &Value::Func((*e).into()), true, docs);
            }
        },
        Ty::Pattern(_) => return None,
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

// if ctx.before.ends_with(':') {
//     ctx.enrich(" ", "");
// }

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
        Some(CheckTarget::Normal(e))
            if matches!(
                e.kind(),
                SyntaxKind::Ident | SyntaxKind::Label | SyntaxKind::Ref | SyntaxKind::Str
            ) => {}
        Some(CheckTarget::Paren { .. }) => {}
        Some(CheckTarget::Normal(..)) => return None,
        None => return None,
    }

    log::debug!("ctx.leaf {:?}", ctx.leaf.clone());

    let ty = ctx
        .ctx
        .literal_type_of_node(ctx.leaf.clone())
        .filter(|ty| !matches!(ty, Ty::Any))?;

    // adjust the completion position
    if is_ident_like(&ctx.leaf) {
        ctx.from = ctx.leaf.offset();
    }

    log::debug!("complete_type: ty  {:?} -> {ty:#?}", ctx.leaf);

    type_completion(ctx, &ty, None);
    if ctx.before.ends_with(',') || ctx.before.ends_with(':') {
        ctx.enrich(" ", "");
    }

    sort_and_explicit_code_completion(ctx);
    Some(())
}

pub fn complete_path(
    ctx: &LocalContext,
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
    let base = id;
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

    // find directory or files in the path
    let folder_completions = vec![];
    let mut module_completions = vec![];
    // todo: test it correctly
    for path in ctx.completion_files(p) {
        log::debug!("compl_check_path: {path:?}");

        // Skip self smartly
        if *path == base {
            continue;
        }

        let label = if has_root {
            // diff with root
            unix_slash(path.vpath().as_rooted_path())
        } else {
            let base = base.vpath().as_rooted_path();
            let path = path.vpath().as_rooted_path();
            let w = pathdiff::diff_paths(path, base)?;
            unix_slash(&w)
        };
        log::debug!("compl_label: {label:?}");

        module_completions.push((label, CompletionKind::File));

        // todo: looks like the folder completion is broken
        // if path.is_dir() {
        //     folder_completions.push((label, CompletionKind::Folder));
        // }
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
