//! Semantic static and dynamic analysis of the source code.

mod bib;
pub(crate) use bib::*;
pub mod call;
pub use call::*;
pub mod color_exprs;
pub use color_exprs::*;
pub mod link_exprs;
pub use link_exprs::*;
pub mod import;
pub use import::*;
pub mod linked_def;
pub use linked_def::*;
pub mod signature;
pub use signature::*;
mod post_tyck;
mod tyck;
pub(crate) use crate::ty::*;
pub(crate) use post_tyck::*;
pub(crate) use tyck::*;
pub mod track_values;
pub use track_values::*;
mod prelude;

mod global;
pub use global::*;

#[cfg(test)]
mod type_check_tests {

    use core::fmt;

    use typst::syntax::Source;

    use crate::analysis::*;
    use crate::tests::*;

    use super::{Ty, TypeScheme};

    #[test]
    fn test() {
        snapshot_testing("type_check", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let result = ctx.expr_stage(&source);
            let result = type_check(ctx.shared_(), result);
            let result = result
                .as_deref()
                .map(|e| format!("{:#?}", TypeCheckSnapshot(&source, e)));
            let result = result.as_deref().unwrap_or("<nil>");

            assert_snapshot!(result);
        });
    }

    struct TypeCheckSnapshot<'a>(&'a Source, &'a TypeScheme);

    impl fmt::Debug for TypeCheckSnapshot<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let source = self.0;
            let info = self.1;
            let mut vars = info
                .vars
                .iter()
                .map(|e| (e.1.name(), e.1))
                .collect::<Vec<_>>();

            vars.sort_by(|x, y| x.1.var.cmp(&y.1.var));

            for (name, var) in vars {
                writeln!(f, "{:?} = {:?}", name, info.simplify(var.as_type(), true))?;
            }

            writeln!(f, "---")?;
            let mut mapping = info
                .mapping
                .iter()
                .map(|e| (source.range(*e.0).unwrap_or_default(), e.1))
                .collect::<Vec<_>>();

            mapping.sort_by(|x, y| {
                x.0.start
                    .cmp(&y.0.start)
                    .then_with(|| x.0.end.cmp(&y.0.end))
            });

            for (range, value) in mapping {
                let ty = Ty::from_types(value.clone().into_iter());
                writeln!(f, "{range:?} -> {ty:?}")?;
            }

            Ok(())
        }
    }
}

#[cfg(test)]
mod post_type_check_tests {

    use insta::with_settings;
    use typst::syntax::LinkedNode;
    use typst_shim::syntax::LinkedNodeExt;

    use crate::analysis::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("post_type_check", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let pos = ctx
                .to_typst_pos(find_test_position(&source), &source)
                .unwrap();
            let root = LinkedNode::new(source.root());
            let node = root.leaf_at_compat(pos + 1).unwrap();
            let text = node.get().clone().into_text();

            let result = ctx.expr_stage(&source);
            let result = type_check(ctx.shared_(), result);
            let literal_type = result.and_then(|info| post_type_check(ctx.shared_(), &info, node));

            with_settings!({
                description => format!("Check on {text:?} ({pos:?})"),
            }, {
                let literal_type = literal_type.map(|e| format!("{e:#?}"))
                    .unwrap_or_else(|| "<nil>".to_string());
                assert_snapshot!(literal_type);
            })
        });
    }
}

#[cfg(test)]
mod type_describe_tests {

    use insta::with_settings;
    use typst::syntax::LinkedNode;
    use typst_shim::syntax::LinkedNodeExt;

    use crate::analysis::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("type_describe", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let pos = ctx
                .to_typst_pos(find_test_position(&source), &source)
                .unwrap();
            let root = LinkedNode::new(source.root());
            let node = root.leaf_at_compat(pos + 1).unwrap();
            let text = node.get().clone().into_text();

            let result = ctx.expr_stage(&source);
            let result = type_check(ctx.shared_(), result);
            let literal_type = result.and_then(|info| post_type_check(ctx.shared_(), &info, node));

            with_settings!({
                description => format!("Check on {text:?} ({pos:?})"),
            }, {
                let literal_type = literal_type.and_then(|e| e.describe())
                    .unwrap_or_else(|| "<nil>".to_string());
                assert_snapshot!(literal_type);
            })
        });
    }
}

#[cfg(test)]
mod module_tests {
    use reflexo::path::unix_slash;
    use serde_json::json;

    use crate::prelude::*;
    use crate::syntax::module::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("modules", &|ctx, _| {
            fn ids(ids: EcoVec<TypstFileId>) -> Vec<String> {
                let mut ids: Vec<String> = ids
                    .into_iter()
                    .map(|id| unix_slash(id.vpath().as_rooted_path()))
                    .collect();
                ids.sort();
                ids
            }

            let dependencies = construct_module_dependencies(&mut ctx.local);

            let mut dependencies = dependencies
                .into_iter()
                .map(|(id, v)| {
                    (
                        unix_slash(id.vpath().as_rooted_path()),
                        ids(v.dependencies),
                        ids(v.dependents),
                    )
                })
                .collect::<Vec<_>>();

            dependencies.sort();
            // remove /main.typ
            dependencies.retain(|(p, _, _)| p != "/main.typ");

            let dependencies = dependencies
                .into_iter()
                .map(|(id, deps, dependents)| {
                    let mut mp = serde_json::Map::new();
                    mp.insert("id".to_string(), json!(id));
                    mp.insert("dependencies".to_string(), json!(deps));
                    mp.insert("dependents".to_string(), json!(dependents));
                    json!(mp)
                })
                .collect::<Vec<_>>();

            assert_snapshot!(JsonRepr::new_pure(dependencies));
        });
    }
}

#[cfg(test)]
mod matcher_tests {

    use typst::syntax::LinkedNode;
    use typst_shim::syntax::LinkedNodeExt;

    use crate::{syntax::get_def_target, tests::*};

    #[test]
    fn test() {
        snapshot_testing("match_def", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let pos = ctx
                .to_typst_pos(find_test_position(&source), &source)
                .unwrap();

            let root = LinkedNode::new(source.root());
            let node = root.leaf_at_compat(pos).unwrap();

            let result = get_def_target(node).map(|e| format!("{:?}", e.node().range()));
            let result = result.as_deref().unwrap_or("<nil>");

            assert_snapshot!(result);
        });
    }
}

#[cfg(test)]
mod expr_tests {

    use typst::syntax::Source;

    use crate::syntax::{Expr, RefExpr};
    use crate::tests::*;

    trait ShowExpr {
        fn show_expr(&self, expr: &Expr) -> String;
    }

    impl ShowExpr for Source {
        fn show_expr(&self, node: &Expr) -> String {
            match node {
                Expr::Decl(decl) => {
                    let range = decl.span().and_then(|s| self.range(s)).unwrap_or_default();
                    let fid = if let Some(fid) = decl.file_id() {
                        format!(" in {fid:?}")
                    } else {
                        "".to_string()
                    };
                    format!("{decl:?}@{range:?}{fid}")
                }
                _ => format!("{node:?}"),
            }
        }
    }

    #[test]
    fn docs() {
        snapshot_testing("docs", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let result = ctx.shared_().expr_stage(&source);
            let mut docstrings = result.docstrings.iter().collect::<Vec<_>>();
            docstrings.sort_by(|x, y| x.0.weak_cmp(y.0));
            let mut docstrings = docstrings
                .into_iter()
                .map(|(ident, expr)| {
                    format!(
                        "{} -> {expr:?}",
                        source.show_expr(&Expr::Decl(ident.clone())),
                    )
                })
                .collect::<Vec<_>>();
            let mut snap = vec![];
            snap.push("= docstings".to_owned());
            snap.append(&mut docstrings);

            assert_snapshot!(snap.join("\n"));
        });
    }

    #[test]
    fn scope() {
        snapshot_testing("expr_of", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let result = ctx.shared_().expr_stage(&source);
            let mut resolves = result.resolves.iter().collect::<Vec<_>>();
            resolves.sort_by(|x, y| x.1.decl.weak_cmp(&y.1.decl));

            let mut resolves = resolves
                .into_iter()
                .map(|(_, expr)| {
                    let RefExpr {
                        decl: ident,
                        of,
                        val,
                    } = expr.as_ref();

                    format!(
                        "{} -> {}, val: {val:?}",
                        source.show_expr(&Expr::Decl(ident.clone())),
                        of.as_ref().map(|e| source.show_expr(e)).unwrap_or_default()
                    )
                })
                .collect::<Vec<_>>();
            let mut exports = result.exports.iter().collect::<Vec<_>>();
            exports.sort_by(|x, y| x.0.cmp(y.0));
            let mut exports = exports
                .into_iter()
                .map(|(ident, node)| {
                    let node = source.show_expr(node);
                    format!("{ident} -> {node}",)
                })
                .collect::<Vec<_>>();

            let mut snap = vec![];
            snap.push("= resolves".to_owned());
            snap.append(&mut resolves);
            snap.push("= exports".to_owned());
            snap.append(&mut exports);

            assert_snapshot!(snap.join("\n"));
        });
    }
}

#[cfg(test)]
mod lexical_hierarchy_tests {

    use crate::syntax::lexical_hierarchy;
    use crate::tests::*;

    #[test]
    fn scope() {
        snapshot_testing("lexical_hierarchy", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let result = lexical_hierarchy::get_lexical_hierarchy(
                source,
                lexical_hierarchy::LexicalScopeKind::DefUse,
            );

            assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
        });
    }
}

#[cfg(test)]
mod signature_tests {

    use core::fmt;

    use typst::foundations::Repr;
    use typst::syntax::LinkedNode;
    use typst_shim::syntax::LinkedNodeExt;

    use crate::analysis::{analyze_signature, Signature, SignatureTarget};
    use crate::syntax::get_deref_target;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("signature", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let pos = ctx
                .to_typst_pos(find_test_position(&source), &source)
                .unwrap();

            let root = LinkedNode::new(source.root());
            let callee_node = root.leaf_at_compat(pos).unwrap();
            let callee_node = get_deref_target(callee_node, pos).unwrap();
            let callee_node = callee_node.node();

            let result = analyze_signature(
                ctx.shared(),
                SignatureTarget::Syntax(source.clone(), callee_node.get().clone()),
            );

            assert_snapshot!(SignatureSnapshot(result.as_ref()));
        });
    }

    struct SignatureSnapshot<'a>(pub Option<&'a Signature>);

    impl<'a> fmt::Display for SignatureSnapshot<'a> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let Some(sig) = self.0 else {
                return write!(f, "<nil>");
            };

            let primary_sig = match sig {
                Signature::Primary(sig) => sig,
                Signature::Partial(sig) => {
                    for w in &sig.with_stack {
                        write!(f, "with ")?;
                        for arg in &w.items {
                            if let Some(name) = &arg.name {
                                write!(f, "{name}: ")?;
                            }
                            write!(
                                f,
                                "{}, ",
                                arg.value.as_ref().map(|v| v.repr()).unwrap_or_default()
                            )?;
                        }
                        f.write_str("\n")?;
                    }

                    &sig.signature
                }
            };

            writeln!(f, "fn(")?;
            for param in primary_sig.pos() {
                writeln!(f, " {},", param.name)?;
            }
            for param in primary_sig.named() {
                if let Some(expr) = &param.default {
                    writeln!(f, " {}: {},", param.name, expr)?;
                } else {
                    writeln!(f, " {},", param.name)?;
                }
            }
            if let Some(param) = primary_sig.rest() {
                writeln!(f, " ...{}, ", param.name)?;
            }
            write!(f, ")")?;

            Ok(())
        }
    }
}

#[cfg(test)]
mod call_info_tests {

    use core::fmt;

    use typst::syntax::{LinkedNode, SyntaxKind};
    use typst_shim::syntax::LinkedNodeExt;

    use crate::analysis::analyze_call;
    use crate::tests::*;

    use super::CallInfo;

    #[test]
    fn test() {
        snapshot_testing("call_info", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let pos = ctx
                .to_typst_pos(find_test_position(&source), &source)
                .unwrap();

            let root = LinkedNode::new(source.root());
            let mut call_node = root.leaf_at_compat(pos + 1).unwrap();

            while let Some(parent) = call_node.parent() {
                if call_node.kind() == SyntaxKind::FuncCall {
                    break;
                }
                call_node = parent.clone();
            }

            let result = analyze_call(ctx, source.clone(), call_node);

            assert_snapshot!(CallSnapshot(result.as_deref()));
        });
    }

    struct CallSnapshot<'a>(pub Option<&'a CallInfo>);

    impl<'a> fmt::Display for CallSnapshot<'a> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let Some(ci) = self.0 else {
                return write!(f, "<nil>");
            };

            let mut w = ci.arg_mapping.iter().collect::<Vec<_>>();
            w.sort_by(|x, y| x.0.span().number().cmp(&y.0.span().number()));

            for (arg, arg_call_info) in w {
                writeln!(f, "{} -> {:?}", arg.clone().into_text(), arg_call_info)?;
            }

            Ok(())
        }
    }
}
