//! Provides code actions for the document.

use ecow::{EcoString, eco_format};
use lsp_types::{ChangeAnnotation, CreateFile, CreateFileOptions};
use regex::Regex;
use tinymist_analysis::syntax::{
    ExprInfo, ModuleItemLayout, PreviousItem, SyntaxClass, adjust_expr, node_ancestors,
    previous_items,
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
    /// Cached expression information for the current source file.
    expr_info: Option<ExprInfo>,
}

impl<'a> CodeActionWorker<'a> {
    /// Creates a new color action worker.
    pub fn new(ctx: &'a mut LocalContext, source: Source) -> Self {
        Self {
            ctx,
            source,
            actions: Vec::new(),
            local_url: OnceLock::new(),
            expr_info: None,
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
            let Some(source) = diag.source.as_deref() else {
                continue;
            };

            let Some(diag_range) = self.ctx.to_typst_range(diag.range, &self.source) else {
                continue;
            };

            match match_autofix_kind(source, diag.message.as_str()) {
                Some(AutofixKind::UnknownVariable) => {
                    self.autofix_unknown_variable(root, &diag_range);
                }
                Some(AutofixKind::FileNotFound) => {
                    self.autofix_file_not_found(root, &diag_range);
                }
                Some(AutofixKind::MarkUnusedSymbol) => {
                    if diag.message.starts_with("unused import:") {
                        self.autofix_remove_unused_import(root, &diag_range);
                    } else if diag.message.starts_with("unused module:") {
                        self.autofix_remove_declaration(root, &diag_range);
                    } else {
                        let Some(binding_range) =
                            self.binding_range_for_diag(root, &diag_range, diag)
                        else {
                            continue;
                        };

                        self.autofix_unused_symbol(&binding_range);
                        self.autofix_replace_with_placeholder(root, &binding_range);
                        self.autofix_remove_declaration(root, &binding_range);
                    }
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

    /// Prefix unused bindings with `_` to silence dead-code lint diagnostics.
    fn autofix_unused_symbol(&mut self, range: &Range<usize>) -> Option<()> {
        if range.is_empty() {
            return None;
        }

        let name = self.source.text().get(range.clone())?;
        if !is_plain_identifier(name) || name.starts_with('_') {
            return None;
        }

        let replacement = eco_format!("_{name}");
        let lsp_range = self.ctx.to_lsp_range(range.clone(), &self.source);
        let edit = self.local_edit(EcoSnippetTextEdit::new_plain(lsp_range, replacement))?;
        let action = CodeAction {
            title: format!("Prefix `_` to `{name}`"),
            kind: Some(CodeActionKind::QUICKFIX),
            edit: Some(edit),
            ..CodeAction::default()
        };
        self.actions.push(action);

        Some(())
    }

    fn autofix_replace_with_placeholder(
        &mut self,
        root: &LinkedNode<'_>,
        range: &Range<usize>,
    ) -> Option<()> {
        if range.is_empty() {
            return None;
        }

        let cursor = (range.start + range.end) / 2;
        let node = root.leaf_at_compat(cursor)?;

        if self.is_spread_binding(&node) || self.is_function_binding(&node) {
            return None;
        }

        let name = self.source.text().get(range.clone())?;
        if !is_plain_identifier(name) {
            return None;
        }

        let lsp_range = self.ctx.to_lsp_range(range.clone(), &self.source);
        let edit = self.local_edit(EcoSnippetTextEdit::new_plain(
            lsp_range,
            EcoString::from("_"),
        ))?;
        let action = CodeAction {
            title: "Replace with `_`".to_string(),
            kind: Some(CodeActionKind::QUICKFIX),
            edit: Some(edit),
            ..CodeAction::default()
        };
        self.actions.push(action);

        Some(())
    }

    fn is_spread_binding(&self, node: &LinkedNode<'_>) -> bool {
        if node.kind() == SyntaxKind::Spread {
            return true;
        }

        node_ancestors(node).any(|ancestor| ancestor.kind() == SyntaxKind::Spread)
    }

    fn is_function_binding(&self, node: &LinkedNode<'_>) -> bool {
        self.find_let_binding_ancestor(node)
            .map(|let_binding| matches!(let_binding.kind(), ast::LetBindingKind::Closure(..)))
            .unwrap_or(false)
    }

    fn find_let_binding_ancestor<'b>(
        &self,
        node: &'b LinkedNode<'b>,
    ) -> Option<ast::LetBinding<'b>> {
        let mut current = Some(node);
        while let Some(n) = current {
            if n.kind() == SyntaxKind::LetBinding {
                return n.cast::<ast::LetBinding>();
            }
            current = n.parent();
        }
        None
    }

    fn find_declaration_ancestor<'b>(
        &self,
        node: &'b LinkedNode<'b>,
    ) -> Option<&'b LinkedNode<'b>> {
        let mut current = Some(node);
        while let Some(n) = current {
            match n.kind() {
                SyntaxKind::LetBinding | SyntaxKind::ModuleImport | SyntaxKind::ModuleInclude => {
                    return Some(n);
                }
                _ => {
                    current = n.parent();
                }
            }
        }
        None
    }

    fn expand_declaration_range(&self, mut range: Range<usize>) -> Range<usize> {
        let bytes = self.source.text().as_bytes();

        if range.start > 0 {
            let mut idx = range.start;
            while idx > 0 && matches!(bytes[idx - 1], b' ' | b'\t') {
                idx -= 1;
            }

            if idx > 0 && bytes[idx - 1] == b'#' {
                range.start = idx - 1;
            }
        }

        if range.end < bytes.len() && bytes[range.end] == b'\n' {
            range.end += 1;
        } else if range.start > 0 && bytes[range.start - 1] == b'\n' {
            range.start -= 1;
        }

        range
    }

    /// Remove the declaration corresponding to an unused binding.
    fn autofix_remove_declaration(
        &mut self,
        root: &LinkedNode<'_>,
        name_range: &Range<usize>,
    ) -> Option<()> {
        if name_range.is_empty() {
            return None;
        }

        let cursor = (name_range.start + name_range.end) / 2;
        let node = root.leaf_at_compat(cursor)?;
        let decl_node = self.find_declaration_ancestor(&node)?;

        if decl_node.kind() == SyntaxKind::LetBinding {
            let let_binding = decl_node.cast::<ast::LetBinding>()?;
            let bindings = let_binding.kind().bindings();

            // remove declarations only when the let binding introduces a single identifier
            // that corresponds to the unused diagnostic.
            if bindings.len() != 1 {
                return None;
            }

            let binding_ident = bindings.first()?;
            let binding_node = decl_node.find(binding_ident.span())?;
            if binding_node.range() != *name_range {
                return None;
            }
        }

        let remove_range = self.expand_declaration_range(decl_node.range());

        let lsp_range = self.ctx.to_lsp_range(remove_range.clone(), &self.source);
        let edit = self.local_edit(EcoSnippetTextEdit::new_plain(lsp_range, EcoString::new()))?;
        let action = CodeAction {
            title: "Remove unused declaration".to_string(),
            kind: Some(CodeActionKind::QUICKFIX),
            edit: Some(edit),
            ..CodeAction::default()
        };
        self.actions.push(action);

        Some(())
    }

    /// Remove an unused import item, handling trailing commas.
    fn autofix_remove_unused_import(
        &mut self,
        root: &LinkedNode<'_>,
        name_range: &Range<usize>,
    ) -> Option<()> {
        // Calculate the range to remove, expand to cover the whole import item
        // (e.g. `foo as bar`) and include trailing comma if present.
        let mut remove_range = if let Some(layout) = self.module_item_layout_for_range(name_range) {
            layout.item_range.clone()
        } else {
            self.module_alias_remove_range(root, name_range)
                .or_else(|| self.find_import_item_range(root, name_range))
                .unwrap_or_else(|| name_range.clone())
        };
        remove_range = self.expand_import_item_range(remove_range);

        let lsp_range = self.ctx.to_lsp_range(remove_range.clone(), &self.source);
        let edit = self.local_edit(EcoSnippetTextEdit::new_plain(lsp_range, EcoString::new()))?;
        let action = CodeAction {
            title: "Remove unused import".to_string(),
            kind: Some(CodeActionKind::QUICKFIX),
            edit: Some(edit),
            ..CodeAction::default()
        };
        self.actions.push(action);

        Some(())
    }

    fn module_item_layout_for_range(
        &mut self,
        binding_range: &Range<usize>,
    ) -> Option<ModuleItemLayout> {
        let info = self.expr_info()?;
        info.module_items
            .values()
            .find(|layout| layout.binding_range == *binding_range)
            .cloned()
    }

    fn binding_range_for_diag(
        &mut self,
        root: &LinkedNode<'_>,
        diag_range: &Range<usize>,
        diag: &lsp_types::Diagnostic,
    ) -> Option<Range<usize>> {
        if diag_range.is_empty() {
            return None;
        }

        if let Some(text) = self.source.text().get(diag_range.clone()) {
            if is_plain_identifier(text) {
                return Some(diag_range.clone());
            }
        }

        let name = extract_backticked_name(&diag.message)?;
        let cursor = (diag_range.start + 1).min(self.source.text().len());
        let node = root.leaf_at_compat(cursor)?;
        let decl_node = self.find_declaration_ancestor(&node)?;

        if decl_node.kind() == SyntaxKind::LetBinding {
            let let_binding = decl_node.cast::<ast::LetBinding>()?;
            for binding in let_binding.kind().bindings() {
                let binding_node = decl_node.find(binding.span())?;
                let range = binding_node.range();
                if self.source.text().get(range.clone())? == name {
                    return Some(range);
                }
            }
        }

        None
    }

    fn expand_import_item_range(&self, mut range: Range<usize>) -> Range<usize> {
        let bytes = self.source.text().as_bytes();
        let len = bytes.len();

        let mut idx = range.end;
        while idx < len && matches!(bytes[idx], b' ' | b'\t' | b'\r' | b'\n') {
            idx += 1;
        }
        if idx < len && bytes[idx] == b',' {
            range.end = idx + 1;
            let mut tail = range.end;
            while tail < len && matches!(bytes[tail], b' ' | b'\t') {
                tail += 1;
            }
            range.end = tail;
            return range;
        }

        let mut idx = range.start;
        while idx > 0 && matches!(bytes[idx - 1], b' ' | b'\t' | b'\r' | b'\n') {
            idx -= 1;
        }
        if idx > 0 && bytes[idx - 1] == b',' {
            range.start = idx - 1;
            while range.start > 0 && matches!(bytes[range.start - 1], b' ' | b'\t') {
                range.start -= 1;
            }
        }

        range
    }

    fn expr_info(&mut self) -> Option<ExprInfo> {
        if self.expr_info.is_none() {
            let info = self.ctx.expr_stage(&self.source);
            self.expr_info = Some(info);
        }
        self.expr_info.clone()
    }

    fn find_import_item_range(
        &self,
        root: &LinkedNode<'_>,
        name_range: &Range<usize>,
    ) -> Option<Range<usize>> {
        if name_range.is_empty() {
            return None;
        }

        let cursor = (name_range.start + name_range.end) / 2;
        let node = root.leaf_at_compat(cursor)?;

        node_ancestors(&node).find_map(|ancestor| match ancestor.kind() {
            SyntaxKind::RenamedImportItem => Some(ancestor.range()),
            _ => None,
        })
    }

    fn module_alias_remove_range(
        &self,
        root: &LinkedNode<'_>,
        name_range: &Range<usize>,
    ) -> Option<Range<usize>> {
        if name_range.is_empty() {
            return None;
        }

        let cursor = (name_range.start + name_range.end) / 2;
        let node = root.leaf_at_compat(cursor)?;

        let mut in_module_import = false;
        for ancestor in node_ancestors(&node) {
            match ancestor.kind() {
                SyntaxKind::RenamedImportItem => return None,
                SyntaxKind::ModuleImport => {
                    in_module_import = true;
                    break;
                }
                _ => {}
            }
        }

        if !in_module_import {
            return None;
        }

        let bytes = self.source.text().as_bytes();
        if name_range.end > bytes.len() || name_range.start > bytes.len() {
            return None;
        }

        let mut idx = name_range.start;
        while idx > 0 && matches!(bytes[idx - 1], b' ' | b'\t') {
            idx -= 1;
        }

        if idx < 2 {
            return None;
        }

        let as_end = idx;
        let as_start = as_end - 2;
        if &bytes[as_start..as_end] != b"as" {
            return None;
        }

        if as_start > 0 && is_ascii_ident(bytes[as_start - 1]) {
            return None;
        }
        if as_end < bytes.len() && is_ascii_ident(bytes[as_end]) {
            return None;
        }

        let mut removal_start = as_start;
        while removal_start > 0 && matches!(bytes[removal_start - 1], b' ' | b'\t') {
            removal_start -= 1;
        }

        let mut removal_end = name_range.end;
        while removal_end < bytes.len() && matches!(bytes[removal_end], b' ' | b'\t') {
            removal_end += 1;
        }

        Some(removal_start..removal_end)
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
        if let Some(link_info) = get_link_exprs_in(link_parent) {
            let objects = link_info.objects.into_iter();
            let object_under_node = objects.filter(|link| link.range.contains(&cursor));

            let mut resolved = false;
            for link in object_under_node {
                if let LinkTarget::Path(id, path) = link.target {
                    // todo: is there a link that is not a path string?
                    resolved = self.path_rewrite(id, &path, node).is_some() || resolved;
                }
            }

            return resolved.then_some(());
        }

        None
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
    MarkUnusedSymbol,
}

fn match_autofix_kind(source: &str, msg: &str) -> Option<AutofixKind> {
    if msg.starts_with("unused ") {
        return Some(AutofixKind::MarkUnusedSymbol);
    }

    if source == "typst" {
        static PATTERNS: &[(&str, AutofixKind)] = &[
            ("unknown variable", AutofixKind::UnknownVariable),
            ("file not found", AutofixKind::FileNotFound),
        ];

        for (pattern, kind) in PATTERNS {
            if msg.starts_with(pattern) {
                return Some(*kind);
            }
        }
    }

    None
}

fn extract_backticked_name(message: &str) -> Option<&str> {
    let start = message.find('`')?;
    let rest = &message[start + 1..];
    let end = rest.find('`')?;
    Some(&rest[..end])
}

fn is_ascii_ident(ch: u8) -> bool {
    matches!(ch, b'_' | b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9')
}

fn is_plain_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }

    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}
