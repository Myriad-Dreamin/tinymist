//! Provides code actions for the document.

use ecow::eco_format;
use lsp_types::{ChangeAnnotation, CreateFile, CreateFileOptions};
use regex::Regex;
use tinymist_analysis::syntax::{
    PreviousItem, SyntaxClass, adjust_expr, node_ancestors, previous_items,
};
use tinymist_std::path::{diff, unix_slash};
use typst::syntax::Side;

use super::get_link_exprs_in;
use crate::analysis::LinkTarget;
use crate::prelude::*;
use crate::syntax::{InterpretMode, interpret_mode_at};

/// Analyzes the document and provides code actions.
pub struct CodeActionWorker<'a> {
    /// The local analysis context to work with.
    ctx: &'a mut LocalContext,
    /// The source document to analyze.
    source: Source,
    /// The code actions to provide.
    pub actions: Vec<CodeAction>,
    /// The lazily calculated local URL to [`Self::source`].
    local_url: OnceLock<Option<Url>>,
}

impl<'a> CodeActionWorker<'a> {
    /// Creates a new color action worker.
    pub fn new(ctx: &'a mut LocalContext, source: Source) -> Self {
        Self {
            ctx,
            source,
            actions: Vec::new(),
            local_url: OnceLock::new(),
        }
    }

    fn local_url(&self) -> Option<&Url> {
        self.local_url
            .get_or_init(|| self.ctx.uri_for_id(self.source.id()).ok())
            .as_ref()
    }

    #[must_use]
    fn local_edits(&self, edits: Vec<EcoSnippetTextEdit>) -> Option<EcoWorkspaceEdit> {
        Some(EcoWorkspaceEdit {
            changes: Some(HashMap::from_iter([(self.local_url()?.clone(), edits)])),
            ..Default::default()
        })
    }

    #[must_use]
    fn local_edit(&self, edit: EcoSnippetTextEdit) -> Option<EcoWorkspaceEdit> {
        self.local_edits(vec![edit])
    }

    pub(crate) fn autofix(
        &mut self,
        root: &LinkedNode<'_>,
        range: &Range<usize>,
        context: &lsp_types::CodeActionContext,
    ) -> Option<()> {
        if let Some(only) = &context.only
            && !only.is_empty()
            && !only
                .iter()
                .any(|kind| *kind == CodeActionKind::EMPTY || *kind == CodeActionKind::QUICKFIX)
        {
            return None;
        }

        for diag in &context.diagnostics {
            if diag.source.as_ref().is_none_or(|t| t != "typst") {
                continue;
            }

            match match_autofix_kind(diag.message.as_str()) {
                Some(AutofixKind::UnknownVariable) => {
                    self.autofix_unknown_variable(root, range);
                }
                Some(AutofixKind::FileNotFound) => {
                    self.autofix_file_not_found(root, range);
                }
                _ => {}
            }
        }

        Some(())
    }

    /// Automatically fixes unknown variable errors.
    pub fn autofix_unknown_variable(
        &mut self,
        root: &LinkedNode,
        range: &Range<usize>,
    ) -> Option<()> {
        let cursor = (range.start + 1).min(self.source.text().len());
        let node = root.leaf_at_compat(cursor)?;
        self.create_missing_variable(root, &node);
        self.add_spaces_to_math_unknown_variable(&node);
        Some(())
    }

    fn create_missing_variable(
        &mut self,
        root: &LinkedNode<'_>,
        node: &LinkedNode<'_>,
    ) -> Option<()> {
        let ident = 'determine_ident: {
            if let Some(ident) = node.cast::<ast::Ident>() {
                break 'determine_ident ident.get().clone();
            }
            if let Some(ident) = node.cast::<ast::MathIdent>() {
                break 'determine_ident ident.get().clone();
            }

            return None;
        };

        enum CreatePosition {
            Before(usize),
            After(usize),
            Bad,
        }

        let previous_decl = previous_items(node.clone(), |item| {
            match item {
                PreviousItem::Parent(parent, ..) => match parent.kind() {
                    SyntaxKind::LetBinding => {
                        let mut create_before = parent.clone();
                        while let Some(before) = create_before.prev_sibling() {
                            if matches!(before.kind(), SyntaxKind::Hash) {
                                create_before = before;
                                continue;
                            }

                            break;
                        }

                        return Some(CreatePosition::Before(create_before.range().start));
                    }
                    SyntaxKind::CodeBlock | SyntaxKind::ContentBlock => {
                        let child = parent.children().find(|child| {
                            matches!(
                                child.kind(),
                                SyntaxKind::LeftBrace | SyntaxKind::LeftBracket
                            )
                        })?;

                        return Some(CreatePosition::After(child.range().end));
                    }
                    SyntaxKind::ModuleImport | SyntaxKind::ModuleInclude => {
                        return Some(CreatePosition::Bad);
                    }
                    _ => {}
                },
                PreviousItem::Sibling(node) => {
                    if matches!(
                        node.kind(),
                        SyntaxKind::ModuleImport | SyntaxKind::ModuleInclude
                    ) {
                        // todo: hash
                        return Some(CreatePosition::After(node.range().end));
                    }
                }
            }

            None
        });

        let (create_pos, side) = match previous_decl {
            Some(CreatePosition::Before(pos)) => (pos, Side::Before),
            Some(CreatePosition::After(pos)) => (pos, Side::After),
            None => (0, Side::After),
            Some(CreatePosition::Bad) => return None,
        };

        let pos_node = root.leaf_at(create_pos, side.clone());
        let mode = match interpret_mode_at(pos_node.as_ref()) {
            InterpretMode::Markup => "#",
            _ => "",
        };

        let extend_assign = if self.ctx.analysis.extended_code_action {
            " = ${1:none}$0"
        } else {
            ""
        };
        let new_text = if matches!(side, Side::Before) {
            eco_format!("{mode}let {ident}{extend_assign}\n\n")
        } else {
            eco_format!("\n\n{mode}let {ident}{extend_assign}")
        };

        let range = self.ctx.to_lsp_range(create_pos..create_pos, &self.source);
        let edit = self.local_edit(EcoSnippetTextEdit::new(range, new_text))?;
        let action = CodeAction {
            title: "Create missing variable".to_string(),
            kind: Some(CodeActionKind::QUICKFIX),
            edit: Some(edit),
            ..CodeAction::default()
        };
        self.actions.push(action);
        Some(())
    }

    /// Add spaces between letters in an unknown math identifier: `$xyz$` -> `$x
    /// y z$`.
    fn add_spaces_to_math_unknown_variable(&mut self, node: &LinkedNode<'_>) -> Option<()> {
        let ident = node.cast::<ast::MathIdent>()?.get();

        // Rewrite `a_ij` as `a_(i j)`, not `a_i j`.
        // Likewise rewrite `ab/c` as `(a b)/c`, not `a b/c`.
        let needs_parens = matches!(
            node.parent_kind(),
            Some(SyntaxKind::MathAttach | SyntaxKind::MathFrac)
        );
        let new_text = if needs_parens {
            eco_format!("({})", ident.chars().join(" "))
        } else {
            ident.chars().join(" ").into()
        };

        let range = self.ctx.to_lsp_range(node.range(), &self.source);
        let edit = self.local_edit(EcoSnippetTextEdit::new(range, new_text))?;
        let action = CodeAction {
            title: "Add spaces between letters".to_string(),
            kind: Some(CodeActionKind::QUICKFIX),
            edit: Some(edit),
            ..CodeAction::default()
        };
        self.actions.push(action);
        Some(())
    }

    /// Automatically fixes file not found errors.
    pub fn autofix_file_not_found(
        &mut self,
        root: &LinkedNode,
        range: &Range<usize>,
    ) -> Option<()> {
        let cursor = (range.start + 1).min(self.source.text().len());
        let node = root.leaf_at_compat(cursor)?;

        let importing = node.cast::<ast::Str>()?.get();
        if importing.starts_with('@') {
            // todo: create local package?
            // if importing.starts_with("@local") { return None; }

            // This is a package import, not a file import.
            return None;
        }

        let file_id = node.span().id()?;
        let root_path = self.ctx.path_for_id(file_id.join("/")).ok()?;
        let path_in_workspace = file_id.vpath().join(importing.as_str());
        let new_path = path_in_workspace.resolve(root_path.as_path())?;
        let new_file_url = path_to_url(&new_path).ok()?;

        let edit = self.create_file(new_file_url, false);

        let file_to_create = unix_slash(path_in_workspace.as_rooted_path());
        let action = CodeAction {
            title: format!("Create missing file at `{file_to_create}`"),
            kind: Some(CodeActionKind::QUICKFIX),
            edit: Some(edit),
            ..CodeAction::default()
        };
        self.actions.push(action);

        Some(())
    }

    /// Starts to work.
    pub fn scoped(&mut self, root: &LinkedNode, range: &Range<usize>) -> Option<()> {
        let cursor = (range.start + 1).min(self.source.text().len());
        let node = root.leaf_at_compat(cursor)?;
        let mut node = &node;

        let mut heading_resolved = false;
        let mut equation_resolved = false;
        let mut path_resolved = false;

        self.wrap_actions(node, range);

        loop {
            match node.kind() {
                // Only the deepest heading is considered
                SyntaxKind::Heading if !heading_resolved => {
                    heading_resolved = true;
                    self.heading_actions(node);
                }
                // Only the deepest equation is considered
                SyntaxKind::Equation if !equation_resolved => {
                    equation_resolved = true;
                    self.equation_actions(node);
                }
                SyntaxKind::Str if !path_resolved => {
                    path_resolved = true;
                    self.path_actions(node, cursor);
                }
                _ => {}
            }

            node = node.parent()?;
        }
    }

    fn path_actions(&mut self, node: &LinkedNode, cursor: usize) -> Option<()> {
        // We can only process the case where the import path is a string.
        if let Some(SyntaxClass::IncludePath(path_node) | SyntaxClass::ImportPath(path_node)) =
            classify_syntax(node.clone(), cursor)
        {
            let str_node = adjust_expr(path_node)?;
            let str_ast = str_node.cast::<ast::Str>()?;
            return self.path_rewrite(self.source.id(), &str_ast.get(), &str_node);
        }

        let link_parent = node_ancestors(node)
            .find(|node| matches!(node.kind(), SyntaxKind::FuncCall))
            .unwrap_or(node);

        // Actually there should be only one link left
        let link_info = get_link_exprs_in(link_parent);
        let objects = link_info.objects.into_iter();
        let object_under_node = objects.filter(|link| link.range.contains(&cursor));

        let mut resolved = false;
        for link in object_under_node {
            if let LinkTarget::Path(id, path) = link.target {
                // todo: is there a link that is not a path string?
                resolved = self.path_rewrite(id, &path, node).is_some() || resolved;
            }
        }

        resolved.then_some(())
    }

    /// Rewrites absolute paths from/to relative paths.
    fn path_rewrite(&mut self, id: TypstFileId, path: &str, node: &LinkedNode) -> Option<()> {
        if !matches!(node.kind(), SyntaxKind::Str) {
            log::warn!("bad path node kind on code action: {:?}", node.kind());
            return None;
        }

        let path = Path::new(path);

        if path.starts_with("/") {
            // Convert absolute path to relative path
            let cur_path = id.vpath().as_rooted_path().parent().unwrap();
            let new_path = diff(path, cur_path)?;
            let edit = self.edit_str(node, unix_slash(&new_path))?;
            let action = CodeAction {
                title: "Convert to relative path".to_string(),
                kind: Some(CodeActionKind::REFACTOR_REWRITE),
                edit: Some(edit),
                ..CodeAction::default()
            };
            self.actions.push(action);
        } else {
            // Convert relative path to absolute path
            let mut new_path = id.vpath().as_rooted_path().parent().unwrap().to_path_buf();
            for i in path.components() {
                match i {
                    std::path::Component::ParentDir => {
                        new_path.pop().then_some(())?;
                    }
                    std::path::Component::Normal(name) => {
                        new_path.push(name);
                    }
                    _ => {}
                }
            }
            let edit = self.edit_str(node, unix_slash(&new_path))?;
            let action = CodeAction {
                title: "Convert to absolute path".to_string(),
                kind: Some(CodeActionKind::REFACTOR_REWRITE),
                edit: Some(edit),
                ..CodeAction::default()
            };
            self.actions.push(action);
        }

        Some(())
    }

    fn edit_str(&mut self, node: &LinkedNode, new_content: String) -> Option<EcoWorkspaceEdit> {
        if !matches!(node.kind(), SyntaxKind::Str) {
            log::warn!("edit_str only works on string AST nodes: {:?}", node.kind());
            return None;
        }

        self.local_edit(EcoSnippetTextEdit::new_plain(
            self.ctx.to_lsp_range(node.range(), &self.source),
            // todo: this is merely occasionally correct, abusing string escape (`fmt::Debug`)
            eco_format!("{new_content:?}"),
        ))
    }

    fn wrap_actions(&mut self, node: &LinkedNode, range: &Range<usize>) -> Option<()> {
        if range.is_empty() {
            return None;
        }

        let start_mode = interpret_mode_at(Some(node));
        if !matches!(start_mode, InterpretMode::Markup | InterpretMode::Math) {
            return None;
        }

        let edit = self.local_edits(vec![
            EcoSnippetTextEdit::new_plain(
                self.ctx
                    .to_lsp_range(range.start..range.start, &self.source),
                EcoString::inline("#["),
            ),
            EcoSnippetTextEdit::new_plain(
                self.ctx.to_lsp_range(range.end..range.end, &self.source),
                EcoString::inline("]"),
            ),
        ])?;

        let action = CodeAction {
            title: "Wrap with content block".to_string(),
            kind: Some(CodeActionKind::REFACTOR_REWRITE),
            edit: Some(edit),
            ..CodeAction::default()
        };
        self.actions.push(action);

        Some(())
    }

    fn heading_actions(&mut self, node: &LinkedNode) -> Option<()> {
        let heading = node.cast::<ast::Heading>()?;
        let depth = heading.depth().get();

        // Only the marker is replaced, for minimal text change
        let marker = node
            .children()
            .find(|child| child.kind() == SyntaxKind::HeadingMarker)?;
        let marker_range = marker.range();

        if depth > 1 {
            // Decrease depth of heading
            let action = CodeAction {
                title: "Decrease depth of heading".to_string(),
                kind: Some(CodeActionKind::REFACTOR_REWRITE),
                edit: Some(self.local_edit(EcoSnippetTextEdit::new_plain(
                    self.ctx.to_lsp_range(marker_range.clone(), &self.source),
                    EcoString::inline("=").repeat(depth - 1),
                ))?),
                ..CodeAction::default()
            };
            self.actions.push(action);
        }

        // Increase depth of heading
        let action = CodeAction {
            title: "Increase depth of heading".to_string(),
            kind: Some(CodeActionKind::REFACTOR_REWRITE),
            edit: Some(self.local_edit(EcoSnippetTextEdit::new_plain(
                self.ctx.to_lsp_range(marker_range, &self.source),
                EcoString::inline("=").repeat(depth + 1),
            ))?),
            ..CodeAction::default()
        };
        self.actions.push(action);

        Some(())
    }

    fn equation_actions(&mut self, node: &LinkedNode) -> Option<()> {
        let equation = node.cast::<ast::Equation>()?;
        let body = equation.body();
        let is_block = equation.block();

        let body = node.find(body.span())?;
        let body_range = body.range();
        let node_end = node.range().end;

        let mut chs = node.children();
        let chs = chs.by_ref();
        let is_dollar = |node: &LinkedNode| node.kind() == SyntaxKind::Dollar;
        let first_dollar = chs.take(1).find(is_dollar)?;
        let last_dollar = chs.rev().take(1).find(is_dollar)?;

        // Erroneous equation is skipped.
        // For example, some unclosed equation.
        if first_dollar.offset() == last_dollar.offset() {
            return None;
        }

        let front_range = self
            .ctx
            .to_lsp_range(first_dollar.range().end..body_range.start, &self.source);
        let back_range = self
            .ctx
            .to_lsp_range(body_range.end..last_dollar.range().start, &self.source);

        // Retrieve punctuation to move
        let mark_after_equation = self
            .source
            .text()
            .get(node_end..)
            .and_then(|text| {
                let mut ch = text.chars();
                let nx = ch.next()?;
                Some((nx, ch.next()))
            })
            .filter(|(ch, ch_next)| {
                static IS_PUNCTUATION: LazyLock<Regex> =
                    LazyLock::new(|| Regex::new(r"\p{Punctuation}").unwrap());
                (ch.is_ascii_punctuation()
                    && ch_next.is_none_or(|ch_next| !ch_next.is_ascii_punctuation()))
                    || (!ch.is_ascii_punctuation() && IS_PUNCTUATION.is_match(&ch.to_string()))
            });
        let punc_modify = if let Some((nx, _)) = mark_after_equation {
            let ch_range = self
                .ctx
                .to_lsp_range(node_end..node_end + nx.len_utf8(), &self.source);
            let remove_edit = EcoSnippetTextEdit::new_plain(ch_range, EcoString::new());
            Some((nx, remove_edit))
        } else {
            None
        };

        let rewrite_action = |title: &str, new_text: &str| {
            let mut edits = vec![
                EcoSnippetTextEdit::new_plain(front_range, new_text.into()),
                EcoSnippetTextEdit::new_plain(
                    back_range,
                    if !new_text.is_empty() {
                        if let Some((ch, _)) = &punc_modify {
                            EcoString::from(*ch) + new_text
                        } else {
                            new_text.into()
                        }
                    } else {
                        EcoString::new()
                    },
                ),
            ];

            if !new_text.is_empty()
                && let Some((_, edit)) = &punc_modify
            {
                edits.push(edit.clone());
            }

            Some(CodeAction {
                title: title.to_owned(),
                kind: Some(CodeActionKind::REFACTOR_REWRITE),
                edit: Some(self.local_edits(edits)?),
                ..CodeAction::default()
            })
        };

        // Prepare actions
        let toggle_action = if is_block {
            rewrite_action("Convert to inline equation", "")?
        } else {
            rewrite_action("Convert to block equation", " ")?
        };
        let block_action = rewrite_action("Convert to multiple-line block equation", "\n");

        self.actions.push(toggle_action);
        if let Some(a2) = block_action {
            self.actions.push(a2);
        }

        Some(())
    }

    fn create_file(&self, uri: Url, needs_confirmation: bool) -> EcoWorkspaceEdit {
        let change_id = "Typst Create Missing Files".to_string();

        let create_op = EcoDocumentChangeOperation::Op(lsp_types::ResourceOp::Create(CreateFile {
            uri,
            options: Some(CreateFileOptions {
                overwrite: Some(false),
                ignore_if_exists: None,
            }),
            annotation_id: Some(change_id.clone()),
        }));

        let mut change_annotations = HashMap::new();
        change_annotations.insert(
            change_id.clone(),
            ChangeAnnotation {
                label: change_id,
                needs_confirmation: Some(needs_confirmation),
                description: Some("The file is missing but required by code".to_string()),
            },
        );

        EcoWorkspaceEdit {
            changes: None,
            document_changes: Some(EcoDocumentChanges::Operations(vec![create_op])),
            change_annotations: Some(change_annotations),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum AutofixKind {
    UnknownVariable,
    FileNotFound,
}

fn match_autofix_kind(msg: &str) -> Option<AutofixKind> {
    static PATTERNS: &[(&str, AutofixKind)] = &[
        ("unknown variable", AutofixKind::UnknownVariable), // typst compiler error
        ("file not found", AutofixKind::FileNotFound),
    ];

    for (pattern, kind) in PATTERNS {
        if msg.starts_with(pattern) {
            return Some(*kind);
        }
    }

    None
}
