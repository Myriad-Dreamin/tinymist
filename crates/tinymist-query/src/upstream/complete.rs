use std::cmp::Reverse;
use std::collections::HashSet;
use std::ops::Range;

use ecow::{eco_format, EcoString};
use if_chain::if_chain;
use lsp_types::TextEdit;
use serde::{Deserialize, Serialize};
use typst::foundations::{fields_on, format_str, repr, Repr, StyleChain, Styles, Value};
use typst::model::Document;
use typst::syntax::{ast, is_id_continue, is_id_start, is_ident, LinkedNode, Source, SyntaxKind};
use typst::text::RawElem;
use typst::World;
use typst_shim::{syntax::LinkedNodeExt, utils::hash128};
use unscanny::Scanner;

use super::{plain_docs_sentence, summarize_font_family};
use crate::adt::interner::Interned;
use crate::analysis::{analyze_labels, DynLabel, LocalContext, Ty};

mod ext;
use ext::*;
pub use ext::{complete_path, CompletionFeat, PostfixSnippet};

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
        || complete_type(&mut ctx).is_none() && {
            log::debug!("continue after completing type");
            complete_labels(&mut ctx)
                || complete_field_accesses(&mut ctx)
                || complete_imports(&mut ctx)
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
    matches!(
        ctx.leaf.kind(),
        SyntaxKind::LineComment | SyntaxKind::BlockComment
    )
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
    if !is_triggerred_by_punc(ctx.trigger_character) && ctx.explicit {
        ctx.from = ctx.cursor;
        markup_completions(ctx);
        return true;
    }

    false
}

/// Add completions for markup snippets.
#[rustfmt::skip]
fn markup_completions(ctx: &mut CompletionContext) {
    ctx.snippet_completion(
        "expression",
        "#${}",
        "Variables, function calls, blocks, and more.",
    );

    ctx.snippet_completion(
        "linebreak",
        "\\\n${}",
        "Inserts a forced linebreak.",
    );

    ctx.snippet_completion(
        "strong text",
        "*${strong}*",
        "Strongly emphasizes content by increasing the font weight.",
    );

    ctx.snippet_completion(
        "emphasized text",
        "_${emphasized}_",
        "Emphasizes content by setting it in italic font style.",
    );

    ctx.snippet_completion(
        "raw text",
        "`${text}`",
        "Displays text verbatim, in monospace.",
    );

    ctx.snippet_completion(
        "code listing",
        "```${lang}\n${code}\n```",
        "Inserts computer code with syntax highlighting.",
    );

    ctx.snippet_completion(
        "hyperlink",
        "https://${example.com}",
        "Links to a URL.",
    );

    ctx.snippet_completion(
        "label",
        "<${name}>",
        "Makes the preceding element referenceable.",
    );

    ctx.snippet_completion(
        "reference",
        "@${name}",
        "Inserts a reference to a label.",
    );

    ctx.snippet_completion(
        "heading",
        "= ${title}",
        "Inserts a section heading.",
    );

    ctx.snippet_completion(
        "list item",
        "- ${item}",
        "Inserts an item of a bullet list.",
    );

    ctx.snippet_completion(
        "enumeration item",
        "+ ${item}",
        "Inserts an item of a numbered list.",
    );

    ctx.snippet_completion(
        "enumeration item (numbered)",
        "${number}. ${item}",
        "Inserts an explicitly numbered list item.",
    );

    ctx.snippet_completion(
        "term list item",
        "/ ${term}: ${description}",
        "Inserts an item of a term list.",
    );

    ctx.snippet_completion(
        "math (inline)",
        "$${x}$",
        "Inserts an inline-level mathematical equation.",
    );

    ctx.snippet_completion(
        "math (block)",
        "$ ${sum_x^2} $",
        "Inserts a block-level mathematical equation.",
    );
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
    if !is_triggerred_by_punc(ctx.trigger_character)
        && matches!(ctx.leaf.kind(), SyntaxKind::Text | SyntaxKind::MathIdent)
    {
        ctx.from = ctx.leaf.offset();
        math_completions(ctx);
        return true;
    }

    // Anywhere: "$|$".
    if !is_triggerred_by_punc(ctx.trigger_character) && ctx.explicit {
        ctx.from = ctx.cursor;
        math_completions(ctx);
        return true;
    }

    false
}

/// Add completions for math snippets.
#[rustfmt::skip]
fn math_completions(ctx: &mut CompletionContext) {
    ctx.scope_completions(true);

    ctx.snippet_completion(
        "subscript",
        "${x}_${2:2}",
        "Sets something in subscript.",
    );

    ctx.snippet_completion(
        "superscript",
        "${x}^${2:2}",
        "Sets something in superscript.",
    );

    ctx.snippet_completion(
        "fraction",
        "${x}/${y}",
        "Inserts a fraction.",
    );
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

/// Complete labels.
fn complete_labels(ctx: &mut CompletionContext) -> bool {
    // A label anywhere in code: "(<la|".
    if (ctx.leaf.kind().is_error() && ctx.leaf.text().starts_with('<'))
        || ctx.leaf.kind() == SyntaxKind::Label
    {
        ctx.from = ctx.leaf.offset() + 1;
        ctx.label_completions(false);
        return true;
    }

    false
}

/// Complete imports.
fn complete_imports(ctx: &mut CompletionContext) -> bool {
    // In an import path for a package:
    // "#import "@|",
    if_chain! {
        if matches!(
            ctx.leaf.parent_kind(),
            Some(SyntaxKind::ModuleImport | SyntaxKind::ModuleInclude)
        );
        if let Some(ast::Expr::Str(str)) = ctx.leaf.cast();
        let value = str.get();
        if value.starts_with('@');
        then {
            let all_versions = value.contains(':');
            ctx.from = ctx.leaf.offset();
            ctx.package_completions(all_versions);
            return true;
        }
    }

    // Behind an import list:
    // "#import "path.typ": |",
    // "#import "path.typ": a, b, |".
    if_chain! {
        if let Some(prev) = ctx.leaf.prev_sibling();
        if let Some(ast::Expr::Import(import)) = prev.get().cast();
        if let Some(ast::Imports::Items(items)) = import.imports();
        if let Some(source) = prev.children().find(|child| child.is::<ast::Expr>());
        then {
            ctx.from = ctx.cursor;
            import_item_completions(ctx, items, &source);
            return true;
        }
    }

    // Behind a half-started identifier in an import list:
    // "#import "path.typ": thi|",
    if_chain! {
        if ctx.leaf.kind() == SyntaxKind::Ident;
        if let Some(parent) = ctx.leaf.parent();
        if parent.kind() == SyntaxKind::ImportItems;
        if let Some(grand) = parent.parent();
        if let Some(ast::Expr::Import(import)) = grand.get().cast();
        if let Some(ast::Imports::Items(items)) = import.imports();
        if let Some(source) = grand.children().find(|child| child.is::<ast::Expr>());
        then {
            ctx.from = ctx.leaf.offset();
            import_item_completions(ctx, items, &source);
            return true;
        }
    }

    false
}

/// Add completions for all exports of a module.
fn import_item_completions<'a>(
    ctx: &mut CompletionContext<'a>,
    existing: ast::ImportItems<'a>,
    source: &LinkedNode,
) {
    let Some(value) = ctx.ctx.analyze_import(source).1 else {
        return;
    };
    let Some(scope) = value.scope() else { return };

    if existing.iter().next().is_none() {
        ctx.snippet_completion("*", "*", "Import everything.");
    }

    for (name, value, _) in scope.iter() {
        if existing
            .iter()
            .all(|item| item.original_name().as_str() != name)
        {
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
    if ctx.leaf.kind() == SyntaxKind::Ident {
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

    ctx.snippet_completion(
        "function call",
        "${function}(${arguments})[${body}]",
        "Evaluates a function.",
    );

    ctx.snippet_completion(
        "code block",
        "{ ${} }",
        "Inserts a nested code block.",
    );

    ctx.snippet_completion(
        "content block",
        "[${content}]",
        "Switches into markup mode.",
    );

    ctx.snippet_completion(
        "set rule",
        "set ${}",
        "Sets style properties on an element.",
    );

    ctx.snippet_completion(
        "show rule",
        "show ${}",
        "Redefines the look of an element.",
    );

    ctx.snippet_completion(
        "show rule (everything)",
        "show: ${}",
        "Transforms everything that follows.",
    );

    ctx.snippet_completion(
        "context expression",
        "context ${}",
        "Provides contextual data.",
    );

    ctx.snippet_completion(
        "let binding",
        "let ${name} = ${value}",
        "Saves a value in a variable.",
    );

    ctx.snippet_completion(
        "let binding (function)",
        "let ${name}(${params}) = ${output}",
        "Defines a function.",
    );

    ctx.snippet_completion(
        "if conditional",
        "if ${1 < 2} {\n\t${}\n}",
        "Computes or inserts something conditionally.",
    );

    ctx.snippet_completion(
        "if-else conditional",
        "if ${1 < 2} {\n\t${}\n} else {\n\t${}\n}",
        "Computes or inserts different things based on a condition.",
    );

    ctx.snippet_completion(
        "while loop",
        "while ${1 < 2} {\n\t${}\n}",
        "Computes or inserts something while a condition is met.",
    );

    ctx.snippet_completion(
        "for loop",
        "for ${value} in ${(1, 2, 3)} {\n\t${}\n}",
        "Computes or inserts something for each value in a collection.",
    );

    ctx.snippet_completion(
        "for loop (with key)",
        "for (${key}, ${value}) in ${(a: 1, b: 2)} {\n\t${}\n}",
        "Computes or inserts something for each key and value in a collection.",
    );

    ctx.snippet_completion(
        "break",
        "break",
        "Exits early from a loop.",
    );

    ctx.snippet_completion(
        "continue",
        "continue",
        "Continues with the next iteration of a loop.",
    );

    ctx.snippet_completion(
        "return",
        "return ${output}",
        "Returns early from a function.",
    );

    ctx.snippet_completion(
        "import module",
        "import \"${}\"",
        "Imports module from another file.",
    );

    ctx.snippet_completion(
        "import module by expression",
        "import ${}",
        "Imports items by expression.",
    );

    ctx.snippet_completion(
        "import package",
        "import \"@${}\": ${items}",
        "Imports variables from another file.",
    );

    ctx.snippet_completion(
        "include (file)",
        "include \"${file}.typ\"",
        "Includes content from another file.",
    );

    ctx.snippet_completion(
        "include (package)",
        "include \"@${}\"",
        "Includes content from another file.",
    );

    ctx.snippet_completion(
        "array literal",
        "(${1, 2, 3})",
        "Creates a sequence of values.",
    );

    ctx.snippet_completion(
        "dictionary literal",
        "(${a: 1, b: 2})",
        "Creates a mapping from names to value.",
    );

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
    pub trigger_suggest: bool,
    pub trigger_parameter_hints: bool,
    pub trigger_named_completion: bool,
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
        trigger_suggest: bool,
        trigger_parameter_hints: bool,
        trigger_named_completion: bool,
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
            trigger_suggest,
            trigger_parameter_hints,
            trigger_named_completion,
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

    /// Add a snippet completion.
    fn snippet_completion(
        &mut self,
        label: &'static str,
        snippet: &'static str,
        docs: &'static str,
    ) {
        self.completions.push(Completion {
            kind: CompletionKind::Syntax,
            label: label.into(),
            apply: Some(snippet.into()),
            detail: Some(docs.into()),
            label_detail: None,
            // VS Code doesn't do that... Auto triggering suggestion only happens on typing (word
            // starts or trigger characters). However, you can use editor.action.triggerSuggest as
            // command on a suggestion to "manually" retrigger suggest after inserting one
            command: (self.trigger_suggest && snippet.contains("${"))
                .then_some("editor.action.triggerSuggest"),
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
        let mut packages: Vec<_> = w.packages().iter().map(|e| (&e.0, e.1.clone())).collect();
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
            Value::Symbol(c) => Some(symbol_detail(c.get())),
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
                command = self
                    .trigger_parameter_hints
                    .then_some("editor.action.triggerParameterHints");
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

fn is_triggerred_by_punc(trigger_character: Option<char>) -> bool {
    trigger_character.is_some_and(|c| c.is_ascii_punctuation())
}
