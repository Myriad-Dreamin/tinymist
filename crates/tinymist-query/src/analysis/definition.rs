use std::collections::HashSet;
use std::ops::Deref;
use std::path::Path;

use log::{debug, trace};
use parking_lot::Mutex;
use typst::syntax::ast::Ident;
use typst::World;
use typst::{
    foundations::{Func, Value},
    syntax::{
        ast::{self, AstNode},
        LinkedNode, Source, Span, SyntaxKind,
    },
};
use typst_ts_core::TypstFileId;

use crate::analysis::find_source_by_import;
use crate::{prelude::*, TypstSpan};

#[derive(Debug, Clone)]
pub struct VariableDefinition<'a> {
    pub def_site: LinkedNode<'a>,
    pub use_site: LinkedNode<'a>,
    pub span: TypstSpan,
}

#[derive(Debug, Clone)]
pub struct FuncDefinition<'a> {
    pub value: Func,
    pub use_site: LinkedNode<'a>,
    pub span: TypstSpan,
}

#[derive(Debug, Clone)]
pub struct ModuleDefinition<'a> {
    pub module: TypstFileId,
    pub use_site: LinkedNode<'a>,
    pub span: TypstSpan,
}

#[derive(Debug, Clone)]
pub struct ExternalDefinition<'a> {
    pub use_site: LinkedNode<'a>,
    pub span: TypstSpan,
}

#[derive(Debug, Clone)]
pub enum Definition<'a> {
    Func(FuncDefinition<'a>),
    Var(VariableDefinition<'a>),
    Module(ModuleDefinition<'a>),
    External(ExternalDefinition<'a>),
}

impl Definition<'_> {
    pub fn span(&self) -> TypstSpan {
        match self {
            Definition::Func(f) => f.span,
            Definition::Var(v) => v.span,
            Definition::Module(m) => m.span,
            Definition::External(s) => s.span,
        }
    }

    pub fn use_site(&self) -> &LinkedNode {
        match self {
            Definition::Func(f) => &f.use_site,
            Definition::Var(v) => &v.use_site,
            Definition::Module(m) => &m.use_site,
            Definition::External(s) => &s.use_site,
        }
    }
}

pub(crate) fn deref_lvalue(mut node: LinkedNode) -> Option<LinkedNode> {
    while let Some(e) = node.cast::<ast::Parenthesized>() {
        node = node.find(e.expr().span())?;
    }
    Some(node)
}

fn advance_prev_adjacent(node: LinkedNode) -> Option<LinkedNode> {
    // this is aworkaround for a bug in the parser
    if node.len() == 0 {
        return None;
    }
    match node.prev_sibling() {
        Some(prev) => Some(prev),
        None => {
            let parent = node.parent()?;
            debug!("no prev sibling parent: {parent:?}");
            advance_prev_adjacent(parent.clone())
        }
    }
}

// #[comemo::memoize]
fn find_definition_in_module<'a>(
    search_ctx: &'a SearchCtx<'a>,
    source: Source,
    name: &str,
) -> Option<Span> {
    {
        let mut s = search_ctx.searched.lock();
        if s.contains(&source.id()) {
            return None;
        }
        s.insert(source.id());
    }
    let root = source.root();
    let node = LinkedNode::new(root);
    let last_expr = if let Some(m) = root.cast::<ast::Markup>() {
        m.exprs().last()?
    } else {
        debug!("unexpected root kind {:?}", root.kind());
        return None;
    };
    let last = node.find(last_expr.span())?;
    let e = find_syntax_definition(search_ctx, last, name)?;
    Some(e.span())
}

enum ImportRef<'a> {
    /// `import "foo" as bar;`
    ///                  ^^^
    ModuleAs(Ident<'a>),
    /// `import "foo.typ"`
    ///          ^^^
    Path(ast::Expr<'a>),
    /// `import "foo": bar`
    ///                ^^^
    Ident(Ident<'a>),
    /// `import "foo": bar as baz`
    ///                       ^^^
    IdentAs(ast::RenamedImportItem<'a>),
    /// `import "foo": *`
    ExternalResolved(Span),
}

fn find_ref_in_import<'b, 'a>(
    ctx: &'b SearchCtx<'b>,
    import_node: ast::ModuleImport<'a>,
    name: &str,
) -> Option<ImportRef<'a>> {
    if let Some(import_node) = import_node.new_name() {
        if import_node.get() == name {
            return Some(ImportRef::ModuleAs(import_node));
        }
    }

    let Some(imports) = import_node.imports() else {
        let v = import_node.source();
        match v {
            ast::Expr::Str(e) => {
                let e = e.get();
                let e = Path::new(e.as_ref());
                let Some(e) = e.file_name() else {
                    return None;
                };
                let e = e.to_string_lossy();
                let e = e.as_ref();
                let Some(e) = e.strip_suffix(".typ") else {
                    return None;
                };
                return (e == name).then_some(ImportRef::Path(v));
            }
            _ => return None,
        }
    };

    match imports {
        ast::Imports::Wildcard => {
            let dep = find_source_by_import(ctx.world, ctx.current, import_node)?;
            let res = find_definition_in_module(ctx, dep, name)?;
            return Some(ImportRef::ExternalResolved(res));
        }
        ast::Imports::Items(items) => {
            for handle in items.iter() {
                match handle {
                    ast::ImportItem::Simple(e) => {
                        if e.get() == name {
                            return Some(ImportRef::Ident(e));
                        }
                    }
                    ast::ImportItem::Renamed(e) => {
                        let o = e.new_name();
                        if o.get() == name {
                            return Some(ImportRef::IdentAs(e));
                        }
                    }
                }
            }
        }
    }

    None
}

fn find_syntax_definition<'b, 'a>(
    search_ctx: &'b SearchCtx<'b>,
    node: LinkedNode<'a>,
    name: &str,
) -> Option<Definition<'a>> {
    struct SyntaxDefinitionWorker<'a, 'b, 'c> {
        ctx: &'c SearchCtx<'c>,
        name: &'b str,
        use_site: LinkedNode<'a>,
    }

    impl<'a, 'b, 'c> SyntaxDefinitionWorker<'a, 'b, 'c> {
        fn find(&mut self, mut node: LinkedNode<'a>) -> Option<Definition<'a>> {
            loop {
                if let Some(def) = self.check(node.clone()) {
                    return Some(def);
                }

                let Some(prev) = advance_prev_adjacent(node) else {
                    debug!("no prev sibling parent");
                    return None;
                };

                node = prev;
            }
        }

        fn resolve_as_var(&self, node: LinkedNode<'a>, name: ast::Ident) -> Option<Definition<'a>> {
            if name.get() != self.name {
                return None;
            }

            let def_site = node.find(name.span())?;
            Some(Definition::Var(VariableDefinition {
                def_site,
                use_site: self.use_site.clone(),
                span: node.span(),
            }))
        }

        fn check(&mut self, node: LinkedNode<'a>) -> Option<Definition<'a>> {
            let node = deref_lvalue(node)?;
            match node.kind() {
                SyntaxKind::LetBinding => {
                    let binding = node.cast::<ast::LetBinding>()?;
                    match binding.kind() {
                        ast::LetBindingKind::Closure(name) => {
                            if name.get() == self.name {
                                let values =
                                    analyze_expr(self.ctx.world.deref(), &node.find(name.span())?);
                                let func = values.into_iter().find_map(|v| match v.0 {
                                    Value::Func(f) => Some(f),
                                    _ => None,
                                });
                                let Some(func) = func else {
                                    debug!("no func found... {name:?}");
                                    return None;
                                };

                                return Some(Definition::Func(FuncDefinition {
                                    value: func,
                                    use_site: self.use_site.clone(),
                                    span: node.span(),
                                }));
                            }
                            None
                        }
                        ast::LetBindingKind::Normal(ast::Pattern::Normal(ast::Expr::Ident(
                            name,
                        ))) => {
                            return self.resolve_as_var(node.clone(), name);
                        }
                        ast::LetBindingKind::Normal(ast::Pattern::Parenthesized(e)) => {
                            let e = deref_lvalue(node.find(e.span())?)?;
                            if let Some(name) = e.cast::<ast::Ident>() {
                                return self.resolve_as_var(e.clone(), name);
                            }
                            None
                        }
                        ast::LetBindingKind::Normal(ast::Pattern::Normal(e)) => {
                            let e = deref_lvalue(node.find(e.span())?)?;
                            if let Some(name) = e.cast::<ast::Ident>() {
                                return self.resolve_as_var(e.clone(), name);
                            }
                            None
                        }
                        ast::LetBindingKind::Normal(ast::Pattern::Destructuring(n)) => {
                            for i in n.bindings() {
                                if i.get() == self.name {
                                    return self.resolve_as_var(node.clone(), i);
                                }
                            }
                            None
                        }
                        ast::LetBindingKind::Normal(ast::Pattern::Placeholder(..)) => None,
                    }
                }
                SyntaxKind::ModuleImport => {
                    let import_node = node.cast::<ast::ModuleImport>()?;

                    match find_ref_in_import(self.ctx, import_node, self.name)? {
                        ImportRef::ModuleAs(ident) => {
                            let m = find_source_by_import(
                                self.ctx.world,
                                self.ctx.current,
                                import_node,
                            )?;
                            return Some(Definition::Module(ModuleDefinition {
                                module: m.id(),
                                use_site: self.use_site.clone(),
                                span: ident.span(),
                            }));
                        }
                        ImportRef::Path(s) => {
                            let m = find_source_by_import(
                                self.ctx.world,
                                self.ctx.current,
                                import_node,
                            )?;
                            return Some(Definition::Module(ModuleDefinition {
                                module: m.id(),
                                use_site: self.use_site.clone(),
                                span: s.span(),
                            }));
                        }
                        ImportRef::Ident(ident) => {
                            return Some(Definition::Var(VariableDefinition {
                                def_site: node.find(ident.span())?,
                                use_site: self.use_site.clone(),
                                span: ident.span(),
                            }));
                        }
                        ImportRef::IdentAs(item) => {
                            let ident = item.new_name();
                            return Some(Definition::Var(VariableDefinition {
                                def_site: node.find(ident.span())?,
                                use_site: self.use_site.clone(),
                                span: ident.span(),
                            }));
                        }
                        ImportRef::ExternalResolved(def_span) => {
                            return Some(Definition::External(ExternalDefinition {
                                use_site: self.use_site.clone(),
                                span: def_span,
                            }));
                        }
                    }
                }
                _ => None,
            }
        }
    }

    let mut worker = SyntaxDefinitionWorker {
        ctx: search_ctx,
        name,
        use_site: node.clone(),
    };
    worker.find(node)
}

struct SearchCtx<'a> {
    world: Tracked<'a, dyn World>,
    current: TypstFileId,
    searched: Mutex<HashSet<TypstFileId>>,
}

// todo: field definition
pub(crate) fn find_definition<'a>(
    world: Tracked<'a, dyn World>,
    current: TypstFileId,
    node: LinkedNode<'a>,
) -> Option<Definition<'a>> {
    let mut search_ctx = SearchCtx {
        world,
        current,
        searched: Mutex::new(HashSet::new()),
    };
    let search_ctx = &mut search_ctx;
    search_ctx.searched.lock().insert(current);

    let mut ancestor = node;
    while !ancestor.is::<ast::Expr>() {
        ancestor = ancestor.parent()?.clone();
    }
    let ancestor = deref_lvalue(ancestor)?;

    let may_ident = ancestor.cast::<ast::Expr>()?;
    if !may_ident.hash() && !matches!(may_ident, ast::Expr::MathIdent(_)) {
        return None;
    }

    let mut is_ident_only = false;
    trace!("got ast_node kind {kind:?}", kind = ancestor.kind());
    let ref_node = match may_ident {
        // todo: label, reference
        // todo: import
        // todo: include
        ast::Expr::FuncCall(call) => call.callee(),
        ast::Expr::Set(set) => set.target(),
        ast::Expr::Ident(..) | ast::Expr::MathIdent(..) | ast::Expr::FieldAccess(..) => {
            is_ident_only = true;
            may_ident
        }
        ast::Expr::Str(..) => {
            if let Some(parent) = ancestor.parent() {
                let e = parent.cast::<ast::ModuleImport>()?;
                let source = find_source_by_import(world, current, e)?;
                let src = ancestor.find(e.source().span())?;
                return Some(Definition::Module(ModuleDefinition {
                    module: source.id(),
                    use_site: src,
                    span: source.root().span(),
                }));
            }
            return None;
        }
        ast::Expr::Import(..) => {
            return None;
        }
        _ => {
            debug!("unsupported kind {kind:?}", kind = ancestor.kind());
            return None;
        }
    };

    let use_site = if is_ident_only {
        ancestor.clone()
    } else {
        ancestor.find(ref_node.span())?
    };

    let values = analyze_expr(world.deref(), &use_site);

    let func = values.into_iter().find_map(|v| match &v.0 {
        Value::Func(..) => Some(v.0),
        _ => None,
    });

    Some(match func {
        Some(Value::Func(f)) => Definition::Func(FuncDefinition {
            value: f.clone(),
            span: f.span(),
            use_site,
        }),
        _ => {
            return match may_ident {
                ast::Expr::Ident(e) => find_syntax_definition(search_ctx, use_site, e.get()),
                ast::Expr::MathIdent(e) => find_syntax_definition(search_ctx, use_site, e.get()),
                ast::Expr::FieldAccess(..) => {
                    debug!("find field access");
                    None
                }
                _ => {
                    debug!("unsupported kind {kind:?}", kind = ancestor.kind());
                    None
                }
            }
        }
    })
}
