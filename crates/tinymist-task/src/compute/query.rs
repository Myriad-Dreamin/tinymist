//! The computation for document query.

use std::sync::Arc;

use comemo::Track;
use ecow::EcoString;
use tinymist_std::error::prelude::*;
use tinymist_std::typst::TypstDocument;
use tinymist_world::{CompilerFeat, ExportComputation, WorldComputeGraph};
use typst::World;
use typst::diag::{SourceResult, StrResult};
use typst::engine::Sink;
use typst::foundations::{Content, IntoValue, LocatableSelector, Scope, Value};
use typst::syntax::Span;
use typst::syntax::SyntaxMode;
use typst_eval::eval_string;

use crate::QueryTask;

/// The computation for document query.
pub struct DocumentQuery;

impl DocumentQuery {
    // todo: query exporter
    /// Retrieve the matches for the selector.
    pub fn retrieve<D: typst::Document>(
        world: &dyn World,
        selector: &str,
        document: &D,
    ) -> StrResult<Vec<Content>> {
        let selector = eval_string(
            &typst::ROUTINES,
            world.track(),
            Sink::new().track_mut(),
            selector,
            Span::detached(),
            SyntaxMode::Code,
            Scope::default(),
        )
        .map_err(|errors| {
            let mut message = EcoString::from("failed to evaluate selector");
            for (i, error) in errors.into_iter().enumerate() {
                message.push_str(if i == 0 { ": " } else { ", " });
                message.push_str(&error.message);
            }
            message
        })?
        .cast::<LocatableSelector>()
        .map_err(|e| EcoString::from(format!("failed to cast: {}", e.message())))?;

        Ok(document
            .introspector()
            .query(&selector.0)
            .into_iter()
            .collect::<Vec<_>>())
    }

    fn run_inner<F: CompilerFeat, D: typst::Document>(
        g: &Arc<WorldComputeGraph<F>>,
        doc: &Arc<D>,
        config: &QueryTask,
    ) -> Result<Vec<Value>> {
        let selector = &config.selector;
        let elements = Self::retrieve(&g.snap.world, selector, doc.as_ref())
            .map_err(|e| anyhow::anyhow!("failed to retrieve: {e}"))?;
        if config.one && elements.len() != 1 {
            bail!("expected exactly one element, found {}", elements.len());
        }

        Ok(elements
            .into_iter()
            .filter_map(|c| match &config.field {
                Some(field) => c.get_by_name(field).ok(),
                _ => Some(c.into_value()),
            })
            .collect())
    }

    /// Queries the document and returns the result as a value.
    pub fn doc_get_as_value<F: CompilerFeat>(
        g: &Arc<WorldComputeGraph<F>>,
        doc: &TypstDocument,
        config: &QueryTask,
    ) -> Result<serde_json::Value> {
        match doc {
            TypstDocument::Paged(doc) => Self::get_as_value(g, doc, config),
            TypstDocument::Html(doc) => Self::get_as_value(g, doc, config),
        }
    }

    /// Queries the document and returns the result as a value.
    pub fn get_as_value<F: CompilerFeat, D: typst::Document>(
        g: &Arc<WorldComputeGraph<F>>,
        doc: &Arc<D>,
        config: &QueryTask,
    ) -> Result<serde_json::Value> {
        let mapped = Self::run_inner(g, doc, config)?;

        let res = if config.one {
            let Some(value) = mapped.first() else {
                bail!("no such field found for element");
            };
            serde_json::to_value(value)
        } else {
            serde_json::to_value(&mapped)
        };

        res.context("failed to serialize")
    }
}

impl<F: CompilerFeat, D: typst::Document> ExportComputation<F, D> for DocumentQuery {
    type Output = SourceResult<String>;
    type Config = QueryTask;

    fn run(
        g: &Arc<WorldComputeGraph<F>>,
        doc: &Arc<D>,
        config: &QueryTask,
    ) -> Result<SourceResult<String>> {
        let pretty = false;
        let mapped = Self::run_inner(g, doc, config)?;

        let res = if config.one {
            let Some(value) = mapped.first() else {
                bail!("no such field found for element");
            };
            serialize(value, &config.format, pretty)
        } else {
            serialize(&mapped, &config.format, pretty)
        };

        res.map(Ok)
    }
}

/// Serialize data to the output format.
fn serialize(data: &impl serde::Serialize, format: &str, pretty: bool) -> Result<String> {
    Ok(match format {
        "json" if pretty => serde_json::to_string_pretty(data).context("serialize query")?,
        "json" => serde_json::to_string(data).context("serialize query")?,
        "yaml" => serde_yaml::to_string(&data).context_ut("serialize query")?,
        "txt" => {
            use serde_json::Value::*;
            let value = serde_json::to_value(data).context("serialize query")?;
            match value {
                String(s) => s,
                _ => {
                    let kind = match value {
                        Null => "null",
                        Bool(_) => "boolean",
                        Number(_) => "number",
                        String(_) => "string",
                        Array(_) => "array",
                        Object(_) => "object",
                    };
                    bail!("expected a string value for format: {format}, got {kind}")
                }
            }
        }
        _ => bail!("unsupported format for query: {format}"),
    })
}
