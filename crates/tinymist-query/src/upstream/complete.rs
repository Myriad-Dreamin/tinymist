use std::cmp::Reverse;
use std::collections::HashSet;
use std::ops::Range;

use ecow::{eco_format, EcoString};
use if_chain::if_chain;
use lsp_types::TextEdit;
use serde::{Deserialize, Serialize};
use typst::foundations::{fields_on, format_str, repr, Repr, StyleChain, Styles, Value};
use typst::model::Document;
use typst::syntax::ast::{AstNode, Param};
use typst::syntax::{ast, is_id_continue, is_id_start, is_ident, LinkedNode, Source, SyntaxKind};
use typst::text::RawElem;
use typst::World;
use typst_shim::{syntax::LinkedNodeExt, utils::hash128};
use unscanny::Scanner;

use super::{plain_docs_sentence, summarize_font_family};
use crate::adt::interner::Interned;
use crate::analysis::{analyze_labels, DynLabel, LocalContext, Ty};
use crate::snippet::{
    CompletionCommand, CompletionContextKey, PrefixSnippet, SurroundingSyntax,
    DEFAULT_PREFIX_SNIPPET,
};
use crate::syntax::InterpretMode;

mod ext;
pub use ext::CompletionFeat;
use ext::*;

/// Autocomplete a cursor position in a source file.
///
/// Returns the position from which the completions apply and a list of
/// completions.
///
/// When `explicit` is `true`, the user requested the completion by pressing
/// control and space or something similar.
///
/// Passing a `document` (from a previous compilation) is optional, but enhances
/// the autocompletions. Label completions, for instance, are only generated
/// when the document is available.
pub fn autocomplete(
    mut ctx: CompletionContext,
) -> Option<(usize, bool, Vec<Completion>, Vec<lsp_types::CompletionItem>)> {
    let _ = complete_comments(&mut ctx)
        || complete_type_and_syntax(&mut ctx).is_none() && {
            crate::log_debug_ct!("continue after completing type and syntax");
            complete_imports(&mut ctx)
                || complete_field_accesses(&mut ctx)
                || complete_markup(&mut ctx)
                || complete_math(&mut ctx)
                || complete_code(&mut ctx, false)
        };

    Some((ctx.from, ctx.incomplete, ctx.completions, ctx.completions2))
}

/// An autocompletion option.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Completion {
    /// The kind of item this completes to.
    pub kind: CompletionKind,
    /// The label the completion is shown with.
    pub label: EcoString,
    /// The label the completion is shown with.
    pub label_detail: Option<EcoString>,
    /// The label the completion is shown with.
    pub sort_text: Option<EcoString>,
    /// The composed text used for filtering.
    pub filter_text: Option<EcoString>,
    /// The character that should be committed when selecting this completion.
    pub commit_char: Option<char>,
    /// The completed version of the input, possibly described with snippet
    /// syntax like `${lhs} + ${rhs}`.
    ///
    /// Should default to the `label` if `None`.
    pub apply: Option<EcoString>,
    /// An optional short description, at most one sentence.
    pub detail: Option<EcoString>,
    /// An optional array of additional text edits that are applied when
    /// selecting this completion. Edits must not overlap with the main edit
    /// nor with themselves.
    pub additional_text_edits: Option<Vec<TextEdit>>,
    /// An optional command to run when the completion is selected.
    pub command: Option<&'static str>,
}

/// A kind of item that can be completed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CompletionKind {
    /// A syntactical structure.
    Syntax,
    /// A function.
    Func,
    /// A type.
    Type,
    /// A function parameter.
    Param,
    /// A field.
    Field,
    /// A constant.
    #[default]
    Constant,
    /// A reference.
    Reference,
    /// A symbol.
    Symbol(char),
    /// A variable.
    Variable,
    /// A module.
    Module,
    /// A file.
    File,
    /// A folder.
    Folder,
}

/// Complete in comments. Or rather, don't!
fn complete_comments(ctx: &mut CompletionContext) -> bool {
    if !matches!(
        ctx.leaf.kind(),
        SyntaxKind::LineComment | SyntaxKind::BlockComment
    ) {
        return false;
    }

    // check if next line defines a function
    if_chain! {
        if let Some(next) = ctx.leaf.next_leaf();
        if let Some(next_next) = next.next_leaf();
        if let Some(next_next) = next_next.next_leaf();
        if matches!(next_next.parent_kind(), Some(SyntaxKind::Closure));
        if let Some(closure) = next_next.parent();
        if let Some(closure) = closure.cast::<ast::Expr>();
        if let ast::Expr::Closure(c) = closure;
        if let Some(id) = ctx.root.span().id();
        if let Some(src) = ctx.ctx.source_by_id(id).ok();
        then {
            let mut doc_snippet = "/// $0\n///".to_string();
            let mut i = 0;
            for param in c.params().children() {
                // TODO: Properly handle Pos and Spread argument
                let param: &EcoString = match param {
                    Param::Pos(p) => {
                        match p {
                            ast::Pattern::Normal(ast::Expr::Ident(ident)) => ident.get(),
                            _ => &"_".into()
                        }
                    }
                    Param::Named(n) => n.name().get(),
                    Param::Spread(s) => {
                        if let Some(ident) = s.sink_ident() {
                            &eco_format!("{}", ident.get())
                        } else {
                            &EcoString::new()
                        }
                    }
                };
                log::info!("param: {param}, index: {i}");
                doc_snippet += &format!("\n/// - {param} (${}): ${}", i + 1, i + 2);
                i += 2;
            }
            doc_snippet += &format!("\n/// -> ${}", i + 1);
            let before = TextEdit {
                range: ctx.ctx.to_lsp_range(ctx.leaf.range().start..ctx.from, &src),
                new_text: String::new(),
            };
            ctx.completions.push(Completion {
                label: "Document function".into(),
                label_detail: Some("Tidy Document Comment".into()),
                apply: Some(doc_snippet.into()),
                additional_text_edits: Some(vec![before]),
                ..Completion::default()
            });
        }
    };

    true
}

/// Complete in markup mode.
fn complete_markup(ctx: &mut CompletionContext) -> bool {
    // Bail if we aren't even in markup.
    if !matches!(
        ctx.leaf.parent_kind(),
        None | Some(SyntaxKind::Markup) | Some(SyntaxKind::Ref)
    ) {
        return false;
    }

    // Start of an interpolated identifier: "#|".
    if ctx.leaf.kind() == SyntaxKind::Hash {
        ctx.from = ctx.cursor;
        code_completions(ctx, true);
        return true;
    }

    // An existing identifier: "#pa|".
    if ctx.leaf.kind() == SyntaxKind::Ident {
        ctx.from = ctx.leaf.offset();
        code_completions(ctx, true);
        return true;
    }

    // Start of a reference: "@|" or "@he|".
    if ctx.leaf.kind() == SyntaxKind::RefMarker {
        ctx.from = ctx.leaf.offset() + 1;
        ctx.ref_completions();
        return true;
    }

    // Behind a half-completed binding: "#let x = |".
    if_chain! {
        if let Some(prev) = ctx.leaf.prev_leaf();
        if prev.kind() == SyntaxKind::Eq;
        if prev.parent_kind() == Some(SyntaxKind::LetBinding);
        then {
            ctx.from = ctx.cursor;
            code_completions(ctx, false);
            return true;
        }
    }

    // Behind a half-completed context block: "#context |".
    if_chain! {
        if let Some(prev) = ctx.leaf.prev_leaf();
        if prev.kind() == SyntaxKind::Context;
        then {
            ctx.from = ctx.cursor;
            code_completions(ctx, false);
            return true;
        }
    }

    // Directly after a raw block.
    let mut s = Scanner::new(ctx.text);
    s.jump(ctx.leaf.offset());
    if s.eat_if("```") {
        s.eat_while('`');
        let start = s.cursor();
        if s.eat_if(is_id_start) {
            s.eat_while(is_id_continue);
        }
        if s.cursor() == ctx.cursor {
            ctx.from = start;
            ctx.raw_completions();
        }
        return true;
    }

    // Anywhere: "|".
    if !is_triggered_by_punc(ctx.trigger_character) && ctx.explicit {
        ctx.from = ctx.cursor;
        ctx.snippet_completions(Some(InterpretMode::Markup), None);
        return true;
    }

    false
}

/// Complete in math mode.
fn complete_math(ctx: &mut CompletionContext) -> bool {
    if !matches!(
        ctx.leaf.parent_kind(),
        Some(SyntaxKind::Equation)
            | Some(SyntaxKind::Math)
            | Some(SyntaxKind::MathFrac)
            | Some(SyntaxKind::MathAttach)
    ) {
        return false;
    }

    // Start of an interpolated identifier: "#|".
    if ctx.leaf.kind() == SyntaxKind::Hash {
        ctx.from = ctx.cursor;
        code_completions(ctx, true);
        return true;
    }

    // Behind existing atom or identifier: "$a|$" or "$abc|$".
    if !is_triggered_by_punc(ctx.trigger_character)
        && matches!(ctx.leaf.kind(), SyntaxKind::Text | SyntaxKind::MathIdent)
    {
        ctx.from = ctx.leaf.offset();
        ctx.scope_completions(true);
        ctx.snippet_completions(Some(InterpretMode::Math), None);
        return true;
    }

    // Anywhere: "$|$".
    if !is_triggered_by_punc(ctx.trigger_character) && ctx.explicit {
        ctx.from = ctx.cursor;
        ctx.scope_completions(true);
        ctx.snippet_completions(Some(InterpretMode::Math), None);
        return true;
    }

    false
}

/// Complete field accesses.
fn complete_field_accesses(ctx: &mut CompletionContext) -> bool {
    // Used to determine whether trivia nodes are allowed before '.'.
    // During an inline expression in markup mode trivia nodes exit the inline
    // expression.
    let in_markup: bool = matches!(
        ctx.leaf.parent_kind(),
        None | Some(SyntaxKind::Markup) | Some(SyntaxKind::Ref)
    );

    // Behind an expression plus dot: "emoji.|".
    if_chain! {
        if ctx.leaf.kind() == SyntaxKind::Dot
            || (ctx.leaf.kind() == SyntaxKind::Text
                && ctx.leaf.text() == ".");
        if ctx.leaf.range().end == ctx.cursor;
        if let Some(prev) = ctx.leaf.prev_sibling();
        if !in_markup || prev.range().end == ctx.leaf.range().start;
        if prev.is::<ast::Expr>();
        if prev.parent_kind() != Some(SyntaxKind::Markup) ||
           prev.prev_sibling_kind() == Some(SyntaxKind::Hash);
        if let Some((value, styles)) = ctx.ctx.analyze_expr(&prev).into_iter().next();
        then {
            ctx.from = ctx.cursor;
            field_access_completions(ctx, &prev, &value, &styles);
            return true;
        }
    }

    // Behind a started field access: "emoji.fa|".
    if_chain! {
        if ctx.leaf.kind() == SyntaxKind::Ident;
        if let Some(prev) = ctx.leaf.prev_sibling();
        if prev.kind() == SyntaxKind::Dot;
        if let Some(prev_prev) = prev.prev_sibling();
        if prev_prev.is::<ast::Expr>();
        if let Some((value, styles)) = ctx.ctx.analyze_expr(&prev_prev).into_iter().next();
        then {
            ctx.from = ctx.leaf.offset();
            field_access_completions(ctx,&prev_prev, &value, &styles);
            return true;
        }
    }

    false
}

/// Add completions for all fields on a value.
fn field_access_completions(
    ctx: &mut CompletionContext,
    node: &LinkedNode,
    value: &Value,
    styles: &Option<Styles>,
) {
    for (name, value, _) in value.ty().scope().iter() {
        ctx.value_completion(Some(name.clone()), value, true, None);
    }

    if let Some(scope) = value.scope() {
        for (name, value, _) in scope.iter() {
            ctx.value_completion(Some(name.clone()), value, true, None);
        }
    }

    for &field in fields_on(value.ty()) {
        // Complete the field name along with its value. Notes:
        // 1. No parentheses since function fields cannot currently be called
        // with method syntax;
        // 2. We can unwrap the field's value since it's a field belonging to
        // this value's type, so accessing it should not fail.
        ctx.value_completion(
            Some(field.into()),
            &value.field(field).unwrap(),
            false,
            None,
        );
    }

    ctx.postfix_completions(node, value);

    match value {
        Value::Symbol(symbol) => {
            for modifier in symbol.modifiers() {
                if let Ok(modified) = symbol.clone().modified(modifier) {
                    ctx.completions.push(Completion {
                        kind: CompletionKind::Symbol(modified.get()),
                        label: modifier.into(),
                        label_detail: Some(symbol_label_detail(modified.get())),
                        ..Completion::default()
                    });
                }
            }

            ctx.ufcs_completions(node, value);
        }
        Value::Content(content) => {
            for (name, value) in content.fields() {
                ctx.value_completion(Some(name.into()), &value, false, None);
            }

            ctx.ufcs_completions(node, value);
        }
        Value::Dict(dict) => {
            for (name, value) in dict.iter() {
                ctx.value_completion(Some(name.clone().into()), value, false, None);
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
                        ctx.value_completion(Some(param.name.into()), &value.unwrap(), false, None);
                    }
                }
            }
        }
        Value::Plugin(plugin) => {
            for name in plugin.iter() {
                ctx.completions.push(Completion {
                    kind: CompletionKind::Func,
                    label: name.clone(),
                    ..Completion::default()
                })
            }
        }
        _ => {}
    }
}

/// Complete imports.
fn complete_imports(ctx: &mut CompletionContext) -> bool {
    // On the colon marker of an import list:
    // "#import "path.typ":|"
    if_chain! {
        if matches!(ctx.leaf.kind(), SyntaxKind::Colon);
        if let Some(parent) = ctx.leaf.clone().parent();
        if let Some(ast::Expr::Import(import)) = parent.get().cast();
        if !matches!(import.imports(), Some(ast::Imports::Wildcard));
        if let Some(source) = parent.children().find(|child| child.is::<ast::Expr>());
        then {
            let items = match import.imports() {
                Some(ast::Imports::Items(items)) => items,
                _ => Default::default(),
            };

            ctx.from = ctx.cursor;

            import_item_completions(ctx, items, vec![], &source);
            if items.iter().next().is_some() {
                ctx.enrich("", ", ");
            }
            return true;
        }
    }

    // Behind an import list:
    // "#import "path.typ": |",
    // "#import "path.typ": a, b, |".
    if_chain! {
        if let Some(prev) = ctx.leaf.prev_sibling();
        if let Some(ast::Expr::Import(import)) = prev.get().cast();
        if !ctx.text[prev.offset()..ctx.cursor].contains('\n');
        if let Some(ast::Imports::Items(items)) = import.imports();
        if let Some(source) = prev.children().find(|child| child.is::<ast::Expr>());
        then {
            ctx.from = ctx.cursor;
            import_item_completions(ctx, items, vec![], &source);
            return true;
        }
    }

    // Behind a comma in an import list:
    // "#import "path.typ": this,|".
    if_chain! {
        if matches!(ctx.leaf.kind(), SyntaxKind::Comma);
        if let Some(parent) = ctx.leaf.clone().parent();
        if parent.kind() == SyntaxKind::ImportItems;
        if let Some(grand) = parent.parent();
        if let Some(ast::Expr::Import(import)) = grand.get().cast();
        if let Some(ast::Imports::Items(items)) = import.imports();
        if let Some(source) = grand.children().find(|child| child.is::<ast::Expr>());
        then {
            import_item_completions(ctx, items, vec![], &source);
            ctx.enrich(" ", "");
            return true;
        }
    }

    // Behind a half-started identifier in an import list:
    // "#import "path.typ": th|".
    if_chain! {
        if matches!(ctx.leaf.kind(), SyntaxKind::Ident | SyntaxKind::Dot);
        if let Some(path_ctx) = ctx.leaf.clone().parent();
        if path_ctx.kind() == SyntaxKind::ImportItemPath;
        if let Some(parent) = path_ctx.parent();
        if parent.kind() == SyntaxKind::ImportItems;
        if let Some(grand) = parent.parent();
        if let Some(ast::Expr::Import(import)) = grand.get().cast();
        if let Some(ast::Imports::Items(items)) = import.imports();
        if let Some(source) = grand.children().find(|child| child.is::<ast::Expr>());
        then {
            if ctx.leaf.kind() == SyntaxKind::Ident {
                ctx.from = ctx.leaf.offset();
            }
            let path = path_ctx.cast::<ast::ImportItemPath>().map(|path| path.iter().take_while(|ident| ident.span() != ctx.leaf.span()).collect());
            import_item_completions(ctx, items, path.unwrap_or_default(), &source);
            return true;
        }
    }

    false
}

/// Add completions for all exports of a module.
fn import_item_completions<'a>(
    ctx: &mut CompletionContext<'a>,
    existing: ast::ImportItems<'a>,
    comps: Vec<ast::Ident>,
    source: &LinkedNode,
) {
    // Select the source by `comps`
    let value = ctx.ctx.module_by_syntax(source);
    let value = comps
        .iter()
        .fold(value.as_ref(), |value, comp| value?.scope()?.get(comp));
    let Some(scope) = value.and_then(|v| v.scope()) else {
        return;
    };

    // Check imported items in the scope
    let seen = existing
        .iter()
        .flat_map(|item| {
            let item_comps = item.path().iter().collect::<Vec<_>>();
            if item_comps.len() == comps.len() + 1
                && item_comps
                    .iter()
                    .zip(comps.as_slice())
                    .all(|(l, r)| l.as_str() == r.as_str())
            {
                // item_comps.len() >= 1
                item_comps.last().cloned()
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    if existing.iter().next().is_none() {
        ctx.snippet_completion("*", "*", "Import everything.");
    }

    for (name, value, _) in scope.iter() {
        if seen.iter().all(|item| item.as_str() != name) {
            ctx.value_completion(Some(name.clone()), value, false, None);
        }
    }
}

/// Complete in code mode.
fn complete_code(ctx: &mut CompletionContext, from_type: bool) -> bool {
    let surrounding_syntax = ctx.surrounding_syntax();

    if matches!(
        (ctx.leaf.parent_kind(), surrounding_syntax),
        (
            None | Some(SyntaxKind::Markup)
                | Some(SyntaxKind::Math)
                | Some(SyntaxKind::MathFrac)
                | Some(SyntaxKind::MathAttach)
                | Some(SyntaxKind::MathRoot),
            SurroundingSyntax::Regular
        )
    ) {
        return false;
    }

    // An existing identifier: "{ pa| }".
    if ctx.leaf.kind() == SyntaxKind::Ident
        && !matches!(ctx.leaf.parent_kind(), Some(SyntaxKind::FieldAccess))
    {
        ctx.from = ctx.leaf.offset();
        code_completions(ctx, false);
        return true;
    }

    // A potential label (only at the start of an argument list): "(<|".
    if !from_type && ctx.before.ends_with("(<") {
        ctx.from = ctx.cursor;
        ctx.label_completions(false);
        return true;
    }

    // Anywhere: "{ | }".
    // But not within or after an expression.
    if ctx.explicit
        && (ctx.leaf.kind().is_trivia()
            || (matches!(
                ctx.leaf.kind(),
                SyntaxKind::LeftParen | SyntaxKind::LeftBrace
            ) || (matches!(ctx.leaf.kind(), SyntaxKind::Colon)
                && ctx.leaf.parent_kind() == Some(SyntaxKind::ShowRule))))
    {
        ctx.from = ctx.cursor;
        code_completions(ctx, false);
        return true;
    }

    false
}

/// Add completions for expression snippets.
#[rustfmt::skip]
fn code_completions(ctx: &mut CompletionContext, hash: bool) {
    // todo: filter code completions
    // matches!(value, Value::Symbol(_) | Value::Func(_) | Value::Type(_) | Value::Module(_))
    ctx.scope_completions(true);

    ctx.snippet_completions(Some(InterpretMode::Code), None);

    if !hash {
        ctx.snippet_completion(
            "function",
            "(${params}) => ${output}",
            "Creates an unnamed function.",
        );
    }
}

/// Context for autocompletion.
pub struct CompletionContext<'a> {
    pub ctx: &'a mut LocalContext,
    pub document: Option<&'a Document>,
    pub text: &'a str,
    pub before: &'a str,
    pub after: &'a str,
    pub root: LinkedNode<'a>,
    pub leaf: LinkedNode<'a>,
    pub cursor: usize,
    pub explicit: bool,
    pub trigger_character: Option<char>,
    pub from: usize,
    pub from_ty: Option<Ty>,
    pub completions: Vec<Completion>,
    pub completions2: Vec<lsp_types::CompletionItem>,
    pub incomplete: bool,
    pub seen_casts: HashSet<u128>,
    pub seen_types: HashSet<Ty>,
    pub seen_fields: HashSet<Interned<str>>,
}

impl<'a> CompletionContext<'a> {
    /// Create a new autocompletion context.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ctx: &'a mut LocalContext,
        document: Option<&'a Document>,
        source: &'a Source,
        cursor: usize,
        explicit: bool,
        trigger_character: Option<char>,
    ) -> Option<Self> {
        let text = source.text();
        let root = LinkedNode::new(source.root());
        let leaf = root.leaf_at_compat(cursor)?;
        Some(Self {
            ctx,
            document,
            text,
            before: &text[..cursor],
            after: &text[cursor..],
            root,
            leaf,
            cursor,
            trigger_character,
            explicit,
            from: cursor,
            from_ty: None,
            incomplete: true,
            completions: vec![],
            completions2: vec![],
            seen_casts: HashSet::new(),
            seen_types: HashSet::new(),
            seen_fields: HashSet::new(),
        })
    }

    /// A small window of context before the cursor.
    fn before_window(&self, size: usize) -> &str {
        slice_at(
            self.before,
            self.cursor.saturating_sub(size)..self.before.len(),
        )
    }

    /// Add a prefix and suffix to all applications.
    fn enrich(&mut self, prefix: &str, suffix: &str) {
        for Completion { label, apply, .. } in &mut self.completions {
            let current = apply.as_ref().unwrap_or(label);
            *apply = Some(eco_format!("{prefix}{current}{suffix}"));
        }
    }

    fn snippet_completions(
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

            let analysis = &self.ctx.analysis;
            let command = match snippet.command {
                Some(CompletionCommand::TriggerSuggest) => analysis.trigger_suggest(true),
                None => analysis.trigger_on_snippet(snippet.snippet.contains("${")),
            };

            self.completions.push(Completion {
                kind: CompletionKind::Syntax,
                label: snippet.label.as_ref().into(),
                apply: Some(snippet.snippet.as_ref().into()),
                detail: Some(snippet.description.as_ref().into()),
                command,
                ..Completion::default()
            });
        }
    }

    /// Add a snippet completion.
    fn snippet_completion(&mut self, label: &str, snippet: &str, docs: &str) {
        self.completions.push(Completion {
            kind: CompletionKind::Syntax,
            label: label.into(),
            apply: Some(snippet.into()),
            detail: Some(docs.into()),
            command: self.ctx.analysis.trigger_on_snippet(snippet.contains("${")),
            ..Completion::default()
        });
    }

    /// Add completions for all font families.
    fn font_completions(&mut self) {
        let equation = self.before_window(25).contains("equation");
        for (family, iter) in self.world().clone().book().families() {
            let detail = summarize_font_family(iter);
            if !equation || family.contains("Math") {
                self.value_completion(
                    None,
                    &Value::Str(family.into()),
                    false,
                    Some(detail.as_str()),
                );
            }
        }
    }

    /// Add completions for all available packages.
    fn package_completions(&mut self, all_versions: bool) {
        let w = self.world().clone();
        let mut packages: Vec<_> = w
            .packages()
            .iter()
            .map(|(spec, desc)| (spec, desc.clone()))
            .collect();
        // local_packages to references and add them to the packages
        let local_packages_refs = self.ctx.local_packages();
        packages.extend(
            local_packages_refs
                .iter()
                .map(|spec| (spec, Some(eco_format!("{} v{}", spec.name, spec.version)))),
        );

        packages.sort_by_key(|(spec, _)| (&spec.namespace, &spec.name, Reverse(spec.version)));
        if !all_versions {
            packages.dedup_by_key(|(spec, _)| (&spec.namespace, &spec.name));
        }
        for (package, description) in packages {
            self.value_completion(
                None,
                &Value::Str(format_str!("{package}")),
                false,
                description.as_deref(),
            );
        }
    }

    /// Add completions for raw block tags.
    fn raw_completions(&mut self) {
        for (name, mut tags) in RawElem::languages() {
            let lower = name.to_lowercase();
            if !tags.contains(&lower.as_str()) {
                tags.push(lower.as_str());
            }

            tags.retain(|tag| is_ident(tag));
            if tags.is_empty() {
                continue;
            }

            self.completions.push(Completion {
                kind: CompletionKind::Constant,
                label: name.into(),
                apply: Some(tags[0].into()),
                detail: Some(repr::separated_list(&tags, " or ").into()),
                ..Completion::default()
            });
        }
    }

    /// Add completions for labels and references.
    fn ref_completions(&mut self) {
        self.label_completions_(false, true);
    }

    /// Add completions for labels and references.
    fn label_completions(&mut self, only_citation: bool) {
        self.label_completions_(only_citation, false);
    }

    /// Add completions for labels and references.
    fn label_completions_(&mut self, only_citation: bool, ref_label: bool) {
        let Some(document) = self.document else {
            return;
        };
        let (labels, split) = analyze_labels(document);

        let head = &self.text[..self.from];
        let at = head.ends_with('@');
        let open = !at && !head.ends_with('<');
        let close = !at && !self.after.starts_with('>');
        let citation = !at && only_citation;

        let (skip, take) = if at || ref_label {
            (0, usize::MAX)
        } else if citation {
            (split, usize::MAX)
        } else {
            (0, split)
        };

        for DynLabel {
            label,
            label_desc,
            detail,
            bib_title,
        } in labels.into_iter().skip(skip).take(take)
        {
            if !self.seen_casts.insert(hash128(&label)) {
                continue;
            }
            let label: EcoString = label.as_str().into();
            let completion = Completion {
                kind: CompletionKind::Reference,
                apply: Some(eco_format!(
                    "{}{}{}",
                    if open { "<" } else { "" },
                    label.as_str(),
                    if close { ">" } else { "" }
                )),
                label: label.clone(),
                label_detail: label_desc.clone(),
                filter_text: Some(label.clone()),
                detail: detail.clone(),
                ..Completion::default()
            };

            if let Some(bib_title) = bib_title {
                // Note that this completion re-uses the above `apply` field to
                // alter the `bib_title` to the corresponding label.
                self.completions.push(Completion {
                    kind: CompletionKind::Constant,
                    label: bib_title.clone(),
                    label_detail: Some(label),
                    filter_text: Some(bib_title),
                    detail,
                    ..completion.clone()
                });
            }

            self.completions.push(completion);
        }
    }

    /// Add a completion for a specific value.
    fn value_completion(
        &mut self,
        label: Option<EcoString>,
        value: &Value,
        parens: bool,
        docs: Option<&str>,
    ) {
        self.value_completion_(
            label,
            value,
            parens,
            match value {
                Value::Symbol(s) => Some(symbol_label_detail(s.get())),
                _ => None,
            },
            docs,
        );
    }

    /// Add a completion for a specific value.
    fn value_completion_(
        &mut self,
        label: Option<EcoString>,
        value: &Value,
        parens: bool,
        label_detail: Option<EcoString>,
        docs: Option<&str>,
    ) {
        // Prevent duplicate completions from appearing.
        if !self.seen_casts.insert(hash128(&(&label, &value))) {
            return;
        }

        let at = label.as_deref().is_some_and(|field| !is_ident(field));
        let label = label.unwrap_or_else(|| value.repr());

        let detail = docs.map(Into::into).or_else(|| match value {
            Value::Symbol(symbol) => Some(symbol_detail(symbol.get())),
            Value::Func(func) => func.docs().map(plain_docs_sentence),
            Value::Type(ty) => Some(plain_docs_sentence(ty.docs())),
            v => {
                let repr = v.repr();
                (repr.as_str() != label).then_some(repr)
            }
        });

        let mut apply = None;
        let mut command = None;
        if parens && matches!(value, Value::Func(_)) {
            if let Value::Func(func) = value {
                command = self.ctx.analysis.trigger_parameter_hints(true);
                if func
                    .params()
                    .is_some_and(|params| params.iter().all(|param| param.name == "self"))
                {
                    apply = Some(eco_format!("{label}()${{}}"));
                } else {
                    apply = Some(eco_format!("{label}(${{}})"));
                }
            }
        } else if at {
            apply = Some(eco_format!("at(\"{label}\")"));
        } else {
            let apply_label = &mut label.as_str();
            if apply_label.ends_with('"') && self.after.starts_with('"') {
                if let Some(trimmed) = apply_label.strip_suffix('"') {
                    *apply_label = trimmed;
                }
            }
            let from_before = slice_at(self.text, 0..self.from);
            if apply_label.starts_with('"') && from_before.ends_with('"') {
                if let Some(trimmed) = apply_label.strip_prefix('"') {
                    *apply_label = trimmed;
                }
            }

            if apply_label.len() != label.len() {
                apply = Some((*apply_label).into());
            }
        }

        self.completions.push(Completion {
            kind: value_to_completion_kind(value),
            label,
            apply,
            detail,
            label_detail,
            command,
            ..Completion::default()
        });
    }
}

/// Slices a smaller string at character boundaries safely.
fn slice_at(s: &str, mut rng: Range<usize>) -> &str {
    while !rng.is_empty() && !s.is_char_boundary(rng.start) {
        rng.start += 1;
    }
    while !rng.is_empty() && !s.is_char_boundary(rng.end) {
        rng.end -= 1;
    }

    if rng.is_empty() {
        return "";
    }

    &s[rng]
}

fn is_triggered_by_punc(trigger_character: Option<char>) -> bool {
    trigger_character.is_some_and(|ch| ch.is_ascii_punctuation())
}
