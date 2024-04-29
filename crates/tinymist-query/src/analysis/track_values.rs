//! Dynamic analysis of an expression or import statement.

use comemo::Track;
use ecow::*;
use typst::engine::{Engine, Route};
use typst::eval::{Tracer, Vm};
use typst::foundations::{Context, Label, Scopes, Styles, Value};
use typst::introspection::{Introspector, Locator};
use typst::model::{BibliographyElem, Document};
use typst::syntax::{ast, LinkedNode, Span, SyntaxKind};
use typst::World;

/// Try to determine a set of possible values for an expression.
pub fn analyze_expr(world: &dyn World, node: &LinkedNode) -> EcoVec<(Value, Option<Styles>)> {
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
                    return analyze_expr(world, &child);
                }
            }

            if let Some(parent) = node.parent() {
                if parent.kind() == SyntaxKind::FieldAccess && node.index() > 0 {
                    return analyze_expr(world, parent);
                }
            }

            let mut tracer = Tracer::new();
            tracer.inspect(node.span());
            typst::compile(world, &mut tracer).ok();
            return tracer.values();
        }
    };

    eco_vec![(val, None)]
}

/// Try to load a module from the current source file.
pub fn analyze_import(world: &dyn World, source: &LinkedNode) -> Option<Value> {
    let source_span = source.span();
    let (source, _) = analyze_expr(world, source).into_iter().next()?;
    if source.scope().is_some() {
        return Some(source);
    }

    let mut locator = Locator::default();
    let introspector = Introspector::default();
    let mut tracer = Tracer::new();
    let engine = Engine {
        world: world.track(),
        route: Route::default(),
        introspector: introspector.track(),
        locator: &mut locator,
        tracer: tracer.track_mut(),
    };

    let context = Context::none();
    let mut vm = Vm::new(
        engine,
        context.track(),
        Scopes::new(Some(world.library())),
        Span::detached(),
    );
    typst::eval::import(&mut vm, source, source_span, true)
        .ok()
        .map(Value::Module)
}

/// A label with a description and details.
pub struct DynLabel {
    /// The label itself.
    pub label: Label,
    /// A description of the label.
    pub label_desc: Option<EcoString>,
    /// Additional details about the label.
    pub detail: Option<EcoString>,
}

/// Find all labels and details for them.
///
/// Returns:
/// - All labels and descriptions for them, if available
/// - A split offset: All labels before this offset belong to nodes, all after
///   belong to a bibliography.
pub fn analyze_labels(document: &Document) -> (Vec<DynLabel>, usize) {
    let mut output = vec![];

    // Labels in the document.
    for elem in document.introspector.all() {
        let Some(label) = elem.label() else { continue };
        let (is_derived, details) = {
            let derived = elem
                .get_by_name("caption")
                .or_else(|| elem.get_by_name("body"));

            match derived {
                Some(Value::Content(content)) => (true, content.plain_text()),
                Some(Value::Str(s)) => (true, s.into()),
                _ => (false, elem.plain_text()),
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
        });
    }

    let split = output.len();

    // Bibliography keys.
    for (key, detail) in BibliographyElem::keys(document.introspector.track()) {
        output.push(DynLabel {
            label: Label::new(&key),
            label_desc: detail.clone(),
            detail,
        });
    }

    (output, split)
}
