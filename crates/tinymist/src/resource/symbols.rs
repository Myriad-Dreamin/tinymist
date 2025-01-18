use std::{collections::BTreeMap, path::Path, sync::Arc};

// use reflexo_typst::font::GlyphId;
use reflexo_typst::{vector::font::GlyphId, TypstDocument, TypstFont};
use sync_lsp::LspResult;
use typst::syntax::VirtualPath;
use typst::World;

use crate::world::{base::ShadowApi, EntryState, TaskInputs};
use crate::{actor::typ_client::WorldSnapFut, z_internal_error};

use super::prelude::*;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResourceSymbolResponse {
    symbols: BTreeMap<String, ResourceSymbolItem>,
    font_selects: Vec<FontItem>,
    glyph_defs: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ResourceSymbolItem {
    category: SymCategory,
    unicode: u32,
    glyphs: Vec<ResourceGlyphDesc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum SymCategory {
    Control,
    Space,
    Delimiter,
    Punctuation,
    Accent,
    Quote,
    Prime,
    Arithmetic,
    Relation,
    SetTheory,
    Calculus,
    Logic,
    FunctionAndCategoryTheory,
    NumberTheory,
    Algebra,
    Geometry,
    // Miscellaneous Technical, Miscellaneous and Nuscekkabeiys letter-likes
    Misc,
    Currency,
    Shape,
    Arrow,
    Harpoon,
    Tack,
    // Lowercase Greek and Uppercase Greek
    Greek,
    Hebrew,
    DoubleStruck,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResourceGlyphDesc {
    font_index: u32,
    x_advance: Option<u16>,
    y_advance: Option<u16>,
    x_min: Option<i16>,
    x_max: Option<i16>,
    y_min: Option<i16>,
    y_max: Option<i16>,
    name: Option<String>,
    shape: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FontItem {
    family: String,
    cap_height: f32,
    ascender: f32,
    descender: f32,
    units_per_em: f32,
    // vertical: bool,
}

type ResourceSymbolMap = BTreeMap<String, ResourceSymbolItem>;

static CAT_MAP: Lazy<HashMap<&str, SymCategory>> = Lazy::new(|| {
    use SymCategory::*;

    HashMap::from_iter([
        // Control
        ("sym.wj", Control),
        ("sym.zwj", Control),
        ("sym.zwnj", Control),
        ("sym.zws", Control),
        ("sym.lrm", Control),
        ("sym.rlm", Control),
        // Space
        ("sym.space", Space),
        ("sym.space.nobreak", Space),
        ("sym.space.nobreak.narrow", Space),
        ("sym.space.en", Space),
        ("sym.space.quad", Space),
        ("sym.space.third", Space),
        ("sym.space.quarter", Space),
        ("sym.space.sixth", Space),
        ("sym.space.med", Space),
        ("sym.space.fig", Space),
        ("sym.space.punct", Space),
        ("sym.space.thin", Space),
        ("sym.space.hair", Space),
        // Delimiters
        ("sym.paren.l", Delimiter),
        ("sym.paren.r", Delimiter),
        ("sym.paren.t", Delimiter),
        ("sym.paren.b", Delimiter),
        ("sym.brace.l", Delimiter),
        ("sym.brace.r", Delimiter),
        ("sym.brace.t", Delimiter),
        ("sym.brace.b", Delimiter),
        ("sym.bracket.l", Delimiter),
        ("sym.bracket.r", Delimiter),
        ("sym.bracket.t", Delimiter),
        ("sym.bracket.b", Delimiter),
        ("sym.turtle.l", Delimiter),
        ("sym.turtle.r", Delimiter),
        ("sym.turtle.t", Delimiter),
        ("sym.turtle.b", Delimiter),
        ("sym.bar.v", Delimiter),
        ("sym.bar.v.double", Delimiter),
        ("sym.bar.v.triple", Delimiter),
        ("sym.bar.v.broken", Delimiter),
        ("sym.bar.v.circle", Delimiter),
        ("sym.bar.h", Delimiter),
        ("sym.fence.l", Delimiter),
        ("sym.fence.r", Delimiter),
        ("sym.fence.dotted", Delimiter),
        ("sym.angle.l", Delimiter),
        ("sym.angle.r", Delimiter),
        ("sym.angle.l.double", Delimiter),
        ("sym.angle.r.double", Delimiter),
        ("sym.angle.acute", Delimiter),
        ("sym.angle.arc", Delimiter),
        ("sym.angle.arc.rev", Delimiter),
        ("sym.angle.rev", Delimiter),
        ("sym.angle.right", Delimiter),
        ("sym.angle.right.rev", Delimiter),
        ("sym.angle.right.arc", Delimiter),
        ("sym.angle.right.dot", Delimiter),
        ("sym.angle.right.sq", Delimiter),
        ("sym.angle.spatial", Delimiter),
        ("sym.angle.spheric", Delimiter),
        ("sym.angle.spheric.rev", Delimiter),
        ("sym.angle.spheric.top", Delimiter),
        // Punctuation.
        ("sym.amp", Punctuation),
        ("sym.amp.inv", Punctuation),
        ("sym.ast.op", Punctuation),
        ("sym.ast.basic", Punctuation),
        ("sym.ast,low", Punctuation),
        ("sym.ast.double", Punctuation),
        ("sym.ast.triple", Punctuation),
        ("sym.ast.small", Punctuation),
        ("sym.ast.circle", Punctuation),
        ("sym.ast.square", Punctuation),
        ("sym.at", Punctuation),
        ("sym.backslash", Punctuation),
        ("sym.backslash.circle", Punctuation),
        ("sym.backslash.not", Punctuation),
        ("sym.co", Punctuation),
        ("sym.colon", Punctuation),
        ("sym.colon.double", Punctuation),
        ("sym.colon.eq", Punctuation),
        ("sym.colon.double.eq", Punctuation),
        ("sym.comma", Punctuation),
        ("sym.dagger", Punctuation),
        ("sym.dagger.double", Punctuation),
        ("sym.dash.en", Punctuation),
        ("sym.dash.em", Punctuation),
        ("sym.dash.fig", Punctuation),
        ("sym.dash.wave", Punctuation),
        ("sym.dash.colon", Punctuation),
        ("sym.dash.circle", Punctuation),
        ("sym.dash.wave.double", Punctuation),
        ("sym.dot.op", Punctuation),
        ("sym.dot.basic", Punctuation),
        ("sym.dot.c", Punctuation),
        ("sym.dot.circle", Punctuation),
        ("sym.dot.square", Punctuation),
        ("sym.dot.double", Punctuation),
        ("sym.dot.triple", Punctuation),
        ("sym.dot.quad", Punctuation),
        ("sym.excl", Punctuation),
        ("sym.excl.double", Punctuation),
        ("sym.excl.inv", Punctuation),
        ("sym.excl.quest", Punctuation),
        ("sym.interrobang", Punctuation),
        ("sym.hash", Punctuation),
        ("sym.hyph", Punctuation),
        ("sym.hyph.minus", Punctuation),
        ("sym.hyph.nobreak", Punctuation),
        ("sym.hyph.point", Punctuation),
        ("sym.hyph.soft", Punctuation),
        ("sym.percent", Punctuation),
        ("sym.copyright", Punctuation),
        ("sym.sopyright.sound", Punctuation),
        ("sym.permille", Punctuation),
        ("sym.pilcrow", Punctuation),
        ("sym.pilcrow.rev", Punctuation),
        ("sym.section", Punctuation),
        ("sym.semi", Punctuation),
        ("sym.semi.rev", Punctuation),
        ("sym.slash", Punctuation),
        ("sym.slash.double", Punctuation),
        ("sym.slash.triple", Punctuation),
        ("sym.slash.big", Punctuation),
        ("sym.dots.h.c", Punctuation),
        ("sym.dots.h", Punctuation),
        ("sym.dots.v", Punctuation),
        ("sym.dots.down", Punctuation),
        ("sym.dots.up", Punctuation),
        ("sym.tilde.op", Punctuation),
        ("sym.tilde.basic", Punctuation),
        ("sym.tilde.dot", Punctuation),
        ("sym.tilde.eq", Punctuation),
        ("sym.tilde.eq.not", Punctuation),
        ("sym.tilde.eq.rev", Punctuation),
        ("sym.tilde.equiv", Punctuation),
        ("sym.tilde.equiv.not", Punctuation),
        ("sym.tilde.nequiv", Punctuation),
        ("sym.tilde.not", Punctuation),
        ("sym.tilde.rev", Punctuation),
        ("sym.tilde.rev.equiv", Punctuation),
        ("sym.tilde.triple", Punctuation),
        // Accents, quotes, and primes.
        ("sym.acute", Accent),
        ("sym.acute.double", Accent),
        ("sym.breve", Accent),
        ("sym.caret", Accent),
        ("sym.caron", Accent),
        ("sym.hat", Accent),
        ("sym.diaer", Accent),
        ("sym.grave", Accent),
        ("sym.macron", Accent),
        ("sym.quote.double", Quote),
        ("sym.quote.single", Quote),
        ("sym.quote.l.double", Quote),
        ("sym.quote.l.single", Quote),
        ("sym.quote.r.double", Quote),
        ("sym.quote.r.single", Quote),
        ("sym.quote.angle.l.double", Quote),
        ("sym.quote.angle.l.single", Quote),
        ("sym.quote.angle.r.double", Quote),
        ("sym.quote.angle.r.single", Quote),
        ("sym.quote.high.double", Quote),
        ("sym.quote.high.single", Quote),
        ("sym.quote.low.double", Quote),
        ("sym.quote.low.single", Quote),
        ("sym.prime", Prime),
        ("sym.prime.rev", Prime),
        ("sym.prime.double", Prime),
        ("sym.prime.double.rev", Prime),
        ("sym.prime.triple", Prime),
        ("sym.prime.triple.rev", Prime),
        ("sym.prime.quad", Prime),
        // Arithmetic.
        ("sym.plus", Arithmetic),
        ("sym.plus.circle", Arithmetic),
        ("sym.plus.circle.arrow", Arithmetic),
        ("sym.plus.circle.big", Arithmetic),
        ("sym.plus.dot", Arithmetic),
        ("sym.plus.minus", Arithmetic),
        ("sym.plus.small", Arithmetic),
        ("sym.plus.square", Arithmetic),
        ("sym.plus.triangle", Arithmetic),
        ("sym.minus", Arithmetic),
        ("sym.minus.circle", Arithmetic),
        ("sym.minus.dot", Arithmetic),
        ("sym.minus.plus", Arithmetic),
        ("sym.minus.square", Arithmetic),
        ("sym.minus.tilde", Arithmetic),
        ("sym.minus.triangle", Arithmetic),
        ("sym.div", Arithmetic),
        ("sym.div.circle", Arithmetic),
        ("sym.times", Arithmetic),
        ("sym.times.big", Arithmetic),
        ("sym.times.circle", Arithmetic),
        ("sym.times.circle.big", Arithmetic),
        ("sym.times.div", Arithmetic),
        ("sym.times.three.l", Arithmetic),
        ("sym.times.three.r", Arithmetic),
        ("sym.times.l", Arithmetic),
        ("sym.times.r", Arithmetic),
        ("sym.times.square", Arithmetic),
        ("sym.times.triangle", Arithmetic),
        ("sym.ratio", Arithmetic),
        // Relations.
        ("sym.eq", Relation),
        ("sym.eq.star", Relation),
        ("sym.eq.circle", Relation),
        ("sym.eq.colon", Relation),
        ("sym.eq.def", Relation),
        ("sym.eq.delta", Relation),
        ("sym.eq.equi", Relation),
        ("sym.eq.est", Relation),
        ("sym.eq.gt", Relation),
        ("sym.eq.lt", Relation),
        ("sym.eq.m", Relation),
        ("sym.eq.not", Relation),
        ("sym.eq.prec", Relation),
        ("sym.eq.quest", Relation),
        ("sym.eq.small", Relation),
        ("sym.eq.succ", Relation),
        ("sym.eq.triple", Relation),
        ("sym.eq.quad", Relation),
        ("sym.gt", Relation),
        ("sym.gt.circle", Relation),
        ("sym.gt.curly", Relation),
        ("sym.gt.curly.approx", Relation),
        ("sym.gt.curly.double", Relation),
        ("sym.gt.curly.eq", Relation),
        ("sym.gt.curly.eq.not", Relation),
        ("sym.gt.curly.equiv", Relation),
        ("sym.gt.curly.napprox", Relation),
        ("sym.gt.curly.nequiv", Relation),
        ("sym.gt.curly.not", Relation),
        ("sym.gt.curly.ntilde", Relation),
        ("sym.gt.curly.tilde", Relation),
        ("sym.gt.dot", Relation),
        ("sym.gt.approx", Relation),
        ("sym.gt.double", Relation),
        ("sym.gt.eq", Relation),
        ("sym.gt.eq.slant", Relation),
        ("sym.gt.eq.lt", Relation),
        ("sym.gt.eq.not", Relation),
        ("sym.gt.equiv", Relation),
        ("sym.gt.lt", Relation),
        ("sym.gt.lt.not", Relation),
        ("sym.gt.napprox", Relation),
        ("sym.gt.nequiv", Relation),
        ("sym.gt.not", Relation),
        ("sym.gt.ntilde", Relation),
        ("sym.gt.small", Relation),
        ("sym.gt.tilde", Relation),
        ("sym.gt.tilde.not", Relation),
        ("sym.gt.tri", Relation),
        ("sym.gt.tri.eq", Relation),
        ("sym.gt.tri.eq.not", Relation),
        ("sym.gt.tri.not", Relation),
        ("sym.gt.triple", Relation),
        ("sym.gt.triple.nested", Relation),
        ("sym.lt", Relation),
        ("sym.lt.circle", Relation),
        ("sym.lt.curly", Relation),
        ("sym.lt.curly.approx", Relation),
        ("sym.lt.curly.double", Relation),
        ("sym.lt.curly.eq", Relation),
        ("sym.lt.curly.eq.not", Relation),
        ("sym.lt.curly.equiv", Relation),
        ("sym.lt.curly.napprox", Relation),
        ("sym.lt.curly.nequiv", Relation),
        ("sym.lt.curly.not", Relation),
        ("sym.lt.curly.ntilde", Relation),
        ("sym.lt.curly.tilde", Relation),
        ("sym.lt.dot", Relation),
        ("sym.lt.approx", Relation),
        ("sym.lt.double", Relation),
        ("sym.lt.eq", Relation),
        ("sym.lt.eq.slant", Relation),
        ("sym.lt.eq.gt", Relation),
        ("sym.lt.eq.not", Relation),
        ("sym.lt.equiv", Relation),
        ("sym.lt.gt", Relation),
        ("sym.lt.gt.not", Relation),
        ("sym.lt.napprox", Relation),
        ("sym.lt.nequiv", Relation),
        ("sym.lt.not", Relation),
        ("sym.lt.ntilde", Relation),
        ("sym.lt.small", Relation),
        ("sym.lt.tilde", Relation),
        ("sym.lt.tilde.not", Relation),
        ("sym.lt.tri", Relation),
        ("sym.lt.tri.eq", Relation),
        ("sym.lt.tri.eq.not", Relation),
        ("sym.lt.tri.not", Relation),
        ("sym.lt.triple", Relation),
        ("sym.lt.triple.nested", Relation),
        ("sym.approx", Relation),
        ("sym.approx.eq", Relation),
        ("sym.approx.not", Relation),
        ("sym.prec", Relation),
        ("sym.prec.approx", Relation),
        ("sym.prec.double", Relation),
        ("sym.prec.eq", Relation),
        ("sym.prec.eq.not", Relation),
        ("sym.prec.equiv", Relation),
        ("sym.prec.napprox", Relation),
        ("sym.prec.nequiv", Relation),
        ("sym.prec.not", Relation),
        ("sym.prec.ntilde", Relation),
        ("sym.prec.tilde", Relation),
        ("sym.succ", Relation),
        ("sym.succ.approx", Relation),
        ("sym.succ.double", Relation),
        ("sym.succ.eq", Relation),
        ("sym.succ.eq.not", Relation),
        ("sym.succ.equiv", Relation),
        ("sym.succ.napprox", Relation),
        ("sym.succ.nequiv", Relation),
        ("sym.succ.not", Relation),
        ("sym.succ.ntilde", Relation),
        ("sym.succ.tilde", Relation),
        ("sym.equiv", Relation),
        ("sym.equiv.not", Relation),
        ("sym.prop", Relation),
        // Set theory.
        ("sym.emptyset", SetTheory),
        ("sym.emptyset.rev", SetTheory),
        ("sym.nothing", SetTheory),
        ("sym.nothing.rev", SetTheory),
        ("sym.without", SetTheory),
        ("sym.complement", SetTheory),
        ("sym.in", SetTheory),
        ("sym.in.not", SetTheory),
        ("sym.in.rev", SetTheory),
        ("sym.in.rev.not", SetTheory),
        ("sym.in.rev.small", SetTheory),
        ("sym.in.small", SetTheory),
        ("sym.subset", SetTheory),
        ("sym.subset.dot", SetTheory),
        ("sym.subset.double", SetTheory),
        ("sym.subset.eq", SetTheory),
        ("sym.subset.eq.not", SetTheory),
        ("sym.subset.eq.sq", SetTheory),
        ("sym.subset.eq.sq.not", SetTheory),
        ("sym.subset.neq", SetTheory),
        ("sym.subset.not", SetTheory),
        ("sym.subset.sq", SetTheory),
        ("sym.subset.sq.neq", SetTheory),
        ("sym.supset", SetTheory),
        ("sym.supset.dot", SetTheory),
        ("sym.supset.double", SetTheory),
        ("sym.supset.eq", SetTheory),
        ("sym.supset.eq.not", SetTheory),
        ("sym.supset.eq.sq", SetTheory),
        ("sym.supset.eq.sq.not", SetTheory),
        ("sym.supset.neq", SetTheory),
        ("sym.supset.not", SetTheory),
        ("sym.supset.sq", SetTheory),
        ("sym.supset.sq.neq", SetTheory),
        ("sym.union", SetTheory),
        ("sym.union.arrow", SetTheory),
        ("sym.union.big", SetTheory),
        ("sym.union.dot", SetTheory),
        ("sym.union.dot.big", SetTheory),
        ("sym.union.double", SetTheory),
        ("sym.union.minus", SetTheory),
        ("sym.union.or", SetTheory),
        ("sym.union.plus", SetTheory),
        ("sym.union.plus.big", SetTheory),
        ("sym.union.sq", SetTheory),
        ("sym.union.sq.big", SetTheory),
        ("sym.union.sq.double", SetTheory),
        ("sym.sect", SetTheory),
        ("sym.sect.and", SetTheory),
        ("sym.sect.big", SetTheory),
        ("sym.sect.dot", SetTheory),
        ("sym.sect.double", SetTheory),
        ("sym.sect.sq", SetTheory),
        ("sym.sect.sq.big", SetTheory),
        ("sym.sect.sq.double", SetTheory),
        // Calculus.
        ("sym.infinity", Calculus),
        ("sym.oo", Calculus),
        ("sym.diff", Calculus),
        ("sym.partial", Calculus),
        ("sym.gradient", Calculus),
        ("sym.nabla", Calculus),
        ("sym.sum", Calculus),
        ("sym.sum.integral", Calculus),
        ("sym.product", Calculus),
        ("sym.product.co", Calculus),
        ("sym.integral", Calculus),
        ("sym.integral.arrow.hook", Calculus),
        ("sym.integral.ccw", Calculus),
        ("sym.integral.cont", Calculus),
        ("sym.integral.cont.ccw", Calculus),
        ("sym.integral.cont.cw", Calculus),
        ("sym.integral.cw", Calculus),
        ("sym.integral.dash", Calculus),
        ("sym.integral.dash.double", Calculus),
        ("sym.integral.double", Calculus),
        ("sym.integral.quad", Calculus),
        ("sym.integral.sect", Calculus),
        ("sym.integral.slash", Calculus),
        ("sym.integral.square", Calculus),
        ("sym.integral.surf", Calculus),
        ("sym.integral.times", Calculus),
        ("sym.integral.triple", Calculus),
        ("sym.integral.union", Calculus),
        ("sym.integral.vol", Calculus),
        ("sym.laplace", Calculus),
        // Logic.
        ("sym.forall", Logic),
        ("sym.exists", Logic),
        ("sym.exists.not", Logic),
        ("sym.top", Logic),
        ("sym.bot", Logic),
        ("sym.not", Logic),
        ("sym.and", Logic),
        ("sym.and.big", Logic),
        ("sym.and.curly", Logic),
        ("sym.and.dot", Logic),
        ("sym.and.double", Logic),
        ("sym.or", Logic),
        ("sym.or.big", Logic),
        ("sym.or.curly", Logic),
        ("sym.or.dot", Logic),
        ("sym.or.double", Logic),
        ("sym.xor", Logic),
        ("sym.xor.big", Logic),
        ("sym.models", Logic),
        ("sym.forces", Logic),
        ("sym.forces.not", Logic),
        ("sym.therefore", Logic),
        ("sym.because", Logic),
        ("sym.qed", Logic),
        // Function and category theory.
        ("sym.compose", FunctionAndCategoryTheory),
        ("sym.convolve", FunctionAndCategoryTheory),
        ("sym.multimap", FunctionAndCategoryTheory),
        // Number theory.
        ("sym.divedes", NumberTheory),
        ("sym.divides.not", NumberTheory),
        // Algebra.
        ("sym.wreath", Algebra),
        // Geometry.
        ("sym.parallel", Geometry),
        ("sym.parallel.circle", Geometry),
        ("sym.parallel.not", Geometry),
        ("sym.perp", Geometry),
        ("sym.perp.circle", Geometry),
        // Miscellaneous Technical.
        ("sym.diameter", Misc),
        ("sym.join", Misc),
        ("sym.join.r", Misc),
        ("sym.join.l", Misc),
        ("sym.join.l.r", Misc),
        ("sym.degree", Misc),
        ("sym.degree.c", Misc),
        ("sym.degree.f", Misc),
        ("sym.smash", Misc),
        // Currency.
        ("sym.bitcoin", Currency),
        ("sym.dollar", Currency),
        ("sym.euro", Currency),
        ("sym.franc", Currency),
        ("sym.lira", Currency),
        ("sym.peso", Currency),
        ("sym.pound", Currency),
        ("sym.ruble", Currency),
        ("sym.rupee", Currency),
        ("sym.won", Currency),
        ("sym.yen", Currency),
        // Miscellaneous.
        ("sym.ballot", Misc),
        ("sym.ballot.x", Misc),
        ("sym.checkmark", Misc),
        ("sym.checkmark.light", Misc),
        ("sym.floral", Misc),
        ("sym.floral.l", Misc),
        ("sym.floral.r", Misc),
        ("sym.notes.up", Misc),
        ("sym.notes.down", Misc),
        ("sym.refmark", Misc),
        ("sym.servicemark", Misc),
        ("sym.maltese", Misc),
        ("sym.suit.club", Misc),
        ("sym.suit.diamond", Misc),
        ("sym.suit.heart", Misc),
        ("sym.suit.spade", Misc),
        // Shapes.
        ("sym.bullet", Shape),
        ("sym.circle.stroked", Shape),
        ("sym.circle.stroked.tiny", Shape),
        ("sym.circle.stroked.small", Shape),
        ("sym.circle.stroked.big", Shape),
        ("sym.circle.filled", Shape),
        ("sym.circle.filled.tiny", Shape),
        ("sym.circle.filled.small", Shape),
        ("sym.circle.filled.big", Shape),
        ("sym.circle.dotted", Shape),
        ("sym.circle.nested", Shape),
        ("sym.ellipse.stroked.h", Shape),
        ("sym.ellipse.stroked.v", Shape),
        ("sym.ellipse.filled.h", Shape),
        ("sym.ellipse.filled.v", Shape),
        ("sym.triangle.stroked.r", Shape),
        ("sym.triangle.stroked.l", Shape),
        ("sym.triangle.stroked.t", Shape),
        ("sym.triangle.stroked.b", Shape),
        ("sym.triangle.stroked.bl", Shape),
        ("sym.triangle.stroked.br", Shape),
        ("sym.triangle.stroked.tl", Shape),
        ("sym.triangle.stroked.tr", Shape),
        ("sym.triangle.stroked.small.r", Shape),
        ("sym.triangle.stroked.small.b", Shape),
        ("sym.triangle.stroked.small.l", Shape),
        ("sym.triangle.stroked.small.t", Shape),
        ("sym.triangle.stroked.rounded", Shape),
        ("sym.triangle.stroked.nested", Shape),
        ("sym.triangle.stroked.dot", Shape),
        ("sym.triangle.filled.r", Shape),
        ("sym.triangle.filled.l", Shape),
        ("sym.triangle.filled.t", Shape),
        ("sym.triangle.filled.b", Shape),
        ("sym.triangle.filled.bl", Shape),
        ("sym.triangle.filled.br", Shape),
        ("sym.triangle.filled.tl", Shape),
        ("sym.triangle.filled.tr", Shape),
        ("sym.triangle.filled.small.r", Shape),
        ("sym.triangle.filled.small.b", Shape),
        ("sym.triangle.filled.small.l", Shape),
        ("sym.triangle.filled.small.t", Shape),
        ("sym.square.stroked", Shape),
        ("sym.square.stroked.tiny", Shape),
        ("sym.square.stroked.small", Shape),
        ("sym.square.stroked.medium", Shape),
        ("sym.square.stroked.big", Shape),
        ("sym.square.stroked.dotted", Shape),
        ("sym.square.stroked.rounded", Shape),
        ("sym.square.filled", Shape),
        ("sym.square.filled.tiny", Shape),
        ("sym.square.filled.small", Shape),
        ("sym.square.filled.medium", Shape),
        ("sym.square.filled.big", Shape),
        ("sym.rect.stroked.h", Shape),
        ("sym.rect.stroked.v", Shape),
        ("sym.rect.filled.h", Shape),
        ("sym.rect.filled.v", Shape),
        ("sym.penta.stroked", Shape),
        ("sym.penta.filled", Shape),
        ("sym.hexa.stroked", Shape),
        ("sym.hexa.filled", Shape),
        ("sym.diamond.stroked", Shape),
        ("sym.diamond.stroked.small", Shape),
        ("sym.diamond.stroked.medium", Shape),
        ("sym.diamond.stroked.dot", Shape),
        ("sym.diamond.filled", Shape),
        ("sym.diamond.filled.medium", Shape),
        ("sym.diamond.filled.small", Shape),
        ("sym.lozenge.stroked", Shape),
        ("sym.lozenge.stroked.small", Shape),
        ("sym.lozenge.stroked.medium", Shape),
        ("sym.lozenge.filled", Shape),
        ("sym.lozenge.filled.small", Shape),
        ("sym.lozenge.filled.medium", Shape),
        ("sym.star.op", Shape),
        ("sym.star.stroked", Shape),
        ("sym.star.filled", Shape),
        // Arrows, harpoons, and tacks.
        ("sym.arrow.r", Arrow),
        ("sym.arrow.r.long.bar", Arrow),
        ("sym.arrow.r.bar", Arrow),
        ("sym.arrow.r.curve", Arrow),
        ("sym.arrow.r.dashed", Arrow),
        ("sym.arrow.r.dotted", Arrow),
        ("sym.arrow.r.double", Arrow),
        ("sym.arrow.r.double.bar", Arrow),
        ("sym.arrow.r.double.long", Arrow),
        ("sym.arrow.r.double.long.bar", Arrow),
        ("sym.arrow.r.double.not", Arrow),
        ("sym.arrow.r.filled", Arrow),
        ("sym.arrow.r.hook", Arrow),
        ("sym.arrow.r.long", Arrow),
        ("sym.arrow.r.long.squiggly", Arrow),
        ("sym.arrow.r.loop", Arrow),
        ("sym.arrow.r.not", Arrow),
        ("sym.arrow.r.quad", Arrow),
        ("sym.arrow.r.squiggly", Arrow),
        ("sym.arrow.r.stop", Arrow),
        ("sym.arrow.r.stroked", Arrow),
        ("sym.arrow.r.tail", Arrow),
        ("sym.arrow.r.tilde", Arrow),
        ("sym.arrow.r.triple", Arrow),
        ("sym.arrow.r.twohead.bar", Arrow),
        ("sym.arrow.r.twohead", Arrow),
        ("sym.arrow.r.wave", Arrow),
        ("sym.arrow.l", Arrow),
        ("sym.arrow.l.bar", Arrow),
        ("sym.arrow.l.curve", Arrow),
        ("sym.arrow.l.dashed", Arrow),
        ("sym.arrow.l.dotted", Arrow),
        ("sym.arrow.l.double", Arrow),
        ("sym.arrow.l.double.bar", Arrow),
        ("sym.arrow.l.double.long", Arrow),
        ("sym.arrow.l.double.long.bar", Arrow),
        ("sym.arrow.l.double.not", Arrow),
        ("sym.arrow.l.filled", Arrow),
        ("sym.arrow.l.hook", Arrow),
        ("sym.arrow.l.long", Arrow),
        ("sym.arrow.l.long.bar", Arrow),
        ("sym.arrow.l.long.squiggly", Arrow),
        ("sym.arrow.l.loop", Arrow),
        ("sym.arrow.l.not", Arrow),
        ("sym.arrow.l.quad", Arrow),
        ("sym.arrow.l.squiggly", Arrow),
        ("sym.arrow.l.stop", Arrow),
        ("sym.arrow.l.stroked", Arrow),
        ("sym.arrow.l.tail", Arrow),
        ("sym.arrow.l.tilde", Arrow),
        ("sym.arrow.l.triple", Arrow),
        ("sym.arrow.l.twohead.bar", Arrow),
        ("sym.arrow.l.twohead", Arrow),
        ("sym.arrow.l.wave", Arrow),
        ("sym.arrow.t", Arrow),
        ("sym.arrow.t.bar", Arrow),
        ("sym.arrow.t.curve", Arrow),
        ("sym.arrow.t.dashed", Arrow),
        ("sym.arrow.t.double", Arrow),
        ("sym.arrow.t.filled", Arrow),
        ("sym.arrow.t.quad", Arrow),
        ("sym.arrow.t.stop", Arrow),
        ("sym.arrow.t.stroked", Arrow),
        ("sym.arrow.t.triple", Arrow),
        ("sym.arrow.t.twohead", Arrow),
        ("sym.arrow.b", Arrow),
        ("sym.arrow.b.bar", Arrow),
        ("sym.arrow.b.curve", Arrow),
        ("sym.arrow.b.dashed", Arrow),
        ("sym.arrow.b.double", Arrow),
        ("sym.arrow.b.filled", Arrow),
        ("sym.arrow.b.quad", Arrow),
        ("sym.arrow.b.stop", Arrow),
        ("sym.arrow.b.stroked", Arrow),
        ("sym.arrow.b.triple", Arrow),
        ("sym.arrow.b.twohead", Arrow),
        ("sym.arrow.l.r", Arrow),
        ("sym.arrow.l.r.double", Arrow),
        ("sym.arrow.l.r.double.long", Arrow),
        ("sym.arrow.l.r.double.not", Arrow),
        ("sym.arrow.l.r.filled", Arrow),
        ("sym.arrow.l.r.long", Arrow),
        ("sym.arrow.l.r.not", Arrow),
        ("sym.arrow.l.r.stroked", Arrow),
        ("sym.arrow.l.r.wave", Arrow),
        ("sym.arrow.t.b", Arrow),
        ("sym.arrow.t.b.double", Arrow),
        ("sym.arrow.t.b.filled", Arrow),
        ("sym.arrow.t.b.stroked", Arrow),
        ("sym.arrow.tr", Arrow),
        ("sym.arrow.tr.double", Arrow),
        ("sym.arrow.tr.filled", Arrow),
        ("sym.arrow.tr.hook", Arrow),
        ("sym.arrow.tr.stroked", Arrow),
        ("sym.arrow.br", Arrow),
        ("sym.arrow.br.double", Arrow),
        ("sym.arrow.br.filled", Arrow),
        ("sym.arrow.br.hook", Arrow),
        ("sym.arrow.br.stroked", Arrow),
        ("sym.arrow.tl", Arrow),
        ("sym.arrow.tl.double", Arrow),
        ("sym.arrow.tl.filled", Arrow),
        ("sym.arrow.tl.hook", Arrow),
        ("sym.arrow.tl.stroked", Arrow),
        ("sym.arrow.bl", Arrow),
        ("sym.arrow.bl.double", Arrow),
        ("sym.arrow.bl.filled", Arrow),
        ("sym.arrow.bl.hook", Arrow),
        ("sym.arrow.bl.stroked", Arrow),
        ("sym.arrow.tl.br", Arrow),
        ("sym.arrow.tr.bl", Arrow),
        ("sym.arrow.ccw", Arrow),
        ("sym.arrow.ccw.half", Arrow),
        ("sym.arrow.cw", Arrow),
        ("sym.arrow.cw.half", Arrow),
        ("sym.arrow.zigzag", Arrow),
        ("sym.arrows.rr", Arrow),
        ("sym.arrows.ll", Arrow),
        ("sym.arrows.tt", Arrow),
        ("sym.arrows.bb", Arrow),
        ("sym.arrows.lr", Arrow),
        ("sym.arrows.lr.stop", Arrow),
        ("sym.arrows.rl", Arrow),
        ("sym.arrows.tb", Arrow),
        ("sym.arrows.bt", Arrow),
        ("sym.arrows.rrr", Arrow),
        ("sym.arrows.lll", Arrow),
        ("sym.arrowhead.t", Arrow),
        ("sym.arrowhead.b", Arrow),
        ("sym.harpoon.rt", Harpoon),
        ("sym.harpoon.rt.bar", Harpoon),
        ("sym.harpoon.rt.stop", Harpoon),
        ("sym.harpoon.rb", Harpoon),
        ("sym.harpoon.rb.bar", Harpoon),
        ("sym.harpoon.rb.stop", Harpoon),
        ("sym.harpoon.lt", Harpoon),
        ("sym.harpoon.lt.bar", Harpoon),
        ("sym.harpoon.lt.stop", Harpoon),
        ("sym.harpoon.lb", Harpoon),
        ("sym.harpoon.lb.bar", Harpoon),
        ("sym.harpoon.lb.stop", Harpoon),
        ("sym.harpoon.tl", Harpoon),
        ("sym.harpoon.tl.bar", Harpoon),
        ("sym.harpoon.tl.stop", Harpoon),
        ("sym.harpoon.tr", Harpoon),
        ("sym.harpoon.tr.bar", Harpoon),
        ("sym.harpoon.tr.stop", Harpoon),
        ("sym.harpoon.bl", Harpoon),
        ("sym.harpoon.bl.bar", Harpoon),
        ("sym.harpoon.bl.stop", Harpoon),
        ("sym.harpoon.br", Harpoon),
        ("sym.harpoon.br.bar", Harpoon),
        ("sym.harpoon.br.stop", Harpoon),
        ("sym.harpoons.rtrb", Harpoon),
        ("sym.harpoons.blbr", Harpoon),
        ("sym.harpoons.bltr", Harpoon),
        ("sym.harpoons.lbrb", Harpoon),
        ("sym.harpoons.ltlb", Harpoon),
        ("sym.harpoons.ltrb", Harpoon),
        ("sym.harpoons.ltrt", Harpoon),
        ("sym.harpoons.rblb", Harpoon),
        ("sym.harpoons.rtlb", Harpoon),
        ("sym.harpoons.rtlt", Harpoon),
        ("sym.harpoons.tlbr", Harpoon),
        ("sym.harpoons.tltr", Harpoon),
        ("sym.tack.r", Tack),
        ("sym.tack.r.not", Tack),
        ("sym.tack.r.long", Tack),
        ("sym.tack.r.short", Tack),
        ("sym.tack.r.double", Tack),
        ("sym.tack.r.double.not", Tack),
        ("sym.tack.l", Tack),
        ("sym.tack.l.long", Tack),
        ("sym.tack.l.short", Tack),
        ("sym.tack.l.double", Tack),
        ("sym.tack.t", Tack),
        ("sym.tack.t.big", Tack),
        ("sym.tack.t.double", Tack),
        ("sym.tack.t.short", Tack),
        ("sym.tack.b", Tack),
        ("sym.tack.b.big", Tack),
        ("sym.tack.b.double", Tack),
        ("sym.tack.b.short", Tack),
        ("sym.tack.l.r", Tack),
        // Lowercase and Uppercase Greek Letters.
        ("sym.alpha", Greek),
        ("sym.beta", Greek),
        ("sym.chi", Greek),
        ("sym.delta", Greek),
        ("sym.epsilon", Greek),
        ("sym.eta", Greek),
        ("sym.gamma", Greek),
        ("sym.iota", Greek),
        ("sym.kai", Greek),
        ("sym.kappa", Greek),
        ("sym.lambda", Greek),
        ("sym.mu", Greek),
        ("sym.nu", Greek),
        ("sym.ohm", Greek),
        ("sym.omega", Greek),
        ("sym.omicron", Greek),
        ("sym.phi", Greek),
        ("sym.pi", Greek),
        ("sym.psi", Greek),
        ("sym.rho", Greek),
        ("sym.sigma", Greek),
        ("sym.tau", Greek),
        ("sym.theta", Greek),
        ("sym.upsilon", Greek),
        ("sym.xi", Greek),
        ("sym.zeta", Greek),
        ("sym.Alpha", Greek),
        ("sym.Beta", Greek),
        ("sym.Chi", Greek),
        ("sym.Delta", Greek),
        ("sym.Epsilon", Greek),
        ("sym.Eta", Greek),
        ("sym.Gamma", Greek),
        ("sym.Iota", Greek),
        ("sym.Kai", Greek),
        ("sym.Kappa", Greek),
        ("sym.Lambda", Greek),
        ("sym.Mu", Greek),
        ("sym.Nu", Greek),
        ("sym.Omega", Greek),
        ("sym.Omicron", Greek),
        ("sym.Phi", Greek),
        ("sym.Pi", Greek),
        ("sym.Psi", Greek),
        ("sym.Rho", Greek),
        ("sym.Sigma", Greek),
        ("sym.Tau", Greek),
        ("sym.Theta", Greek),
        ("sym.Upsilon", Greek),
        ("sym.Xi", Greek),
        ("sym.Zeta", Greek),
        // Hebrew.
        ("sym.aleph", Hebrew),
        ("sym.alef", Hebrew),
        ("sym.beth", Hebrew),
        ("sym.bet", Hebrew),
        ("sym.gimmel", Hebrew),
        ("sym.gimel", Hebrew),
        ("sym.daleth", Hebrew),
        ("sym.dalet", Hebrew),
        ("sym.shin", Hebrew),
        // Double-struck.
        ("sym.AA", DoubleStruck),
        ("sym.BB", DoubleStruck),
        ("sym.CC", DoubleStruck),
        ("sym.DD", DoubleStruck),
        ("sym.EE", DoubleStruck),
        ("sym.FF", DoubleStruck),
        ("sym.GG", DoubleStruck),
        ("sym.HH", DoubleStruck),
        ("sym.II", DoubleStruck),
        ("sym.JJ", DoubleStruck),
        ("sym.KK", DoubleStruck),
        ("sym.LL", DoubleStruck),
        ("sym.MM", DoubleStruck),
        ("sym.NN", DoubleStruck),
        ("sym.OO", DoubleStruck),
        ("sym.PP", DoubleStruck),
        ("sym.QQ", DoubleStruck),
        ("sym.RR", DoubleStruck),
        ("sym.SS", DoubleStruck),
        ("sym.TT", DoubleStruck),
        ("sym.UU", DoubleStruck),
        ("sym.VV", DoubleStruck),
        ("sym.WW", DoubleStruck),
        ("sym.XX", DoubleStruck),
        ("sym.YY", DoubleStruck),
        ("sym.ZZ", DoubleStruck),
        // Miscellaneous letter-likes.
        ("sym.ell", Misc),
        ("sym.planck", Misc),
        ("sym.plank.reduced", Misc),
        ("sym.angstrom", Misc),
        ("sym.kelvin", Misc),
        ("sym.Re", Misc),
        ("sym.Im", Misc),
        ("sym.dotless.i", Misc),
        ("sym.dotless.j", Misc),
    ])
});

impl LanguageState {
    /// Get the all valid symbols
    pub async fn get_symbol_resources(snap: WorldSnapFut) -> LspResult<JsonValue> {
        let snap = snap.receive().await.map_err(z_internal_error)?;

        let mut symbols = ResourceSymbolMap::new();
        use typst::symbols::{emoji, sym};
        populate_scope(sym().scope(), "sym", SymCategory::Misc, &mut symbols);
        // todo: disabling emoji module, as there is performant issue on emojis
        let _ = emoji;
        // populate_scope(emoji().scope(), "emoji", SymCategory::Emoji, &mut symbols);

        const PRELUDE: &str = r#"#show math.equation: set text(font: (
  "New Computer Modern Math",
  "Latin Modern Math",
  "STIX Two Math",
  "Cambria Math",
  "New Computer Modern",
  "Cambria",
))
"#;

        let math_shaping_text = symbols.iter().fold(PRELUDE.to_owned(), |mut o, (k, e)| {
            use std::fmt::Write;
            writeln!(o, "$#{k}$/* {} */#pagebreak()", e.unicode).ok();
            o
        });
        log::debug!("math shaping text: {text}", text = math_shaping_text);

        let symbols_ref = symbols.keys().cloned().collect::<Vec<_>>();

        let font = {
            let entry_path: Arc<Path> = Path::new("/._sym_.typ").into();

            let new_entry = EntryState::new_rootless(VirtualPath::new(&entry_path));

            let mut forked = snap.world.task(TaskInputs {
                entry: Some(new_entry),
                ..Default::default()
            });
            forked
                .map_shadow_by_id(forked.main(), math_shaping_text.into_bytes().into())
                .map_err(|e| error_once!("cannot map shadow", err: e))
                .map_err(z_internal_error)?;

            let sym_doc = std::marker::PhantomData
                .compile(&forked, &mut Default::default())
                .map_err(|e| error_once!("cannot compile symbols", err: format!("{e:?}")))
                .map_err(z_internal_error)?;

            log::debug!("sym doc: {sym_doc:?}");
            Some(trait_symbol_fonts(&sym_doc.output, &symbols_ref))
        };

        let mut glyph_def = String::new();

        let mut collected_fonts = None;

        if let Some(glyph_mapping) = font.clone() {
            let glyph_provider = reflexo_vec2svg::GlyphProvider::default();
            let glyph_pass =
                reflexo_typst::vector::pass::ConvertInnerImpl::new(glyph_provider, false);

            let mut glyph_renderer = Svg::default();
            let mut glyphs = vec![];

            let font_collected = glyph_mapping
                .values()
                .map(|e| e.0.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();

            let mut render_sym = |u| {
                let (font, id) = glyph_mapping.get(u)?.clone();
                let font_index = font_collected.iter().position(|e| e == &font).unwrap() as u32;

                let width = font.ttf().glyph_hor_advance(id);
                let height = font.ttf().glyph_ver_advance(id);
                let bbox = font.ttf().glyph_bounding_box(id);

                let glyph = glyph_pass.must_flat_glyph(&GlyphItem::Raw(font.clone(), id))?;

                let g_ref = GlyphRef {
                    font_hash: font_index,
                    glyph_idx: id.0 as u32,
                };

                glyphs.push((g_ref, glyph));

                Some(ResourceGlyphDesc {
                    font_index,
                    x_advance: width,
                    y_advance: height,
                    x_min: bbox.map(|e| e.x_min),
                    x_max: bbox.map(|e| e.x_max),
                    y_min: bbox.map(|e| e.y_min),
                    y_max: bbox.map(|e| e.y_max),
                    name: font.ttf().glyph_name(id).map(|e| e.to_owned()),
                    shape: Some(g_ref.as_svg_id("g")),
                })
            };

            for (k, v) in symbols.iter_mut() {
                let Some(desc) = render_sym(k) else {
                    continue;
                };

                v.glyphs.push(desc);
            }

            let mut svg = vec![];

            // attach the glyph defs
            svg.push(r#"<defs class="glyph">"#.into());
            svg.extend(glyph_renderer.render_glyphs(glyphs.iter().map(|(id, item)| (*id, item))));
            svg.push("</defs>".into());

            glyph_def = SvgText::join(svg);

            collected_fonts = Some(font_collected);
        }

        let resp = ResourceSymbolResponse {
            symbols,
            font_selects: collected_fonts
                .map(|e| e.into_iter())
                .into_iter()
                .flatten()
                .map(|e| FontItem {
                    family: e.info().family.clone(),
                    cap_height: e.metrics().cap_height.get() as f32,
                    ascender: e.metrics().ascender.get() as f32,
                    descender: e.metrics().descender.get() as f32,
                    units_per_em: e.metrics().units_per_em as f32,
                })
                .collect::<Vec<_>>(),
            glyph_defs: glyph_def,
        };

        serde_json::to_value(resp)
            .context("cannot serialize response")
            .map_err(z_internal_error)
    }
}

fn trait_symbol_fonts(
    doc: &TypstDocument,
    symbols: &[String],
) -> HashMap<String, (TypstFont, GlyphId)> {
    use typst::layout::Frame;
    use typst::layout::FrameItem;

    let mut worker = Worker {
        symbols,
        active: "",
        res: HashMap::new(),
    };
    worker.work(doc);
    let res = worker.res;

    struct Worker<'a> {
        symbols: &'a [String],
        active: &'a str,
        res: HashMap<String, (TypstFont, GlyphId)>,
    }

    impl Worker<'_> {
        fn work(&mut self, doc: &TypstDocument) {
            for (pg, s) in doc.pages.iter().zip(self.symbols.iter()) {
                self.active = s;
                self.work_frame(&pg.frame);
            }
        }

        fn work_frame(&mut self, k: &Frame) {
            for (_, item) in k.items() {
                let text = match item {
                    FrameItem::Group(g) => {
                        self.work_frame(&g.frame);
                        continue;
                    }
                    FrameItem::Text(text) => text,
                    FrameItem::Shape(_, _)
                    | FrameItem::Image(_, _, _)
                    | FrameItem::Link(_, _)
                    | FrameItem::Tag(_) => continue,
                    #[cfg(not(feature = "no-content-hint"))]
                    FrameItem::ContentHint(_) => continue,
                };

                let font = text.font.clone();
                for g in &text.glyphs {
                    let g_text = &text.text[g.range()];
                    let chars_count = g_text.chars().count();
                    if chars_count > 1 {
                        log::warn!("multi char glyph: {g_text}");
                        continue;
                    }
                    let Some(ch) = g_text.chars().next() else {
                        continue;
                    };
                    if ch.is_whitespace() {
                        continue;
                    }
                    log::debug!(
                        "glyph: {active} => {ch} ({chc:x})",
                        active = self.active,
                        chc = ch as u32
                    );
                    self.res
                        .insert(self.active.to_owned(), (font.clone(), GlyphId(g.id)));
                }
            }
        }
    }

    res
}

fn populate(
    sym: &Symbol,
    mod_name: &str,
    sym_name: &str,
    fallback_cat: SymCategory,
    out: &mut ResourceSymbolMap,
) {
    for (modifier_name, ch) in sym.variants() {
        let mut name =
            String::with_capacity(mod_name.len() + sym_name.len() + modifier_name.len() + 2);

        name.push_str(mod_name);
        name.push('.');
        name.push_str(sym_name);

        if !modifier_name.is_empty() {
            name.push('.');
            name.push_str(modifier_name);
        }

        let category = CAT_MAP.get(name.as_str()).cloned().unwrap_or(fallback_cat);
        out.insert(
            name,
            ResourceSymbolItem {
                category,
                unicode: ch.char() as u32,
                glyphs: vec![],
            },
        );
    }
}

fn populate_scope(
    sym: &Scope,
    mod_name: &str,
    fallback_cat: SymCategory,
    out: &mut ResourceSymbolMap,
) {
    for (k, v, _) in sym.iter() {
        let Value::Symbol(sym) = v else {
            continue;
        };

        populate(sym, mod_name, k, fallback_cat, out)
    }
}
