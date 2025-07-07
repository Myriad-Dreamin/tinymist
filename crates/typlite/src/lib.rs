//! # Typlite

pub mod attributes;
pub mod common;
mod error;
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

pub use crate::common::Format;
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

#[derive(Clone)]
pub struct MarkdownDocument {
    pub base: HtmlDocument,
    world: Arc<LspWorld>,
    feat: TypliteFeat,
    ast: Option<Node>,
}

impl MarkdownDocument {
    /// Create a new MarkdownDocument instance
    pub fn new(base: HtmlDocument, world: Arc<LspWorld>, feat: TypliteFeat) -> Self {
        Self {
            base,
            world,
            feat,
            ast: None,
        }
    }

    /// Create a MarkdownDocument instance with pre-parsed AST
    pub fn with_ast(
        base: HtmlDocument,
        world: Arc<LspWorld>,
        feat: TypliteFeat,
        ast: Node,
    ) -> Self {
        Self {
            base,
            world,
            feat,
            ast: Some(ast),
        }
    }

    /// Parse HTML document to AST
    pub fn parse(&self) -> tinymist_std::Result<Node> {
        if let Some(ast) = &self.ast {
            return Ok(ast.clone());
        }
        let parser = HtmlToAstParser::new(self.feat.clone(), &self.world);
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
    pub fn to_tex_string(&self) -> tinymist_std::Result<ecow::EcoString> {
        let mut output = ecow::EcoString::new();
        let ast = self.parse()?;

        let mut writer = WriterFactory::create(Format::LaTeX);
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
    /// Import context for code examples (e.g., "#import \"/path/to/file.typ\":
    /// *")
    pub import_context: Option<String>,
    /// Specifies the package to process markup.
    ///
    /// ## `article` function
    ///
    /// The article function is used to wrap the typst content during
    /// compilation.
    ///
    /// typlite exactly uses the `#article` function to process the content as
    /// follow:
    ///
    /// ```typst
    /// #import "@local/processor": article
    /// #article(include "the-processed-content.typ")
    /// ```
    ///
    /// It resembles the regular typst show rule function, like `#show:
    /// article`.
    pub processor: Option<String>,
}

impl TypliteFeat {
    pub fn prepare_world(
        &self,
        world: &LspWorld,
        format: Format,
    ) -> tinymist_std::Result<LspWorld> {
        let entry = world.entry_state();
        let main = entry.main();
        let current = main.context("no main file in workspace")?;

        if WorkspaceResolver::is_package_file(current) {
            bail!("package file is not supported");
        }

        let wrap_main_id = current.join("__wrap_md_main.typ");

        let (main_id, main_content) = match self.processor.as_ref() {
            None => (wrap_main_id, None),
            Some(processor) => {
                let main_id = current.join("__md_main.typ");
                let content = format!(
                    r#"#import {processor:?}: article
#article(include "__wrap_md_main.typ")"#
                );

                (main_id, Some(Bytes::from_string(content)))
            }
        };

        let mut dict = TypstDict::new();
        dict.insert("x-target".into(), Str("md".into()));
        if format == Format::Text || self.remove_html {
            dict.insert("x-remove-html".into(), Str("true".into()));
        }

        let task_inputs = TaskInputs {
            entry: Some(entry.select_in_workspace(main_id.vpath().as_rooted_path())),
            inputs: Some(Arc::new(LazyHash::new(dict))),
        };

        let mut world = world.task(task_inputs).html_task().into_owned();

        let markdown_id = FileId::new(
            Some(
                typst_syntax::package::PackageSpec::from_str("@local/_markdown:0.1.0")
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
            .map_shadow_by_id(
                wrap_main_id,
                Bytes::from_string(format!(
                    r#"#import "@local/_markdown:0.1.0": md-doc, example; #show: md-doc
{}"#,
                    world
                        .source(current)
                        .context_ut("failed to get main file content")?
                        .text()
                )),
            )
            .context_ut("cannot map source for main file")?;

        if let Some(main_content) = main_content {
            world
                .map_shadow_by_id(main_id, main_content)
                .context_ut("cannot map source for main file")?;
        }

        Ok(world)
    }
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
    /// Creates a new Typlite instance from a [`World`].
    pub fn new(world: Arc<LspWorld>) -> Self {
        Self {
            world,
            feat: Default::default(),
            format: Format::Md,
        }
    }

    /// Sets conversion features
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
            Format::LaTeX => self.convert_doc(Format::LaTeX)?.to_tex_string(),
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
        let world = Arc::new(self.feat.prepare_world(&self.world, format)?);
        let feat = self.feat.clone();
        Self::convert_doc_prepared(feat, format, world)
    }

    /// Convert the content to a markdown document.
    pub fn convert_doc_prepared(
        feat: TypliteFeat,
        format: Format,
        world: Arc<LspWorld>,
    ) -> tinymist_std::Result<MarkdownDocument> {
        // todo: ignoring warnings
        let base = typst::compile(&world).output?;
        let mut feat = feat;
        feat.target = format;
        Ok(MarkdownDocument::new(base, world.clone(), feat))
    }
}

#[cfg(test)]
mod tests;
