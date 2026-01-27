use std::cmp::Reverse;
use std::str::FromStr;

use lsp_types::{InlayHintKind, InlayHintLabel};
use tinymist_world::package::{PackageSpec, PackageSpecExt};

use crate::{
    analysis::{ParamKind, analyze_call},
    prelude::*,
};

/// Configuration for inlay hints.
pub struct InlayHintConfig {
    // positional arguments group
    /// Show inlay hints for positional arguments.
    pub on_pos_args: bool,
    /// Disable inlay hints for single positional arguments.
    pub off_single_pos_arg: bool,

    // variadic arguments group
    /// Show inlay hints for variadic arguments.
    pub on_variadic_args: bool,
    /// Disable inlay hints for all variadic arguments but the first variadic
    /// argument.
    pub only_first_variadic_args: bool,

    // The typst sugar grammar
    /// Show inlay hints for content block arguments.
    pub on_content_block_args: bool,

    // package version status
    /// Show package version status decorations.
    pub on_package_version_status: bool,
}

impl InlayHintConfig {
    /// A smart configuration that enables most useful inlay hints.
    pub const fn smart() -> Self {
        Self {
            on_pos_args: true,
            off_single_pos_arg: true,

            on_variadic_args: true,
            only_first_variadic_args: true,

            on_content_block_args: false,

            on_package_version_status: true,
        }
    }
}

/// The [`textDocument/inlayHint`] request is sent from the client to the server
/// to compute inlay hints for a given `(text document, range)` tuple that may
/// be rendered in the editor in place with other text.
///
/// [`textDocument/inlayHint`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_inlayHint
///
/// # Compatibility
///
/// This request was introduced in specification version 3.17.0
#[derive(Debug, Clone)]
pub struct InlayHintRequest {
    /// The path of the document to get inlay hints for.
    pub path: PathBuf,
    /// The range of the document to get inlay hints for.
    pub range: LspRange,
}

impl SemanticRequest for InlayHintRequest {
    type Response = Vec<InlayHint>;

    fn request(self, ctx: &mut LocalContext) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;
        let range = ctx.to_typst_range(self.range, &source)?;

        let root = LinkedNode::new(source.root());
        let mut worker = InlayHintWorker {
            ctx,
            source: &source,
            range,
            hints: vec![],
        };
        worker.work(root);

        (!worker.hints.is_empty()).then_some(worker.hints)
    }
}

const SMART: InlayHintConfig = InlayHintConfig::smart();

struct InlayHintWorker<'a> {
    ctx: &'a mut LocalContext,
    source: &'a Source,
    range: Range<usize>,
    hints: Vec<InlayHint>,
}

impl InlayHintWorker<'_> {
    fn work(&mut self, node: LinkedNode) {
        let rng = node.range();
        if rng.start >= self.range.end || rng.end <= self.range.start {
            return;
        }

        self.analyze_node(&node);

        if node.get().children().len() == 0 {
            return;
        }

        // todo: survey bad performance children?
        for child in node.children() {
            self.work(child);
        }
    }

    fn analyze_node(&mut self, node: &LinkedNode) -> Option<()> {
        // analyze node self
        match node.kind() {
            // Type inlay hints
            SyntaxKind::LetBinding => {
                log::trace!("let binding found: {node:?}");
            }
            // Assignment inlay hints
            SyntaxKind::Eq => {
                log::trace!("assignment found: {node:?}");
            }
            SyntaxKind::DestructAssignment => {
                log::trace!("destruct assignment found: {node:?}");
            }
            // Package import version status
            SyntaxKind::Str if SMART.on_package_version_status => {
                self.check_package_import(node);
            }
            // Parameter inlay hints
            SyntaxKind::FuncCall => {
                log::trace!("func call found: {node:?}");
                let call_info = analyze_call(self.ctx, self.source.clone(), node.clone())?;
                crate::log_debug_ct!("got call_info {call_info:?}");

                let call = node.cast::<ast::FuncCall>().unwrap();
                let args = call.args();
                let args_node = node.find(args.span())?;

                let check_single_pos_arg = || {
                    let mut pos = 0;
                    let mut has_rest = false;
                    let mut content_pos = 0;

                    for arg in args.items() {
                        let Some(arg_node) = args_node.find(arg.span()) else {
                            continue;
                        };

                        let Some(info) = call_info.arg_mapping.get(&arg_node) else {
                            continue;
                        };

                        if info.kind != ParamKind::Named {
                            if info.kind == ParamKind::Rest {
                                has_rest = true;
                                continue;
                            }
                            if info.is_content_block {
                                content_pos += 1;
                            } else {
                                pos += 1;
                            };

                            if pos > 1 && content_pos > 1 {
                                break;
                            }
                        }
                    }

                    (pos <= if has_rest { 0 } else { 1 }, content_pos <= 1)
                };

                let (disable_by_single_pos_arg, disable_by_single_content_pos_arg) =
                    if SMART.on_pos_args && SMART.off_single_pos_arg {
                        check_single_pos_arg()
                    } else {
                        (false, false)
                    };

                let disable_by_single_line_content_block = !SMART.on_content_block_args
                    || 'one_line: {
                        for arg in args.items() {
                            let Some(arg_node) = args_node.find(arg.span()) else {
                                continue;
                            };

                            let Some(info) = call_info.arg_mapping.get(&arg_node) else {
                                continue;
                            };

                            if info.kind != ParamKind::Named
                                && info.is_content_block
                                && !is_one_line(self.source, &arg_node)
                            {
                                break 'one_line false;
                            }
                        }

                        true
                    };

                let mut is_first_variadic_arg = true;

                for arg in args.items() {
                    let Some(arg_node) = args_node.find(arg.span()) else {
                        continue;
                    };

                    let Some(info) = call_info.arg_mapping.get(&arg_node) else {
                        continue;
                    };

                    let name = &info.param_name;
                    if name.is_empty() {
                        continue;
                    }

                    match info.kind {
                        ParamKind::Named => {
                            continue;
                        }
                        ParamKind::Positional
                            if call_info.signature.primary().has_fill_or_size_or_stroke =>
                        {
                            continue;
                        }
                        ParamKind::Positional
                            if !SMART.on_pos_args
                                || (info.is_content_block
                                    && (disable_by_single_content_pos_arg
                                        || disable_by_single_line_content_block))
                                || (!info.is_content_block && disable_by_single_pos_arg) =>
                        {
                            continue;
                        }
                        ParamKind::Rest
                            if (!SMART.on_variadic_args
                                || disable_by_single_pos_arg
                                || (!is_first_variadic_arg && SMART.only_first_variadic_args)) =>
                        {
                            is_first_variadic_arg = false;
                            continue;
                        }
                        ParamKind::Rest => {
                            is_first_variadic_arg = false;
                        }
                        ParamKind::Positional => {}
                    }

                    let pos = arg_node.range().start;
                    let lsp_pos = self.ctx.to_lsp_pos(pos, self.source);

                    let label = InlayHintLabel::String(if info.kind == ParamKind::Rest {
                        format!("..{name}:")
                    } else {
                        format!("{name}:")
                    });

                    self.hints.push(InlayHint {
                        position: lsp_pos,
                        label,
                        kind: Some(InlayHintKind::PARAMETER),
                        text_edits: None,
                        tooltip: None,
                        padding_left: None,
                        padding_right: Some(true),
                        data: None,
                    });
                }

                // todo: union signatures
            }
            SyntaxKind::Set => {
                log::trace!("set rule found: {node:?}");
            }
            _ => {}
        }

        None
    }

    fn check_package_import(&mut self, node: &LinkedNode) -> Option<()> {
        // Node should be a Str (string literal)
        if !matches!(node.kind(), SyntaxKind::Str) {
            return None;
        }

        // Navigate up to find the ModuleImport node
        let import_node = node.parent()?.cast::<ast::ModuleImport>()?;

        // Check if this is a package import (starts with @)
        let ast::Expr::Str(str_node) = import_node.source() else {
            return None;
        };
        let import_str = str_node.get();
        if !import_str.starts_with("@") {
            return None;
        }

        // Parse the package spec
        let Ok(package_spec) = PackageSpec::from_str(&import_str) else {
            return None;
        };

        let versionless_spec = package_spec.versionless();

        // Get all matching packages
        let w = self.ctx.world().clone();
        let mut packages = vec![];
        if package_spec.is_preview() {
            packages.extend(
                w.packages()
                    .iter()
                    .filter(|it| it.matches_versionless(&versionless_spec)),
            );
        }
        // Add non-preview packages
        #[cfg(feature = "local-registry")]
        let local_packages = self.ctx.non_preview_packages();
        #[cfg(feature = "local-registry")]
        if !package_spec.is_preview() {
            packages.extend(
                local_packages
                    .iter()
                    .filter(|it| it.matches_versionless(&versionless_spec)),
            );
        }

        // Sort by version descending
        packages.sort_by_key(|entry| Reverse(entry.package.version));

        // Determine version status
        let current_entry = packages
            .iter()
            .find(|entry| entry.package.version == package_spec.version);

        let (label, tooltip) = if current_entry.is_none() {
            // Version not found - invalid
            let version_str = package_spec.version.to_string();
            (
                tinymist_l10n::t!("inlay-hint.package.version-not-found", "❗ not found"),
                Some(tinymist_l10n::t!(
                    "inlay-hint.package.version-not-found-tooltip",
                    "Version {version} not found",
                    version = version_str.as_str().into()
                )),
            )
        } else if let Some(latest) = packages.first() {
            let latest_version = &latest.package.version;
            if *latest_version != package_spec.version {
                // Upgradable - newer version available
                let latest_str = latest_version.to_string();
                (
                    tinymist_l10n::t!(
                        "inlay-hint.package.upgradable",
                        "⬆️ {version}",
                        version = latest_str.as_str().into()
                    ),
                    Some(tinymist_l10n::t!(
                        "inlay-hint.package.upgradable-tooltip",
                        "Newer version available: {version}",
                        version = latest_str.as_str().into()
                    )),
                )
            } else {
                // Up to date - latest version
                (
                    tinymist_l10n::t!("inlay-hint.package.up-to-date", "✅ latest"),
                    Some(tinymist_l10n::t!(
                        "inlay-hint.package.up-to-date-tooltip",
                        "Up to date (latest version)"
                    )),
                )
            }
        } else {
            // No packages found at all
            return None;
        };

        // Position for the hint - at the end of the string node
        let pos = node.range().end;
        let lsp_pos = self.ctx.to_lsp_pos(pos, self.source);

        self.hints.push(InlayHint {
            position: lsp_pos,
            label: InlayHintLabel::String(label.to_string()),
            kind: Some(InlayHintKind::TYPE),
            text_edits: None,
            tooltip: tooltip.map(|t| lsp_types::InlayHintTooltip::String(t.to_string())),
            padding_left: Some(true),
            padding_right: None,
            data: None,
        });

        Some(())
    }
}

fn is_one_line(src: &Source, arg_node: &LinkedNode<'_>) -> bool {
    is_one_line_(src, arg_node).unwrap_or(true)
}

fn is_one_line_(src: &Source, arg_node: &LinkedNode<'_>) -> Option<bool> {
    let lb = arg_node.children().next()?;
    let rb = arg_node.children().next_back()?;
    let ll = src.lines().byte_to_line(lb.offset())?;
    let rl = src.lines().byte_to_line(rb.offset())?;
    Some(ll == rl)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn smart() {
        snapshot_testing("inlay_hints", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let request = InlayHintRequest {
                path: path.clone(),
                range: to_lsp_range(0..source.text().len(), &source, PositionEncoding::Utf16),
            };

            let result = request.request(ctx);
            assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
        });
    }
}
