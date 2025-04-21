use std::fmt::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use base64::Engine;
use ecow::{eco_format, EcoString};
use tinymist_analysis;
use tinymist_project::base::ShadowApi;
use tinymist_project::{EntryReader, LspWorld};
use typst::foundations::{Bytes, Dict, IntoValue};
use typst::layout::Abs;
use typst::syntax::{FileId, Source, SyntaxKind, SyntaxNode};
use typst::utils::LazyHash;
use typst::World;
use typst::WorldExt;
use typst_syntax::ast::{self, AstNode};
use typst_syntax::ast::{Emph, Equation, Heading, Raw, Strong};

use crate::scopes::Scopes;
use crate::tinymist_std::path::unix_slash;
use crate::value::{Args, Value};
use crate::worker::SyntaxKind::Text;
use crate::Result;
use crate::TypliteFeat;
use crate::WorkspaceResolver;

/// Typlite worker for converting syntax nodes to markdown
#[derive(Clone)]
pub struct TypliteWorker {
    pub current: FileId,
    pub scopes: Arc<Scopes<Value>>,
    pub world: Arc<LspWorld>,
    pub list_depth: usize,
    pub prepend_code: EcoString,
    pub assets_numbering: usize,
    /// Features for the conversion.
    pub feat: TypliteFeat,
}

impl TypliteWorker {
    /// Convert the content to a markdown string.
    pub fn convert(&mut self, node: &SyntaxNode) -> Result<EcoString> {
        Ok(Self::value(self.eval(node)?))
    }

    /// Eval the content
    pub fn eval(&mut self, node: &SyntaxNode) -> Result<Value> {
        use SyntaxKind::*;
        let res = match node.kind() {
            RawLang | RawDelim | RawTrimmed => Err("converting clause")?,

            Math | MathIdent | MathAlignPoint | MathDelimited | MathAttach | MathPrimes
            | MathFrac | MathRoot | MathShorthand | MathText => Err("converting math node")?,

            // Error nodes
            Error => Err(node.clone().into_text().to_string())?,
            None | End => Ok(Value::None),

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
                self.eval(parenthesized.expr().to_untyped())
            }

            // Text nodes
            Text | Space | Parbreak => Self::str(node),
            Linebreak => Self::char('\n'),

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
            ImportItemPath => Ok(Value::None),
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
            ModuleInclude => self.include(node),

            // Ignored comments
            LineComment => Ok(Value::None),
            BlockComment => Ok(Value::None),
            Shebang => Ok(Value::None),
        };
        if res.clone()? == Value::None
            && !matches!(
                node.kind(),
                Ident | Bool | Int | Float | Numeric | Str | Array | Dict
            )
        {
            self.prepend_code += node.clone().into_text();
            if node.kind() != Hash {
                self.prepend_code += "\n"
            };
        }
        res
    }

    fn reduce(&mut self, node: &SyntaxNode) -> Result<Value> {
        let mut s = EcoString::new();

        for child in node.children() {
            s.push_str(&Self::value(self.eval(child)?));
        }

        Ok(Value::Content(s))
    }

    pub fn to_raw_block(&mut self, node: &SyntaxNode, inline: bool) -> Result<Value> {
        let content = node.clone().into_text();

        let s = if inline {
            let mut s = EcoString::with_capacity(content.len() + 2);
            s.push_str("`");
            s.push_str(&content);
            s.push_str("`");
            s
        } else {
            let mut s = EcoString::with_capacity(content.len() + 15);
            s.push_str("```");
            let lang = match node.cast::<ast::Expr>() {
                Some(ast::Expr::Text(..) | ast::Expr::Space(..)) => "typ",
                Some(..) => "typc",
                None => "typ",
            };
            s.push_str(lang);
            s.push('\n');
            s.push_str(&content);
            s.push('\n');
            s.push_str("```");
            s
        };

        Ok(Value::Content(s))
    }

    pub fn render(
        &mut self,
        prepend_node: &SyntaxNode,
        node: &SyntaxNode,
        inline: bool,
    ) -> Result<Value> {
        self.assets_numbering += 1;
        let prepend_code = prepend_node.clone().into_text();
        let code = node.clone().into_text();
        if let Some(assets_src_path) = &self.feat.assets_src_path {
            let file_name = assets_src_path
                .join(self.assets_numbering.to_string())
                .with_extension("typ");
            if let Err(e) = std::fs::write(&file_name, format!("#{{\n// render_code\n{}\n}}", code))
            {
                return Err(format!("failed to write code to file: {}", e).into());
            }
        }
        self.render_code(&prepend_code, &code, false, "center", "", inline)
    }

    pub fn render_code(
        &mut self,
        prepend_code: &str,
        code: &str,
        is_markup: bool,
        align: &str,
        extra_attrs: &str,
        inline: bool,
    ) -> Result<Value> {
        let theme = self.feat.color_theme;

        let code_file_name = if let Some(assets_src_path) = &self.feat.assets_src_path {
            Some(
                assets_src_path
                    .join(self.assets_numbering.to_string())
                    .with_extension("typ"),
            )
        } else {
            None
        };

        let mut render = |theme| self.render_inner(prepend_code, code, is_markup, theme);

        let mut content = EcoString::new();

        let inline_attrs = if inline {
            r#" style="vertical-align: -0.35em""#
        } else {
            ""
        };

        let write_error = |content: &mut EcoString, err: &str| {
            let err = err.replace("`", r#"\`"#);
            let _ = write!(content, "```\nRender Error\n{err}\n```");
        };

        let write_image = |content: &mut EcoString,
                           file_name: &std::path::Path,
                           code_file_name: Option<&PathBuf>,
                           inline_attrs: &str,
                           extra_attrs: &str| {
            if let Some(code_file_name) = code_file_name {
                let _ = write!(
                    content,
                    r#"<a href="{}"><img{inline_attrs} alt="typst-block" src="{}" {extra_attrs}/></a>"#,
                    code_file_name.display(),
                    file_name.display()
                );
            } else {
                let _ = write!(
                    content,
                    r#"<img{inline_attrs} alt="typst-block" src="{}" {extra_attrs}/>"#,
                    file_name.display()
                );
            }
        };

        let write_picture = |content: &mut EcoString,
                             dark_file_name: &std::path::Path,
                             light_file_name: &std::path::Path,
                             code_file_name: Option<&PathBuf>,
                             inline_attrs: &str,
                             extra_attrs: &str| {
            if let Some(code_file_name) = code_file_name {
                let _ = write!(
                    content,
                    r#"<a href="{}"><picture><source media="(prefers-color-scheme: dark)" srcset="{}"><img{inline_attrs} alt="typst-block" src="{}" {extra_attrs}/></picture></a>"#,
                    code_file_name.display(),
                    dark_file_name.display(),
                    light_file_name.display()
                );
            } else {
                let _ = write!(
                    content,
                    r#"<picture><source media="(prefers-color-scheme: dark)" srcset="{}"><img{inline_attrs} alt="typst-block" src="{}" {extra_attrs}/></picture>"#,
                    dark_file_name.display(),
                    light_file_name.display()
                );
            }
        };

        match theme {
            Some(theme) => {
                let data = match render(theme) {
                    Ok(data) => data,
                    Err(err) if self.feat.soft_error => {
                        write_error(&mut content, &err.to_string());
                        return Ok(Value::Content(content));
                    }
                    Err(err) => return Err(err),
                };

                if !inline {
                    let _ = write!(content, r#"<p align="{align}">"#);
                }
                if let Some(assets_path) = &self.feat.assets_path {
                    let file_name =
                        assets_path.join(format!("{}_{:?}.svg", self.assets_numbering, theme));
                    std::fs::write(&file_name, &data)
                        .map_err(|e| format!("failed to write SVG to file: {}", e))?;

                    write_image(
                        &mut content,
                        &file_name,
                        code_file_name.as_ref(),
                        inline_attrs,
                        extra_attrs,
                    );
                } else {
                    let _ = write!(
                        content,
                        r#"<img{inline_attrs} alt="typst-block" src="data:image/svg+xml;base64,{data}" {extra_attrs}/>"#
                    );
                }
                if !inline {
                    content.push_str("</p>");
                }
            }
            None => {
                let dark = match render(crate::ColorTheme::Dark) {
                    Ok(d) => d,
                    Err(err) if self.feat.soft_error => {
                        write_error(&mut content, &err.to_string());
                        return Ok(Value::Content(content));
                    }
                    Err(err) => return Err(err),
                };
                let light = match render(crate::ColorTheme::Light) {
                    Ok(l) => l,
                    Err(err) if self.feat.soft_error => {
                        write_error(&mut content, &err.to_string());
                        return Ok(Value::Content(content));
                    }
                    Err(err) => return Err(err),
                };

                if !inline {
                    let _ = write!(content, r#"<p align="{align}">"#);
                }
                if let Some(assets_path) = &self.feat.assets_path {
                    let dark_file_name = assets_path.join(format!(
                        "{}_{:?}.svg",
                        self.assets_numbering,
                        crate::ColorTheme::Dark
                    ));
                    let light_file_name = assets_path.join(format!(
                        "{}_{:?}.svg",
                        self.assets_numbering,
                        crate::ColorTheme::Light
                    ));

                    write_picture(
                        &mut content,
                        &dark_file_name,
                        &light_file_name,
                        code_file_name.as_ref(),
                        inline_attrs,
                        extra_attrs,
                    );
                } else {
                    let _ = write!(
                        content,
                        r#"<picture><source media="(prefers-color-scheme: dark)" srcset="data:image/svg+xml;base64,{dark}"><img{inline_attrs} alt="typst-block" src="data:image/svg+xml;base64,{light}" {extra_attrs}/></picture>"#
                    );
                }
                if !inline {
                    content.push_str("</p>");
                }
            }
        }

        Ok(Value::Content(content))
    }

    fn render_inner(
        &mut self,
        prepend_code: &str,
        code: &str,
        is_markup: bool,
        theme: crate::ColorTheme,
    ) -> Result<String> {
        static DARK_THEME_INPUT: LazyLock<Arc<LazyHash<Dict>>> = LazyLock::new(|| {
            Arc::new(LazyHash::new(Dict::from_iter(std::iter::once((
                "x-color-theme".into(),
                "dark".into_value(),
            )))))
        });

        let code = WrapCode(code, is_markup);
        let inputs = match theme {
            crate::ColorTheme::Dark => Some(DARK_THEME_INPUT.clone()),
            crate::ColorTheme::Light => None,
        };
        let code = eco_format!(
            r##"{prepend_code}
            #set page(width: auto, height: auto, margin: (y: 0.45em, rest: 0em), fill: none);
            #set text(fill: rgb("#c0caf5")) if sys.inputs.at("x-color-theme", default: none) == "dark";
            {code}"##
        );
        let main = Bytes::new(code.as_bytes().to_owned());

        let path = Path::new("__render__.typ");
        let entry = self.world.entry_state().select_in_workspace(path);
        let mut world = self.world.task(tinymist_project::TaskInputs {
            entry: Some(entry),
            inputs,
        });
        world.take_db();
        world.map_shadow_by_id(world.main(), main).unwrap();

        let document = typst::compile(&world).output;
        let document = document.map_err(|diagnostics| {
            let mut err = String::new();
            let _ = write!(err, "compiling node: ");
            let write_span = |span: typst_syntax::Span, err: &mut String| {
                let file = span.id().map(|id| match id.package() {
                    Some(package) if WorkspaceResolver::is_package_file(id) => {
                        format!("{package}:{}", unix_slash(id.vpath().as_rooted_path()))
                    }
                    Some(_) | None => unix_slash(id.vpath().as_rooted_path()),
                });
                let range = world.range(span);
                match (file, range) {
                    (Some(file), Some(range)) => {
                        let _ = write!(err, "{file:?}:{range:?}");
                    }
                    (Some(file), None) => {
                        let _ = write!(err, "{file:?}");
                    }
                    (None, Some(range)) => {
                        let _ = write!(err, "{range:?}");
                    }
                    _ => {
                        let _ = write!(err, "unknown location");
                    }
                }
            };

            for s in diagnostics.iter() {
                match s.severity {
                    typst::diag::Severity::Error => {
                        let _ = write!(err, "error: ");
                    }
                    typst::diag::Severity::Warning => {
                        let _ = write!(err, "warning: ");
                    }
                }

                err.push_str(&s.message);
                err.push_str(" at ");
                write_span(s.span, &mut err);

                for hint in s.hints.iter() {
                    err.push_str("\nHint: ");
                    err.push_str(hint);
                }

                for trace in &s.trace {
                    write!(err, "\nTrace: {} at ", trace.v).unwrap();
                    write_span(trace.span, &mut err);
                }

                err.push('\n');
            }

            err
        })?;

        let svg_payload = typst_svg::svg_merged(&document, Abs::zero());

        if let Some(assets_path) = &self.feat.assets_path {
            let file_name = assets_path.join(format!("{}_{:?}.svg", self.assets_numbering, theme));
            if let Err(e) = std::fs::write(&file_name, &svg_payload) {
                return Err(format!("failed to write SVG to file: {}", e).into());
            }
            Ok(file_name.to_string_lossy().to_string())
        } else {
            Ok(base64::engine::general_purpose::STANDARD.encode(svg_payload))
        }
    }

    fn char(arg: char) -> Result<Value> {
        Ok(Value::Content(arg.into()))
    }

    fn str(node: &SyntaxNode) -> Result<Value> {
        Ok(Value::Content(node.clone().into_text()))
    }

    pub fn value(res: Value) -> EcoString {
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

        let strong = node.cast::<Strong>().unwrap();
        s.push_str("**");
        s.push_str(&Self::value(self.eval(strong.body().to_untyped())?));
        s.push_str("**");

        Ok(Value::Content(s))
    }

    fn emph(&mut self, node: &SyntaxNode) -> Result<Value> {
        let mut s = EcoString::new();
        let emph = node.cast::<Emph>().unwrap();
        s.push('_');
        s.push_str(&Self::value(self.eval(emph.body().to_untyped())?));
        s.push('_');
        Ok(Value::Content(s))
    }

    fn heading(&mut self, node: &SyntaxNode) -> Result<Value> {
        let mut s = EcoString::new();
        let heading = node.cast::<Heading>().unwrap();
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
        let raw = node.cast::<Raw>().unwrap();

        // Raw codes with typlite language will not be treated as a code block but
        // directly output into the Markdown result.
        if let Some(lang) = raw.lang() {
            if &EcoString::from("typlite") == lang.get() {
                for line in raw.lines() {
                    s.push_str(&Self::value(Self::str(line.to_untyped())?));
                    s.push('\n');
                }
                return Ok(Value::Content(s));
            }
        }

        if raw.block() {
            s.push_str(&Self::value(Self::str(node)?));
            return Ok(Value::Content(s));
        }
        s.push('`');
        for line in raw.lines() {
            s.push_str(&Self::value(Self::str(line.to_untyped())?));
        }
        s.push('`');
        Ok(Value::Content(s))
    }

    fn link(&mut self, node: &SyntaxNode) -> Result<Value> {
        // GFM supports autolinks
        if self.feat.gfm {
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
        let mut s = EcoString::new();

        let list_item = node.cast::<ast::ListItem>().unwrap();

        // 添加适当的缩进
        for _ in 0..self.list_depth {
            s.push_str("  ");
        }
        
        s.push_str("- ");
        if self.feat.annotate_elem {
            let _ = write!(s, "<!-- typlite:begin:list-item {} -->", self.list_depth);
            self.list_depth += 1;
        }
        s.push_str(&Self::value(self.eval(list_item.body().to_untyped())?));
        if self.feat.annotate_elem {
            self.list_depth -= 1;
            let _ = write!(s, "<!-- typlite:end:list-item {} -->", self.list_depth);
        }

        Ok(Value::Content(s))
    }

    fn enum_item(&mut self, node: &SyntaxNode) -> Result<Value> {
        let enum_item = node.cast::<ast::EnumItem>().unwrap();
        let mut s = EcoString::new();
        
        // 添加适当的缩进
        for _ in 0..self.list_depth {
            s.push_str("  ");
        }
        
        if self.feat.annotate_elem {
            let _ = write!(s, "<!-- typlite:begin:enum-item {} -->", self.list_depth);
            self.list_depth += 1;
        }
        
        // 获取枚举项的编号（如果有）或默认为 1
        if let Some(num) = enum_item.number() {
            s.push_str(&format!("{}. ", num));
        } else {
            s.push_str("1. ");
        }
        
        s.push_str(&Self::value(self.eval(enum_item.body().to_untyped())?));
        
        if self.feat.annotate_elem {
            self.list_depth -= 1;
            let _ = write!(s, "<!-- typlite:end:enum-item {} -->", self.list_depth);
        }

        Ok(Value::Content(s))
    }

    fn term_item(&mut self, node: &SyntaxNode) -> Result<Value> {
        self.reduce(node)
    }

    fn equation(&mut self, node: &SyntaxNode) -> Result<Value> {
        let equation: Equation = node.cast().unwrap();

        if self.feat.remove_html {
            return self.to_raw_block(node, !equation.block());
        }

        self.render(&SyntaxNode::leaf(Text, ""), node, !equation.block())
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

    fn contextual(&mut self, node: &SyntaxNode) -> Result<Value> {
        if self.feat.remove_html {
            return self.to_raw_block(node, false);
        }
        // Trim the last `#` in the prepend code. (#context)
        self.prepend_code = self.prepend_code.trim_end_matches('#').into();
        self.render(
            &SyntaxNode::leaf(node.kind(), self.prepend_code.clone()),
            node,
            false,
        )
    }

    fn include(&self, node: &SyntaxNode) -> Result<Value> {
        let include: ast::ModuleInclude = node.cast().unwrap();

        let path = include.source();
        let src =
            tinymist_analysis::syntax::find_source_by_expr(self.world.as_ref(), self.current, path)
                .ok_or_else(|| format!("failed to find source on path {path:?}"))?;

        self.clone().sub_file(src).map(Value::Content)
    }

    fn sub_file(mut self, src: Source) -> Result<EcoString> {
        self.current = src.id();
        self.convert(src.root())
    }
}

struct WrapCode<'a>(&'a str, bool);

impl fmt::Display for WrapCode<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let is_markup = self.1;
        if is_markup {
            f.write_str("#[")?;
        } else {
            f.write_str("#{")?;
        }
        f.write_str(self.0)?;
        if is_markup {
            f.write_str("]")
        } else {
            f.write_str("}")
        }
    }
}
