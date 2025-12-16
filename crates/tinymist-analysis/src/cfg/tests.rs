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
