//! Provides code actions for the document.

use regex::Regex;
use tinymist_analysis::syntax::{adjust_expr, node_ancestors, SyntaxClass};

use crate::analysis::LinkTarget;
use crate::prelude::*;
use crate::syntax::{interpret_mode_at, InterpretMode};

use super::get_link_exprs_in;

/// Analyzes the document and provides code actions.
pub struct CodeActionWorker<'a> {
    /// The local analysis context to work with.
    ctx: &'a mut LocalContext,
    /// The source document to analyze.
    source: Source,
    /// The code actions to provide.
    pub actions: Vec<CodeActionOrCommand>,
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
    fn local_edits(&self, edits: Vec<TextEdit>) -> Option<WorkspaceEdit> {
        Some(WorkspaceEdit {
            changes: Some(HashMap::from_iter([(self.local_url()?.clone(), edits)])),
            ..Default::default()
        })
    }

    #[must_use]
    fn local_edit(&self, edit: TextEdit) -> Option<WorkspaceEdit> {
        self.local_edits(vec![edit])
    }

    /// Starts to work.
    pub fn work(&mut self, root: LinkedNode, range: Range<usize>) -> Option<()> {
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
        // We can only process on the cases that import path is a string.
        if let Some(SyntaxClass::ImportPath(path_node)) = classify_syntax(node.clone(), cursor) {
            let str_node = adjust_expr(path_node)?;
            let str_ast = str_node.cast::<ast::Str>()?;
            return self.path_rewrite(self.source.id(), &str_ast.get(), &str_node);
        }

        let link_parent = node_ancestors(node)
            .find(|node| {
                use SyntaxKind::*;
                matches!(node.kind(), FuncCall | ModuleInclude | ModuleImport)
            })
            .unwrap_or(node);

        // Actually there should be only one link left
        if let Some(link_info) = get_link_exprs_in(link_parent) {
            let objects = link_info.objects.into_iter();
            let object_under_node = objects.filter(|link| link.range.contains(&cursor));

            let mut resolved = false;
            for link in object_under_node {
                if let LinkTarget::Path(id, path) = link.target {
                    // todo: is there a link that is not a string?
                    resolved = self.path_rewrite(id, &path, node).is_some() || resolved;
                }
            }

            return Some(());
        }

        None
    }

    fn path_rewrite(&mut self, id: TypstFileId, path: &str, node: &LinkedNode) -> Option<()> {
        if !matches!(node.kind(), SyntaxKind::Str) {
            log::warn!("bad path node kind on code action: {:?}", node.kind());
            return None;
        }

        let path = Path::new(path);

        if path.is_absolute() {
            // Convert absolute path to relative path
            let canonicalize = |path: &Path| {
                let mut path_buf = PathBuf::new();
                for component in path.components() {
                    match component {
                        std::path::Component::ParentDir => {
                            path_buf.pop();
                        }
                        std::path::Component::CurDir => {}
                        component => {
                            path_buf.push(component);
                        }
                    }
                }
                path_buf
            };

            let path = canonicalize(path);
            let mut path_iter = path.components();
            path_iter.next(); // skip the first `RootDir`
            let mut last_path_iter = path_iter.clone();
            let cur_path = id.vpath().as_rooted_path().parent().unwrap();
            let mut cur_path_iter = cur_path.components();
            cur_path_iter.next(); // skip the first `RootDir`
            let mut last_cur_path_iter = cur_path_iter.clone();

            // current `/a`, path `/b/c`, convert to `../b/c`
            let mut new_path = PathBuf::new();
            while let (
                Some(std::path::Component::Normal(name1)),
                Some(std::path::Component::Normal(name2)),
            ) = (cur_path_iter.next(), path_iter.next())
            {
                if name1 != name2 {
                    break;
                }
                last_path_iter = path_iter.clone();
                last_cur_path_iter = cur_path_iter.clone();
            }

            for _ in last_cur_path_iter {
                new_path.push("..");
            }

            for component in last_path_iter {
                if let std::path::Component::Normal(name) = component {
                    new_path.push(name);
                }
            }
            let new_path = new_path.to_string_lossy().to_string();
            let edit = self.edit_str(node, new_path)?;
            let action = CodeActionOrCommand::CodeAction(CodeAction {
                title: "Convert to relative path".to_string(),
                kind: Some(CodeActionKind::REFACTOR_REWRITE),
                edit: Some(edit),
                ..CodeAction::default()
            });
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
            let new_path = new_path.to_string_lossy().to_string();
            let edit = self.edit_str(node, new_path)?;
            let action = CodeActionOrCommand::CodeAction(CodeAction {
                title: "Convert to absolute path".to_string(),
                kind: Some(CodeActionKind::REFACTOR_REWRITE),
                edit: Some(edit),
                ..CodeAction::default()
            });
            self.actions.push(action);
        }

        Some(())
    }

    fn edit_str(&mut self, node: &LinkedNode, new_content: String) -> Option<WorkspaceEdit> {
        if !matches!(node.kind(), SyntaxKind::Str) {
            log::warn!("edit_str only works on string AST nodes: {:?}", node.kind());
            return None;
        }

        self.local_edit(TextEdit {
            range: self.ctx.to_lsp_range(node.range(), &self.source),
            // todo: this is merely ocasionally correct, abusing string escape (`fmt::Debug`)
            new_text: format!("{new_content:?}"),
        })
    }

    fn wrap_actions(&mut self, node: &LinkedNode, range: Range<usize>) -> Option<()> {
        if range.is_empty() {
            return None;
        }

        let start_mode = interpret_mode_at(Some(node));
        if !matches!(start_mode, InterpretMode::Markup | InterpretMode::Math) {
            return None;
        }

        let edit = self.local_edits(vec![
            TextEdit {
                range: self
                    .ctx
                    .to_lsp_range(range.start..range.start, &self.source),
                new_text: "#[".into(),
            },
            TextEdit {
                range: self.ctx.to_lsp_range(range.end..range.end, &self.source),
                new_text: "]".into(),
            },
        ])?;

        let action = CodeActionOrCommand::CodeAction(CodeAction {
            title: "Wrap with content block".to_string(),
            kind: Some(CodeActionKind::REFACTOR_REWRITE),
            edit: Some(edit),
            ..CodeAction::default()
        });
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
            let action = CodeActionOrCommand::CodeAction(CodeAction {
                title: "Decrease depth of heading".to_string(),
                kind: Some(CodeActionKind::REFACTOR_REWRITE),
                edit: Some(self.local_edit(TextEdit {
                    range: self.ctx.to_lsp_range(marker_range.clone(), &self.source),
                    new_text: "=".repeat(depth - 1),
                })?),
                ..CodeAction::default()
            });
            self.actions.push(action);
        }

        // Increase depth of heading
        let action = CodeActionOrCommand::CodeAction(CodeAction {
            title: "Increase depth of heading".to_string(),
            kind: Some(CodeActionKind::REFACTOR_REWRITE),
            edit: Some(self.local_edit(TextEdit {
                range: self.ctx.to_lsp_range(marker_range, &self.source),
                new_text: "=".repeat(depth + 1),
            })?),
            ..CodeAction::default()
        });
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
                    && ch_next.map_or(true, |ch_next| !ch_next.is_ascii_punctuation()))
                    || (!ch.is_ascii_punctuation() && IS_PUNCTUATION.is_match(&ch.to_string()))
            });
        let punc_modify = if let Some((nx, _)) = mark_after_equation {
            let ch_range = self
                .ctx
                .to_lsp_range(node_end..node_end + nx.len_utf8(), &self.source);
            let remove_edit = TextEdit {
                range: ch_range,
                new_text: "".to_owned(),
            };
            Some((nx, remove_edit))
        } else {
            None
        };

        let rewrite_action = |title: &str, new_text: &str| {
            let mut edits = vec![
                TextEdit {
                    range: front_range,
                    new_text: new_text.to_owned(),
                },
                TextEdit {
                    range: back_range,
                    new_text: if !new_text.is_empty() {
                        if let Some((ch, _)) = &punc_modify {
                            ch.to_string() + new_text
                        } else {
                            new_text.to_owned()
                        }
                    } else {
                        "".to_owned()
                    },
                },
            ];

            if !new_text.is_empty() {
                if let Some((_, edit)) = &punc_modify {
                    edits.push(edit.clone());
                }
            }

            Some(CodeActionOrCommand::CodeAction(CodeAction {
                title: title.to_owned(),
                kind: Some(CodeActionKind::REFACTOR_REWRITE),
                edit: Some(self.local_edits(edits)?),
                ..CodeAction::default()
            }))
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
}
