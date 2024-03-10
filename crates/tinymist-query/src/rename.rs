use std::{collections::HashSet, os::windows::fs::FileTypeExt};

use log::{debug, warn};
use lsp_types::TextEdit;

use crate::{
    analysis::{find_definition, find_imports, find_lexical_references_after, Definition},
    prelude::*,
};

#[derive(Debug, Clone)]
pub struct RenameRequest {
    pub path: PathBuf,
    pub position: LspPosition,
    pub new_name: String,
}

impl RenameRequest {
    pub fn request(
        self,
        world: &TypstSystemWorld,
        position_encoding: PositionEncoding,
    ) -> Option<WorkspaceEdit> {
        let source = get_suitable_source_in_workspace(world, &self.path).ok()?;
        let typst_offset = lsp_to_typst::position(self.position, position_encoding, &source)?;

        let ast_node = LinkedNode::new(source.root()).leaf_at(typst_offset + 1)?;

        let t: &dyn World = world;
        let Definition::Func(func) = find_definition(t.track(), source.id(), ast_node)? else {
            // todo: handle other definitions
            return None;
        };

        // todo: unwrap parentheses

        let ident = match func.use_site.kind() {
            SyntaxKind::Ident | SyntaxKind::MathIdent => func.use_site.text(),
            _ => return None,
        };
        debug!("prepare_rename: {ident}");

        let def_id = func.span.id()?;
        if def_id.package().is_some() {
            debug!(
                "prepare_rename: {ident} is in a package {pkg:?}",
                pkg = def_id.package()
            );
            return None;
        }

        let mut editions = HashMap::new();

        let def_source = world.source(def_id).ok()?;
        let def_id = def_source.id();
        let def_path = world.path_for_id(def_id).ok()?;
        let def_node = def_source.find(func.span)?;
        let mut def_node = &def_node;
        loop {
            if def_node.kind() == SyntaxKind::LetBinding {
                break;
            }
            def_node = def_node.parent()?;
        }

        debug!(
            "rename: def_node found: {def_node:?} in {path}",
            path = def_path.display()
        );

        let def_func = def_node.cast::<ast::LetBinding>()?;
        let def_names = def_func.kind().bindings();
        if def_names.len() != 1 {
            return None;
        }
        let def_name = def_names.first().unwrap();
        let def_name_node = def_node.find(def_name.span())?;

        // find after function definition
        let def_root = LinkedNode::new(def_source.root());
        let parent = def_node.parent().unwrap_or(&def_root).clone();
        let idents = find_lexical_references_after(parent, def_node.clone(), ident);
        debug!("rename: in file idents found: {idents:?}");

        let def_uri = Url::from_file_path(def_path).unwrap();
        for i in (Some(def_name_node).into_iter()).chain(idents) {
            let range = typst_to_lsp::range(i.range(), &def_source, position_encoding);

            editions.insert(
                def_uri.clone(),
                vec![TextEdit {
                    range,
                    new_text: self.new_name.clone(),
                }],
            );
        }

        // check whether it is in a sub scope
        if is_rooted_definition(def_node) {
            let mut wq = WorkQueue::default();
            wq.push(def_id);
            while let Some(id) = wq.pop() {
                search_in_workspace(
                    world,
                    id,
                    ident,
                    &self.new_name,
                    &mut editions,
                    &mut wq,
                    position_encoding,
                )?;
            }
        }

        // todo: conflict analysis

        Some(WorkspaceEdit {
            changes: Some(editions),
            ..Default::default()
        })
    }
}

#[derive(Debug, Clone, Default)]
struct WorkQueue {
    searched: HashSet<TypstFileId>,
    queue: Vec<TypstFileId>,
}

impl WorkQueue {
    fn push(&mut self, id: TypstFileId) {
        if self.searched.contains(&id) {
            return;
        }
        self.searched.insert(id);
        self.queue.push(id);
    }

    fn pop(&mut self) -> Option<TypstFileId> {
        let id = self.queue.pop()?;
        Some(id)
    }
}

fn is_rooted_definition(node: &LinkedNode) -> bool {
    // check whether it is in a sub scope
    let mut parent_has_block = false;
    let mut parent = node.parent();
    while let Some(p) = parent {
        if matches!(p.kind(), SyntaxKind::CodeBlock | SyntaxKind::ContentBlock) {
            parent_has_block = true;
            break;
        }
        parent = p.parent();
    }

    !parent_has_block
}

fn search_in_workspace(
    world: &TypstSystemWorld,
    def_id: TypstFileId,
    ident: &str,
    new_name: &str,
    editions: &mut HashMap<Url, Vec<TextEdit>>,
    wq: &mut WorkQueue,
    position_encoding: PositionEncoding,
) -> Option<()> {
    for path in walkdir::WalkDir::new(world.root.clone())
        .follow_links(false)
        .into_iter()
    {
        let Ok(de) = path else {
            continue;
        };
        if !de.file_type().is_file() && !de.file_type().is_symlink_file() {
            continue;
        }
        if !de
            .path()
            .extension()
            .is_some_and(|e| e == "typ" || e == "typc")
        {
            continue;
        }

        let Ok(source) = get_suitable_source_in_workspace(world, de.path()) else {
            warn!("rename: failed to get source for {}", de.path().display());
            return None;
        };

        let use_id = source.id();
        // todo: whether we can rename identifiers in packages?
        if use_id.package().is_some() || wq.searched.contains(&use_id) {
            continue;
        }

        // todo: find dynamically
        let mut res = vec![];

        if def_id != use_id {
            // find import statement
            let imports = find_imports(&source, Some(def_id));
            debug!("rename: imports found: {imports:?}");

            // todo: precise import analysis
            if imports.is_empty() {
                continue;
            }

            let root = LinkedNode::new(source.root());

            for i in imports {
                let stack_store = i.1.clone();
                let Some(import_node) = stack_store.cast::<ast::ModuleImport>() else {
                    continue;
                };
                // todo: don't ignore import node
                if import_node.new_name().is_some() {
                    continue;
                }
                let Some(imports) = import_node.imports() else {
                    continue;
                };

                let mut found = false;
                let mut found_ident = None;
                match imports {
                    ast::Imports::Wildcard => found = true,
                    ast::Imports::Items(items) => {
                        for handle in items.iter() {
                            match handle {
                                ast::ImportItem::Simple(e) => {
                                    if e.get() == ident {
                                        found = true;
                                        found_ident = Some((e, false));
                                        break;
                                    }
                                }
                                ast::ImportItem::Renamed(e) => {
                                    let o = e.original_name();
                                    if o.get() == ident {
                                        found = true;
                                        found_ident = Some((o, true));
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }

                if !found {
                    continue;
                }
                debug!("rename: import ident found in {:?}", de.path().display());

                let is_renamed = found_ident.as_ref().map(|e| e.1).unwrap_or(false);
                let found_ident = found_ident.map(|e| e.0);

                if !is_renamed && is_rooted_definition(&i.1) {
                    wq.push(use_id);
                    debug!("rename: push {use_id:?} to work queue");
                }

                let idents = if !is_renamed {
                    let parent = i.1.parent().unwrap_or(&root).clone();
                    Some(find_lexical_references_after(parent, i.1.clone(), ident))
                } else {
                    None
                };
                debug!("rename: idents found: {idents:?}");

                let found_ident = found_ident.map(|found_ident| {
                    let Some(found_ident) = i.1.find(found_ident.span()) else {
                        warn!(
                            "rename: found_ident not found: {found_ident:?} in {:?} in {}",
                            i.1,
                            de.path().display()
                        );
                        return None;
                    };

                    Some(found_ident)
                });

                // we do early return because there may be some unreliability during
                // analysis
                if found_ident.as_ref().is_some_and(Option::is_none) {
                    return None;
                }
                let found_ident = found_ident.flatten();

                for i in idents.into_iter().flatten().chain(found_ident.into_iter()) {
                    let range = typst_to_lsp::range(i.range(), &source, position_encoding);

                    res.push(TextEdit {
                        range,
                        new_text: new_name.to_owned(),
                    });
                }
            }
        }
        if !res.is_empty() {
            let use_path = world.path_for_id(use_id).unwrap();
            let uri = Url::from_file_path(use_path).unwrap();
            editions.insert(uri, res);
        }
    }

    Some(())
}
