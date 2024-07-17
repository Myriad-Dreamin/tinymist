//! `typos` as spell checking backend.

use typos::tokens::{Identifier, Word};
use typos_cli::policy;
use typst::{
    diag::EcoString,
    syntax::{
        ast::{self, AstNode},
        Source, Span, SyntaxKind, SyntaxNode,
    },
};

#[derive(Debug, Clone, Copy)]
enum TextPos {
    Markup,
    Code,
}

fn scan_idents(src: &Source, mut f: impl FnMut(TextPos, &str, Span)) {
    fn scan_group(src: &SyntaxNode, f: &mut impl FnMut(TextPos, &str, Span)) {
        for ch in src.children() {
            scan_idents_impl(ch, f)
        }
    }

    fn scan_text(src: &SyntaxNode, f: &mut impl FnMut(TextPos, &str, Span)) {
        let t = src.text();
        if !t.is_empty() {
            f(TextPos::Markup, t, src.span());
        }
    }

    fn scan_ident(src: &SyntaxNode, f: &mut impl FnMut(TextPos, &str, Span)) {
        let t = src.text();
        if !t.is_empty() {
            f(TextPos::Code, t, src.span());
        }
    }

    fn scan_idents_impl(src: &SyntaxNode, f: &mut impl FnMut(TextPos, &str, Span)) {
        if matches!(src.kind(), SyntaxKind::Error) {
            return;
        }

        use SyntaxKind::*;
        match src.kind() {
            Markup => scan_group(src, f),
            Text => scan_text(src, f),
            Space => {}
            Linebreak => {}
            Parbreak => {}
            Escape => {}
            Shorthand => {}
            SmartQuote => {}
            Strong => scan_group(src, f),
            Emph => scan_group(src, f),
            Raw => {}
            RawLang => {}
            RawDelim => {}
            RawTrimmed => {}
            Link => {}
            Label => {}
            Ref => {}
            RefMarker => {}
            Heading => scan_group(src, f),
            HeadingMarker => {}
            ListItem => scan_group(src, f),
            ListMarker => {}
            EnumItem => scan_group(src, f),
            EnumMarker => {}
            TermItem => scan_group(src, f),
            TermMarker => {}
            Equation => scan_group(src, f),
            Math => scan_group(src, f),
            MathAlignPoint => scan_group(src, f),
            MathDelimited => scan_group(src, f),
            MathAttach => scan_group(src, f),
            MathPrimes => scan_group(src, f),
            MathFrac => scan_group(src, f),
            MathRoot => scan_group(src, f),
            Hash | LeftBrace | RightBrace | LeftBracket | RightBracket | LeftParen | RightParen
            | Comma | Semicolon | Colon | Star | Underscore | Dollar | Plus | Minus | Slash
            | Hat | Prime | Dot | Eq | EqEq | ExclEq | Lt | LtEq | Gt | GtEq | PlusEq | HyphEq
            | StarEq | SlashEq | Dots | Arrow | Root | Not | And | Or | None | Auto | Let | Set
            | Show | Context | If | Else | For | In | While | Break | Continue | Return
            | Import | Include | As | Bool | Int | Float | Numeric | Str => {}
            MathIdent | Ident => scan_ident(src, f),
            Code => scan_group(src, f),
            CodeBlock => scan_group(src, f),
            ContentBlock => scan_group(src, f),
            Parenthesized => scan_group(src, f),
            Array => scan_group(src, f),
            Dict => scan_group(src, f),
            Named => scan_group(src, f),
            Keyed => scan_group(src, f),
            Unary => scan_group(src, f),
            Binary => scan_group(src, f),
            FieldAccess => {}
            FuncCall => {
                let fc = src.cast::<ast::FuncCall>().unwrap();
                scan_idents_impl(fc.args().to_untyped(), f);
            }
            Args => {
                let args = src.cast::<ast::Args>().unwrap();
                for arg in args.items() {
                    match arg {
                        ast::Arg::Pos(p) => scan_idents_impl(p.to_untyped(), f),
                        ast::Arg::Named(p) => scan_idents_impl(p.expr().to_untyped(), f),
                        ast::Arg::Spread(..) => {}
                    }
                }
            }
            Params => {
                let args = src.cast::<ast::Params>().unwrap();
                for arg in args.children() {
                    match arg {
                        ast::Param::Pos(p) => scan_idents_impl(p.to_untyped(), f),
                        ast::Param::Named(p) => scan_idents_impl(p.expr().to_untyped(), f),
                        ast::Param::Spread(..) => {}
                    }
                }
            }
            Spread => scan_group(src, f),
            Closure => scan_group(src, f),
            LetBinding => scan_group(src, f),
            SetRule => scan_group(src, f),
            ShowRule => scan_group(src, f),
            Contextual => scan_group(src, f),
            Conditional => scan_group(src, f),
            WhileLoop => scan_group(src, f),
            ForLoop => scan_group(src, f),
            ModuleImport => {}
            ImportItems => {}
            RenamedImportItem => {}
            ModuleInclude => {}
            LoopBreak => scan_group(src, f),
            LoopContinue => scan_group(src, f),
            FuncReturn => scan_group(src, f),
            Destructuring => scan_group(src, f),
            DestructAssignment => scan_group(src, f),
            LineComment => {}
            BlockComment => {}
            Error => {}
            Eof => {}
        }
    }

    scan_idents_impl(src.root(), &mut f)
}

/// Check for typos in the source.
pub fn typos_check(src: &Source, mut f: impl FnMut(Span, usize, Vec<EcoString>)) {
    let default_policy = policy::Policy::default();

    let dict = default_policy.dict;

    let mut report = |span: Span, offset: usize, cc: Status<'_>| match cc {
        Status::Valid => {}
        Status::Invalid => {
            f(span, offset, vec![]);
        }
        Status::Corrections(corrections) => {
            let cc = corrections.into_iter().map(|c| c.as_ref().into()).collect();
            f(span, offset, cc);
        }
    };

    use typos::tokens::Case;
    use typos::Status;
    scan_idents(src, |pos, t, span| match pos {
        TextPos::Markup => {
            println!("checking {:?} -> {pos:?}:{t:?}", src.range(span).unwrap());
            let ident = Word::new_unchecked(t, Case::Lower, 0);
            if let Some(st) = dict.correct_word(ident) {
                report(span, 0, st);
            }
        }
        TextPos::Code => {
            println!("checking {:?} -> {pos:?}:{t:?}", src.range(span).unwrap());
            let ident = Identifier::new_unchecked(t, Case::None, 0);
            let res = dict.correct_ident(ident);
            match res {
                Some(Status::Valid) => {}
                Some(st @ (Status::Invalid | Status::Corrections(..))) => report(span, 0, st),
                None => {
                    for ident in ident.split() {
                        if let Some(st) = dict.correct_word(ident) {
                            report(span, 0, st);
                        }
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    use typst::syntax::Source;

    fn check_typos(src: &Source) -> String {
        let mut res = String::new();
        typos_check(src, |span, _, cc| {
            res.push_str(&format!("{:?} -> {:?}\n", src.range(span).unwrap(), cc));
        });

        res
    }

    #[test]
    fn test() {
        snapshot_testing("typos", &|src| {
            assert_snapshot!(check_typos(&src));
        });
    }
}

// /// Suggestion for change in a text.
// #[derive(Debug, Clone)]
// pub struct Suggestion {
//     pub source: String,
//     pub message: String,
//     pub span: Option<MappedSpan>,
//     pub replacements: Vec<String>,
// }

// pub fn nlp_check_docs(doc: Arc<TypstDocument>) -> Option<Vec<Suggestion>> {
//     let annotated = super::text_export::TextExporter::default()
//         .annotate(doc)
//         .ok()?;
//     let suggestions = nlp_check(&annotated.content);
//     let spans = suggestions
//         .iter()
//         .map(|s| s.span().char().clone())
//         .collect::<Vec<_>>();
//     let spans = annotated.map_back_spans(spans);
//     Some(
//         suggestions
//             .into_iter()
//             .zip(spans)
//             .map(|(suggestion, span)| Suggestion {
//                 source: suggestion.source().to_string(),
//                 message: suggestion.message().to_string(),
//                 span,
//                 replacements: suggestion
//                     .replacements()
//                     .iter()
//                     .map(|x| x.to_string())
//                     .collect(),
//             })
//             .collect(),
//     )
// }

// pub fn diag_from_suggestion(suggestion: Suggestion) ->
// typst::diag::SourceDiagnostic {     typst::diag::SourceDiagnostic {
//         severity: typst::diag::Severity::Warning,
//         message: eco_format!("{:?}", suggestion.message),
//         span: suggestion
//             .span
//             .map(|s| s.span.span)
//             .unwrap_or(Span::detached()),
//         trace: Default::default(),
//         hints: Default::default(),
//     }
// }
