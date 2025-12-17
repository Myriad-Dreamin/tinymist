use super::*;

use std::path::Path;

use typst::syntax::Source;
use typst::syntax::ast::AstNode;
use typst::syntax::{FileId, Span, VirtualPath, ast};

fn walk_exprs<'a>(node: &'a typst::syntax::SyntaxNode, f: &mut impl FnMut(ast::Expr<'a>)) {
    for child in node.children() {
        if let Some(expr) = child.cast::<ast::Expr<'a>>() {
            f(expr);
            walk_exprs(expr.to_untyped(), f);
        } else {
            walk_exprs(child, f);
        }
    }
}

#[test]
fn cfg_break_creates_orphan_block() {
    let source = Source::detached(
        r#"#{
  while true { break; 1 }
}"#,
    );
    let cfgs = build_cfgs(source.root());
    let root = &cfgs.bodies[0];

    let orphans = orphan_blocks(root);
    assert!(
        !orphans.is_empty(),
        "expected an orphan block for code after `break`"
    );
}

#[test]
fn cfg_contextual_return_is_local() {
    let source = Source::detached(
        r#"#{
  context { return 1; 2 }
}"#,
    );
    let cfgs = build_cfgs(source.root());
    let root = &cfgs.bodies[0];
    let orphans = orphan_blocks(root);
    assert!(
        !orphans.is_empty(),
        "expected an orphan block for code after `return` in `context`"
    );
}

#[test]
fn cfg_dominators_detect_back_edge() {
    let source = Source::detached(r#"#while true { 1 }"#);
    let cfgs = build_cfgs(source.root());
    let root = &cfgs.bodies[0];

    let dom = dominators(root);
    let backs = back_edges(root, &dom);
    assert!(
        !backs.is_empty(),
        "expected at least one back edge for a while loop"
    );
}

#[test]
fn cfg_if_one_branch_returns_still_reaches_join() {
    let source = Source::detached(
        r#"#let f(a) = {
  if a { return 1 } else { 2 }
  3
}"#,
    );
    let cfgs = build_cfgs(source.root());

    let root = cfgs
        .bodies
        .iter()
        .find(|b| matches!(b.kind, BodyKind::Root))
        .expect("root CFG");
    let unreachable: Vec<_> = (0..root.blocks.len())
        .map(BlockId)
        .filter(|bb| {
            *bb != root.entry
                && *bb != root.exit
                && *bb != root.error_exit
                && !root.reachable_blocks().contains(bb)
        })
        .collect();
    assert!(
        unreachable.is_empty(),
        "root CFG should have no unreachable blocks, got {unreachable:?}\n{}",
        root.debug_dump()
    );

    let closure = cfgs
        .bodies
        .iter()
        .find(|b| matches!(b.kind, BodyKind::Closure))
        .expect("closure CFG");
    let unreachable: Vec<_> = (0..closure.blocks.len())
        .map(BlockId)
        .filter(|bb| {
            *bb != closure.entry
                && *bb != closure.exit
                && *bb != closure.error_exit
                && !closure.reachable_blocks().contains(bb)
        })
        .collect();
    assert!(
        unreachable.is_empty(),
        "closure CFG should have no unreachable blocks, got {unreachable:?}\n{}",
        closure.debug_dump()
    );
}

#[test]
fn ipcfg_direct_closure_call_edge() {
    let source = Source::detached(
        r#"#{
  ((x) => { return 1; 2 })(0)
}"#,
    );
    let ip = build_interprocedural_cfg(source.root(), None);

    let root = ip
        .cfgs
        .bodies
        .iter()
        .find(|b| matches!(b.kind, BodyKind::Root))
        .expect("root CFG");
    let closure = ip
        .cfgs
        .bodies
        .iter()
        .find(|b| matches!(b.kind, BodyKind::Closure))
        .expect("closure CFG");

    assert!(
        ip.calls
            .iter()
            .any(|e| e.caller_body == root.id && e.callee_body == closure.id),
        "expected a call edge from root to closure, got {:#?}",
        ip.calls
    );
}

#[test]
fn ipcfg_let_bound_closure_call_edge_with_resolve_map() {
    let source = Source::detached(
        r#"#{
  let f(x) = { x }
  f(1)
}"#,
    );

    let mut def_span: Option<Span> = None;
    let mut use_span: Option<Span> = None;
    walk_exprs(source.root(), &mut |expr| match expr {
        ast::Expr::LetBinding(let_) => {
            if let ast::LetBindingKind::Closure(ident) = let_.kind() {
                def_span = Some(ident.span());
            }
        }
        ast::Expr::FuncCall(call) => {
            if let ast::Expr::Ident(ident) = call.callee()
                && ident.get() == "f"
            {
                use_span = Some(ident.span());
            }
        }
        _ => {}
    });

    let def_span = def_span.expect("def span");
    let use_span = use_span.expect("use span");

    let mut resolves = ResolveMap::default();
    resolves.insert(use_span, def_span);

    let ip = build_interprocedural_cfg(source.root(), Some(&resolves));
    let callee = ip
        .cfgs
        .decl_body(def_span)
        .expect("callee body for declaration");
    assert!(
        ip.calls.iter().any(|e| e.callee_body == callee),
        "expected a call edge into the let-bound closure body, got {:#?}",
        ip.calls
    );
}

#[test]
fn ipcfg_let_var_bound_closure_call_edge_with_resolve_map() {
    let source = Source::detached(
        r#"#{
  let f = (x) => { x }
  f(1)
}"#,
    );

    let mut def_span: Option<Span> = None;
    let mut use_span: Option<Span> = None;
    walk_exprs(source.root(), &mut |expr| match expr {
        ast::Expr::LetBinding(let_) => {
            if let ast::LetBindingKind::Normal(pattern) = let_.kind()
                && let ast::Pattern::Normal(ast::Expr::Ident(ident)) = pattern
                && ident.get() == "f"
            {
                def_span = Some(ident.span());
            }
        }
        ast::Expr::FuncCall(call) => {
            if let ast::Expr::Ident(ident) = call.callee()
                && ident.get() == "f"
            {
                use_span = Some(ident.span());
            }
        }
        _ => {}
    });

    let def_span = def_span.expect("def span");
    let use_span = use_span.expect("use span");

    let mut resolves = ResolveMap::default();
    resolves.insert(use_span, def_span);

    let ip = build_interprocedural_cfg(source.root(), Some(&resolves));
    let callee = ip
        .cfgs
        .decl_body(def_span)
        .expect("callee body for declaration");
    assert!(
        ip.calls.iter().any(|e| e.callee_body == callee),
        "expected a call edge into the var-bound closure body, got {:#?}",
        ip.calls
    );
}

fn source_at(path: &str, text: &str) -> Source {
    let id = FileId::new(None, VirtualPath::new(Path::new(path)));
    Source::new(id, text.to_owned())
}

#[test]
fn ipcfg_cross_file_imported_ident_call_edge_with_resolve_map() {
    let callee_src = source_at(
        "/b.typ",
        r#"#{
  let f(x) = { x }
}"#,
    );
    let caller_src = source_at(
        "/a.typ",
        r#"#{
  import "/b.typ": f
  f(1)
}"#,
    );

    let mut def_span: Option<Span> = None;
    walk_exprs(callee_src.root(), &mut |expr| {
        if let ast::Expr::LetBinding(let_) = expr
            && let ast::LetBindingKind::Closure(ident) = let_.kind()
            && ident.get() == "f"
        {
            def_span = Some(ident.span());
        }
    });

    let mut use_span: Option<Span> = None;
    walk_exprs(caller_src.root(), &mut |expr| {
        if let ast::Expr::FuncCall(call) = expr
            && let ast::Expr::Ident(ident) = call.callee()
            && ident.get() == "f"
        {
            use_span = Some(ident.span());
        }
    });

    let def_span = def_span.expect("def span");
    let use_span = use_span.expect("use span");

    let mut resolves = ResolveMap::default();
    resolves.insert(use_span, def_span);

    let ip =
        build_interprocedural_cfg_many([caller_src.root(), callee_src.root()], Some(&resolves));

    let callee = ip
        .cfgs
        .decl_body(def_span)
        .expect("callee body for declaration");
    assert!(
        ip.calls.iter().any(|e| e.callee_body == callee),
        "expected a call edge into the imported closure body, got {:#?}",
        ip.calls
    );
}

#[test]
fn ipcfg_cross_file_imported_field_access_call_edge_with_resolve_map() {
    let callee_src = source_at(
        "/b.typ",
        r#"#{
  let f(x) = { x }
}"#,
    );
    let caller_src = source_at(
        "/a.typ",
        r#"#{
  import "/b.typ" as m
  m.f(1)
}"#,
    );

    let mut def_span: Option<Span> = None;
    walk_exprs(callee_src.root(), &mut |expr| {
        if let ast::Expr::LetBinding(let_) = expr
            && let ast::LetBindingKind::Closure(ident) = let_.kind()
            && ident.get() == "f"
        {
            def_span = Some(ident.span());
        }
    });

    let mut use_span: Option<Span> = None;
    walk_exprs(caller_src.root(), &mut |expr| {
        if let ast::Expr::FuncCall(call) = expr
            && let ast::Expr::FieldAccess(access) = call.callee()
            && access.field().get() == "f"
        {
            use_span = Some(access.field().span());
        }
    });

    let def_span = def_span.expect("def span");
    let use_span = use_span.expect("use span");

    let mut resolves = ResolveMap::default();
    resolves.insert(use_span, def_span);

    let ip =
        build_interprocedural_cfg_many([caller_src.root(), callee_src.root()], Some(&resolves));

    let callee = ip
        .cfgs
        .decl_body(def_span)
        .expect("callee body for declaration");
    assert!(
        ip.calls.iter().any(|e| e.callee_body == callee),
        "expected a call edge into the field-accessed imported closure body, got {:#?}",
        ip.calls
    );
}
