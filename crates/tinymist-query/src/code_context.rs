use std::ops::Deref;

use comemo::Track;
use serde::{Deserialize, Serialize};
use tinymist_analysis::analyze_expr;
use tinymist_project::{DiagnosticFormat, PathPattern};
use tinymist_std::error::prelude::*;
use tinymist_world::vfs::WorkspaceResolver;
use tinymist_world::{EntryReader, EntryState, ShadowApi, diag::print_diagnostics_to_string};
use typst::diag::{At, SourceResult};
use typst::foundations::{Args, Dict, NativeFunc, eco_format};
use typst::syntax::Span;
use typst::utils::LazyHash;
use typst::{
    foundations::{Bytes, IntoValue, StyleChain},
    text::TextElem,
};
use typst_shim::eval::{Eval, Vm};
use typst_shim::syntax::LinkedNodeExt;

use crate::{
    prelude::*,
    syntax::{InterpretMode, interpret_mode_at},
};

/// A query to get the mode at a specific position in a text document.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InteractCodeContextQuery {
    /// (Experimental) Evaluate a path expression at a specific position in a
    /// text document.
    PathAt {
        /// Code to evaluate. If the code starts with `{` and ends with `}`, it
        /// will be evaluated as a code expression, otherwise it will be
        /// evaluated as a path pattern.
        ///
        /// ## Example
        ///
        /// evaluate a path pattern, which could use following definitions:
        ///
        /// ```plain
        /// $root/x/$dir/../$name // is evaluated as
        /// /path/to/root/x/dir/../main
        /// ```
        ///
        /// ## Example
        ///
        /// evaluate a code expression, which could use following definitions:
        /// - `root`: the root of the workspace
        /// - `dir`: the directory of the current file
        /// - `name`: the name of the current file
        /// - `join(a, b, ...)`: join the arguments with the path separator
        ///
        /// ```plain
        /// { join(root, "x", dir, "y", name) } // is evaluated as
        /// /path/to/root/x/dir/y/main
        /// ```
        code: String,
        /// The extra `sys.inputs` for the code expression.
        inputs: Dict,
    },
    /// Get the mode at a specific position in a text document.
    ModeAt {
        /// The position inside the text document.
        position: LspPosition,
    },
    /// Get the style at a specific position in a text document.
    StyleAt {
        /// The position inside the text document.
        position: LspPosition,
        /// Style to query
        style: Vec<String>,
    },
}

/// A response to a `InteractCodeContextQuery`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InteractCodeContextResponse {
    /// Evaluate a path expression at a specific position in a text document.
    PathAt(QueryResult<serde_json::Value>),
    /// Get the mode at a specific position in a text document.
    ModeAt {
        /// The mode at the requested position.
        mode: InterpretMode,
    },
    /// Get the style at a specific position in a text document.
    StyleAt {
        /// The style at the requested position.
        style: Vec<Option<JsonValue>>,
    },
}

/// A request to get the code context of a text document.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind")]
pub struct InteractCodeContextRequest {
    /// The path to the text document.
    pub path: PathBuf,
    /// The queries to execute.
    pub query: Vec<Option<InteractCodeContextQuery>>,
}

impl SemanticRequest for InteractCodeContextRequest {
    type Response = Vec<Option<InteractCodeContextResponse>>;

    fn request(self, ctx: &mut LocalContext) -> Option<Self::Response> {
        let mut responses = Vec::new();

        let source = ctx.source_by_path(&self.path).ok()?;

        for query in self.query {
            responses.push(query.and_then(|query| match query {
                InteractCodeContextQuery::PathAt { code, inputs: base } => {
                    let res = eval_path_expr(ctx, &code, base)?;
                    Some(InteractCodeContextResponse::PathAt(res))
                }
                InteractCodeContextQuery::ModeAt { position } => {
                    let cursor = ctx.to_typst_pos(position, &source)?;
                    let mode = Self::mode_at(&source, cursor)?;
                    Some(InteractCodeContextResponse::ModeAt { mode })
                }
                InteractCodeContextQuery::StyleAt { position, style } => {
                    let mut world = ctx.world().clone();
                    log::info!(
                        "style at position {position:?} . {style:?} when main is {:?}",
                        world.main()
                    );
                    let cursor = ctx.to_typst_pos(position, &source)?;
                    let root = LinkedNode::new(source.root());
                    let mut leaf = root.leaf_at_compat(cursor)?;
                    log::info!("style at leaf {leaf:?} . {style:?}");

                    if !matches!(leaf.kind(), SyntaxKind::Text | SyntaxKind::MathText) {
                        return None;
                    }

                    if matches!(leaf.parent_kind(), Some(SyntaxKind::Raw)) {
                        leaf = leaf.parent()?.clone();
                    }

                    let mode = Self::mode_at(&source, cursor);
                    if !matches!(
                        mode,
                        Some(InterpretMode::Code | InterpretMode::Markup | InterpretMode::Math)
                    ) {
                        leaf = leaf.parent()?.clone();
                    }
                    let mut mapped_source = source.clone();
                    let (with, offset) = match mode {
                        Some(InterpretMode::Code) => ("context text.font", 8),
                        _ => ("#context text.font", 10),
                    };
                    let start = leaf.range().start;
                    mapped_source.edit(leaf.range(), with);

                    let _ = world.map_shadow_by_id(
                        mapped_source.id(),
                        Bytes::new(mapped_source.text().as_bytes().to_vec()),
                    );
                    world.take_db();

                    let root = LinkedNode::new(mapped_source.root());
                    let leaf = root.leaf_at_compat(start + offset)?;

                    log::info!("style at new_leaf {leaf:?} . {style:?}");

                    let mut cursor_styles = analyze_expr(&world, &leaf)
                        .iter()
                        .filter_map(|s| s.1.clone())
                        .collect::<Vec<_>>();
                    cursor_styles.sort_by_key(|x| x.as_slice().len());
                    log::info!("style at styles {cursor_styles:?} . {style:?}");
                    let cursor_style = cursor_styles.into_iter().next_back().unwrap_or_default();
                    let cursor_style = StyleChain::new(&cursor_style);

                    log::info!("style at style {cursor_style:?} . {style:?}");

                    let style = style
                        .iter()
                        .map(|style| Self::style_at(cursor_style, style))
                        .collect();
                    let _ = world.map_shadow_by_id(
                        mapped_source.id(),
                        Bytes::new(source.text().as_bytes().to_vec()),
                    );

                    Some(InteractCodeContextResponse::StyleAt { style })
                }
            }));
        }

        Some(responses)
    }
}

impl InteractCodeContextRequest {
    fn mode_at(source: &Source, pos: usize) -> Option<InterpretMode> {
        // Smart special cases that is definitely at markup
        if pos == 0 || pos >= source.text().len() {
            return Some(InterpretMode::Markup);
        }

        // Get mode
        let root = LinkedNode::new(source.root());
        Some(interpret_mode_at(root.leaf_at_compat(pos).as_ref()))
    }

    fn style_at(cursor_style: StyleChain, style: &str) -> Option<JsonValue> {
        match style {
            "text.font" => {
                let font = cursor_style.get_cloned(TextElem::font).into_value();
                serde_json::to_value(font).ok()
            }
            _ => None,
        }
    }
}

fn eval_path_expr(
    ctx: &mut LocalContext,
    code: &str,
    inputs: Dict,
) -> Option<QueryResult<serde_json::Value>> {
    let entry = ctx.world().entry_state();
    let path = if code.starts_with("{") && code.ends_with("}") {
        let id = entry
            .select_in_workspace(Path::new("/__path__.typ"))
            .main()?;

        let inputs = make_sys(&entry, ctx.world().inputs(), inputs);
        let (inputs, root, dir, name) = match inputs {
            Some(EvalSysCtx {
                inputs,
                root,
                dir,
                name,
            }) => (Some(inputs), Some(root), dir, Some(name)),
            None => (None, None, None, None),
        };

        let mut world = ctx.world().task(tinymist_world::TaskInputs {
            entry: None,
            inputs,
        });
        // todo: bad performance
        world.take_db();
        let _ = world.map_shadow_by_id(id, Bytes::from_string(code.to_owned()));

        tinymist_analysis::upstream::with_vm((&world as &dyn World).track(), |vm| {
            define_val(vm, "join", Value::Func(join::data().into()));
            for (key, value) in [("root", root), ("dir", dir), ("name", name)] {
                if let Some(value) = value {
                    define_val(vm, key, value);
                }
            }

            let mut expr = typst::syntax::parse_code(code);
            let span = Span::from_range(id, 0..code.len());
            expr.synthesize(span);

            let expr = match expr.cast::<ast::Code>() {
                Some(v) => v,
                None => bail!(
                    "code is not a valid code expression: kind={:?}",
                    expr.kind()
                ),
            };
            match expr.eval(vm) {
                Ok(value) => serde_json::to_value(value).context_ut("failed to serialize path"),
                Err(e) => {
                    let res =
                        print_diagnostics_to_string(&world, e.iter(), DiagnosticFormat::Human);
                    let err = res.unwrap_or_else(|e| e);
                    bail!("failed to evaluate path expression: {err}")
                }
            }
        })
    } else {
        PathPattern::new(code)
            .substitute(&entry)
            .context_ut("failed to substitute path pattern")
            .and_then(|path| {
                serde_json::to_value(path.deref()).context_ut("failed to serialize path")
            })
    };
    Some(path.into())
}

#[derive(Debug, Clone, Hash)]
struct EvalSysCtx {
    inputs: Arc<LazyHash<Dict>>,
    root: Value,
    dir: Option<Value>,
    name: Value,
}

#[comemo::memoize]
fn make_sys(entry: &EntryState, base: Arc<LazyHash<Dict>>, inputs: Dict) -> Option<EvalSysCtx> {
    let root = entry.root();
    let main = entry.main();

    log::debug!("Check path {main:?} and root {root:?}");

    let (root, main) = root.zip(main)?;

    // Files in packages are not exported
    if WorkspaceResolver::is_package_file(main) {
        return None;
    }
    // Files without a path are not exported
    let path = main.vpath().resolve(&root)?;

    // todo: handle untitled path
    if path.strip_prefix("/untitled").is_ok() {
        return None;
    }

    let path = path.strip_prefix(&root).ok()?;
    let dir = path.parent();
    let file_name = path.file_name().unwrap_or_default();

    let root = Value::Str(root.to_string_lossy().into());

    let dir = dir.map(|d| Value::Str(d.to_string_lossy().into()));

    let name = file_name.to_string_lossy();
    let name = name.as_ref().strip_suffix(".typ").unwrap_or(name.as_ref());
    let name = Value::Str(name.into());

    let mut dict = base.as_ref().deref().clone();
    for (key, value) in inputs {
        dict.insert(key, value);
    }
    dict.insert("root".into(), root.clone());
    if let Some(dir) = &dir {
        dict.insert("dir".into(), dir.clone());
    }
    dict.insert("name".into(), name.clone());

    Some(EvalSysCtx {
        inputs: Arc::new(LazyHash::new(dict)),
        root,
        dir,
        name,
    })
}

fn define_val(vm: &mut Vm, name: &str, value: Value) {
    let ident = SyntaxNode::leaf(SyntaxKind::Ident, name);
    vm.define(ident.cast::<ast::Ident>().unwrap(), value);
}

#[typst_macros::func(title = "Join function")]
fn join(args: &mut Args) -> SourceResult<Value> {
    let pos = args.take().to_pos();
    let mut res = PathBuf::new();
    for arg in pos {
        match arg {
            Value::Str(s) => res.push(s.as_str()),
            _ => {
                return Err(eco_format!("join argument is not a string: {arg:?}")).at(args.span);
            }
        };
    }
    Ok(Value::Str(res.to_string_lossy().into()))
}

/// A result of a query.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum QueryResult<T> {
    /// A successful result.
    Success {
        /// The value of the result.
        value: T,
    },
    /// An error result.
    Error {
        /// The error message.
        error: EcoString,
    },
}

impl<T> QueryResult<T> {
    /// Creates a successful result.
    pub fn success(value: T) -> Self {
        Self::Success { value }
    }

    /// Creates an error result.
    pub fn error(error: EcoString) -> Self {
        Self::Error { error }
    }
}

impl<T, E: std::error::Error> From<Result<T, E>> for QueryResult<T> {
    fn from(value: Result<T, E>) -> Self {
        match value {
            Ok(value) => QueryResult::success(value),
            Err(error) => QueryResult::error(eco_format!("{error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use typst::foundations::dict;

    use super::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("code_context_path_at", &|ctx, path| {
            let patterns = [
                "$root/$dir/$name",
                "$root/$name",
                "$root/assets",
                "$root/assets/$name",
                r#"{ join(root, "x", dir, "y", name) }"#,
                r#"{ join(root, 1) }"#,
                r#"{ join(roo, 1) }"#,
            ];
            let inp = [
                dict! {
                    "x-path-context" => "vscode-paste",
                    "x-path-input-uri" => "https://huh.io/img.png",
                    "x-path-input-name" => "img.png",
                },
                dict! {
                    "x-path-context" => "vscode-paste",
                    "x-path-input-uri" => "https://huh.io/text.md",
                    "x-path-input-name" => "text.md",
                },
            ];

            let cases = patterns
                .iter()
                .map(|pat| (*pat, inp[0].clone()))
                .chain(inp.iter().map(|inp| {
                    (
                        r#"{ import "/resolve.typ": resolve; resolve(join, root, dir, name) }"#,
                        inp.clone(),
                    )
                }));

            let result = cases
                .map(|(code, inputs)| {
                    let request = InteractCodeContextRequest {
                        path: path.clone(),
                        query: vec![Some(InteractCodeContextQuery::PathAt {
                            code: code.to_string(),
                            inputs: inputs.clone(),
                        })],
                    };
                    json!({ "code": code, "inputs": inputs, "response": request.request(ctx) })
                })
                .collect::<Vec<_>>();
            assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
        });
    }
}
