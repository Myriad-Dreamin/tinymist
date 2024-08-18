//! # Typlite

mod error;
mod library;
pub mod scopes;
mod value;
// mod world;

use std::sync::{Arc, LazyLock};

pub use error::*;

use base64::Engine;
use scopes::Scopes;
use tinymist_world::{
    base::ShadowApi, CompileFontArgs, EntryReader, EntryState, FontResolverImpl,
    LspUniverseBuilder, LspWorld,
};
use typst::{eval::Tracer, foundations::Bytes, layout::Abs};
use value::{Args, Value};
// use world::LiteWorld;

use ecow::{eco_format, EcoString};
use typst_syntax::{
    ast::{self, AstNode},
    FileId, Source, SyntaxKind, SyntaxNode, VirtualPath,
};

/// The result type for typlite.
pub type Result<T, Err = Error> = std::result::Result<T, Err>;

pub use tinymist_world::CompileOnceArgs;

/// Task builder for converting a typst document to Markdown.
pub struct Typlite {
    /// The document to convert.
    main: Source,
    /// Whether to enable GFM (GitHub Flavored Markdown) features.
    gfm: bool,
    /// The universe to use for the conversion.
    world: Option<LspWorld>,
}

impl Typlite {
    /// Create a new Typlite instance from a string.
    /// # Example
    /// ```rust
    /// use typlite::Typlite;
    /// let content = "= Hello, World";
    /// let res = Typlite::new_with_content(content).convert();
    /// assert!(matches!(res, Ok(e) if e == "# Hello, World"));
    /// ```
    pub fn new_with_content(content: &str) -> Self {
        let main = Source::detached(content);
        Self {
            main,
            gfm: false,
            world: None,
        }
    }

    /// Create a new Typlite instance from a [`Source`].
    ///
    /// This is useful when you have a [`Source`] instance and you can avoid
    /// reparsing the content.
    pub fn new_with_src(main: Source) -> Self {
        Self {
            main,
            gfm: false,
            world: None,
        }
    }

    /// With a common world.
    pub fn with_world(mut self, world: LspWorld) -> Self {
        self.world = Some(world);
        self
    }

    /// Convert the content to a markdown string.
    pub fn convert(self) -> Result<EcoString> {
        static FONT_RESOLVER: LazyLock<Result<Arc<FontResolverImpl>>> = LazyLock::new(|| {
            Ok(Arc::new(
                LspUniverseBuilder::resolve_fonts(CompileFontArgs::default())
                    .map_err(|e| format!("{e:?}"))?,
            ))
        });

        let world = match self.world {
            Some(u) => u,
            None => {
                let font_resolver = FONT_RESOLVER.clone();
                let cwd = std::env::current_dir().map_err(|e| format!("{e:?}"))?;
                let u = LspUniverseBuilder::build(
                    EntryState::new_workspace(cwd.as_path().into()),
                    font_resolver?,
                    Default::default(),
                )
                .map_err(|e| format!("{e:?}"))?;
                u.snapshot()
            }
        };

        let mut worker = TypliteWorker {
            gfm: self.gfm,
            scopes: library::library(),
            world,
        };

        worker.convert(self.main.root())
    }
}

struct TypliteWorker {
    gfm: bool,
    scopes: Scopes<Value>,
    world: LspWorld,
}

impl TypliteWorker {
    /// Convert the content to a markdown string.
    pub fn convert(&mut self, node: &SyntaxNode) -> Result<EcoString> {
        Ok(Self::value(self.eval(node)?))
    }

    /// Eval the content
    pub fn eval(&mut self, node: &SyntaxNode) -> Result<Value> {
        use SyntaxKind::*;
        match node.kind() {
            RawLang | RawDelim | RawTrimmed => Err("converting clause")?,

            Math | MathIdent | MathAlignPoint | MathDelimited | MathAttach | MathPrimes
            | MathFrac | MathRoot => Err("converting math node")?,

            // Error nodes
            Error => Err(node.clone().into_text().to_string())?,
            Eof | None => Ok(Value::None),

            // Non-leaf nodes
            Markup => self.reduce(node),
            Code => self.reduce(node),
            Equation => self.equation(node),
            CodeBlock => {
                let code_block: ast::CodeBlock = node.cast().unwrap();
                self.eval(code_block.body().to_untyped())
            }
            ContentBlock => {
                let content_block: ast::ContentBlock = node.cast().unwrap();
                self.eval(content_block.body().to_untyped())
            }
            Parenthesized => {
                let parenthesized: ast::Parenthesized = node.cast().unwrap();
                // self.convert_to(parenthesized.expr().to_untyped(), )
                self.eval(parenthesized.expr().to_untyped())
            }

            // Text nodes
            Text | Space | Linebreak | Parbreak => Self::str(node),

            // Semantic nodes
            Escape => Self::escape(node),
            Shorthand => Self::shorthand(node),
            SmartQuote => Self::str(node),
            Strong => self.strong(node),
            Emph => self.emph(node),
            Raw => Self::raw(node),
            Link => self.link(node),
            Label => Self::label(node),
            Ref => Self::label_ref(node),
            RefMarker => Self::ref_marker(node),
            Heading => self.heading(node),
            HeadingMarker => Self::str(node),
            ListItem => self.list_item(node),
            ListMarker => Self::str(node),
            EnumItem => self.enum_item(node),
            EnumMarker => Self::str(node),
            TermItem => self.term_item(node),
            TermMarker => Self::str(node),

            // Punctuation
            // Hash => Self::char('#'),
            Hash => Ok(Value::None),
            LeftBrace => Self::char('{'),
            RightBrace => Self::char('}'),
            LeftBracket => Self::char('['),
            RightBracket => Self::char(']'),
            LeftParen => Self::char('('),
            RightParen => Self::char(')'),
            Comma => Self::char(','),
            Semicolon => Ok(Value::None),
            Colon => Self::char(':'),
            Star => Self::char('*'),
            Underscore => Self::char('_'),
            Dollar => Self::char('$'),
            Plus => Self::char('+'),
            Minus => Self::char('-'),
            Slash => Self::char('/'),
            Hat => Self::char('^'),
            Prime => Self::char('\''),
            Dot => Self::char('.'),
            Eq => Self::char('='),
            Lt => Self::char('<'),
            Gt => Self::char('>'),

            // Compound punctuation
            EqEq => Self::str(node),
            ExclEq => Self::str(node),
            LtEq => Self::str(node),
            GtEq => Self::str(node),
            PlusEq => Self::str(node),
            HyphEq => Self::str(node),
            StarEq => Self::str(node),
            SlashEq => Self::str(node),
            Dots => Self::str(node),
            Arrow => Self::str(node),
            Root => Self::str(node),

            // Keywords
            Auto => Self::str(node),
            Not => Self::str(node),
            And => Self::str(node),
            Or => Self::str(node),
            Let => Self::str(node),
            Set => Self::str(node),
            Show => Self::str(node),
            Context => Self::str(node),
            If => Self::str(node),
            Else => Self::str(node),
            For => Self::str(node),
            In => Self::str(node),
            While => Self::str(node),
            Break => Self::str(node),
            Continue => Self::str(node),
            Return => Self::str(node),
            Import => Self::str(node),
            Include => Self::str(node),
            As => Self::str(node),

            LetBinding => self.let_binding(node),
            FieldAccess => self.field_access(node),
            FuncCall => self.func_call(node),
            Contextual => self.contextual(node),

            // Clause nodes
            Named => Ok(Value::None),
            Keyed => Ok(Value::None),
            Unary => Ok(Value::None),
            Binary => Ok(Value::None),
            Spread => Ok(Value::None),
            ImportItems => Ok(Value::None),
            RenamedImportItem => Ok(Value::None),
            Closure => Ok(Value::None),
            Args => Ok(Value::None),
            Params => Ok(Value::None),

            // Ignored code expressions
            Ident => Ok(Value::None),
            Bool => Ok(Value::None),
            Int => Ok(Value::None),
            Float => Ok(Value::None),
            Numeric => Ok(Value::None),
            Str => Ok(Value::Str({
                let s: ast::Str = node.cast().unwrap();
                s.get()
            })),
            Array => Ok(Value::None),
            Dict => Ok(Value::None),

            // Ignored code expressions
            SetRule => Ok(Value::None),
            ShowRule => Ok(Value::None),
            Destructuring => Ok(Value::None),
            DestructAssignment => Ok(Value::None),

            Conditional => Ok(Value::None),
            WhileLoop => Ok(Value::None),
            ForLoop => Ok(Value::None),
            LoopBreak => Ok(Value::None),
            LoopContinue => Ok(Value::None),
            FuncReturn => Ok(Value::None),

            ModuleImport => Ok(Value::None),
            ModuleInclude => Ok(Value::None),

            // Ignored comments
            LineComment => Ok(Value::None),
            BlockComment => Ok(Value::None),
        }
    }

    fn reduce(&mut self, node: &SyntaxNode) -> Result<Value> {
        let mut s = EcoString::new();

        for child in node.children() {
            // self.convert_to(child)?;
            s.push_str(&Self::value(self.eval(child)?));
        }

        Ok(Value::Content(s))
    }

    fn render(&mut self, node: &SyntaxNode, inline: bool) -> Result<Value> {
        let color = "#c0caf5";

        let main = Bytes::from(eco_format!(
            r##"#set page(width: auto, height: auto, margin: (y: 0.45em, rest: 0em));#set text(rgb("{color}"))
{}"##,
            node.clone().into_text()
        ).as_bytes().to_owned());
        // let world = LiteWorld::new(main);
        let main_id = FileId::new(None, VirtualPath::new("__render__.typ"));
        let entry = self.world.entry_state().select_in_workspace(main_id);
        let mut world = self.world.task(tinymist_world::base::TaskInputs {
            entry: Some(entry),
            inputs: None,
        });
        world.map_shadow_by_id(main_id, main).unwrap();

        let mut tracer = Tracer::default();
        let document = typst::compile(&world, &mut tracer)
            .map_err(|e| format!("compiling math node: {e:?}"))?;

        let svg_payload = typst_svg::svg_merged(&document, Abs::zero());
        let base64 = base64::engine::general_purpose::STANDARD.encode(svg_payload);

        if inline {
            Ok(Value::Content(eco_format!(
                r#"<img style="vertical-align: -0.35em" src="data:image/svg+xml;base64,{base64}" alt="typst-block" />"#
            )))
        } else {
            Ok(Value::Content(eco_format!(
                r#"<p align="center"><img src="data:image/svg+xml;base64,{base64}" alt="typst-block" /></p>"#
            )))
        }
    }

    fn char(arg: char) -> Result<Value> {
        Ok(Value::Content(arg.into()))
    }

    fn str(node: &SyntaxNode) -> Result<Value> {
        Ok(Value::Content(node.clone().into_text()))
    }

    fn value(res: Value) -> EcoString {
        match res {
            Value::None => EcoString::new(),
            Value::Content(content) => content,
            Value::Str(s) => s,
            Value::Image { path, alt } => eco_format!("![{alt}]({path})"),
            _ => eco_format!("{res:?}"),
        }
    }

    fn escape(node: &SyntaxNode) -> Result<Value> {
        // todo: escape characters
        Self::str(node)
    }

    fn shorthand(node: &SyntaxNode) -> Result<Value> {
        // todo: shorthands
        Self::str(node)
    }

    fn strong(&mut self, node: &SyntaxNode) -> Result<Value> {
        let mut s = EcoString::new();

        let strong = node.cast::<ast::Strong>().unwrap();
        s.push_str("**");
        s.push_str(&Self::value(self.eval(strong.body().to_untyped())?));
        s.push_str("**");

        Ok(Value::Content(s))
    }

    fn emph(&mut self, node: &SyntaxNode) -> Result<Value> {
        let mut s = EcoString::new();
        let emph = node.cast::<ast::Emph>().unwrap();
        s.push('_');
        s.push_str(&Self::value(self.eval(emph.body().to_untyped())?));
        s.push('_');
        Ok(Value::Content(s))
    }

    fn heading(&mut self, node: &SyntaxNode) -> Result<Value> {
        let mut s = EcoString::new();
        let heading = node.cast::<ast::Heading>().unwrap();
        let level = heading.depth();
        for _ in 0..level.get() {
            s.push('#');
        }
        s.push(' ');
        s.push_str(&Self::value(self.eval(heading.body().to_untyped())?));
        Ok(Value::Content(s))
    }

    fn raw(node: &SyntaxNode) -> Result<Value> {
        let mut s = EcoString::new();
        let raw = node.cast::<ast::Raw>().unwrap();
        if raw.block() {
            s.push_str(&Self::value(Self::str(node)?));
            return Ok(Value::Content(s));
        }
        s.push('`');
        for e in raw.lines() {
            s.push_str(&Self::value(Self::str(e.to_untyped())?));
        }
        s.push('`');
        Ok(Value::Content(s))
    }

    fn link(&mut self, node: &SyntaxNode) -> Result<Value> {
        // GFM supports autolinks
        if self.gfm {
            // return Self::str(node, s);
            return Self::str(node);
        }
        let mut s = EcoString::new();
        s.push('[');
        s.push_str(&Self::value(Self::str(node)?));
        s.push(']');
        s.push('(');
        s.push_str(&Self::value(Self::str(node)?));
        s.push(')');

        Ok(Value::Content(s))
    }

    fn label(_node: &SyntaxNode) -> Result<Value> {
        Result::Ok(Value::None)
    }

    fn label_ref(node: &SyntaxNode) -> Result<Value> {
        Self::str(node)
    }

    fn ref_marker(node: &SyntaxNode) -> Result<Value> {
        Self::str(node)
    }

    fn list_item(&mut self, node: &SyntaxNode) -> Result<Value> {
        self.reduce(node)
    }

    fn enum_item(&mut self, node: &SyntaxNode) -> Result<Value> {
        let enum_item = node.cast::<ast::EnumItem>().unwrap();

        let body = Self::value(self.eval(enum_item.body().to_untyped())?);

        let s = if let Some(num) = enum_item.number() {
            eco_format!("{num}. ")
        } else {
            "1. ".into()
        };

        Ok(Value::Content(eco_format!("{s}{body}")))
    }

    fn term_item(&mut self, node: &SyntaxNode) -> Result<Value> {
        self.reduce(node)
    }

    fn equation(&mut self, node: &SyntaxNode) -> Result<Value> {
        let equation: ast::Equation = node.cast().unwrap();

        self.render(node, !equation.block())
    }

    fn let_binding(&self, node: &SyntaxNode) -> Result<Value> {
        let _ = node;

        Ok(Value::None)
    }

    fn field_access(&self, node: &SyntaxNode) -> Result<Value> {
        let _ = node;

        Ok(Value::None)
    }

    fn func_call(&mut self, node: &SyntaxNode) -> Result<Value> {
        let c: ast::FuncCall = node.cast().unwrap();

        let callee = match c.callee() {
            ast::Expr::Ident(callee) => self.scopes.get(callee.get()),
            ast::Expr::FieldAccess(..) => return Ok(Value::None),
            _ => return Ok(Value::None),
        }?;

        let Value::RawFunc(func) = callee else {
            return Err("callee is not a function")?;
        };

        func(Args::new(self, c.args()))
    }

    fn contextual(&self, node: &SyntaxNode) -> Result<Value> {
        let _ = node;

        Ok(Value::None)
    }
}

#[cfg(test)]
mod tests;
