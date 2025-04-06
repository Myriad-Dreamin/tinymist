//! Tinymist coverage support for Typst.
use core::fmt;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use parking_lot::Mutex;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use tinymist_analysis::location::PositionEncoding;
use tinymist_std::hash::FxHashMap;
use tinymist_world::debug_loc::LspRange;
use tinymist_world::vfs::{FileId, WorkspaceResolver};
use tinymist_world::{CompilerFeat, CompilerWorld};
use typst::diag::FileResult;
use typst::foundations::func;
use typst::syntax::ast::AstNode;
use typst::syntax::{ast, Source, Span, SyntaxNode};
use typst::{World, WorldExt};

use crate::instrument::Instrumenter;

/// The coverage result.
pub struct CoverageResult {
    /// The coverage meta.
    pub meta: FxHashMap<FileId, Arc<InstrumentMeta>>,
    /// The coverage map.
    pub regions: FxHashMap<FileId, CovRegion>,
}

impl CoverageResult {
    /// Converts the coverage result to JSON.
    pub fn to_json<F: CompilerFeat>(&self, w: &CompilerWorld<F>) -> serde_json::Value {
        let lsp_position_encoding = PositionEncoding::Utf16;

        let mut result = VscodeCoverage::new();

        for (file_id, region) in &self.regions {
            let file_path = w
                .path_for_id(*file_id)
                .unwrap()
                .as_path()
                .to_str()
                .unwrap()
                .to_string();

            let mut details = vec![];

            let meta = self.meta.get(file_id).unwrap();

            let Ok(typst_source) = w.source(*file_id) else {
                continue;
            };

            let hits = region.hits.lock();
            for (idx, (span, _kind)) in meta.meta.iter().enumerate() {
                let Some(typst_range) = w.range(*span) else {
                    continue;
                };

                let rng = tinymist_analysis::location::to_lsp_range(
                    typst_range,
                    &typst_source,
                    lsp_position_encoding,
                );

                details.push(VscodeFileCoverageDetail {
                    executed: hits[idx] > 0,
                    location: rng,
                });
            }

            result.insert(file_path, details);
        }

        serde_json::to_value(result).unwrap()
    }

    /// Summarizes the coverage result.
    pub fn summarize<'a>(&'a self, short: bool, prefix: &'a str) -> SummarizedCoverage<'a> {
        SummarizedCoverage {
            prefix,
            result: self,
            short,
        }
    }
}

pub struct SummarizedCoverage<'a> {
    prefix: &'a str,
    result: &'a CoverageResult,
    short: bool,
}

impl SummarizedCoverage<'_> {
    fn line(
        &self,
        f: &mut fmt::Formatter<'_>,
        name: &str,
        total: usize,
        cov: usize,
        is_summary: bool,
    ) -> fmt::Result {
        let pre = self.prefix;
        let r = if total == 0 {
            100.0
        } else {
            cov as f64 / total as f64 * 100.0
        };
        if is_summary {
            write!(f, "{pre}{name} {cov}/{total} ({r:.2}%)")
        } else {
            let r = format!("{r:.2}");
            writeln!(f, "{pre} {cov:<5} / {total:<5} ({r:>6}%)  {name}")
        }
    }
}

impl fmt::Display for SummarizedCoverage<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut ids = self.result.regions.keys().collect::<Vec<_>>();
        ids.sort_by(|a, b| {
            a.package()
                .map(crate::PackageSpecCmp::from)
                .cmp(&b.package().map(crate::PackageSpecCmp::from))
                .then_with(|| a.vpath().cmp(b.vpath()))
        });

        let summary = ids
            .par_iter()
            .flat_map(|&id| {
                let region = self.result.regions.get(id)?;
                let meta = self.result.meta.get(id)?;

                let hits = region.hits.lock();
                let region_covered = hits.par_iter().filter(|&&x| x > 0).count();

                Some((id, region_covered, meta.meta.len()))
            })
            .collect::<Vec<_>>();

        let total = summary.iter().map(|(_, _, l)| l).sum::<usize>();
        let covered = summary.iter().map(|(_, c, _)| c).sum::<usize>();

        if !self.short {
            for (id, covered, total) in summary {
                let id = format!("{:?}", WorkspaceResolver::display(Some(*id)));
                self.line(f, &id, total, covered, false)?;
            }
        }
        self.line(f, "Coverage Summary", total, covered, true)?;

        Ok(())
    }
}

/// The coverage result in the format of the VSCode coverage data.
pub type VscodeCoverage = HashMap<String, Vec<VscodeFileCoverageDetail>>;

/// Converts the coverage result to the VSCode coverage data.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VscodeFileCoverageDetail {
    /// Whether the location is being executed
    pub executed: bool,
    /// The location of the coverage.
    pub location: LspRange,
}

#[derive(Default)]
pub struct CovInstr {
    /// The coverage map.
    pub map: Mutex<FxHashMap<FileId, Arc<InstrumentMeta>>>,
}

impl Instrumenter for CovInstr {
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
            let mut hits = last_hit.1.hits.lock();
            let c = &mut hits[pc as usize];
            *c = c.saturating_add(1);
            return;
        }
    }

    let region = map.regions.entry(fid).or_default();
    {
        let mut hits = region.hits.lock();
        let c = &mut hits[pc as usize];
        *c = c.saturating_add(1);
    }
    map.last_hit = Some((fid, region.clone()));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    OpenBrace,
    CloseBrace,
    Show,
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
                    let is_set = matches!(show_rule.transform(), ast::Expr::Set(..));

                    for child in node.children() {
                        if transform == child.span() {
                            if is_set {
                                self.instrument_show_set(child);
                            } else {
                                self.instrument_show_transform(child);
                            }
                        } else {
                            self.visit_node(child);
                        }
                    }
                    return;
                }
                ast::Expr::Contextual(body) => {
                    self.instrumented.push_str("context (");
                    self.visit_node(body.body().to_untyped());
                    self.instrumented.push(')');
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
        self.instrumented.push('}');
    }

    fn instrument_show_set(&mut self, child: &SyntaxNode) {
        self.instrumented.push_str("__it => {");
        self.make_cov(child.span(), Kind::Show);
        self.visit_node(child);
        self.instrumented.push_str("\n__it; }\n");
    }

    fn instrument_show_transform(&mut self, child: &SyntaxNode) {
        self.instrumented.push_str("{\nlet __cov_show_body = ");
        let s = child.span();
        self.visit_node(child);
        self.instrumented.push_str("\n__it => {");
        self.make_cov(s, Kind::Show);
        self.instrumented
            .push_str("if type(__cov_show_body) == function { __cov_show_body(__it); } else { __cov_show_body } } }\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instr(input: &str) -> String {
        let source = Source::detached(input);
        let (new, _meta) = instrument_coverage(source).unwrap();
        new.text().to_string()
    }

    #[test]
    fn test_physica_vector() {
        let instrumented = instr(include_str!("fixtures/instr_coverage/physica_vector.typ"));
        insta::assert_snapshot!(instrumented, @r###"
        // A show rule, should be used like:
        //   #show: super-plus-as-dagger
        //   U^+U = U U^+ = I
        // or in scope:
        //   #[
        //     #show: super-plus-as-dagger
        //     U^+U = U U^+ = I
        //   ]
        #let super-plus-as-dagger(document) = {
        __cov_pc(0);
        {
          show math.attach: {
        let __cov_show_body = elem => {
        __cov_pc(1);
        {
            if __eligible(elem.base) and elem.at("t", default: none) == [+] {
        __cov_pc(2);
        {
              $attach(elem.base, t: dagger, b: elem.at("b", default: #none))$
            }
        __cov_pc(3);
        } else {
        __cov_pc(4);
        {
              elem
            }
        __cov_pc(5);
        }
          }
        __cov_pc(6);
        }
        __it => {__cov_pc(7);
        if type(__cov_show_body) == function { __cov_show_body(__it); } else { __cov_show_body } } }


          document
        }
        __cov_pc(8);
        }
        "###);
    }

    #[test]
    fn test_playground() {
        let instrumented = instr(include_str!("fixtures/instr_coverage/playground.typ"));
        insta::assert_snapshot!(instrumented, @"");
    }

    #[test]
    fn test_instrument_coverage() {
        let source = Source::detached("#let a = 1;");
        let (new, _meta) = instrument_coverage(source).unwrap();
        insta::assert_snapshot!(new.text(), @"#let a = 1;");
    }

    #[test]
    fn test_instrument_coverage_show_content() {
        let source = Source::detached("#show math.equation: context it => it");
        let (new, _meta) = instrument_coverage(source).unwrap();
        insta::assert_snapshot!(new.text(), @r###"
        #show math.equation: {
        let __cov_show_body = context (it => {
        __cov_pc(0);
        it
        __cov_pc(1);
        })
        __it => {__cov_pc(2);
        if type(__cov_show_body) == function { __cov_show_body(__it); } else { __cov_show_body } } }
        "###);
    }

    #[test]
    fn test_instrument_inline_block() {
        let source = Source::detached("#let main-size = {1} + 2 + {3}");
        let (new, _meta) = instrument_coverage(source).unwrap();
        insta::assert_snapshot!(new.text(), @r###"
        #let main-size = {
        __cov_pc(0);
        {1}
        __cov_pc(1);
        } + 2 + {
        __cov_pc(2);
        {3}
        __cov_pc(3);
        }
        "###);
    }

    #[test]
    fn test_instrument_if() {
        let source = Source::detached(
            "#let main-size = if is-web-target {
  16pt
} else {
  10.5pt
}",
        );
        let (new, _meta) = instrument_coverage(source).unwrap();
        insta::assert_snapshot!(new.text(), @r###"
        #let main-size = if is-web-target {
        __cov_pc(0);
        {
          16pt
        }
        __cov_pc(1);
        } else {
        __cov_pc(2);
        {
          10.5pt
        }
        __cov_pc(3);
        }
        "###);
    }

    #[test]
    fn test_instrument_coverage_nested() {
        let source = Source::detached("#let a = {1};");
        let (new, _meta) = instrument_coverage(source).unwrap();
        insta::assert_snapshot!(new.text(), @r###"
        #let a = {
        __cov_pc(0);
        {1}
        __cov_pc(1);
        };
        "###);
    }

    #[test]
    fn test_instrument_coverage_functor() {
        let source = Source::detached("#show: main");
        let (new, _meta) = instrument_coverage(source).unwrap();
        insta::assert_snapshot!(new.text(), @r###"
        #show: {
        let __cov_show_body = main
        __it => {__cov_pc(0);
        if type(__cov_show_body) == function { __cov_show_body(__it); } else { __cov_show_body } } }
        "###);
    }

    #[test]
    fn test_instrument_coverage_set() {
        let source = Source::detached("#show raw: set text(12pt)");
        let (new, _meta) = instrument_coverage(source).unwrap();
        insta::assert_snapshot!(new.text(), @r###"
        #show raw: __it => {__cov_pc(0);
        set text(12pt)
        __it; }
        "###);
    }
}
