use typst::diag::FileError;
use typst::syntax::SyntaxNode;
use typst::syntax::ast::{self, AstNode};

use super::*;

impl Instrumenter for BreakpointInstr {
    fn instrument(&self, _source: Source) -> FileResult<Source> {
        let (new, meta) = instrument_breakpoints(_source)?;

        let mut session = DEBUG_SESSION.write();
        let session = session
            .as_mut()
            .ok_or_else(|| FileError::Other(Some("No active debug session".into())))?;

        session.enable_breakpoints_for(new.id(), &meta);
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
                ast::Expr::CodeBlock(..) => {
                    self.instrument_block(node);
                    return;
                }
                ast::Expr::WhileLoop(while_expr) => {
                    self.instrument_block_child(node, while_expr.body().span(), Span::detached());
                    return;
                }
                ast::Expr::ForLoop(for_expr) => {
                    self.instrument_block_child(node, for_expr.body().span(), Span::detached());
                    return;
                }
                ast::Expr::Conditional(cond_expr) => {
                    self.instrument_block_child(
                        node,
                        cond_expr.if_body().span(),
                        cond_expr
                            .else_body()
                            .map(|expr| expr.span())
                            .unwrap_or(Span::detached()),
                    );
                    return;
                }
                ast::Expr::Closure(closure) => {
                    self.instrument_closure(node, closure);
                    return;
                }
                ast::Expr::ShowRule(show_rule) => {
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
                | ast::Expr::ListItem(..)
                | ast::Expr::EnumItem(..)
                | ast::Expr::TermItem(..)
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
                | ast::Expr::MathFieldAccess(..)
                | ast::Expr::MathCall(..)
                | ast::Expr::Ident(..)
                | ast::Expr::None(..)
                | ast::Expr::Auto(..)
                | ast::Expr::Bool(..)
                | ast::Expr::Int(..)
                | ast::Expr::Float(..)
                | ast::Expr::Numeric(..)
                | ast::Expr::Str(..)
                | ast::Expr::ContentBlock(..)
                | ast::Expr::Parenthesized(..)
                | ast::Expr::Array(..)
                | ast::Expr::Dict(..)
                | ast::Expr::Unary(..)
                | ast::Expr::Binary(..)
                | ast::Expr::FieldAccess(..)
                | ast::Expr::FuncCall(..)
                | ast::Expr::LetBinding(..)
                | ast::Expr::DestructAssignment(..)
                | ast::Expr::SetRule(..)
                | ast::Expr::Contextual(..)
                | ast::Expr::ModuleImport(..)
                | ast::Expr::ModuleInclude(..)
                | ast::Expr::LoopBreak(..)
                | ast::Expr::LoopContinue(..)
                | ast::Expr::FuncReturn(..) => {}
            }
        }

        self.visit_node_fallback(node);
    }

    fn visit_node_fallback(&mut self, node: &SyntaxNode) {
        let txt = node.leaf_text();
        if !txt.is_empty() {
            self.instrumented.push_str(txt);
        }

        for child in node.children() {
            self.visit_node(child);
        }
    }

    fn make_cov(&mut self, span: Span, kind: BreakpointKind) {
        self.make_cov_with_scope(span, kind, "(:)", None);
    }

    fn make_cov_with_scope(
        &mut self,
        span: Span,
        kind: BreakpointKind,
        scope: &str,
        function_name: Option<String>,
    ) {
        let it = self.meta.meta.len();
        self.meta.meta.push(BreakpointItem {
            kind,
            function_name,
            origin_span: span,
        });
        self.instrumented.push_str("if __breakpoint_");
        self.instrumented.push_str(kind.to_str());
        self.instrumented.push('(');
        self.instrumented.push_str(&it.to_string());
        self.instrumented.push_str(") {");
        self.instrumented.push_str("__breakpoint_");
        self.instrumented.push_str(kind.to_str());
        self.instrumented.push_str("_handle(");
        self.instrumented.push_str(&it.to_string());
        self.instrumented.push_str(", ");
        self.instrumented.push_str(scope);
        self.instrumented.push_str("); ");
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
        self.visit_node(child);
        self.instrumented.push_str("\n__it => {");
        self.make_cov(s, BreakpointKind::ShowStart);
        self.instrumented.push_str("__bp_functor(__it); } }\n");
    }

    fn instrument_closure(&mut self, node: &SyntaxNode, closure: ast::Closure) {
        let body = closure.body().span();
        let name = closure.name();
        let origin = name.map_or_else(|| node.span(), |name| name.span());
        let function_name = name.map(|name| name.as_str().to_owned());
        let scope = Self::closure_scope(closure);

        for child in node.children() {
            if body == child.span() {
                self.instrumented.push_str("{\n");
                self.make_cov_with_scope(
                    origin,
                    BreakpointKind::Function,
                    &scope,
                    function_name.clone(),
                );
                self.visit_node(child);
                self.instrumented.push_str("\n}\n");
            } else {
                self.visit_node(child);
            }
        }
    }

    fn closure_scope(closure: ast::Closure) -> String {
        let mut bindings = Vec::new();
        for param in closure.params().children() {
            match param {
                ast::Param::Pos(pattern) => {
                    for binding in pattern.bindings() {
                        Self::push_scope_binding(&mut bindings, binding);
                    }
                }
                ast::Param::Named(named) => {
                    Self::push_scope_binding(&mut bindings, named.name());
                }
                ast::Param::Spread(spread) => {
                    if let Some(binding) = spread.sink_ident() {
                        Self::push_scope_binding(&mut bindings, binding);
                    }
                }
            }
        }

        if bindings.is_empty() {
            return "(:)".to_owned();
        }

        let mut scope = String::from("(");
        for (idx, binding) in bindings.iter().enumerate() {
            if idx > 0 {
                scope.push_str(", ");
            }
            scope.push_str(binding);
            scope.push_str(": ");
            scope.push_str(binding);
        }
        scope.push(')');
        scope
    }

    fn push_scope_binding(bindings: &mut Vec<String>, binding: ast::Ident) {
        let binding = binding.as_str();
        if !bindings.iter().any(|it| it == binding) {
            bindings.push(binding.to_owned());
        }
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
        insta::assert_snapshot!(instrumented, @r#"
        // A show rule, should be used like:
        //   #show: super-plus-as-dagger
        //   U^+U = U U^+ = I
        // or in scope:
        //   #[
        //     #show: super-plus-as-dagger
        //     U^+U = U U^+ = I
        //   ]
        #let super-plus-as-dagger(document) = {
        if __breakpoint_function(0) {__breakpoint_function_handle(0, (document: document)); };
        {
        if __breakpoint_block_start(1) {__breakpoint_block_start_handle(1, (:)); };
        {
          show math.attach: {
        let __bp_functor = elem => {
        if __breakpoint_function(2) {__breakpoint_function_handle(2, (elem: elem)); };
        {
        if __breakpoint_block_start(3) {__breakpoint_block_start_handle(3, (:)); };
        {
            if __eligible(elem.base) and elem.at("t", default: none) == [+] {
        if __breakpoint_block_start(4) {__breakpoint_block_start_handle(4, (:)); };
        {
              $attach(elem.base, t: dagger, b: elem.at("b", default: #none))$
            }
        if __breakpoint_block_end(5) {__breakpoint_block_end_handle(5, (:)); };
        }
         else {
        if __breakpoint_block_start(6) {__breakpoint_block_start_handle(6, (:)); };
        {
              elem
            }
        if __breakpoint_block_end(7) {__breakpoint_block_end_handle(7, (:)); };
        }

          }
        if __breakpoint_block_end(8) {__breakpoint_block_end_handle(8, (:)); };
        }

        }

        __it => {if __breakpoint_show_start(9) {__breakpoint_show_start_handle(9, (:)); };
        __bp_functor(__it); } }


          document
        }
        if __breakpoint_block_end(10) {__breakpoint_block_end_handle(10, (:)); };
        }

        }
        "#);
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
        insta::assert_snapshot!(new.text(), @"
        #let a = {
        if __breakpoint_block_start(0) {__breakpoint_block_start_handle(0, (:)); };
        {1}
        if __breakpoint_block_end(1) {__breakpoint_block_end_handle(1, (:)); };
        }
        ;
        ");
    }

    #[test]
    fn test_instrument_breakpoint_function() {
        let source = Source::detached(
            r#"#let add(x, y: 1, ..rest) = x + y
#let inc = value => value + 1"#,
        );
        let (new, _meta) = instrument_breakpoints(source).unwrap();
        assert!(new.root().errors_and_warnings().0.is_empty());
        insta::assert_snapshot!(new.text(), @r###"
        #let add(x, y: 1, ..rest) = {
        if __breakpoint_function(0) {__breakpoint_function_handle(0, (x: x, y: y, rest: rest)); };
        x + y
        }

        #let inc = value => {
        if __breakpoint_function(1) {__breakpoint_function_handle(1, (value: value)); };
        value + 1
        }
        "###);
    }

    #[test]
    fn test_instrument_breakpoint_functor() {
        let source = Source::detached("#show: main");
        let (new, _meta) = instrument_breakpoints(source).unwrap();
        insta::assert_snapshot!(new.text(), @"
        #show: {
        let __bp_functor = main
        __it => {if __breakpoint_show_start(0) {__breakpoint_show_start_handle(0, (:)); };
        __bp_functor(__it); } }
        ");
    }
}
