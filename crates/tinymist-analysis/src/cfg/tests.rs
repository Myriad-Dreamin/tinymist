use super::*;

use typst::syntax::Source;

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
