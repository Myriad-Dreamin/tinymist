//! # Typlite

mod library;
pub mod scopes;
mod value;

use library::ArgGetter;
use scopes::Scopes;
use value::Value;

use std::borrow::Cow;
use std::fmt::Write;

use ecow::{eco_format, EcoString};
use typst_syntax::{
    ast::{self, AstNode},
    Source, SyntaxKind, SyntaxNode,
};

type Result<T, Err = Cow<'static, str>> = std::result::Result<T, Err>;

/// Task builder for converting a typst document to Markdown.
#[derive(Debug, Clone)]
pub struct Typlite {
    /// The document to convert.
    main: Source,
    /// Whether to enable GFM (GitHub Flavored Markdown) features.
    gfm: bool,
}

impl Typlite {
    /// Create a new Typlite instance from a string.
    /// # Example
    /// ```rust
    /// use typlite::Typlite;
    /// let content = "= Hello, World";
    /// let res = Typlite::new_with_content(content).convert();
    /// assert_eq!(res, Ok("# Hello, World".into()));
    /// ```
    pub fn new_with_content(content: &str) -> Self {
        let main = Source::detached(content);
        Self { main, gfm: false }
    }

    /// Create a new Typlite instance from a [`Source`].
    ///
    /// This is useful when you have a [`Source`] instance and you can avoid
    /// reparsing the content.
    pub fn new_with_src(main: Source) -> Self {
        Self { main, gfm: false }
    }

    /// Convert the content to a markdown string.
    pub fn convert(self) -> Result<EcoString> {
        let mut res = EcoString::new();
        let mut worker = TypliteWorker {
            gfm: self.gfm,
            scopes: library::library(),
        };

        worker.convert_to(self.main.root(), &mut res)?;
        Ok(res)
    }
}

struct TypliteWorker {
    gfm: bool,
    scopes: Scopes<Value>,
}

impl TypliteWorker {
    pub fn convert(&mut self, node: &SyntaxNode) -> Result<EcoString> {
        let mut res = EcoString::new();
        self.convert_to(node, &mut res)?;
        Ok(res)
    }

    /// Convert the content to a markdown string.
    pub fn convert_to(&mut self, node: &SyntaxNode, s: &mut EcoString) -> Result<()> {
        use SyntaxKind::*;
        match node.kind() {
            RawLang | RawDelim | RawTrimmed => Err("converting clause")?,

            // Error nodes
            Error => Err(node.clone().into_text().to_string())?,
            Eof | None => Ok(()),

            // Non-leaf nodes
            Math => self.reduce(node, s),
            Markup => self.reduce(node, s),
            Code => self.reduce(node, s),

            CodeBlock => {
                let code_block: ast::CodeBlock = node.cast().unwrap();
                self.convert_to(code_block.body().to_untyped(), s)
            }
            ContentBlock => {
                let content_block: ast::ContentBlock = node.cast().unwrap();
                self.convert_to(content_block.body().to_untyped(), s)
            }
            Parenthesized => {
                let parenthesized: ast::Parenthesized = node.cast().unwrap();
                self.convert_to(parenthesized.expr().to_untyped(), s)
            }

            // Text nodes
            Text | Space | Linebreak | Parbreak => Self::str(node, s),

            // Semantic nodes
            Escape => Self::escape(node, s),
            Shorthand => Self::shorthand(node, s),
            SmartQuote => Self::str(node, s),
            Strong => self.strong(node, s),
            Emph => self.emph(node, s),
            Raw => Self::raw(node, s),
            Link => self.link(node, s),
            Label => Self::label(node, s),
            Ref => Self::label_ref(node, s),
            RefMarker => Self::ref_marker(node, s),
            Heading => self.heading(node, s),
            HeadingMarker => Self::str(node, s),
            ListItem => self.list_item(node, s),
            ListMarker => Self::str(node, s),
            EnumItem => self.enum_item(node, s),
            EnumMarker => Self::str(node, s),
            TermItem => self.term_item(node, s),
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

            LetBinding => self.let_binding(node, s),
            FieldAccess => self.field_access(node, s),
            FuncCall => Self::absorb(self.func_call(node), s),
            Contextual => self.contextual(node, s),

            // Clause nodes
            Named => Ok(()),
            Keyed => Ok(()),
            Unary => Ok(()),
            Binary => Ok(()),
            Spread => Ok(()),
            ImportItems => Ok(()),
            RenamedImportItem => Ok(()),
            Closure => Ok(()),
            Args => Ok(()),
            Params => Ok(()),

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
            SetRule => Ok(()),
            ShowRule => Ok(()),
            Destructuring => Ok(()),
            DestructAssignment => Ok(()),

            Conditional => Ok(()),
            WhileLoop => Ok(()),
            ForLoop => Ok(()),
            LoopBreak => Ok(()),
            LoopContinue => Ok(()),
            FuncReturn => Ok(()),

            ModuleImport => Ok(()),
            ModuleInclude => Ok(()),

            // Ignored comments
            LineComment => Ok(()),
            BlockComment => Ok(()),
        }
    }

    fn reduce(&mut self, node: &SyntaxNode, s: &mut EcoString) -> Result<()> {
        for child in node.children() {
            self.convert_to(child, s)?;
        }

        Ok(())
    }

    fn absorb(u: Result<EcoString>, v: &mut EcoString) -> Result<()> {
        v.push_str(&u?);
        Ok(())
    }

    fn char(arg: char, s: &mut EcoString) -> Result<()> {
        s.push(arg);
        Ok(())
    }

    fn str(node: &SyntaxNode, s: &mut EcoString) -> Result<()> {
        s.push_str(node.clone().into_text().as_str());
        Ok(())
    }

    fn value(res: Value) -> EcoString {
        let Value::Content(content) = res else {
            return eco_format!("{res:?}");
        };

        content
    }

    fn escape(node: &SyntaxNode, s: &mut EcoString) -> Result<()> {
        // todo: escape characters
        Self::str(node, s)
    }

    fn shorthand(node: &SyntaxNode, s: &mut EcoString) -> Result<()> {
        // todo: shorthands
        Self::str(node, s)
    }

    fn strong(&mut self, node: &SyntaxNode, s: &mut EcoString) -> Result<()> {
        let strong = node.cast::<ast::Strong>().unwrap();
        s.push_str("**");
        self.convert_to(strong.body().to_untyped(), s)?;
        s.push_str("**");
        Ok(())
    }

    fn emph(&mut self, node: &SyntaxNode, s: &mut EcoString) -> Result<()> {
        let emph = node.cast::<ast::Emph>().unwrap();
        s.push('_');
        self.convert_to(emph.body().to_untyped(), s)?;
        s.push('_');
        Ok(())
    }

    fn heading(&mut self, node: &SyntaxNode, s: &mut EcoString) -> Result<()> {
        let heading = node.cast::<ast::Heading>().unwrap();
        let level = heading.depth();
        for _ in 0..level.get() {
            s.push('#');
        }
        s.push(' ');
        self.convert_to(heading.body().to_untyped(), s)
    }

    fn raw(node: &SyntaxNode, s: &mut EcoString) -> Result<()> {
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

    fn link(&mut self, node: &SyntaxNode, s: &mut EcoString) -> Result<()> {
        // GFM supports autolinks
        if self.gfm {
            return Self::str(node, s);
        }
        s.push('[');
        Self::str(node, s)?;
        s.push(']');
        s.push('(');
        Self::str(node, s)?;
        s.push(')');

        Ok(())
    }

    fn label(node: &SyntaxNode, s: &mut EcoString) -> Result<()> {
        Self::str(node, s)
    }

    fn label_ref(node: &SyntaxNode, s: &mut EcoString) -> Result<()> {
        Self::str(node, s)
    }

    fn ref_marker(node: &SyntaxNode, s: &mut EcoString) -> Result<()> {
        Self::str(node, s)
    }

    fn list_item(&mut self, node: &SyntaxNode, s: &mut EcoString) -> Result<()> {
        self.reduce(node, s)
    }

    fn enum_item(&mut self, node: &SyntaxNode, s: &mut EcoString) -> Result<()> {
        let enum_item = node.cast::<ast::EnumItem>().unwrap();
        if let Some(num) = enum_item.number() {
            write!(s, "{num}. ").map_err(|_| "cannot write enum item number")?;
        } else {
            s.push_str("1. ");
        }
        self.convert_to(enum_item.body().to_untyped(), s)
    }

    fn term_item(&mut self, node: &SyntaxNode, s: &mut EcoString) -> Result<()> {
        self.reduce(node, s)
    }

    #[cfg(not(feature = "texmath"))]
    fn equation(node: &SyntaxNode, s: &mut EcoString) -> Result<()> {
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

    fn let_binding(&self, node: &SyntaxNode, s: &mut EcoString) -> Result<()> {
        let _ = node;
        let _ = s;

        Ok(())
    }

    fn field_access(&self, node: &SyntaxNode, s: &mut EcoString) -> Result<()> {
        let _ = node;
        let _ = s;

        Ok(())
    }

    fn func_call(&mut self, node: &SyntaxNode) -> Result<EcoString> {
        let c: ast::FuncCall = node.cast().unwrap();

        let callee = match c.callee() {
            ast::Expr::Ident(callee) => self.scopes.get(callee.get()),
            ast::Expr::FieldAccess(..) => return Ok(EcoString::new()),
            _ => return Ok(EcoString::new()),
        }?;

        let Value::RawFunc(func) = callee else {
            return Err("callee is not a function")?;
        };

        Ok(Self::value(func(ArgGetter::new(self, c.args()))?))
    }

    fn contextual(&self, node: &SyntaxNode, s: &mut EcoString) -> Result<()> {
        let _ = node;
        let _ = s;

        Ok(())
    }
}

#[cfg(test)]
mod tests;
