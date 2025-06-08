use std::fmt::Write;

use ecow::{eco_format, EcoString};
use if_chain::if_chain;
use typst::engine::Sink;
use typst::foundations::{repr, Capturer, Value};
use typst::layout::Length;
use typst::syntax::{ast, LinkedNode, Source, SyntaxKind};
use typst::World;
use typst_shim::eval::CapturesVisitor;
use typst_shim::syntax::LinkedNodeExt;
use typst_shim::utils::{round_2, Numeric};

use super::{summarize_font_family, truncated_repr};
use crate::analyze_expr;

/// Describe the item under the cursor.
///
/// Passing a `document` (from a previous compilation) is optional, but enhances
/// the autocompletions. Label completions, for instance, are only generated
/// when the document is available.
pub fn tooltip_(world: &dyn World, source: &Source, cursor: usize) -> Option<Tooltip> {
    let leaf = LinkedNode::new(source.root()).leaf_at_compat(cursor)?;
    if leaf.kind().is_trivia() {
        return None;
    }

    font_tooltip(world, &leaf)
        // todo: test that label_tooltip can be removed safely
        // .or_else(|| document.and_then(|doc| label_tooltip(doc, &leaf)))
        .or_else(|| expr_tooltip(world, &leaf))
        .or_else(|| closure_tooltip(&leaf))
}

/// A hover tooltip.
#[derive(Debug, Clone)]
pub enum Tooltip {
    /// A string of text.
    Text(EcoString),
    /// A string of Typst code.
    Code(EcoString),
}

/// Tooltip for a hovered expression.
pub fn expr_tooltip(world: &dyn World, leaf: &LinkedNode) -> Option<Tooltip> {
    let mut ancestor = leaf;
    while !ancestor.is::<ast::Expr>() {
        ancestor = ancestor.parent()?;
    }

    let expr = ancestor.cast::<ast::Expr>()?;
    if !expr.hash() && !matches!(expr, ast::Expr::MathIdent(_)) {
        return None;
    }

    let values = analyze_expr(world, ancestor);

    if let [(Value::Length(length), _)] = values.as_slice() {
        if let Some(tooltip) = length_tooltip(*length) {
            return Some(tooltip);
        }
    }

    if expr.is_literal() {
        return None;
    }

    let mut last = None;
    let mut pieces: Vec<EcoString> = vec![];
    let mut unique_func: Option<Value> = None;
    let mut unique = true;
    let mut iter = values.iter();
    for (value, _) in (&mut iter).take(Sink::MAX_VALUES - 1) {
        if let Some((prev, count)) = &mut last {
            if *prev == value {
                *count += 1;
                continue;
            } else if *count > 1 {
                write!(pieces.last_mut().unwrap(), " (x{count})").unwrap();
            }
        }

        if matches!(value, Value::Func(..) | Value::Type(..)) {
            match &unique_func {
                Some(unique_func) if unique => {
                    unique = unique_func == value;
                }
                Some(_) => {}
                None => {
                    unique_func = Some(value.clone());
                }
            }
        } else {
            unique = false;
        }

        pieces.push(truncated_repr(value));
        last = Some((value, 1));
    }

    // Don't report the only function reference...
    // Note we usually expect the `definition` analyzer work in this case, otherwise
    // please open an issue for this.
    if unique_func.is_some() && unique {
        return None;
    }

    if let Some((_, count)) = last {
        if count > 1 {
            write!(pieces.last_mut().unwrap(), " (x{count})").unwrap();
        }
    }

    if iter.next().is_some() {
        pieces.push("...".into());
    }

    let tooltip = repr::pretty_comma_list(&pieces, false);
    // todo: check sensible length, value highlighting
    (!tooltip.is_empty()).then(|| Tooltip::Code(tooltip.into()))
}

/// Tooltip for a hovered closure.
fn closure_tooltip(leaf: &LinkedNode) -> Option<Tooltip> {
    // Only show this tooltip when hovering over the equals sign or arrow of
    // the closure. Showing it across the whole subtree is too noisy.
    if !matches!(leaf.kind(), SyntaxKind::Eq | SyntaxKind::Arrow) {
        return None;
    }

    // Find the closure to analyze.
    let parent = leaf.parent()?;
    if parent.kind() != SyntaxKind::Closure {
        return None;
    }

    // Analyze the closure's captures.
    let mut visitor = CapturesVisitor::new(None, Capturer::Function);
    visitor.visit(parent);

    let captures = visitor.finish();
    let mut names: Vec<_> = captures
        .iter()
        .map(|(name, _)| eco_format!("`{name}`"))
        .collect();
    if names.is_empty() {
        return None;
    }

    names.sort();

    let tooltip = repr::separated_list(&names, "and");
    Some(Tooltip::Text(eco_format!(
        "This closure captures {tooltip}."
    )))
}

/// Tooltip text for a hovered length.
fn length_tooltip(length: Length) -> Option<Tooltip> {
    length.em.is_zero().then(|| {
        Tooltip::Code(eco_format!(
            "{}pt = {}mm = {}cm = {}in",
            round_2(length.abs.to_pt()),
            round_2(length.abs.to_mm()),
            round_2(length.abs.to_cm()),
            round_2(length.abs.to_inches())
        ))
    })
}

/// Tooltip for font.
fn font_tooltip(world: &dyn World, leaf: &LinkedNode) -> Option<Tooltip> {
    if_chain! {
        // Ensure that we are on top of a string.
        if let Some(string) = leaf.cast::<ast::Str>();
        let lower = string.get().to_lowercase();

        // Ensure that we are in the arguments to the text function.
        if let Some(parent) = leaf.parent();
        if let Some(named) = parent.cast::<ast::Named>();
        if named.name().as_str() == "font";

        // Find the font family.
        if let Some((_, iter)) = world
            .book()
            .families()
            .find(|&(family, _)| family.to_lowercase().as_str() == lower.as_str());

        then {
            let detail = summarize_font_family(iter);
            return Some(Tooltip::Text(detail));
        }
    };

    None
}
