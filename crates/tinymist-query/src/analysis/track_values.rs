//! Dynamic analysis of an expression or import statement.

use comemo::Track;
use ecow::*;
use reflexo::typst::TypstPagedDocument;
use typst::engine::{Engine, Route, Sink, Traced};
use typst::foundations::{Label, Styles, Value};
use typst::introspection::Introspector;
use typst::model::BibliographyElem;
use typst::syntax::{ast, LinkedNode, SyntaxKind, SyntaxNode};
use typst::World;

/// Try to determine a set of possible values for an expression.
pub fn analyze_expr(world: &dyn World, node: &LinkedNode) -> EcoVec<(Value, Option<Styles>)> {
    if let Some(parent) = node.parent() {
        if parent.kind() == SyntaxKind::FieldAccess && node.index() > 0 {
            return analyze_expr(world, parent);
        }
    }

    analyze_expr_(world, node.get())
}

/// Try to determine a set of possible values for an expression.
pub fn analyze_expr_(world: &dyn World, node: &SyntaxNode) -> EcoVec<(Value, Option<Styles>)> {
    let Some(expr) = node.cast::<ast::Expr>() else {
        return eco_vec![];
    };

    let val = match expr {
        ast::Expr::None(_) => Value::None,
        ast::Expr::Auto(_) => Value::Auto,
        ast::Expr::Bool(v) => Value::Bool(v.get()),
        ast::Expr::Int(v) => Value::Int(v.get()),
        ast::Expr::Float(v) => Value::Float(v.get()),
        ast::Expr::Numeric(v) => Value::numeric(v.get()),
        ast::Expr::Str(v) => Value::Str(v.get().into()),
        _ => {
            if node.kind() == SyntaxKind::Contextual {
                if let Some(child) = node.children().last() {
                    return analyze_expr_(world, child);
                }
            }

            return typst::trace::<TypstPagedDocument>(world, node.span());
        }
    };

    eco_vec![(val, None)]
}

/// Try to load a module from the current source file.
pub fn analyze_import_(world: &dyn World, source: &SyntaxNode) -> (Option<Value>, Option<Value>) {
    let source_span = source.span();
    let Some((source, _)) = analyze_expr_(world, source).into_iter().next() else {
        return (None, None);
    };
    if source.scope().is_some() {
        return (Some(source.clone()), Some(source));
    }

    let introspector = Introspector::default();
    let traced = Traced::default();
    let mut sink = Sink::new();
    let mut engine = Engine {
        routines: &typst::ROUTINES,
        world: world.track(),
        route: Route::default(),
        introspector: introspector.track(),
        traced: traced.track(),
        sink: sink.track_mut(),
    };

    let module = source
        .clone()
        .cast::<typst::foundations::Str>()
        .ok()
        .and_then(|source| typst_eval::import(&mut engine, source.as_str(), source_span).ok())
        .map(Value::Module);

    (Some(source), module)
}

/// A label with a description and details.
pub struct DynLabel {
    /// The label itself.
    pub label: Label,
    /// A description of the label.
    pub label_desc: Option<EcoString>,
    /// Additional details about the label.
    pub detail: Option<EcoString>,
    /// The title of the bibliography entry. Not present for non-bibliography
    /// labels.
    pub bib_title: Option<EcoString>,
}

/// Find all labels and details for them.
///
/// Returns:
/// - All labels and descriptions for them, if available
/// - A split offset: All labels before this offset belong to nodes, all after
///   belong to a bibliography.
pub fn analyze_labels(document: &TypstPagedDocument) -> (Vec<DynLabel>, usize) {
    let mut output = vec![];

    // Labels in the document.
    for elem in document.introspector.all() {
        let Some(label) = elem.label() else { continue };
        let (is_derived, details) = {
            let derived = elem
                .get_by_name("caption")
                .or_else(|_| elem.get_by_name("body"));

            match derived {
                Ok(Value::Content(content)) => (true, content.plain_text()),
                Ok(Value::Str(s)) => (true, s.into()),
                Ok(_) => (false, elem.plain_text()),
                Err(_) => (false, elem.plain_text()),
            }
        };
        output.push(DynLabel {
            label,
            label_desc: Some(if is_derived {
                details.clone()
            } else {
                eco_format!("{}(..)", elem.func().name())
            }),
            detail: Some(details),
            bib_title: None,
        });
    }

    let split = output.len();

    // Bibliography keys.
    for (label, detail) in BibliographyElem::keys(document.introspector.track()) {
        output.push(DynLabel {
            label,
            label_desc: detail.clone(),
            detail: detail.clone(),
            bib_title: detail,
        });
    }

    (output, split)
}
