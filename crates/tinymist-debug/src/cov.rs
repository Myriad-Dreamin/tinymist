//! Tinymist coverage support for Typst.
use std::sync::{Arc, LazyLock};

use parking_lot::Mutex;
use tinymist_std::hash::FxHashMap;
use tinymist_world::vfs::FileId;
use typst::diag::FileResult;
use typst::foundations::func;
use typst::syntax::ast::AstNode;
use typst::syntax::{ast, Source, Span, SyntaxNode};

use crate::instrument::Instrumenter;

#[derive(Default)]
pub struct CoverageInstrumenter {
    /// The coverage map.
    pub map: Mutex<FxHashMap<FileId, Arc<InstrumentMeta>>>,
}

impl Instrumenter for CoverageInstrumenter {
    fn instrument(&self, _source: Source) -> FileResult<Source> {
        let (new, meta) = instrument_coverage(_source)?;
        let region = CovRegion {
            hits: Arc::new(Mutex::new(vec![0; meta.meta.len()])),
        };

        let mut map = self.map.lock();
        map.insert(new.id(), meta);

        let mut cov_map = COVERAGE_MAP.lock();
        cov_map.regions.insert(new.id(), region);

        Ok(new)
    }
}

/// The coverage map.
#[derive(Default)]
pub struct CoverageMap {
    last_hit: Option<(FileId, CovRegion)>,
    /// The coverage map.
    pub regions: FxHashMap<FileId, CovRegion>,
}

/// The coverage region
#[derive(Default, Clone)]
pub struct CovRegion {
    /// The hits
    pub hits: Arc<Mutex<Vec<u8>>>,
}

pub static COVERAGE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(Mutex::default);
pub static COVERAGE_MAP: LazyLock<Mutex<CoverageMap>> = LazyLock::new(Mutex::default);

#[func(name = "__cov_pc", title = "Coverage function")]
pub fn __cov_pc(span: Span, pc: i64) {
    let Some(fid) = span.id() else {
        return;
    };
    let mut map = COVERAGE_MAP.lock();
    if let Some(last_hit) = map.last_hit.as_ref() {
        if last_hit.0 == fid {
            last_hit.1.hits.lock()[pc as usize] += 1;
            return;
        }
    }

    let region = map.regions.entry(fid).or_default();
    region.hits.lock()[pc as usize] += 1;
    map.last_hit = Some((fid, region.clone()));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    OpenBrace,
    CloseBrace,
    Functor,
}

#[derive(Default)]
pub struct InstrumentMeta {
    pub meta: Vec<(Span, Kind)>,
}

#[comemo::memoize]
fn instrument_coverage(source: Source) -> FileResult<(Source, Arc<InstrumentMeta>)> {
    let node = source.root();
    let mut worker = InstrumentWorker {
        meta: InstrumentMeta::default(),
        instrumented: String::new(),
    };

    worker.visit_node(node);
    let new_source: Source = Source::new(source.id(), worker.instrumented);

    Ok((new_source, Arc::new(worker.meta)))
}

struct InstrumentWorker {
    meta: InstrumentMeta,
    instrumented: String,
}

impl InstrumentWorker {
    fn instrument_block_child(&mut self, container: &SyntaxNode, b1: Span, b2: Span) {
        for child in container.children() {
            if b1 == child.span() || b2 == child.span() {
                self.instrument_block(child);
            } else {
                self.visit_node(child);
            }
        }
    }

    fn visit_node(&mut self, node: &SyntaxNode) {
        if let Some(expr) = node.cast::<ast::Expr>() {
            match expr {
                ast::Expr::Code(..) => {
                    self.instrument_block(node);
                    return;
                }
                ast::Expr::While(while_expr) => {
                    self.instrument_block_child(node, while_expr.body().span(), Span::detached());
                    return;
                }
                ast::Expr::For(for_expr) => {
                    self.instrument_block_child(node, for_expr.body().span(), Span::detached());
                    return;
                }
                ast::Expr::Conditional(cond_expr) => {
                    self.instrument_block_child(
                        node,
                        cond_expr.if_body().span(),
                        cond_expr.else_body().unwrap_or_default().span(),
                    );
                    return;
                }
                ast::Expr::Closure(closure) => {
                    self.instrument_block_child(node, closure.body().span(), Span::detached());
                    return;
                }
                ast::Expr::Show(show_rule) => {
                    let transform = show_rule.transform().to_untyped().span();

                    for child in node.children() {
                        if transform == child.span() {
                            self.instrument_functor(child);
                        } else {
                            self.visit_node(child);
                        }
                    }
                    return;
                }
                ast::Expr::Text(..)
                | ast::Expr::Space(..)
                | ast::Expr::Linebreak(..)
                | ast::Expr::Parbreak(..)
                | ast::Expr::Escape(..)
                | ast::Expr::Shorthand(..)
                | ast::Expr::SmartQuote(..)
                | ast::Expr::Strong(..)
                | ast::Expr::Emph(..)
                | ast::Expr::Raw(..)
                | ast::Expr::Link(..)
                | ast::Expr::Label(..)
                | ast::Expr::Ref(..)
                | ast::Expr::Heading(..)
                | ast::Expr::List(..)
                | ast::Expr::Enum(..)
                | ast::Expr::Term(..)
                | ast::Expr::Equation(..)
                | ast::Expr::Math(..)
                | ast::Expr::MathText(..)
                | ast::Expr::MathIdent(..)
                | ast::Expr::MathShorthand(..)
                | ast::Expr::MathAlignPoint(..)
                | ast::Expr::MathDelimited(..)
                | ast::Expr::MathAttach(..)
                | ast::Expr::MathPrimes(..)
                | ast::Expr::MathFrac(..)
                | ast::Expr::MathRoot(..)
                | ast::Expr::Ident(..)
                | ast::Expr::None(..)
                | ast::Expr::Auto(..)
                | ast::Expr::Bool(..)
                | ast::Expr::Int(..)
                | ast::Expr::Float(..)
                | ast::Expr::Numeric(..)
                | ast::Expr::Str(..)
                | ast::Expr::Content(..)
                | ast::Expr::Parenthesized(..)
                | ast::Expr::Array(..)
                | ast::Expr::Dict(..)
                | ast::Expr::Unary(..)
                | ast::Expr::Binary(..)
                | ast::Expr::FieldAccess(..)
                | ast::Expr::FuncCall(..)
                | ast::Expr::Let(..)
                | ast::Expr::DestructAssign(..)
                | ast::Expr::Set(..)
                | ast::Expr::Contextual(..)
                | ast::Expr::Import(..)
                | ast::Expr::Include(..)
                | ast::Expr::Break(..)
                | ast::Expr::Continue(..)
                | ast::Expr::Return(..) => {}
            }
        }

        self.visit_node_fallback(node);
    }

    fn visit_node_fallback(&mut self, node: &SyntaxNode) {
        let txt = node.text();
        if !txt.is_empty() {
            self.instrumented.push_str(txt);
        }

        for child in node.children() {
            self.visit_node(child);
        }
    }

    fn make_cov(&mut self, span: Span, kind: Kind) {
        let it = self.meta.meta.len();
        self.meta.meta.push((span, kind));
        self.instrumented.push_str("__cov_pc(");
        self.instrumented.push_str(&it.to_string());
        self.instrumented.push_str(");\n");
    }

    fn instrument_block(&mut self, child: &SyntaxNode) {
        self.instrumented.push_str("{\n");
        let (first, last) = {
            let mut children = child.children();
            let first = children
                .next()
                .map(|s| s.span())
                .unwrap_or_else(Span::detached);
            let last = children
                .last()
                .map(|s| s.span())
                .unwrap_or_else(Span::detached);

            (first, last)
        };
        self.make_cov(first, Kind::OpenBrace);
        self.visit_node_fallback(child);
        self.instrumented.push('\n');
        self.make_cov(last, Kind::CloseBrace);
        self.instrumented.push_str("}\n");
    }

    fn instrument_functor(&mut self, child: &SyntaxNode) {
        self.instrumented.push_str("{\nlet __cov_functor = ");
        let s = child.span();
        self.visit_node_fallback(child);
        self.instrumented.push_str("\n__it => {");
        self.make_cov(s, Kind::Functor);
        self.instrumented.push_str("; __cov_functor(__it); } }\n");
    }
}
