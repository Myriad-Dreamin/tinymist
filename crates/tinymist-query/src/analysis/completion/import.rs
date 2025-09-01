//! Completion for import items.

use super::*;
impl CompletionPair<'_, '_, '_> {
    /// Complete imports.
    pub fn complete_imports(&mut self) -> bool {
        // On the colon marker of an import list:
        // "#import "path.typ":|"
        if matches!(self.cursor.leaf.kind(), SyntaxKind::Colon)
            && let Some(parent) = self.cursor.leaf.clone().parent()
            && let Some(ast::Expr::Import(import)) = parent.get().cast()
            && !matches!(import.imports(), Some(ast::Imports::Wildcard))
            && let Some(source) = parent.children().find(|child| child.is::<ast::Expr>())
        {
            let items = match import.imports() {
                Some(ast::Imports::Items(items)) => items,
                _ => Default::default(),
            };

            self.cursor.from = self.cursor.cursor;

            self.import_item_completions(items, vec![], &source);
            if items.iter().next().is_some() {
                self.worker.enrich("", ", ");
            }
            return true;
        }

        // Behind an import list:
        // "#import "path.typ": |",
        // "#import "path.typ": a, b, |".

        if let Some(prev) = self.cursor.leaf.prev_sibling()
            && let Some(ast::Expr::Import(import)) = prev.get().cast()
            && !self.cursor.text[prev.offset()..self.cursor.cursor].contains('\n')
            && let Some(ast::Imports::Items(items)) = import.imports()
            && let Some(source) = prev.children().find(|child| child.is::<ast::Expr>())
        {
            self.cursor.from = self.cursor.cursor;
            self.import_item_completions(items, vec![], &source);
            return true;
        }

        // Behind a comma in an import list:
        // "#import "path.typ": this,|".
        if matches!(self.cursor.leaf.kind(), SyntaxKind::Comma)
            && let Some(parent) = self.cursor.leaf.clone().parent()
            && parent.kind() == SyntaxKind::ImportItems
            && let Some(grand) = parent.parent()
            && let Some(ast::Expr::Import(import)) = grand.get().cast()
            && let Some(ast::Imports::Items(items)) = import.imports()
            && let Some(source) = grand.children().find(|child| child.is::<ast::Expr>())
        {
            self.import_item_completions(items, vec![], &source);
            self.worker.enrich(" ", "");
            return true;
        }

        // Behind a half-started identifier in an import list:
        // "#import "path.typ": th|".
        if matches!(self.cursor.leaf.kind(), SyntaxKind::Ident | SyntaxKind::Dot)
            && let Some(path_ctx) = self.cursor.leaf.clone().parent()
            && path_ctx.kind() == SyntaxKind::ImportItemPath
            && let Some(parent) = path_ctx.parent()
            && parent.kind() == SyntaxKind::ImportItems
            && let Some(grand) = parent.parent()
            && let Some(ast::Expr::Import(import)) = grand.get().cast()
            && let Some(ast::Imports::Items(items)) = import.imports()
            && let Some(source) = grand.children().find(|child| child.is::<ast::Expr>())
        {
            if self.cursor.leaf.kind() == SyntaxKind::Ident {
                self.cursor.from = self.cursor.leaf.offset();
            }
            let path = path_ctx.cast::<ast::ImportItemPath>().map(|path| {
                path.iter()
                    .take_while(|ident| ident.span() != self.cursor.leaf.span())
                    .collect()
            });
            self.import_item_completions(items, path.unwrap_or_default(), &source);
            return true;
        }

        false
    }

    /// Add completions for all exports of a module.
    pub fn import_item_completions(
        &mut self,
        existing: ast::ImportItems,
        comps: Vec<ast::Ident>,
        source: &LinkedNode,
    ) {
        // Select the source by `comps`
        let value = self.worker.ctx.module_by_syntax(source);
        let value = comps.iter().fold(value.as_ref(), |value, comp| {
            value?.scope()?.get(comp)?.read().into()
        });
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

        for (name, bind) in scope.iter() {
            if seen.iter().all(|item| item.as_str() != name) {
                self.value_completion(Some(name.clone()), bind.read(), false, None);
            }
        }
    }
}
