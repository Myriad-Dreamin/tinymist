use typst::diag::FileError;
use typst::syntax::ast::{self, AstNode};
use typst::syntax::SyntaxNode;

use super::*;

impl Instrumenter for BreakpointInstr {
    fn instrument(&self, _source: Source) -> FileResult<Source> {
        let (new, meta) = instrument_breakpoints(_source)?;

        let mut session = DEBUG_SESSION.write();
        let session = session
            .as_mut()
            .ok_or_else(|| FileError::Other(Some("No active debug session".into())))?;

        session.breakpoints.insert(new.id(), meta);

        Ok(new)
    }
}

#[comemo::memoize]
fn instrument_breakpoints(source: Source) -> FileResult<(Source, Arc<BreakpointInfo>)> {
    let node = source.root();
    let mut worker = InstrumentWorker {
        meta: BreakpointInfo::default(),
        instrumented: String::new(),
    };

    worker.visit_node(node);
    let new_source: Source = Source::new(source.id(), worker.instrumented);

    Ok((new_source, Arc::new(worker.meta)))
}

struct InstrumentWorker {
    meta: BreakpointInfo,
    instrumented: String,
}

impl InstrumentWorker {
    fn instrument_block_child(&mut self, container: &SyntaxNode, b1: Span, b2: Span) {
        for child in container.children() {
            if b1 == child.span() || b2 == child.span() {
                self.instrument_block(child);
            } else {
                self.visit_node(child);
            }
        }
    }

    fn visit_node(&mut self, node: &SyntaxNode) {
        if let Some(expr) = node.cast::<ast::Expr>() {
            match expr {
                ast::Expr::Code(..) => {
                    self.instrument_block(node);
                    return;
                }
                ast::Expr::While(while_expr) => {
                    self.instrument_block_child(node, while_expr.body().span(), Span::detached());
                    return;
                }
                ast::Expr::For(for_expr) => {
                    self.instrument_block_child(node, for_expr.body().span(), Span::detached());
                    return;
                }
                ast::Expr::Conditional(cond_expr) => {
                    self.instrument_block_child(
                        node,
                        cond_expr.if_body().span(),
                        cond_expr.else_body().unwrap_or_default().span(),
                    );
                    return;
                }
                ast::Expr::Closure(closure) => {
                    self.instrument_block_child(node, closure.body().span(), Span::detached());
                    return;
                }
                ast::Expr::Show(show_rule) => {
                    let transform = show_rule.transform().to_untyped().span();

                    for child in node.children() {
                        if transform == child.span() {
                            self.instrument_functor(child);
                        } else {
                            self.visit_node(child);
                        }
                    }
                    return;
                }
                ast::Expr::Text(..)
                | ast::Expr::Space(..)
                | ast::Expr::Linebreak(..)
                | ast::Expr::Parbreak(..)
                | ast::Expr::Escape(..)
                | ast::Expr::Shorthand(..)
                | ast::Expr::SmartQuote(..)
                | ast::Expr::Strong(..)
                | ast::Expr::Emph(..)
                | ast::Expr::Raw(..)
                | ast::Expr::Link(..)
                | ast::Expr::Label(..)
                | ast::Expr::Ref(..)
                | ast::Expr::Heading(..)
                | ast::Expr::List(..)
                | ast::Expr::Enum(..)
                | ast::Expr::Term(..)
                | ast::Expr::Equation(..)
                | ast::Expr::Math(..)
                | ast::Expr::MathText(..)
                | ast::Expr::MathIdent(..)
                | ast::Expr::MathShorthand(..)
                | ast::Expr::MathAlignPoint(..)
                | ast::Expr::MathDelimited(..)
                | ast::Expr::MathAttach(..)
                | ast::Expr::MathPrimes(..)
                | ast::Expr::MathFrac(..)
                | ast::Expr::MathRoot(..)
                | ast::Expr::Ident(..)
                | ast::Expr::None(..)
                | ast::Expr::Auto(..)
                | ast::Expr::Bool(..)
                | ast::Expr::Int(..)
                | ast::Expr::Float(..)
                | ast::Expr::Numeric(..)
                | ast::Expr::Str(..)
                | ast::Expr::Content(..)
                | ast::Expr::Parenthesized(..)
                | ast::Expr::Array(..)
                | ast::Expr::Dict(..)
                | ast::Expr::Unary(..)
                | ast::Expr::Binary(..)
                | ast::Expr::FieldAccess(..)
                | ast::Expr::FuncCall(..)
                | ast::Expr::Let(..)
                | ast::Expr::DestructAssign(..)
                | ast::Expr::Set(..)
                | ast::Expr::Contextual(..)
                | ast::Expr::Import(..)
                | ast::Expr::Include(..)
                | ast::Expr::Break(..)
                | ast::Expr::Continue(..)
                | ast::Expr::Return(..) => {}
            }
        }

        self.visit_node_fallback(node);
    }

    fn visit_node_fallback(&mut self, node: &SyntaxNode) {
        let txt = node.text();
        if !txt.is_empty() {
            self.instrumented.push_str(txt);
        }

        for child in node.children() {
            self.visit_node(child);
        }
    }

    fn make_cov(&mut self, span: Span, kind: BreakpointKind) {
        let it = self.meta.meta.len();
        self.meta.meta.push(BreakpointItem { origin_span: span });
        self.instrumented.push_str("if __breakpoint_");
        self.instrumented.push_str(kind.to_str());
        self.instrumented.push('(');
        self.instrumented.push_str(&it.to_string());
        self.instrumented.push_str(") {");
        self.instrumented.push_str("__breakpoint_");
        self.instrumented.push_str(kind.to_str());
        self.instrumented.push_str("_handle(");
        self.instrumented.push_str(&it.to_string());
        self.instrumented.push_str(", (:)); ");
        self.instrumented.push_str("};\n");
    }

    fn instrument_block(&mut self, child: &SyntaxNode) {
        self.instrumented.push_str("{\n");
        let (first, last) = {
            let mut children = child.children();
            let first = children
                .next()
                .map(|s| s.span())
                .unwrap_or_else(Span::detached);
            let last = children
                .last()
                .map(|s| s.span())
                .unwrap_or_else(Span::detached);

            (first, last)
        };
        self.make_cov(first, BreakpointKind::BlockStart);
        self.visit_node_fallback(child);
        self.instrumented.push('\n');
        self.make_cov(last, BreakpointKind::BlockEnd);
        self.instrumented.push_str("}\n");
    }

    fn instrument_functor(&mut self, child: &SyntaxNode) {
        self.instrumented.push_str("{\nlet __bp_functor = ");
        let s = child.span();
        self.visit_node_fallback(child);
        self.instrumented.push_str("\n__it => {");
        self.make_cov(s, BreakpointKind::ShowStart);
        self.instrumented.push_str("__bp_functor(__it); } }\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instr(input: &str) -> String {
        let source = Source::detached(input);
        let (new, _meta) = instrument_breakpoints(source).unwrap();
        new.text().to_string()
    }

    #[test]
    fn test_physica_vector() {
        let instrumented = instr(include_str!(
            "../fixtures/instr_coverage/physica_vector.typ"
        ));
        insta::assert_snapshot!(instrumented, @r###"
        // A show rule, should be used like:
        //   #show: super-plus-as-dagger
        //   U^+U = U U^+ = I
        // or in scope:
        //   #[
        //     #show: super-plus-as-dagger
        //     U^+U = U U^+ = I
        //   ]
        #let super-plus-as-dagger(document) = {
        if __breakpoint_block_start(0) {__breakpoint_block_start_handle(0, (:)); };
        {
          show math.attach: {
        let __bp_functor = elem => {
        if __breakpoint_block_start(1) {__breakpoint_block_start_handle(1, (:)); };
        {
            if __eligible(elem.base) and elem.at("t", default: none) == [+] {
        if __breakpoint_block_start(2) {__breakpoint_block_start_handle(2, (:)); };
        {
              $attach(elem.base, t: dagger, b: elem.at("b", default: #none))$
            }
        if __breakpoint_block_end(3) {__breakpoint_block_end_handle(3, (:)); };
        }
         else {
        if __breakpoint_block_start(4) {__breakpoint_block_start_handle(4, (:)); };
        {
              elem
            }
        if __breakpoint_block_end(5) {__breakpoint_block_end_handle(5, (:)); };
        }

          }
        if __breakpoint_block_end(6) {__breakpoint_block_end_handle(6, (:)); };
        }

        __it => {if __breakpoint_show_start(7) {__breakpoint_show_start_handle(7, (:)); };
        __bp_functor(__it); } }


          document
        }
        if __breakpoint_block_end(8) {__breakpoint_block_end_handle(8, (:)); };
        }
        "###);
    }

    #[test]
    fn test_playground() {
        let instrumented = instr(include_str!("../fixtures/instr_coverage/playground.typ"));
        insta::assert_snapshot!(instrumented, @"");
    }

    #[test]
    fn test_instrument_breakpoint() {
        let source = Source::detached("#let a = 1;");
        let (new, _meta) = instrument_breakpoints(source).unwrap();
        insta::assert_snapshot!(new.text(), @"#let a = 1;");
    }

    #[test]
    fn test_instrument_breakpoint_nested() {
        let source = Source::detached("#let a = {1};");
        let (new, _meta) = instrument_breakpoints(source).unwrap();
        insta::assert_snapshot!(new.text(), @r###"
        #let a = {
        if __breakpoint_block_start(0) {__breakpoint_block_start_handle(0, (:)); };
        {1}
        if __breakpoint_block_end(1) {__breakpoint_block_end_handle(1, (:)); };
        }
        ;
        "###);
    }

    #[test]
    fn test_instrument_breakpoint_functor() {
        let source = Source::detached("#show: main");
        let (new, _meta) = instrument_breakpoints(source).unwrap();
        insta::assert_snapshot!(new.text(), @r###"
        #show: {
        let __bp_functor = main
        __it => {if __breakpoint_show_start(0) {__breakpoint_show_start_handle(0, (:)); };
        __bp_functor(__it); } }
        "###);
    }
}
