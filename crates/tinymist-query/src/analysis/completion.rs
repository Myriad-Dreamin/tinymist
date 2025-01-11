//! Provides completions for the document.

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashSet};
use std::ops::Range;

use ecow::{eco_format, EcoString};
use if_chain::if_chain;
use lsp_types::InsertTextFormat;
use once_cell::sync::Lazy;
use reflexo::path::unix_slash;
use reflexo_typst::TypstDocument;
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use tinymist_derive::BindTyCtx;
use tinymist_world::LspWorld;
use typst::foundations::{
    fields_on, format_str, repr, AutoValue, Func, Label, NoneValue, Repr, Scope, StyleChain, Type,
    Value,
};
use typst::syntax::ast::{self, AstNode, Param};
use typst::syntax::{is_id_continue, is_id_start, is_ident};
use typst::text::RawElem;
use typst::visualize::Color;
use typst::World;
use typst_shim::{syntax::LinkedNodeExt, utils::hash128};
use unscanny::Scanner;

use crate::adt::interner::Interned;
use crate::analysis::{
    analyze_labels, func_signature, BuiltinTy, DynLabel, LocalContext, PathPreference, Ty,
};
use crate::completion::{
    Completion, CompletionCommand, CompletionContextKey, CompletionItem, CompletionKind,
    EcoTextEdit, ParsedSnippet, PostfixSnippet, PostfixSnippetScope, PrefixSnippet,
    DEFAULT_POSTFIX_SNIPPET, DEFAULT_PREFIX_SNIPPET,
};
use crate::prelude::*;
use crate::syntax::{
    classify_context, interpret_mode_at, is_ident_like, node_ancestors, previous_decls,
    surrounding_syntax, InterpretMode, PreviousDecl, SurroundingSyntax, SyntaxClass, SyntaxContext,
    VarClass,
};
use crate::ty::{
    DynTypeBounds, Iface, IfaceChecker, InsTy, SigTy, TyCtx, TypeInfo, TypeInterface, TypeVar,
};
use crate::upstream::{plain_docs_sentence, summarize_font_family};

use super::SharedContext;

mod field_access;
mod import;
mod kind;
mod mode;
mod param;
mod path;
mod scope;
mod snippet;
#[path = "completion/type.rs"]
mod type_;
mod typst_specific;
use kind::*;
use scope::*;
use type_::*;

type LspCompletion = CompletionItem;

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
    /// Whether to enable any postfix completion.
    pub(crate) fn postfix(&self) -> bool {
        self.postfix.unwrap_or(true)
    }

    /// Whether to enable any ufcs completion.
    pub(crate) fn any_ufcs(&self) -> bool {
        self.ufcs() || self.ufcs_left() || self.ufcs_right()
    }

    /// Whether to enable ufcs completion.
    pub(crate) fn ufcs(&self) -> bool {
        self.postfix() && self.postfix_ufcs.unwrap_or(true)
    }

    /// Whether to enable ufcs completion (left variant).
    pub(crate) fn ufcs_left(&self) -> bool {
        self.postfix() && self.postfix_ufcs_left.unwrap_or(true)
    }

    /// Whether to enable ufcs completion (right variant).
    pub(crate) fn ufcs_right(&self) -> bool {
        self.postfix() && self.postfix_ufcs_right.unwrap_or(true)
    }

    /// Gets the postfix snippets.
    pub(crate) fn postfix_snippets(&self) -> &EcoVec<PostfixSnippet> {
        self.postfix_snippets
            .as_ref()
            .unwrap_or(&DEFAULT_POSTFIX_SNIPPET)
    }
}

/// The struct describing how a completion worker views the editor's cursor.
pub struct CompletionCursor<'a> {
    /// The shared context
    ctx: Arc<SharedContext>,
    /// The position from which the completions apply.
    from: usize,
    /// The cursor position.
    cursor: usize,
    /// The parsed source.
    source: Source,
    /// The source text.
    text: &'a str,
    /// The text before the cursor.
    before: &'a str,
    /// The text after the cursor.
    after: &'a str,
    /// The leaf node at the cursor.
    leaf: LinkedNode<'a>,
    /// The syntax class at the cursor.
    syntax: Option<SyntaxClass<'a>>,
    /// The syntax context at the cursor.
    syntax_context: Option<SyntaxContext<'a>>,
    /// The surrounding syntax at the cursor
    surrounding_syntax: SurroundingSyntax,

    /// Cache for the last lsp range conversion.
    last_lsp_range_pair: Option<(Range<usize>, LspRange)>,
    /// Cache for the ident cursor.
    ident_cursor: OnceLock<Option<LinkedNode<'a>>>,
    /// Cache for the arg cursor.
    arg_cursor: OnceLock<Option<SyntaxNode>>,
}

impl<'a> CompletionCursor<'a> {
    /// Creates a completion cursor.
    pub fn new(ctx: Arc<SharedContext>, source: &'a Source, cursor: usize) -> Option<Self> {
        let text = source.text();
        let root = LinkedNode::new(source.root());
        let leaf = root.leaf_at_compat(cursor)?;
        // todo: cache
        let syntax = classify_syntax(leaf.clone(), cursor);
        let syntax_context = classify_context(leaf.clone(), Some(cursor));
        let surrounding_syntax = surrounding_syntax(&leaf);

        crate::log_debug_ct!("CompletionCursor: syntax {leaf:?} -> {syntax:#?}");
        crate::log_debug_ct!("CompletionCursor: context {leaf:?} -> {syntax_context:#?}");
        crate::log_debug_ct!("CompletionCursor: surrounding {leaf:?} -> {surrounding_syntax:#?}");
        Some(Self {
            ctx,
            text,
            source: source.clone(),
            before: &text[..cursor],
            after: &text[cursor..],
            leaf,
            syntax,
            syntax_context,
            surrounding_syntax,
            cursor,
            from: cursor,
            last_lsp_range_pair: None,
            ident_cursor: OnceLock::new(),
            arg_cursor: OnceLock::new(),
        })
    }

    /// A small window of context before the cursor.
    fn before_window(&self, size: usize) -> &str {
        slice_at(
            self.before,
            self.cursor.saturating_sub(size)..self.before.len(),
        )
    }

    /// Whether the cursor is related to a callee item.
    fn is_callee(&self) -> bool {
        matches!(self.syntax, Some(SyntaxClass::Callee(..)))
    }

    /// Gets Identifier under cursor.
    fn ident_cursor(&self) -> &Option<LinkedNode> {
        self.ident_cursor.get_or_init(|| {
            let is_from_ident = matches!(
                self.syntax,
                Some(SyntaxClass::Callee(..) | SyntaxClass::VarAccess(..))
            ) && is_ident_like(&self.leaf)
                && self.leaf.offset() == self.from;

            is_from_ident.then(|| self.leaf.clone())
        })
    }

    /// Gets the argument cursor.
    fn arg_cursor(&self) -> &Option<SyntaxNode> {
        self.arg_cursor.get_or_init(|| {
            let mut args_node = None;

            match self.syntax_context.clone() {
                Some(SyntaxContext::Arg { args, .. }) => {
                    args_node = Some(args.cast::<ast::Args>()?.to_untyped().clone());
                }
                Some(SyntaxContext::Normal(node))
                    if (matches!(node.kind(), SyntaxKind::ContentBlock)
                        && matches!(self.leaf.kind(), SyntaxKind::LeftBracket)) =>
                {
                    args_node = node.parent().map(|s| s.get().clone());
                }
                Some(
                    SyntaxContext::Element { .. }
                    | SyntaxContext::ImportPath(..)
                    | SyntaxContext::IncludePath(..)
                    | SyntaxContext::VarAccess(..)
                    | SyntaxContext::Paren { .. }
                    | SyntaxContext::Label { .. }
                    | SyntaxContext::Normal(..),
                )
                | None => {}
            }

            args_node
        })
    }

    /// Gets the LSP range of a given range with caching.
    fn lsp_range_of(&mut self, rng: Range<usize>) -> LspRange {
        // self.ctx.to_lsp_range(rng, &self.source)
        if let Some((last_rng, last_lsp_rng)) = &self.last_lsp_range_pair {
            if *last_rng == rng {
                return *last_lsp_rng;
            }
        }

        let lsp_rng = self.ctx.to_lsp_range(rng.clone(), &self.source);
        self.last_lsp_range_pair = Some((rng, lsp_rng));
        lsp_rng
    }

    /// Makes a full completion item from a cursor-insensitive completion.
    fn lsp_item_of(&mut self, item: &Completion) -> LspCompletion {
        // Determine range to replace
        let mut snippet = item.apply.as_ref().unwrap_or(&item.label).clone();
        let replace_range = if let Some(from_ident) = self.ident_cursor() {
            let mut rng = from_ident.range();

            // if modifying some arguments, we need to truncate and add a comma
            if !self.is_callee() && self.cursor != rng.end && is_arg_like_context(from_ident) {
                // extend comma
                if !snippet.trim_end().ends_with(',') {
                    snippet.push_str(", ");
                }

                // Truncate
                rng.end = self.cursor;
            }

            self.lsp_range_of(rng)
        } else {
            self.lsp_range_of(self.from..self.cursor)
        };

        let text_edit = EcoTextEdit::new(replace_range, snippet);

        LspCompletion {
            label: item.label.clone(),
            kind: item.kind,
            detail: item.detail.clone(),
            sort_text: item.sort_text.clone(),
            filter_text: item.filter_text.clone(),
            label_details: item.label_details.clone().map(From::from),
            text_edit: Some(text_edit),
            additional_text_edits: item.additional_text_edits.clone(),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            command: item.command.clone(),
            ..Default::default()
        }
    }
}

/// Alias for a completion cursor, [`CompletionCursor`].
type Cursor<'a> = CompletionCursor<'a>;

/// Autocomplete a cursor position in a source file.
///
/// Returns the position from which the completions apply and a list of
/// completions.
///
/// When `explicit` is `true`, the user requested the completion by pressing
/// control and space or something similar.
///
/// Passing a `document` (from a previous compilation) is optional, but
/// enhances the autocompletions. Label completions, for instance, are
/// only generated when the document is available.
pub struct CompletionWorker<'a> {
    /// The completions.
    pub completions: Vec<LspCompletion>,
    /// Whether the completion is incomplete.
    pub incomplete: bool,

    /// The analysis local context.
    ctx: &'a mut LocalContext,
    /// The compiled document.
    document: Option<&'a TypstDocument>,
    /// Whether the completion was explicitly requested.
    explicit: bool,
    /// The trigger character.
    trigger_character: Option<char>,
    /// The set of cast completions seen so far.
    seen_casts: HashSet<u128>,
    /// The set of type completions seen so far.
    seen_types: HashSet<Ty>,
    /// The set of field completions seen so far.
    seen_fields: HashSet<Interned<str>>,
}

impl<'a> CompletionWorker<'a> {
    /// Create a completion worker.
    pub fn new(
        ctx: &'a mut LocalContext,
        document: Option<&'a TypstDocument>,
        explicit: bool,
        trigger_character: Option<char>,
    ) -> Option<Self> {
        Some(Self {
            ctx,
            document,
            trigger_character,
            explicit,
            incomplete: true,
            completions: vec![],
            seen_casts: HashSet::new(),
            seen_types: HashSet::new(),
            seen_fields: HashSet::new(),
        })
    }

    /// Gets the world.
    pub fn world(&self) -> &LspWorld {
        self.ctx.world()
    }

    fn seen_field(&mut self, field: Interned<str>) -> bool {
        !self.seen_fields.insert(field)
    }

    /// Adds a prefix and suffix to all applications.
    fn enrich(&mut self, prefix: &str, suffix: &str) {
        for LspCompletion { text_edit, .. } in &mut self.completions {
            let apply = match text_edit {
                Some(EcoTextEdit { new_text, .. }) => new_text,
                _ => continue,
            };

            *apply = eco_format!("{prefix}{apply}{suffix}");
        }
    }

    // if ctx.before.ends_with(':') {
    //     ctx.enrich(" ", "");
    // }

    /// Starts the completion process.
    pub(crate) fn work(&mut self, cursor: &mut Cursor) -> Option<()> {
        // Skip if is the let binding item *directly*
        if let Some(SyntaxClass::VarAccess(var)) = &cursor.syntax {
            let node = var.node();
            match node.parent_kind() {
                // complete the init part of the let binding
                Some(SyntaxKind::LetBinding) => {
                    let parent = node.parent()?;
                    let parent_init = parent.cast::<ast::LetBinding>()?.init()?;
                    let parent_init = parent.find(parent_init.span())?;
                    parent_init.find(node.span())?;
                }
                Some(SyntaxKind::Closure) => {
                    let parent = node.parent()?;
                    let parent_body = parent.cast::<ast::Closure>()?.body();
                    let parent_body = parent.find(parent_body.span())?;
                    parent_body.find(node.span())?;
                }
                _ => {}
            }
        }

        // Skip if an error node starts with number (e.g. `1pt`)
        if matches!(
            cursor.syntax,
            Some(SyntaxClass::Callee(..) | SyntaxClass::VarAccess(..) | SyntaxClass::Normal(..))
        ) && cursor.leaf.erroneous()
        {
            let mut chars = cursor.leaf.text().chars();
            match chars.next() {
                Some(ch) if ch.is_numeric() => return None,
                Some('.') => {
                    if matches!(chars.next(), Some(ch) if ch.is_numeric()) {
                        return None;
                    }
                }
                _ => {}
            }
        }

        // Exclude it self from auto completion
        // e.g. `#let x = (1.);`
        let self_ty = cursor.leaf.cast::<ast::Expr>().and_then(|leaf| {
            let v = self.ctx.mini_eval(leaf)?;
            Some(Ty::Value(InsTy::new(v)))
        });

        if let Some(self_ty) = self_ty {
            self.seen_types.insert(self_ty);
        };

        let mut pair = Pair {
            worker: self,
            cursor,
        };
        let _ = pair.complete_cursor();

        // Filter
        if let Some(from_ident) = cursor.ident_cursor() {
            let ident_prefix = cursor.text[from_ident.offset()..cursor.cursor].to_string();

            self.completions.retain(|item| {
                let mut prefix_matcher = item.label.chars();
                'ident_matching: for ch in ident_prefix.chars() {
                    for item in prefix_matcher.by_ref() {
                        if item == ch {
                            continue 'ident_matching;
                        }
                    }

                    return false;
                }

                true
            });
        }

        for item in &mut self.completions {
            if let Some(EcoTextEdit {
                ref mut new_text, ..
            }) = item.text_edit
            {
                *new_text = to_lsp_snippet(new_text);
            }
        }

        Some(())
    }
}

struct CompletionPair<'a, 'b, 'c> {
    worker: &'c mut CompletionWorker<'a>,
    cursor: &'c mut Cursor<'b>,
}

type Pair<'a, 'b, 'c> = CompletionPair<'a, 'b, 'c>;

impl CompletionPair<'_, '_, '_> {
    /// Starts the completion on a cursor.
    pub(crate) fn complete_cursor(&mut self) -> Option<()> {
        use SurroundingSyntax::*;

        // Special completions, we should remove them finally
        if matches!(
            self.cursor.leaf.kind(),
            SyntaxKind::LineComment | SyntaxKind::BlockComment
        ) {
            return self.complete_comments().then_some(());
        }

        let surrounding_syntax = self.cursor.surrounding_syntax;
        let mode = interpret_mode_at(Some(&self.cursor.leaf));

        // Special completions 2, we should remove them finally
        if matches!(surrounding_syntax, ImportList) {
            return self.complete_imports().then_some(());
        }

        // Special completions 3, we should remove them finally
        if matches!(surrounding_syntax, ParamList) {
            return self.complete_params();
        }

        // Checks and completes `self.cursor.syntax_context`
        match self.cursor.syntax_context.clone() {
            Some(SyntaxContext::Element { container, .. }) => {
                // The existing dictionary fields are not interesting
                if let Some(container) = container.cast::<ast::Dict>() {
                    for named in container.items() {
                        if let ast::DictItem::Named(named) = named {
                            self.worker.seen_field(named.name().into());
                        }
                    }
                };
            }
            Some(SyntaxContext::Arg { args, .. }) => {
                // The existing arguments are not interesting
                let args = args.cast::<ast::Args>()?;
                for arg in args.items() {
                    if let ast::Arg::Named(named) = arg {
                        self.worker.seen_field(named.name().into());
                    }
                }
            }
            // todo: complete field by types
            Some(SyntaxContext::VarAccess(
                var @ (VarClass::FieldAccess { .. } | VarClass::DotAccess { .. }),
            )) => {
                let target = var.accessed_node()?;
                let field = var.accessing_field()?;

                self.cursor.from = field.offset(&self.cursor.source)?;

                self.field_access_completions(&target);
                return Some(());
            }
            Some(SyntaxContext::ImportPath(path) | SyntaxContext::IncludePath(path)) => {
                let Some(ast::Expr::Str(str)) = path.cast() else {
                    return None;
                };
                self.cursor.from = path.offset();
                let value = str.get();
                if value.starts_with('@') {
                    let all_versions = value.contains(':');
                    self.package_completions(all_versions);
                    return Some(());
                } else {
                    let paths = self.complete_path(&crate::analysis::PathPreference::Source {
                        allow_package: true,
                    });
                    // todo: remove ctx.completions
                    self.worker.completions.extend(paths.unwrap_or_default());
                }

                return Some(());
            }
            // todo: complete reference by type
            Some(SyntaxContext::Normal(node)) if (matches!(node.kind(), SyntaxKind::Ref)) => {
                self.cursor.from = self.cursor.leaf.offset() + 1;
                self.ref_completions();
                return Some(());
            }
            Some(
                SyntaxContext::VarAccess(VarClass::Ident { .. })
                | SyntaxContext::Paren { .. }
                | SyntaxContext::Label { .. }
                | SyntaxContext::Normal(..),
            )
            | None => {}
        }

        // Triggers a complete type checking.
        let ty = self
            .worker
            .ctx
            .post_type_of_node(self.cursor.leaf.clone())
            .filter(|ty| !matches!(ty, Ty::Any));

        crate::log_debug_ct!(
            "complete_type: {:?} -> ({surrounding_syntax:?}, {ty:#?})",
            self.cursor.leaf
        );

        // Adjusts the completion position
        // todo: syntax class seems not being considering `is_ident_like`
        // todo: merge ident_content_offset and label_content_offset
        if is_ident_like(&self.cursor.leaf) {
            self.cursor.from = self.cursor.leaf.offset();
        } else if let Some(offset) = self
            .cursor
            .syntax
            .as_ref()
            .and_then(SyntaxClass::complete_offset)
        {
            self.cursor.from = offset;
        }

        // Completion by types.
        if let Some(ty) = ty {
            let filter = |ty: &Ty| match surrounding_syntax {
                SurroundingSyntax::StringContent => match ty {
                    Ty::Builtin(BuiltinTy::Path(..) | BuiltinTy::TextFont) => true,
                    Ty::Value(val) => matches!(val.val, Value::Str(..)),
                    Ty::Builtin(BuiltinTy::Type(ty)) => {
                        *ty == Type::of::<typst::foundations::Str>()
                    }
                    _ => false,
                },
                _ => true,
            };
            let mut ctx = TypeCompletionWorker {
                base: self,
                filter: &filter,
            };
            ctx.type_completion(&ty, None);
        }
        let mut type_completions = std::mem::take(&mut self.worker.completions);

        // Completion by [`crate::syntax::InterpretMode`].
        match mode {
            InterpretMode::Code => {
                self.complete_code();
            }
            InterpretMode::Math => {
                self.complete_math();
            }
            InterpretMode::Raw => {
                self.complete_markup();
            }
            InterpretMode::Markup => match surrounding_syntax {
                Regular => {
                    self.complete_markup();
                }
                Selector | ShowTransform | SetRule => {
                    self.complete_code();
                }
                StringContent | ImportList | ParamList => {}
            },
            InterpretMode::Comment | InterpretMode::String => {}
        };

        // Snippet completions associated by surrounding_syntax.
        match surrounding_syntax {
            Regular | StringContent | ImportList | ParamList | SetRule => {}
            Selector => {
                self.snippet_completion(
                    "text selector",
                    "\"${text}\"",
                    "Replace occurrences of specific text.",
                );

                self.snippet_completion(
                    "regex selector",
                    "regex(\"${regex}\")",
                    "Replace matches of a regular expression.",
                );
            }
            ShowTransform => {
                self.snippet_completion(
                    "replacement",
                    "[${content}]",
                    "Replace the selected element with content.",
                );

                self.snippet_completion(
                    "replacement (string)",
                    "\"${text}\"",
                    "Replace the selected element with a string of text.",
                );

                self.snippet_completion(
                    "transformation",
                    "element => [${content}]",
                    "Transform the element with a function.",
                );
            }
        }

        // todo: filter completions by type
        // ctx.strict_scope_completions(false, |value| value.ty() == *ty);
        // let length_ty = Type::of::<Length>();
        // ctx.strict_scope_completions(false, |value| value.ty() == length_ty);
        // let color_ty = Type::of::<Color>();
        // ctx.strict_scope_completions(false, |value| value.ty() == color_ty);
        // let ty = Type::of::<Dir>();
        // ctx.strict_scope_completions(false, |value| value.ty() == ty);

        crate::log_debug_ct!(
            "sort completions: {type_completions:#?} {:#?}",
            self.worker.completions
        );

        // Sorts completions
        type_completions.sort_by(|a, b| {
            a.sort_text
                .as_ref()
                .cmp(&b.sort_text.as_ref())
                .then_with(|| a.label.cmp(&b.label))
        });
        self.worker.completions.sort_by(|a, b| {
            a.sort_text
                .as_ref()
                .cmp(&b.sort_text.as_ref())
                .then_with(|| a.label.cmp(&b.label))
        });

        for (idx, compl) in type_completions
            .iter_mut()
            .chain(self.worker.completions.iter_mut())
            .enumerate()
        {
            compl.sort_text = Some(eco_format!("{idx:03}"));
        }

        self.worker.completions.append(&mut type_completions);

        crate::log_debug_ct!("sort completions after: {:#?}", self.worker.completions);

        if let Some(node) = self.cursor.arg_cursor() {
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
                self.worker.enrich("", ")");
            }
        }

        if self.cursor.before.ends_with(',') || self.cursor.before.ends_with(':') {
            self.worker.enrich(" ", "");
        }
        match surrounding_syntax {
            Regular | ImportList | ParamList | ShowTransform | SetRule | StringContent => {}
            Selector => {
                self.worker.enrich("", ": ${}");
            }
        }

        crate::log_debug_ct!("enrich completions: {:?}", self.worker.completions);

        Some(())
    }

    /// Pushes a cursor-insensitive completion item.
    fn push_completion(&mut self, completion: Completion) {
        self.worker
            .completions
            .push(self.cursor.lsp_item_of(&completion));
    }
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

static TYPST_SNIPPET_PLACEHOLDER_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\$\{(.*?)\}").unwrap());

/// Adds numbering to placeholders in snippets
fn to_lsp_snippet(typst_snippet: &str) -> EcoString {
    let mut counter = 1;
    let result = TYPST_SNIPPET_PLACEHOLDER_RE.replace_all(typst_snippet, |cap: &Captures| {
        let substitution = format!("${{{}:{}}}", counter, &cap[1]);
        counter += 1;
        substitution
    });

    result.into()
}

fn is_hash_expr(leaf: &LinkedNode<'_>) -> bool {
    is_hash_expr_(leaf).is_some()
}

fn is_hash_expr_(leaf: &LinkedNode<'_>) -> Option<()> {
    match leaf.kind() {
        SyntaxKind::Hash => Some(()),
        SyntaxKind::Ident => {
            let prev_leaf = leaf.prev_leaf()?;
            if prev_leaf.kind() == SyntaxKind::Hash {
                Some(())
            } else {
                None
            }
        }
        _ => None,
    }
}

fn is_triggered_by_punc(trigger_character: Option<char>) -> bool {
    trigger_character.is_some_and(|ch| ch.is_ascii_punctuation())
}

fn is_arg_like_context(mut matching: &LinkedNode) -> bool {
    while let Some(parent) = matching.parent() {
        use SyntaxKind::*;

        // todo: contextual
        match parent.kind() {
            ContentBlock | Equation | CodeBlock | Markup | Math | Code => return false,
            Args | Params | Destructuring | Array | Dict => return true,
            _ => {}
        }

        matching = parent;
    }
    false
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

#[cfg(test)]
mod tests {
    use super::slice_at;

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
