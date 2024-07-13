//! # Typlite

use std::borrow::Cow;
use std::fmt::Write;

use typst_syntax::{
    ast::{self, AstNode},
    Source, SyntaxKind, SyntaxNode,
};

type Result<T, Err = Cow<'static, str>> = std::result::Result<T, Err>;

/// Task builder for converting a typst document to Markdown.
#[derive(Debug, Clone)]
pub struct Typlite {
    root: SyntaxNode,
}

impl Typlite {
    /// Create a new Typlite instance from a string.
    /// # Example
    /// ```rust
    /// use typlite::Typlite;
    /// let content = "= Hello, World";
    /// let res = Typlite::new_with_content(content).convert();
    /// assert_eq!(res, Ok("# Hello, World".to_string()));
    /// ```
    pub fn new_with_content(content: &str) -> Self {
        let root = typst_syntax::parse(content);
        Self { root }
    }

    /// Create a new Typlite instance from a [`Source`].
    ///
    /// This is useful when you have a [`Source`] instance and you can avoid
    /// reparsing the content.
    pub fn new_with_src(src: Source) -> Self {
        let root = src.root().clone();
        Self { root }
    }

    /// Convert the content to a markdown string.
    pub fn convert(self) -> Result<String> {
        let mut res = String::new();
        Self::convert_to(&self.root, &mut res)?;
        Ok(res)
    }

    /// Convert the content to a markdown string.
    pub fn convert_to(node: &SyntaxNode, s: &mut String) -> Result<()> {
        use SyntaxKind::*;
        match node.kind() {
            RawLang | RawDelim | RawTrimmed => Err("converting clause")?,

            // Error nodes
            Error => Err(node.clone().into_text().to_string())?,
            Eof | None => Ok(()),

            // Non-leaf nodes
            Math => Self::reduce(node, s),
            Markup => Self::reduce(node, s),
            Code => Self::reduce(node, s),
            CodeBlock => Self::reduce(node, s),
            ContentBlock => Self::reduce(node, s),
            Parenthesized => Self::reduce(node, s),

            // Text nodes
            Text | Space | Linebreak | Parbreak => Self::str(node, s),

            // Semantic nodes
            Escape => Self::escape(node, s),
            Shorthand => Self::shorthand(node, s),
            SmartQuote => Self::str(node, s),
            Strong => Self::strong(node, s),
            Emph => Self::emph(node, s),
            Raw => Self::raw(node, s),
            Link => Self::link(node, s),
            Label => Self::label(node, s),
            Ref => Self::label_ref(node, s),
            RefMarker => Self::ref_marker(node, s),
            Heading => Self::heading(node, s),
            HeadingMarker => Self::str(node, s),
            ListItem => Self::list_item(node, s),
            ListMarker => Self::str(node, s),
            EnumItem => Self::enum_item(node, s),
            EnumMarker => Self::str(node, s),
            TermItem => Self::term_item(node, s),
            TermMarker => Self::str(node, s),
            Equation => Self::equation(node, s),
            MathIdent => Self::str(node, s),
            MathAlignPoint => Self::str(node, s),
            MathDelimited => Self::str(node, s),
            MathAttach => Self::str(node, s),
            MathPrimes => Self::str(node, s),
            MathFrac => Self::str(node, s),
            MathRoot => Self::str(node, s),

            // Punctuation
            // Hash => Self::char('#', s),
            Hash => Ok(()),
            LeftBrace => Self::char('{', s),
            RightBrace => Self::char('}', s),
            LeftBracket => Self::char('[', s),
            RightBracket => Self::char(']', s),
            LeftParen => Self::char('(', s),
            RightParen => Self::char(')', s),
            Comma => Self::char(',', s),
            Semicolon => Self::char(';', s),
            Colon => Self::char(':', s),
            Star => Self::char('*', s),
            Underscore => Self::char('_', s),
            Dollar => Self::char('$', s),
            Plus => Self::char('+', s),
            Minus => Self::char('-', s),
            Slash => Self::char('/', s),
            Hat => Self::char('^', s),
            Prime => Self::char('\'', s),
            Dot => Self::char('.', s),
            Eq => Self::char('=', s),
            Lt => Self::char('<', s),
            Gt => Self::char('>', s),

            // Compound punctuation
            EqEq => Self::str(node, s),
            ExclEq => Self::str(node, s),
            LtEq => Self::str(node, s),
            GtEq => Self::str(node, s),
            PlusEq => Self::str(node, s),
            HyphEq => Self::str(node, s),
            StarEq => Self::str(node, s),
            SlashEq => Self::str(node, s),
            Dots => Self::str(node, s),
            Arrow => Self::str(node, s),
            Root => Self::str(node, s),

            // Keywords
            Auto => Self::str(node, s),
            Not => Self::str(node, s),
            And => Self::str(node, s),
            Or => Self::str(node, s),
            Let => Self::str(node, s),
            Set => Self::str(node, s),
            Show => Self::str(node, s),
            Context => Self::str(node, s),
            If => Self::str(node, s),
            Else => Self::str(node, s),
            For => Self::str(node, s),
            In => Self::str(node, s),
            While => Self::str(node, s),
            Break => Self::str(node, s),
            Continue => Self::str(node, s),
            Return => Self::str(node, s),
            Import => Self::str(node, s),
            Include => Self::str(node, s),
            As => Self::str(node, s),

            // Clause nodes
            Named => Ok(()),
            Keyed => Ok(()),
            Unary => Ok(()),
            Binary => Ok(()),

            // Ignored code expressions
            Ident => Ok(()),
            Bool => Ok(()),
            Int => Ok(()),
            Float => Ok(()),
            Numeric => Ok(()),
            Str => Ok(()),
            Array => Ok(()),
            Dict => Ok(()),

            // Ignored code expressions
            FieldAccess => Ok(()),
            FuncCall => Ok(()),
            Args => Ok(()),
            Spread => Ok(()),
            Closure => Ok(()),
            Params => Ok(()),
            LetBinding => Ok(()),
            SetRule => Ok(()),
            ShowRule => Ok(()),
            Contextual => Ok(()),
            Conditional => Ok(()),
            WhileLoop => Ok(()),
            ForLoop => Ok(()),
            ModuleImport => Ok(()),
            ImportItems => Ok(()),
            RenamedImportItem => Ok(()),
            ModuleInclude => Ok(()),
            LoopBreak => Ok(()),
            LoopContinue => Ok(()),
            FuncReturn => Ok(()),
            Destructuring => Ok(()),
            DestructAssignment => Ok(()),

            // Ignored comments
            LineComment => Ok(()),
            BlockComment => Ok(()),
        }
    }

    fn reduce(node: &SyntaxNode, s: &mut String) -> Result<()> {
        for child in node.children() {
            Self::convert_to(child, s)?;
        }

        Ok(())
    }

    fn char(arg: char, s: &mut String) -> Result<()> {
        s.push(arg);
        Ok(())
    }

    fn str(node: &SyntaxNode, s: &mut String) -> Result<()> {
        s.push_str(node.clone().into_text().as_str());
        Ok(())
    }

    fn escape(node: &SyntaxNode, s: &mut String) -> Result<()> {
        // todo: escape characters
        Self::str(node, s)
    }

    fn shorthand(node: &SyntaxNode, s: &mut String) -> Result<()> {
        // todo: shorthands
        Self::str(node, s)
    }

    fn strong(node: &SyntaxNode, s: &mut String) -> Result<()> {
        let strong = node.cast::<ast::Strong>().unwrap();
        s.push_str("**");
        Self::convert_to(strong.body().to_untyped(), s)?;
        s.push_str("**");
        Ok(())
    }

    fn emph(node: &SyntaxNode, s: &mut String) -> Result<()> {
        let emph = node.cast::<ast::Emph>().unwrap();
        s.push('_');
        Self::convert_to(emph.body().to_untyped(), s)?;
        s.push('_');
        Ok(())
    }

    fn heading(node: &SyntaxNode, s: &mut String) -> Result<()> {
        let heading = node.cast::<ast::Heading>().unwrap();
        let level = heading.depth();
        for _ in 0..level.get() {
            s.push('#');
        }
        s.push(' ');
        Self::convert_to(heading.body().to_untyped(), s)
    }

    fn raw(node: &SyntaxNode, s: &mut String) -> Result<()> {
        let raw = node.cast::<ast::Raw>().unwrap();
        if raw.block() {
            return Self::str(node, s);
        }
        s.push('`');
        for e in raw.lines() {
            Self::str(e.to_untyped(), s)?;
        }
        s.push('`');
        Ok(())
    }

    fn link(node: &SyntaxNode, s: &mut String) -> Result<()> {
        Self::str(node, s)
    }

    fn label(node: &SyntaxNode, s: &mut String) -> Result<()> {
        Self::str(node, s)
    }

    fn label_ref(node: &SyntaxNode, s: &mut String) -> Result<()> {
        Self::str(node, s)
    }

    fn ref_marker(node: &SyntaxNode, s: &mut String) -> Result<()> {
        Self::str(node, s)
    }

    fn list_item(node: &SyntaxNode, s: &mut String) -> Result<()> {
        Self::reduce(node, s)
    }

    fn enum_item(node: &SyntaxNode, s: &mut String) -> Result<()> {
        let enum_item = node.cast::<ast::EnumItem>().unwrap();
        if let Some(num) = enum_item.number() {
            write!(s, "{num}. ").map_err(|_| "cannot write enum item number")?;
        } else {
            s.push_str("1. ");
        }
        Self::convert_to(enum_item.body().to_untyped(), s)
    }

    fn term_item(node: &SyntaxNode, s: &mut String) -> Result<()> {
        Self::reduce(node, s)
    }

    #[cfg(not(feature = "texmath"))]
    fn equation(node: &SyntaxNode, s: &mut String) -> Result<()> {
        let equation = node.cast::<ast::Equation>().unwrap();

        #[rustfmt::skip]
        s.push_str(if equation.block() { "```typ\n$\n" } else { "`$" });
        for e in equation.body().exprs() {
            Self::str(e.to_untyped(), s)?;
        }
        #[rustfmt::skip]
        s.push_str(if equation.block() { "\n$\n```\n" } else { "$`" });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conv(s: &str) -> String {
        Typlite::new_with_content(s.trim()).convert().unwrap()
    }

    #[test]
    fn test_converted() {
        insta::assert_snapshot!(conv(r###"
= Hello, World!
This is a typst document.
        "###), @r###"
        # Hello, World!
        This is a typst document.
        "###);
        insta::assert_snapshot!(conv(r###"
Some inlined raw `a`, ```c b```
        "###), @"Some inlined raw `a`, `b`");
        insta::assert_snapshot!(conv(r###"
- Some *item*
- Another _item_
        "###), @r###"
        - Some **item**
        - Another _item_
        "###);
        insta::assert_snapshot!(conv(r###"
+ A
+ B
        "###), @r###"
        1. A
        1. B
        "###);
        insta::assert_snapshot!(conv(r###"
2. A
+ B
        "###), @r###"
        2. A
        1. B
        "###);
        #[cfg(not(feature = "texmath"))]
        insta::assert_snapshot!(conv(r###"
$
1/2 + 1/3 = 5/6
$
        "###), @r###"
        ```typ
        $
        1/2 + 1/3 = 5/6
        $
        ```
        "###);
    }
}
