//! Provides completions for the document.

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashSet};
use std::ops::{Deref, Range};

use ecow::{eco_format, EcoString};
use if_chain::if_chain;
use lsp_types::{
    Command, CompletionItem, CompletionItemLabelDetails, CompletionTextEdit, InsertTextFormat,
    TextEdit,
};
use once_cell::sync::Lazy;
use reflexo::path::unix_slash;
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use tinymist_derive::BindTyCtx;
use tinymist_world::LspWorld;
use typst::foundations::{
    fields_on, format_str, repr, AutoValue, Func, Label, NoneValue, Repr, Scope, StyleChain, Type,
    Value,
};
use typst::model::Document;
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
use crate::prelude::*;
use crate::snippet::{
    CompletionCommand, CompletionContextKey, ParsedSnippet, PostfixSnippet, PostfixSnippetScope,
    PrefixSnippet, DEFAULT_POSTFIX_SNIPPET, DEFAULT_PREFIX_SNIPPET,
};
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

type LspCompletion = lsp_types::CompletionItem;
type LspCompletionKind = lsp_types::CompletionItemKind;

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

/// A kind of item that can be completed.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
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

impl From<CompletionKind> for LspCompletionKind {
    fn from(value: CompletionKind) -> Self {
        match value {
            CompletionKind::Syntax => Self::SNIPPET,
            CompletionKind::Func => Self::FUNCTION,
            CompletionKind::Param => Self::VARIABLE,
            CompletionKind::Field => Self::FIELD,
            CompletionKind::Variable => Self::VARIABLE,
            CompletionKind::Constant => Self::CONSTANT,
            CompletionKind::Reference => Self::REFERENCE,
            CompletionKind::Symbol(_) => Self::FIELD,
            CompletionKind::Type => Self::CLASS,
            CompletionKind::Module => Self::MODULE,
            CompletionKind::File => Self::FILE,
            CompletionKind::Folder => Self::FOLDER,
        }
    }
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

    from_ident: OnceLock<Option<LinkedNode<'a>>>,
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

        crate::log_debug_ct!("CompletionCursor: context {leaf:?} -> {syntax_context:#?}");
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
            from_ident: OnceLock::new(),
        })
    }

    /// A small window of context before the cursor.
    fn before_window(&self, size: usize) -> &str {
        slice_at(
            self.before,
            self.cursor.saturating_sub(size)..self.before.len(),
        )
    }

    fn is_callee(&self) -> bool {
        matches!(self.syntax, Some(SyntaxClass::Callee(..)))
    }

    /// Gets Identifier under cursor.
    fn ident_cursor(&self) -> &Option<LinkedNode> {
        self.from_ident.get_or_init(|| {
            let is_from_ident = matches!(
                self.syntax,
                Some(SyntaxClass::Callee(..) | SyntaxClass::VarAccess(..))
            ) && is_ident_like(&self.leaf)
                && self.leaf.offset() == self.from;

            is_from_ident.then(|| self.leaf.clone())
        })
    }

    fn to_lsp_range(&self, rng: Range<usize>) -> LspRange {
        self.ctx.to_lsp_range(rng, &self.source)
    }
}

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
    /// The analysis local context.
    pub ctx: &'a mut LocalContext,
    /// The compiled document.
    pub document: Option<&'a Document>,
    /// Whether the completion was explicitly requested.
    pub explicit: bool,
    /// The trigger character.
    pub trigger_character: Option<char>,
    /// The completions.
    pub raw_completions: Vec<Completion>,
    /// The (lsp_types) completions.
    pub completions: Vec<lsp_types::CompletionItem>,
    /// Whether the completion is incomplete.
    pub incomplete: bool,
    /// The set of cast completions seen so far.
    pub seen_casts: HashSet<u128>,
    /// The set of type completions seen so far.
    pub seen_types: HashSet<Ty>,
    /// The set of field completions seen so far.
    pub seen_fields: HashSet<Interned<str>>,
}

impl<'a> CompletionWorker<'a> {
    /// Create a completion worker.
    pub fn new(
        ctx: &'a mut LocalContext,
        document: Option<&'a Document>,
        explicit: bool,
        trigger_character: Option<char>,
    ) -> Option<Self> {
        Some(Self {
            ctx,
            document,
            trigger_character,
            explicit,
            incomplete: true,
            raw_completions: vec![],
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
        for Completion { label, apply, .. } in &mut self.raw_completions {
            let current = apply.as_ref().unwrap_or(label);
            *apply = Some(eco_format!("{prefix}{current}{suffix}"));
        }
    }

    // if ctx.before.ends_with(':') {
    //     ctx.enrich(" ", "");
    // }

    /// Starts the completion process.
    pub(crate) fn work(mut self, cursor: &mut Cursor) -> Option<(bool, Vec<LspCompletion>)> {
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

        let _ = self.complete_root(cursor);

        // Filter
        if let Some(from_ident) = cursor.ident_cursor() {
            let ident_prefix = cursor.text[from_ident.offset()..cursor.cursor].to_string();

            self.raw_completions.retain(|item| {
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

        // Determine range to replace
        let replace_range = if let Some(from_ident) = cursor.ident_cursor() {
            let mut rng = from_ident.range();

            // if modifying some arguments, we need to truncate and add a comma
            if !cursor.is_callee() && cursor.cursor != rng.end && is_arg_like_context(from_ident) {
                // extend comma
                for item in self.raw_completions.iter_mut() {
                    let apply = match &mut item.apply {
                        Some(w) => w,
                        None => {
                            item.apply = Some(item.label.clone());
                            item.apply.as_mut().unwrap()
                        }
                    };
                    if apply.trim_end().ends_with(',') {
                        continue;
                    }
                    apply.push_str(", ");
                }

                // Truncate
                rng.end = cursor.cursor;
            }

            cursor.to_lsp_range(rng)
        } else {
            cursor.to_lsp_range(cursor.from..cursor.cursor)
        };

        let completions = self.raw_completions.iter().map(|typst_completion| {
            let typst_snippet = typst_completion
                .apply
                .as_ref()
                .unwrap_or(&typst_completion.label);
            let lsp_snippet = to_lsp_snippet(typst_snippet);
            let text_edit = CompletionTextEdit::Edit(TextEdit::new(replace_range, lsp_snippet));

            LspCompletion {
                label: typst_completion.label.to_string(),
                kind: Some(typst_completion.kind.into()),
                detail: typst_completion.detail.as_ref().map(String::from),
                sort_text: typst_completion.sort_text.as_ref().map(String::from),
                filter_text: typst_completion.filter_text.as_ref().map(String::from),
                label_details: typst_completion.label_detail.as_ref().map(|desc| {
                    CompletionItemLabelDetails {
                        detail: None,
                        description: Some(desc.to_string()),
                    }
                }),
                text_edit: Some(text_edit),
                additional_text_edits: typst_completion.additional_text_edits.clone(),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                commit_characters: typst_completion
                    .commit_char
                    .as_ref()
                    .map(|v| vec![v.to_string()]),
                command: typst_completion.command.as_ref().map(|cmd| Command {
                    command: cmd.to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }
        });
        let mut items = completions.collect_vec();
        items.append(&mut self.completions);

        Some((self.incomplete, items))
    }

    pub(crate) fn complete_root(&mut self, cursor: &mut Cursor) -> Option<()> {
        use SurroundingSyntax::*;

        if matches!(
            cursor.leaf.kind(),
            SyntaxKind::LineComment | SyntaxKind::BlockComment
        ) {
            return self.complete_comments(cursor).then_some(());
        }

        let scope = cursor.surrounding_syntax;
        let mode = interpret_mode_at(Some(&cursor.leaf));
        if matches!(scope, ImportList) {
            return self.complete_imports(cursor).then_some(());
        }

        let mut args_node = None;

        match cursor.syntax_context.clone() {
            Some(SyntaxContext::Element { container, .. }) => {
                if let Some(container) = container.cast::<ast::Dict>() {
                    for named in container.items() {
                        if let ast::DictItem::Named(named) = named {
                            self.seen_field(named.name().into());
                        }
                    }
                };
            }
            Some(SyntaxContext::Arg { args, .. }) => {
                let args = args.cast::<ast::Args>()?;
                for arg in args.items() {
                    if let ast::Arg::Named(named) = arg {
                        self.seen_field(named.name().into());
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

                cursor.from = field.offset(&cursor.source)?;

                self.field_access_completions(cursor, &target);
                return Some(());
            }
            Some(SyntaxContext::ImportPath(path) | SyntaxContext::IncludePath(path)) => {
                let Some(ast::Expr::Str(str)) = path.cast() else {
                    return None;
                };
                cursor.from = path.offset();
                let value = str.get();
                if value.starts_with('@') {
                    let all_versions = value.contains(':');
                    self.package_completions(cursor, all_versions);
                    return Some(());
                } else {
                    let paths = self.complete_path(
                        cursor,
                        &crate::analysis::PathPreference::Source {
                            allow_package: true,
                        },
                    );
                    // todo: remove ctx.completions
                    self.completions.extend(paths.unwrap_or_default());
                }

                return Some(());
            }
            Some(SyntaxContext::Normal(node))
                if (matches!(node.kind(), SyntaxKind::ContentBlock)
                    && matches!(cursor.leaf.kind(), SyntaxKind::LeftBracket)) =>
            {
                args_node = node.parent().map(|s| s.get().clone());
            }
            // todo: complete reference by type
            Some(SyntaxContext::Normal(node)) if (matches!(node.kind(), SyntaxKind::Ref)) => {
                cursor.from = cursor.leaf.offset() + 1;
                self.ref_completions(cursor);
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

        let ty = self
            .ctx
            .post_type_of_node(cursor.leaf.clone())
            .filter(|ty| !matches!(ty, Ty::Any));

        crate::log_debug_ct!("complete_type: {:?} -> ({scope:?}, {ty:#?})", cursor.leaf);

        // adjust the completion position
        // todo: syntax class seems not being considering `is_ident_like`
        // todo: merge ident_content_offset and label_content_offset
        if is_ident_like(&cursor.leaf) {
            cursor.from = cursor.leaf.offset();
        } else if let Some(offset) = cursor
            .syntax
            .as_ref()
            .and_then(SyntaxClass::complete_offset)
        {
            cursor.from = offset;
        }

        if let Some(ty) = ty {
            let filter = |ty: &Ty| match scope {
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
            ctx.type_completion(cursor, &ty, None);
        }

        let mut completions = std::mem::take(&mut self.raw_completions);
        match mode {
            InterpretMode::Code => {
                self.complete_code(cursor);
            }
            InterpretMode::Math => {
                self.complete_math(cursor);
            }
            InterpretMode::Raw => {
                self.complete_markup(cursor);
            }
            InterpretMode::Markup => match scope {
                Regular => {
                    self.complete_markup(cursor);
                }
                Selector | ShowTransform | SetRule => {
                    self.complete_code(cursor);
                }
                StringContent | ImportList => {}
            },
            InterpretMode::Comment | InterpretMode::String => {}
        };

        match scope {
            Regular | StringContent | ImportList | SetRule => {}
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

        // ctx.strict_scope_completions(false, |value| value.ty() == *ty);
        // let length_ty = Type::of::<Length>();
        // ctx.strict_scope_completions(false, |value| value.ty() == length_ty);
        // let color_ty = Type::of::<Color>();
        // ctx.strict_scope_completions(false, |value| value.ty() == color_ty);
        // let ty = Type::of::<Dir>();
        // ctx.strict_scope_completions(false, |value| value.ty() == ty);

        crate::log_debug_ct!(
            "sort_and_explicit_code_completion: {completions:#?} {:#?}",
            self.raw_completions
        );

        completions.sort_by(|a, b| {
            a.sort_text
                .as_ref()
                .cmp(&b.sort_text.as_ref())
                .then_with(|| a.label.cmp(&b.label))
        });
        self.raw_completions.sort_by(|a, b| {
            a.sort_text
                .as_ref()
                .cmp(&b.sort_text.as_ref())
                .then_with(|| a.label.cmp(&b.label))
        });

        // todo: this is a bit messy, we can refactor for improving maintainability
        // The messy code will finally gone, but to help us go over the mess stage, I
        // drop some comment here.
        //
        // currently, there are only path completions in ctx.completions
        // and type/named param/positional param completions in completions
        // and all rest less relevant completions inctx.completions
        for (idx, compl) in self.completions.iter_mut().enumerate() {
            compl.sort_text = Some(format!("{idx:03}"));
        }
        let sort_base = self.completions.len();
        for (idx, compl) in (completions
            .iter_mut()
            .chain(self.raw_completions.iter_mut()))
        .enumerate()
        {
            compl.sort_text = Some(eco_format!("{:03}", idx + sort_base));
        }

        crate::log_debug_ct!(
            "sort_and_explicit_code_completion after: {completions:#?} {:#?}",
            self.raw_completions
        );

        self.raw_completions.append(&mut completions);

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
                self.enrich("", ")");
            }
        }

        if cursor.before.ends_with(',') || cursor.before.ends_with(':') {
            self.enrich(" ", "");
        }
        match scope {
            Regular | ImportList | ShowTransform | SetRule | StringContent => {}
            Selector => {
                self.enrich("", ": ${}");
            }
        }

        crate::log_debug_ct!(
            "sort_and_explicit_code_completion: {:?}",
            self.raw_completions
        );

        Some(())
    }

    /// Complete in comments. Or rather, don't!
    fn complete_comments(&mut self, cursor: &mut Cursor) -> bool {
        let text = cursor.leaf.get().text();
        // check if next line defines a function
        if_chain! {
            if text == "///" || text == "/// ";
            // hash node
            if let Some(next) = cursor.leaf.next_leaf();
            // let node
            if let Some(next_next) = next.next_leaf();
            if let Some(next_next) = next_next.next_leaf();
            if matches!(next_next.parent_kind(), Some(SyntaxKind::Closure));
            if let Some(closure) = next_next.parent();
            if let Some(closure) = closure.cast::<ast::Expr>();
            if let ast::Expr::Closure(c) = closure;
            then {
                let mut doc_snippet: String = if text == "///" {
                    " $0\n///".to_string()
                } else {
                    "$0\n///".to_string()
                };
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
                self.raw_completions.push(Completion {
                    label: "Document function".into(),
                    apply: Some(doc_snippet.into()),
                    ..Completion::default()
                });
            }
        };

        true
    }

    /// Complete in markup mode.
    fn complete_markup(&mut self, cursor: &mut Cursor) -> bool {
        let parent_raw =
            node_ancestors(&cursor.leaf).find(|node| matches!(node.kind(), SyntaxKind::Raw));

        // Behind a half-completed binding: "#let x = |" or `#let f(x) = |`.
        if_chain! {
            if let Some(prev) = cursor.leaf.prev_leaf();
            if matches!(prev.kind(), SyntaxKind::Eq | SyntaxKind::Arrow);
            if matches!( prev.parent_kind(), Some(SyntaxKind::LetBinding | SyntaxKind::Closure));
            then {
                cursor.from = cursor.cursor;
                self.code_completions(cursor, false);
                return true;
            }
        }

        // Behind a half-completed context block: "#context |".
        if_chain! {
            if let Some(prev) = cursor.leaf.prev_leaf();
            if prev.kind() == SyntaxKind::Context;
            then {
                cursor.from = cursor.cursor;
                self.code_completions(cursor, false);
                return true;
            }
        }

        // Directly after a raw block.
        if let Some(parent_raw) = parent_raw {
            let mut s = Scanner::new(cursor.text);
            s.jump(parent_raw.offset());
            if s.eat_if("```") {
                s.eat_while('`');
                let start = s.cursor();
                if s.eat_if(is_id_start) {
                    s.eat_while(is_id_continue);
                }
                if s.cursor() == cursor.cursor {
                    cursor.from = start;
                    self.raw_completions();
                }
                return true;
            }
        }

        // Anywhere: "|".
        if !is_triggered_by_punc(self.trigger_character) && self.explicit {
            cursor.from = cursor.cursor;
            self.snippet_completions(Some(InterpretMode::Markup), None);
            return true;
        }

        false
    }

    /// Complete in math mode.
    fn complete_math(&mut self, cursor: &mut Cursor) -> bool {
        // Behind existing atom or identifier: "$a|$" or "$abc|$".
        if !is_triggered_by_punc(self.trigger_character)
            && matches!(cursor.leaf.kind(), SyntaxKind::Text | SyntaxKind::MathIdent)
        {
            cursor.from = cursor.leaf.offset();
            self.scope_completions(cursor, true);
            self.snippet_completions(Some(InterpretMode::Math), None);
            return true;
        }

        // Anywhere: "$|$".
        if !is_triggered_by_punc(self.trigger_character) && self.explicit {
            cursor.from = cursor.cursor;
            self.scope_completions(cursor, true);
            self.snippet_completions(Some(InterpretMode::Math), None);
            return true;
        }

        false
    }

    /// Complete in code mode.
    fn complete_code(&mut self, cursor: &mut Cursor) -> bool {
        // Start of an interpolated identifier: "#|".
        if cursor.leaf.kind() == SyntaxKind::Hash {
            cursor.from = cursor.cursor;
            self.code_completions(cursor, true);

            return true;
        }

        // Start of an interpolated identifier: "#pa|".
        if cursor.leaf.kind() == SyntaxKind::Ident {
            cursor.from = cursor.leaf.offset();
            self.code_completions(cursor, is_hash_expr(&cursor.leaf));
            return true;
        }

        // Behind a half-completed context block: "context |".
        if_chain! {
            if let Some(prev) = cursor.leaf.prev_leaf();
            if prev.kind() == SyntaxKind::Context;
            then {
                cursor.from = cursor.cursor;
                self.code_completions(cursor, false);
                return true;
            }
        }

        // An existing identifier: "{ pa| }".
        if cursor.leaf.kind() == SyntaxKind::Ident
            && !matches!(cursor.leaf.parent_kind(), Some(SyntaxKind::FieldAccess))
        {
            cursor.from = cursor.leaf.offset();
            self.code_completions(cursor, false);
            return true;
        }

        // Anywhere: "{ | }".
        // But not within or after an expression.
        // ctx.explicit &&
        if cursor.leaf.kind().is_trivia()
            || (matches!(
                cursor.leaf.kind(),
                SyntaxKind::LeftParen | SyntaxKind::LeftBrace
            ) || (matches!(cursor.leaf.kind(), SyntaxKind::Colon)
                && cursor.leaf.parent_kind() == Some(SyntaxKind::ShowRule)))
        {
            cursor.from = cursor.cursor;
            self.code_completions(cursor, false);
            return true;
        }

        false
    }

    /// Add completions for expression snippets.
    fn code_completions(&mut self, cursor: &mut Cursor, hash: bool) {
        // todo: filter code completions
        // matches!(value, Value::Symbol(_) | Value::Func(_) | Value::Type(_) |
        // Value::Module(_))
        self.scope_completions(cursor, true);

        self.snippet_completions(Some(InterpretMode::Code), None);

        if !hash {
            self.snippet_completion(
                "function",
                "(${params}) => ${output}",
                "Creates an unnamed function.",
            );
        }
    }

    /// Complete imports.
    fn complete_imports(&mut self, cursor: &mut Cursor) -> bool {
        // On the colon marker of an import list:
        // "#import "path.typ":|"
        if_chain! {
            if matches!(cursor.leaf.kind(), SyntaxKind::Colon);
            if let Some(parent) = cursor.leaf.clone().parent();
            if let Some(ast::Expr::Import(import)) = parent.get().cast();
            if !matches!(import.imports(), Some(ast::Imports::Wildcard));
            if let Some(source) = parent.children().find(|child| child.is::<ast::Expr>());
            then {
                let items = match import.imports() {
                    Some(ast::Imports::Items(items)) => items,
                    _ => Default::default(),
                };

                cursor.from = cursor.cursor;

                self.import_item_completions(cursor, items, vec![], &source);
                if items.iter().next().is_some() {
                    self.enrich("", ", ");
                }
                return true;
            }
        }

        // Behind an import list:
        // "#import "path.typ": |",
        // "#import "path.typ": a, b, |".
        if_chain! {
            if let Some(prev) = cursor.leaf.prev_sibling();
            if let Some(ast::Expr::Import(import)) = prev.get().cast();
            if !cursor.text[prev.offset()..cursor.cursor].contains('\n');
            if let Some(ast::Imports::Items(items)) = import.imports();
            if let Some(source) = prev.children().find(|child| child.is::<ast::Expr>());
            then {
                cursor.from = cursor.cursor;
                self.import_item_completions( cursor,items, vec![], &source);
                return true;
            }
        }

        // Behind a comma in an import list:
        // "#import "path.typ": this,|".
        if_chain! {
            if matches!(cursor.leaf.kind(), SyntaxKind::Comma);
            if let Some(parent) = cursor.leaf.clone().parent();
            if parent.kind() == SyntaxKind::ImportItems;
            if let Some(grand) = parent.parent();
            if let Some(ast::Expr::Import(import)) = grand.get().cast();
            if let Some(ast::Imports::Items(items)) = import.imports();
            if let Some(source) = grand.children().find(|child| child.is::<ast::Expr>());
            then {
                self.import_item_completions(cursor, items, vec![], &source);
                self.enrich(" ", "");
                return true;
            }
        }

        // Behind a half-started identifier in an import list:
        // "#import "path.typ": th|".
        if_chain! {
            if matches!(cursor.leaf.kind(), SyntaxKind::Ident | SyntaxKind::Dot);
            if let Some(path_ctx) = cursor.leaf.clone().parent();
            if path_ctx.kind() == SyntaxKind::ImportItemPath;
            if let Some(parent) = path_ctx.parent();
            if parent.kind() == SyntaxKind::ImportItems;
            if let Some(grand) = parent.parent();
            if let Some(ast::Expr::Import(import)) = grand.get().cast();
            if let Some(ast::Imports::Items(items)) = import.imports();
            if let Some(source) = grand.children().find(|child| child.is::<ast::Expr>());
            then {
                if cursor.leaf.kind() == SyntaxKind::Ident {
                    cursor.from = cursor.leaf.offset();
                }
                let path = path_ctx.cast::<ast::ImportItemPath>().map(|path| path.iter().take_while(|ident| ident.span() != cursor.leaf.span()).collect());
                self.import_item_completions(cursor, items, path.unwrap_or_default(), &source);
                return true;
            }
        }

        false
    }

    /// Add completions for all exports of a module.
    fn import_item_completions<'b>(
        &mut self,
        cursor: &mut Cursor<'b>,
        existing: ast::ImportItems<'b>,
        comps: Vec<ast::Ident>,
        source: &LinkedNode,
    ) {
        // Select the source by `comps`
        let value = self.ctx.module_by_syntax(source);
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
            self.snippet_completion("*", "*", "Import everything.");
        }

        for (name, value, _) in scope.iter() {
            if seen.iter().all(|item| item.as_str() != name) {
                self.value_completion(cursor, Some(name.clone()), value, false, None);
            }
        }
    }

    fn complete_path(
        &mut self,
        cursor: &mut Cursor,
        preference: &PathPreference,
    ) -> Option<Vec<CompletionItem>> {
        let id = cursor.source.id();
        if id.package().is_some() {
            return None;
        }

        let is_in_text;
        let text;
        let rng;
        // todo: the non-str case
        if cursor.leaf.is::<ast::Str>() {
            let vr = cursor.leaf.range();
            rng = vr.start + 1..vr.end - 1;
            if rng.start > rng.end || (cursor.cursor != rng.end && !rng.contains(&cursor.cursor)) {
                return None;
            }

            let mut w = EcoString::new();
            w.push('"');
            w.push_str(&cursor.text[rng.start..cursor.cursor]);
            w.push('"');
            let partial_str = SyntaxNode::leaf(SyntaxKind::Str, w);

            text = partial_str.cast::<ast::Str>()?.get();
            is_in_text = true;
        } else {
            text = EcoString::default();
            rng = cursor.cursor..cursor.cursor;
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
            log::warn!(
                "absolute path completion is not supported for security consideration {path:?}"
            );
            return None;
        }

        // find directory or files in the path
        let folder_completions = vec![];
        let mut module_completions = vec![];
        // todo: test it correctly
        for path in self.ctx.completion_files(preference) {
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

        let replace_range = cursor.to_lsp_range(rng);

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
                    LspCompletion {
                        label: typst_completion.0.to_string(),
                        kind: Some(typst_completion.1.into()),
                        detail: None,
                        text_edit: Some(text_edit),
                        // don't sort me
                        sort_text: Some(sort_text),
                        filter_text: Some("".to_owned()),
                        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                        ..Default::default()
                    }
                })
                .collect_vec(),
        )
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

            self.raw_completions.push(Completion {
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
        self.raw_completions.push(Completion {
            kind: CompletionKind::Syntax,
            label: label.into(),
            apply: Some(snippet.into()),
            detail: Some(docs.into()),
            command: self.ctx.analysis.trigger_on_snippet(snippet.contains("${")),
            ..Completion::default()
        });
    }

    /// Add completions for all font families.
    fn font_completions(&mut self, cursor: &mut Cursor) {
        let equation = cursor.before_window(25).contains("equation");
        for (family, iter) in self.world().clone().book().families() {
            let detail = summarize_font_family(iter);
            if !equation || family.contains("Math") {
                self.value_completion(
                    cursor,
                    None,
                    &Value::Str(family.into()),
                    false,
                    Some(detail.as_str()),
                );
            }
        }
    }

    /// Add completions for all available packages.
    fn package_completions(&mut self, cursor: &mut Cursor, all_versions: bool) {
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
                cursor,
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

            self.raw_completions.push(Completion {
                kind: CompletionKind::Constant,
                label: name.into(),
                apply: Some(tags[0].into()),
                detail: Some(repr::separated_list(&tags, " or ").into()),
                ..Completion::default()
            });
        }
    }

    /// Add completions for labels and references.
    fn ref_completions(&mut self, cursor: &mut Cursor) {
        self.label_completions_(cursor, false, true);
    }

    /// Add completions for labels and references.
    fn label_completions(&mut self, cursor: &mut Cursor, only_citation: bool) {
        self.label_completions_(cursor, only_citation, false);
    }

    /// Add completions for labels and references.
    fn label_completions_(&mut self, cursor: &mut Cursor, only_citation: bool, ref_label: bool) {
        let Some(document) = self.document else {
            return;
        };
        let (labels, split) = analyze_labels(document);

        let head = &cursor.text[..cursor.from];
        let at = head.ends_with('@');
        let open = !at && !head.ends_with('<');
        let close = !at && !cursor.after.starts_with('>');
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
                self.raw_completions.push(Completion {
                    kind: CompletionKind::Constant,
                    label: bib_title.clone(),
                    label_detail: Some(label),
                    filter_text: Some(bib_title),
                    detail,
                    ..completion.clone()
                });
            }

            self.raw_completions.push(completion);
        }
    }

    /// Add a completion for a specific value.
    fn value_completion(
        &mut self,
        cursor: &mut Cursor,
        label: Option<EcoString>,
        value: &Value,
        parens: bool,
        docs: Option<&str>,
    ) {
        self.value_completion_(
            cursor,
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
        cursor: &mut Cursor,
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
            if apply_label.ends_with('"') && cursor.after.starts_with('"') {
                if let Some(trimmed) = apply_label.strip_suffix('"') {
                    *apply_label = trimmed;
                }
            }
            let from_before = slice_at(cursor.text, 0..cursor.from);
            if apply_label.starts_with('"') && from_before.ends_with('"') {
                if let Some(trimmed) = apply_label.strip_prefix('"') {
                    *apply_label = trimmed;
                }
            }

            if apply_label.len() != label.len() {
                apply = Some((*apply_label).into());
            }
        }

        self.raw_completions.push(Completion {
            kind: value_to_completion_kind(value),
            label,
            apply,
            detail,
            label_detail,
            command,
            ..Completion::default()
        });
    }

    fn scope_defs(&mut self, cursor: &mut Cursor) -> Option<Defines> {
        let mut defines = Defines {
            types: self.ctx.type_check(&cursor.source),
            defines: Default::default(),
            docs: Default::default(),
        };

        let mode = interpret_mode_at(Some(&cursor.leaf));
        let in_math = matches!(mode, InterpretMode::Math);

        let lib = self.world().library();
        let scope = if in_math { &lib.math } else { &lib.global }
            .scope()
            .clone();
        defines.insert_scope(&scope);

        previous_decls(cursor.leaf.clone(), |node| -> Option<()> {
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

        Some(defines)
    }

    fn postfix_completions(
        &mut self,
        cursor: &mut Cursor,
        node: &LinkedNode,
        ty: Ty,
    ) -> Option<()> {
        if !self.ctx.analysis.completion_feat.postfix() {
            return None;
        }

        let _ = node;

        if !matches!(cursor.surrounding_syntax, SurroundingSyntax::Regular) {
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
                    range: cursor.to_lsp_range(rng.start..cursor.from),
                    new_text: String::new(),
                };

                self.raw_completions.push(Completion {
                    apply: Some(eco_format!(
                        "{node_before_before_cursor}{node_before}{node_content}{node_after}"
                    )),
                    additional_text_edits: Some(vec![before]),
                    ..base
                });
            } else {
                let before = TextEdit {
                    range: cursor.to_lsp_range(rng.start..rng.start),
                    new_text: node_before.as_ref().into(),
                };
                let after = TextEdit {
                    range: cursor.to_lsp_range(rng.end..cursor.from),
                    new_text: "".into(),
                };
                self.raw_completions.push(Completion {
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
    pub fn ufcs_completions(&mut self, cursor: &mut Cursor, node: &LinkedNode) {
        if !self.ctx.analysis.completion_feat.any_ufcs() {
            return;
        }

        if !matches!(cursor.surrounding_syntax, SurroundingSyntax::Regular) {
            return;
        }

        let Some(defines) = self.scope_defs(cursor) else {
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
                    range: cursor.to_lsp_range(rng.start..rng.start),
                    new_text: format!("{name}{lb}"),
                };
                let after = TextEdit {
                    range: cursor.to_lsp_range(rng.end..cursor.from),
                    new_text: rb.into(),
                };

                self.raw_completions.push(Completion {
                    label: name.clone(),
                    additional_text_edits: Some(vec![before, after]),
                    ..base.clone()
                });
            }
            let more_args = fn_feat.min_pos() > 1 || fn_feat.min_named() > 0;
            if self.ctx.analysis.completion_feat.ufcs_left() && more_args {
                let node_content = node.get().clone().into_text();
                let before = TextEdit {
                    range: cursor.to_lsp_range(rng.start..cursor.from),
                    new_text: format!("{name}{lb}"),
                };
                self.raw_completions.push(Completion {
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
                    range: cursor.to_lsp_range(rng.start..rng.start),
                    new_text: format!("{name}("),
                };
                let after = TextEdit {
                    range: cursor.to_lsp_range(rng.end..cursor.from),
                    new_text: "".into(),
                };
                self.raw_completions.push(Completion {
                    apply: Some(eco_format!("${{}})")),
                    label: eco_format!("{name})"),
                    additional_text_edits: Some(vec![before, after]),
                    ..base
                });
            }
        }
    }

    /// Add completions for definitions that are available at the cursor.
    pub fn scope_completions(&mut self, cursor: &mut Cursor, parens: bool) {
        let Some(defines) = self.scope_defs(cursor) else {
            return;
        };

        self.def_completions(cursor, defines, parens);
    }

    /// Add completions for definitions.
    fn def_completions(&mut self, cursor: &mut Cursor, defines: Defines, parens: bool) {
        let default_docs = defines.docs;
        let defines = defines.defines;

        let mode = interpret_mode_at(Some(&cursor.leaf));

        let mut kind_checker = CompletionKindChecker {
            symbols: HashSet::default(),
            functions: HashSet::default(),
        };

        let filter = |checker: &CompletionKindChecker| {
            match cursor.surrounding_syntax {
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
                self.raw_completions.push(Completion {
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

                if matches!(cursor.surrounding_syntax, SurroundingSyntax::ShowTransform)
                    && (fn_feat.min_pos() > 0 || fn_feat.min_named() > 0)
                {
                    self.raw_completions.push(Completion {
                        label: eco_format!("{}.with", name),
                        apply: Some(eco_format!("{}.with(${{}})", name)),
                        ..base.clone()
                    });
                }
                if fn_feat.is_element
                    && matches!(cursor.surrounding_syntax, SurroundingSyntax::Selector)
                {
                    self.raw_completions.push(Completion {
                        label: eco_format!("{}.where", name),
                        apply: Some(eco_format!("{}.where(${{}})", name)),
                        ..base.clone()
                    });
                }

                let bad_instantiate = matches!(
                    cursor.surrounding_syntax,
                    SurroundingSyntax::Selector | SurroundingSyntax::SetRule
                ) && !fn_feat.is_element;
                if !bad_instantiate {
                    if !parens || matches!(cursor.surrounding_syntax, SurroundingSyntax::Selector) {
                        self.raw_completions.push(Completion {
                            label: name,
                            ..base
                        });
                    } else if fn_feat.min_pos() < 1 && !fn_feat.has_rest {
                        self.raw_completions.push(Completion {
                            apply: Some(eco_format!("{}()${{}}", name)),
                            label: name,
                            ..base
                        });
                    } else {
                        let accept_content_arg = fn_feat.next_arg_is_content && !fn_feat.has_rest;
                        let scope_reject_content = matches!(mode, InterpretMode::Math)
                            || matches!(
                                cursor.surrounding_syntax,
                                SurroundingSyntax::Selector | SurroundingSyntax::SetRule
                            );
                        self.raw_completions.push(Completion {
                            apply: Some(eco_format!("{name}(${{}})")),
                            label: name.clone(),
                            ..base.clone()
                        });
                        if !scope_reject_content && accept_content_arg {
                            self.raw_completions.push(Completion {
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
            self.raw_completions.push(Completion {
                kind,
                label: name,
                label_detail: label_detail.clone(),
                detail,
                ..Completion::default()
            });
        }
    }
    /// Add completions for all fields on a node.
    fn field_access_completions(&mut self, cursor: &mut Cursor, target: &LinkedNode) -> Option<()> {
        self.value_field_access_completions(cursor, target)
            .or_else(|| self.type_field_access_completions(cursor, target))
    }

    /// Add completions for all fields on a type.
    fn type_field_access_completions(
        &mut self,
        cursor: &mut Cursor,
        target: &LinkedNode,
    ) -> Option<()> {
        let ty = self
            .ctx
            .post_type_of_node(target.clone())
            .filter(|ty| !matches!(ty, Ty::Any));
        crate::log_debug_ct!("type_field_access_completions_on: {target:?} -> {ty:?}");
        let mut defines = Defines {
            types: self.ctx.type_check(&cursor.source),
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

        self.def_completions(cursor, defines, true);
        Some(())
    }

    /// Add completions for all fields on a value.
    fn value_field_access_completions(
        &mut self,
        cursor: &mut Cursor,
        target: &LinkedNode,
    ) -> Option<()> {
        let (value, styles) = self.ctx.analyze_expr(target).into_iter().next()?;
        for (name, value, _) in value.ty().scope().iter() {
            self.value_completion(cursor, Some(name.clone()), value, true, None);
        }

        if let Some(scope) = value.scope() {
            for (name, value, _) in scope.iter() {
                self.value_completion(cursor, Some(name.clone()), value, true, None);
            }
        }

        for &field in fields_on(value.ty()) {
            // Complete the field name along with its value. Notes:
            // 1. No parentheses since function fields cannot currently be called
            // with method syntax;
            // 2. We can unwrap the field's value since it's a field belonging to
            // this value's type, so accessing it should not fail.
            self.value_completion(
                cursor,
                Some(field.into()),
                &value.field(field).unwrap(),
                false,
                None,
            );
        }

        self.postfix_completions(cursor, target, Ty::Value(InsTy::new(value.clone())));

        match value {
            Value::Symbol(symbol) => {
                for modifier in symbol.modifiers() {
                    if let Ok(modified) = symbol.clone().modified(modifier) {
                        self.raw_completions.push(Completion {
                            kind: CompletionKind::Symbol(modified.get()),
                            label: modifier.into(),
                            label_detail: Some(symbol_label_detail(modified.get())),
                            ..Completion::default()
                        });
                    }
                }

                self.ufcs_completions(cursor, target);
            }
            Value::Content(content) => {
                for (name, value) in content.fields() {
                    self.value_completion(cursor, Some(name.into()), &value, false, None);
                }

                self.ufcs_completions(cursor, target);
            }
            Value::Dict(dict) => {
                for (name, value) in dict.iter() {
                    self.value_completion(cursor, Some(name.clone().into()), value, false, None);
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
                                cursor,
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
                    self.raw_completions.push(Completion {
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

struct TypeCompletionWorker<'a, 'b> {
    base: &'a mut CompletionWorker<'b>,
    filter: &'a dyn Fn(&Ty) -> bool,
}

impl TypeCompletionWorker<'_, '_> {
    fn snippet_completion(&mut self, label: &str, apply: &str, detail: &str) {
        if !(self.filter)(&Ty::Any) {
            return;
        }

        self.base.snippet_completion(label, apply, detail);
    }

    fn type_completion(
        &mut self,
        cursor: &mut Cursor,
        infer_type: &Ty,
        docs: Option<&str>,
    ) -> Option<()> {
        // Prevent duplicate completions from appearing.
        if !self.base.seen_types.insert(infer_type.clone()) {
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
                    self.type_completion(cursor, info, docs);
                }
            }
            Ty::Let(bounds) => {
                for ut in bounds.ubs.iter() {
                    self.type_completion(cursor, ut, docs);
                }
                for lt in bounds.lbs.iter() {
                    self.type_completion(cursor, lt, docs);
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
                self.builtin_type_completion(cursor, v, docs);
            }
            Ty::Value(v) => {
                if !(self.filter)(infer_type) {
                    return None;
                }
                let docs = v.syntax.as_ref().map(|s| s.doc.as_ref()).or(docs);

                if let Value::Type(ty) = &v.val {
                    self.type_completion(cursor, &Ty::Builtin(BuiltinTy::Type(*ty)), docs);
                } else if v.val.ty() == Type::of::<NoneValue>() {
                    self.type_completion(cursor, &Ty::Builtin(BuiltinTy::None), docs);
                } else if v.val.ty() == Type::of::<AutoValue>() {
                    self.type_completion(cursor, &Ty::Builtin(BuiltinTy::Auto), docs);
                } else {
                    self.base.value_completion(cursor, None, &v.val, true, docs);
                }
            }
            Ty::Param(param) => {
                // todo: variadic

                let docs = docs.or_else(|| param.docs.as_deref());
                if param.attrs.positional {
                    self.type_completion(cursor, &param.ty, docs);
                }
                if !param.attrs.named {
                    return Some(());
                }

                let field = &param.name;
                if self.base.seen_field(field.clone()) {
                    return Some(());
                }
                if !(self.filter)(infer_type) {
                    return None;
                }

                let mut rev_stream = cursor.before.chars().rev();
                let ch = rev_stream.find(|ch| !typst::syntax::is_id_continue(*ch));
                // skip label/ref completion.
                // todo: more elegant way
                if matches!(ch, Some('<' | '@')) {
                    return Some(());
                }

                self.base.raw_completions.push(Completion {
                    kind: CompletionKind::Field,
                    label: field.into(),
                    apply: Some(eco_format!("{}: ${{}}", field)),
                    label_detail: param.ty.describe(),
                    detail: docs.map(Into::into),
                    command: self
                        .base
                        .ctx
                        .analysis
                        .trigger_on_snippet_with_param_hint(true),
                    ..Completion::default()
                });
            }
        };

        Some(())
    }

    fn builtin_type_completion(
        &mut self,
        cursor: &mut Cursor,
        v: &BuiltinTy,
        docs: Option<&str>,
    ) -> Option<()> {
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
                let items = self.base.complete_path(cursor, preference);
                self.base.completions.extend(items.into_iter().flatten());
            }
            BuiltinTy::Args => return None,
            BuiltinTy::Stroke => {
                self.snippet_completion("stroke()", "stroke(${})", "Stroke type.");
                self.snippet_completion("()", "(${})", "Stroke dictionary.");
                self.type_completion(cursor, &Ty::Builtin(BuiltinTy::Color), docs);
                self.type_completion(cursor, &Ty::Builtin(BuiltinTy::Length), docs);
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
                    self.base.raw_completions.push(Completion {
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
                    self.base.raw_completions.push(Completion {
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
                self.base.font_completions(cursor);
            }
            BuiltinTy::Margin => {
                self.snippet_completion("()", "(${})", "Margin dictionary.");
                self.type_completion(cursor, &Ty::Builtin(BuiltinTy::Length), docs);
            }
            BuiltinTy::Inset => {
                self.snippet_completion("()", "(${})", "Inset dictionary.");
                self.type_completion(cursor, &Ty::Builtin(BuiltinTy::Length), docs);
            }
            BuiltinTy::Outset => {
                self.snippet_completion("()", "(${})", "Outset dictionary.");
                self.type_completion(cursor, &Ty::Builtin(BuiltinTy::Length), docs);
            }
            BuiltinTy::Radius => {
                self.snippet_completion("()", "(${})", "Radius dictionary.");
                self.type_completion(cursor, &Ty::Builtin(BuiltinTy::Length), docs);
            }
            BuiltinTy::Length => {
                self.snippet_completion("pt", "${1}pt", "Point length unit.");
                self.snippet_completion("mm", "${1}mm", "Millimeter length unit.");
                self.snippet_completion("cm", "${1}cm", "Centimeter length unit.");
                self.snippet_completion("in", "${1}in", "Inch length unit.");
                self.snippet_completion("em", "${1}em", "Em length unit.");
                self.type_completion(cursor, &Ty::Builtin(BuiltinTy::Auto), docs);
            }
            BuiltinTy::Float => {
                self.snippet_completion(
                    "exponential notation",
                    "${1}e${0}",
                    "Exponential notation",
                );
            }
            BuiltinTy::Label => {
                self.base.label_completions(cursor, false);
            }
            BuiltinTy::CiteLabel => {
                self.base.label_completions(cursor, true);
            }
            BuiltinTy::RefLabel => {
                self.base.ref_completions(cursor);
            }
            BuiltinTy::TypeType(ty) | BuiltinTy::Type(ty) => {
                if *ty == Type::of::<NoneValue>() {
                    let docs = docs.or(Some("Nothing."));
                    self.type_completion(cursor, &Ty::Builtin(BuiltinTy::None), docs);
                } else if *ty == Type::of::<AutoValue>() {
                    let docs = docs.or(Some("A smart default."));
                    self.type_completion(cursor, &Ty::Builtin(BuiltinTy::Auto), docs);
                } else if *ty == Type::of::<bool>() {
                    self.snippet_completion("false", "false", "No / Disabled.");
                    self.snippet_completion("true", "true", "Yes / Enabled.");
                } else if *ty == Type::of::<Color>() {
                    self.type_completion(cursor, &Ty::Builtin(BuiltinTy::Color), docs);
                } else if *ty == Type::of::<Label>() {
                    self.base.label_completions(cursor, false)
                } else if *ty == Type::of::<Func>() {
                    self.snippet_completion(
                        "function",
                        "(${params}) => ${output}",
                        "A custom function.",
                    );
                } else {
                    self.base.raw_completions.push(Completion {
                        kind: CompletionKind::Syntax,
                        label: ty.short_name().into(),
                        apply: Some(eco_format!("${{{ty}}}")),
                        detail: Some(eco_format!("A value of type {ty}.")),
                        ..Completion::default()
                    });
                }
            }
            BuiltinTy::Element(elem) => {
                self.base.value_completion(
                    cursor,
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

fn ty_to_completion_kind(ty: &Ty) -> CompletionKind {
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

fn value_to_completion_kind(value: &Value) -> CompletionKind {
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
fn to_lsp_snippet(typst_snippet: &EcoString) -> String {
    let mut counter = 1;
    let result =
        TYPST_SNIPPET_PLACEHOLDER_RE.replace_all(typst_snippet.as_str(), |cap: &Captures| {
            let substitution = format!("${{{}:{}}}", counter, &cap[1]);
            counter += 1;
            substitution
        });

    result.to_string()
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
