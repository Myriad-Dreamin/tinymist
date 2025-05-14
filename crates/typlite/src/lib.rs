//! # Typlite

pub mod attributes;
pub mod common;
mod error;
pub mod library;
pub mod parser;
pub mod tags;
pub mod writer;

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

pub use error::*;

use cmark_writer::ast::Node;
use tinymist_project::base::ShadowApi;
use tinymist_project::vfs::WorkspaceResolver;
use tinymist_project::{EntryReader, LspWorld, TaskInputs};
use tinymist_std::error::prelude::*;
use typst::foundations::Bytes;
use typst::html::HtmlDocument;
use typst::World;
use typst_syntax::VirtualPath;
use writer::LaTeXWriter;

use crate::common::Format;
use crate::parser::HtmlToAstParser;
use crate::writer::WriterFactory;
use typst_syntax::FileId;

use crate::tinymist_std::typst::foundations::Value::Str;
use crate::tinymist_std::typst::{LazyHash, TypstDict};

/// The result type for typlite.
pub type Result<T, Err = Error> = std::result::Result<T, Err>;

pub use cmark_writer::ast;
pub use tinymist_project::CompileOnceArgs;
pub use tinymist_std;

#[derive(Debug, Clone)]
pub struct MarkdownDocument {
    pub base: HtmlDocument,
    feat: TypliteFeat,
    ast: Option<Node>,
}

impl MarkdownDocument {
    /// Create a new MarkdownDocument instance
    pub fn new(base: HtmlDocument, feat: TypliteFeat) -> Self {
        Self {
            base,
            feat,
            ast: None,
        }
    }

    /// Create a MarkdownDocument instance with pre-parsed AST
    pub fn with_ast(base: HtmlDocument, feat: TypliteFeat, ast: Node) -> Self {
        Self {
            base,
            feat,
            ast: Some(ast),
        }
    }

    /// Parse HTML document to AST
    pub fn parse(&self) -> tinymist_std::Result<Node> {
        if let Some(ast) = &self.ast {
            return Ok(ast.clone());
        }
        let parser = HtmlToAstParser::new(self.feat.clone());
        parser.parse(&self.base.root).context_ut("failed to parse")
    }

    /// Convert content to markdown string
    pub fn to_md_string(&self) -> tinymist_std::Result<ecow::EcoString> {
        let mut output = ecow::EcoString::new();
        let ast = self.parse()?;

        let mut writer = WriterFactory::create(Format::Md);
        writer
            .write_eco(&ast, &mut output)
            .context_ut("failed to write")?;

        Ok(output)
    }

    /// Convert content to plain text string
    pub fn to_text_string(&self) -> tinymist_std::Result<ecow::EcoString> {
        let mut output = ecow::EcoString::new();
        let ast = self.parse()?;

        let mut writer = WriterFactory::create(Format::Text);
        writer
            .write_eco(&ast, &mut output)
            .context_ut("failed to write")?;

        Ok(output)
    }

    /// Convert the content to a LaTeX string.
    pub fn to_tex_string(&self, prelude: bool) -> tinymist_std::Result<ecow::EcoString> {
        let mut output = ecow::EcoString::new();
        let ast = self.parse()?;

        let mut writer = WriterFactory::create(Format::LaTeX);
        if prelude {
            LaTeXWriter::default_prelude(&mut output);
        }
        writer
            .write_eco(&ast, &mut output)
            .context_ut("failed to write")?;

        Ok(output)
    }

    /// Convert the content to a DOCX document
    #[cfg(feature = "docx")]
    pub fn to_docx(&self) -> tinymist_std::Result<Vec<u8>> {
        let ast = self.parse()?;

        let mut writer = WriterFactory::create(Format::Docx);
        writer.write_vec(&ast).context_ut("failed to write")
    }
}

/// A color theme for rendering the content. The valid values can be checked in [color-scheme](https://developer.mozilla.org/en-US/docs/Web/CSS/color-scheme).
#[derive(Debug, Default, Clone, Copy)]
pub enum ColorTheme {
    #[default]
    Light,
    Dark,
}

#[derive(Debug, Default, Clone)]
pub struct TypliteFeat {
    /// The preferred color theme.
    pub color_theme: Option<ColorTheme>,
    /// The path of external assets directory.
    pub assets_path: Option<PathBuf>,
    /// Allows GFM (GitHub Flavored Markdown) markups.
    pub gfm: bool,
    /// Annotate the elements for identification.
    pub annotate_elem: bool,
    /// Embed errors in the output instead of yielding them.
    pub soft_error: bool,
    /// Remove HTML tags from the output.
    pub remove_html: bool,
    /// The target to convert
    pub target: Format,
}

/// Task builder for converting a typst document to Markdown.
pub struct Typlite {
    /// The universe to use for the conversion.
    world: Arc<LspWorld>,
    /// Features for the conversion.
    feat: TypliteFeat,
    /// The format to use for the conversion.
    format: Format,
}

impl Typlite {
    /// Create a new Typlite instance from a [`World`].
    ///
    /// This is useful when you have a [`Source`] instance and you can avoid
    /// reparsing the content.
    pub fn new(world: Arc<LspWorld>) -> Self {
        Self {
            world,
            feat: Default::default(),
            format: Format::Md,
        }
    }

    /// Set conversion feature
    pub fn with_feature(mut self, feat: TypliteFeat) -> Self {
        self.feat = feat;
        self
    }

    pub fn with_format(mut self, format: Format) -> Self {
        self.format = format;
        self
    }

    /// Convert the content to a markdown string.
    pub fn convert(self) -> tinymist_std::Result<ecow::EcoString> {
        match self.format {
            Format::Md => self.convert_doc(Format::Md)?.to_md_string(),
            Format::LaTeX => self.convert_doc(Format::LaTeX)?.to_tex_string(true),
            Format::Text => self.convert_doc(Format::Text)?.to_text_string(),
            #[cfg(feature = "docx")]
            Format::Docx => bail!("docx format is not supported"),
        }
    }

    /// Convert the content to a DOCX document
    #[cfg(feature = "docx")]
    pub fn to_docx(self) -> tinymist_std::Result<Vec<u8>> {
        if self.format != Format::Docx {
            bail!("format is not DOCX");
        }
        self.convert_doc(Format::Docx)?.to_docx()
    }

    /// Convert the content to a markdown document.
    pub fn convert_doc(self, format: Format) -> tinymist_std::Result<MarkdownDocument> {
        let entry = self.world.entry_state();
        let main = entry.main();
        let current = main.context("no main file in workspace")?;
        let world = self.world;

        if WorkspaceResolver::is_package_file(current) {
            bail!("package file is not supported");
        }

        let wrap_main_id = current.join("__wrap_md_main.typ");
        let wrap_main_path = world
            .path_for_id(wrap_main_id)
            .context_ut("getting source for main file")?;

        let task_inputs = TaskInputs {
            entry: Some(entry.select_in_workspace(wrap_main_id.vpath().as_rooted_path())),
            inputs: if format == Format::Text || self.feat.remove_html {
                let mut dict = TypstDict::new();
                dict.insert("x-remove-html".into(), Str("true".into()));
                Some(Arc::new(LazyHash::new(dict)))
            } else {
                None
            },
        };

        let mut world = world.task(task_inputs).html_task().into_owned();

        let markdown_id = FileId::new(
            Some(
                typst_syntax::package::PackageSpec::from_str("@local/markdown:0.1.0")
                    .context_ut("failed to import markdown package")?,
            ),
            VirtualPath::new("lib.typ"),
        );

        world
            .map_shadow_by_id(
                markdown_id.join("typst.toml"),
                Bytes::from_string(include_str!("markdown-typst.toml")),
            )
            .context_ut("cannot map markdown-typst.toml")?;
        world
            .map_shadow_by_id(
                markdown_id,
                Bytes::from_string(include_str!("markdown.typ")),
            )
            .context_ut("cannot map markdown.typ")?;

        world
            .map_shadow(
                wrap_main_path.as_path(),
                Bytes::from_string(format!(
                    r#"#import "@local/markdown:0.1.0": md-doc, example
#show: md-doc
{}"#,
                    world
                        .source(current)
                        .context_ut("failed to get main file content")?
                        .text()
                )),
            )
            .context_ut("cannot map source for main file")?;

        // todo: ignoring warnings
        let base = typst::compile(&world).output?;
        let mut feat = self.feat;
        feat.target = format;
        Ok(MarkdownDocument::new(base, feat))
    }
}

#[cfg(test)]
mod tests;
