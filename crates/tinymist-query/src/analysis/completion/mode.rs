//! Completion by [`crate::syntax::InterpretMode`].

use super::*;
impl CompletionPair<'_, '_, '_> {
    /// Complete in comments. Or rather, don't!
    pub fn complete_comments(&mut self) -> bool {
        let text = self.cursor.leaf.get().text();
        // check if next line defines a function
        if_chain! {
            if text == "///" || text == "/// ";
            // hash node
            if let Some(next) = self.cursor.leaf.next_leaf();
            // let node
            if let Some(next_next) = next.next_leaf();
            if let Some(next_next) = next_next.next_leaf();
            if matches!(next_next.parent_kind(), Some(SyntaxKind::Closure));
            if let Some(closure) = next_next.parent();
            if let Some(closure) = closure.cast::<ast::Expr>();
            if let ast::Expr::Closure(c) = closure;
            then {
                let mut doc_snippet: String = if text == "///" {
                    " $0\n///".to_string()
                } else {
                    "$0\n///".to_string()
                };
                let mut i = 0;
                for param in c.params().children() {
                    // TODO: Properly handle Pos and Spread argument
                    let param: &EcoString = match param {
                        Param::Pos(p) => {
                            match p {
                                ast::Pattern::Normal(ast::Expr::Ident(ident)) => ident.get(),
                                _ => &"_".into()
                            }
                        }
                        Param::Named(n) => n.name().get(),
                        Param::Spread(s) => {
                            if let Some(ident) = s.sink_ident() {
                                &eco_format!("{}", ident.get())
                            } else {
                                &EcoString::new()
                            }
                        }
                    };
                    log::info!("param: {param}, index: {i}");
                    doc_snippet += &format!("\n/// - {param} (${}): ${}", i + 1, i + 2);
                    i += 2;
                }
                doc_snippet += &format!("\n/// -> ${}", i + 1);
                self.push_completion(Completion {
                    label: "Document function".into(),
                    apply: Some(doc_snippet.into()),
                    ..Completion::default()
                });
            }
        };

        true
    }

    /// Complete in markup mode.
    pub fn complete_markup(&mut self) -> bool {
        let parent_raw =
            node_ancestors(&self.cursor.leaf).find(|node| matches!(node.kind(), SyntaxKind::Raw));

        // Behind a half-completed binding: "#let x = |" or `#let f(x) = |`.
        if_chain! {
            if let Some(prev) = self.cursor.leaf.prev_leaf();
            if matches!(prev.kind(), SyntaxKind::Eq | SyntaxKind::Arrow);
            if matches!( prev.parent_kind(), Some(SyntaxKind::LetBinding | SyntaxKind::Closure));
            then {
                self.cursor.from = self.cursor.cursor;
                self.code_completions( false);
                return true;
            }
        }

        // Behind a half-completed context block: "#context |".
        if_chain! {
            if let Some(prev) = self.cursor.leaf.prev_leaf();
            if prev.kind() == SyntaxKind::Context;
            then {
                self.cursor.from = self.cursor.cursor;
                self.code_completions(false);
                return true;
            }
        }

        // Directly after a raw block.
        if let Some(parent_raw) = parent_raw {
            let mut s = Scanner::new(self.cursor.text);
            s.jump(parent_raw.offset());
            if s.eat_if("```") {
                s.eat_while('`');
                let start = s.cursor();
                if s.eat_if(is_id_start) {
                    s.eat_while(is_id_continue);
                }
                if s.cursor() == self.cursor.cursor {
                    self.cursor.from = start;
                    self.raw_completions();
                }
                return true;
            }
        }

        // Anywhere: "|".
        if !is_triggered_by_punc(self.worker.trigger_character) && self.worker.explicit {
            self.cursor.from = self.cursor.cursor;
            self.snippet_completions(Some(InterpretMode::Markup), None);
            return true;
        }

        false
    }

    /// Complete in math mode.
    pub fn complete_math(&mut self) -> bool {
        // Behind existing atom or identifier: "$a|$" or "$abc|$".
        if !is_triggered_by_punc(self.worker.trigger_character)
            && matches!(
                self.cursor.leaf.kind(),
                SyntaxKind::Text | SyntaxKind::MathIdent
            )
        {
            self.cursor.from = self.cursor.leaf.offset();
            self.scope_completions(true);
            self.snippet_completions(Some(InterpretMode::Math), None);
            return true;
        }

        // Anywhere: "$|$".
        if !is_triggered_by_punc(self.worker.trigger_character) && self.worker.explicit {
            self.cursor.from = self.cursor.cursor;
            self.scope_completions(true);
            self.snippet_completions(Some(InterpretMode::Math), None);
            return true;
        }

        false
    }

    /// Complete in code mode.
    pub fn complete_code(&mut self) -> bool {
        // Start of an interpolated identifier: "#|".
        if self.cursor.leaf.kind() == SyntaxKind::Hash {
            self.cursor.from = self.cursor.cursor;
            self.code_completions(true);

            return true;
        }

        // Start of an interpolated identifier: "#pa|".
        if self.cursor.leaf.kind() == SyntaxKind::Ident {
            self.cursor.from = self.cursor.leaf.offset();
            self.code_completions(is_hash_expr(&self.cursor.leaf));
            return true;
        }

        // Behind a half-completed context block: "context |".
        if_chain! {
            if let Some(prev) = self.cursor.leaf.prev_leaf();
            if prev.kind() == SyntaxKind::Context;
            then {
                self.cursor.from = self.cursor.cursor;
                self.code_completions(false);
                return true;
            }
        }

        // An existing identifier: "{ pa| }".
        if self.cursor.leaf.kind() == SyntaxKind::Ident
            && !matches!(
                self.cursor.leaf.parent_kind(),
                Some(SyntaxKind::FieldAccess)
            )
        {
            self.cursor.from = self.cursor.leaf.offset();
            self.code_completions(false);
            return true;
        }

        // Anywhere: "{ | }".
        // But not within or after an expression.
        // ctx.explicit &&
        if self.cursor.leaf.kind().is_trivia()
            || (matches!(
                self.cursor.leaf.kind(),
                SyntaxKind::LeftParen | SyntaxKind::LeftBrace
            ) || (matches!(self.cursor.leaf.kind(), SyntaxKind::Colon)
                && self.cursor.leaf.parent_kind() == Some(SyntaxKind::ShowRule)))
        {
            self.cursor.from = self.cursor.cursor;
            self.code_completions(false);
            return true;
        }

        false
    }

    /// Add completions for expression snippets.
    fn code_completions(&mut self, hash: bool) {
        // todo: filter code completions
        // matches!(value, Value::Symbol(_) | Value::Func(_) | Value::Type(_) |
        // Value::Module(_))
        self.scope_completions(true);

        self.snippet_completions(Some(InterpretMode::Code), None);

        if !hash {
            self.snippet_completion(
                "function",
                "(${params}) => ${output}",
                "Creates an unnamed function.",
            );
        }
    }
}
