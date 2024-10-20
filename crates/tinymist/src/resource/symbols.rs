use std::{collections::BTreeMap, path::Path, sync::Arc};

// use reflexo_typst::font::GlyphId;
use reflexo_typst::{
    vector::font::GlyphId, world::EntryState, ShadowApi, TaskInputs, TypstDocument, TypstFont,
};
use sync_lsp::LspResult;

use crate::{actor::typ_client::QuerySnap, z_internal_error};

pub use super::prelude::*;

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
    Accent,
    Greek,
    ControlOrSpace,
    Misc,
    Emoji,
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
        ("sym.cancel", Accent),
        ("sym.grave", Accent),
        ("sym.acute", Accent),
        ("sym.hat", Accent),
        ("sym.widehat", Accent),
        ("sym.tilde", Accent),
        ("sym.macron", Accent),
        ("sym.breve", Accent),
        ("sym.dot", Accent),
        ("sym.dot.double", Accent),
        ("sym.dot.triple", Accent),
        ("sym.dot.quad", Accent),
        ("sym.acute.double", Accent),
        ("sym.caron", Accent),
        ("sym.breve", Accent),
        ("sym.caron", Accent),
        ("sym.circle", Accent),
        ("sym.alpha", Greek),
        ("sym.beta", Greek),
        ("sym.gamma", Greek),
        ("sym.delta", Greek),
        ("sym.epsilon.alt", Greek),
        ("sym.zeta", Greek),
        ("sym.eta", Greek),
        ("sym.theta", Greek),
        ("sym.iota", Greek),
        ("sym.kappa", Greek),
        ("sym.lambda", Greek),
        ("sym.mu", Greek),
        ("sym.nu", Greek),
        ("sym.xi", Greek),
        ("sym.omicron", Greek),
        ("sym.pi", Greek),
        ("sym.rho", Greek),
        ("sym.sigma", Greek),
        ("sym.tau", Greek),
        ("sym.upsilon", Greek),
        ("sym.phi.alt", Greek),
        ("sym.chi", Greek),
        ("sym.psi", Greek),
        ("sym.omega", Greek),
        ("sym.Alpha", Greek),
        ("sym.Beta", Greek),
        ("sym.Gamma", Greek),
        ("sym.Delta", Greek),
        ("sym.Epsilon", Greek),
        ("sym.Zeta", Greek),
        ("sym.Eta", Greek),
        ("sym.Theta", Greek),
        ("sym.Iota", Greek),
        ("sym.Kappa", Greek),
        ("sym.Lambda", Greek),
        ("sym.Mu", Greek),
        ("sym.Nu", Greek),
        ("sym.Xi", Greek),
        ("sym.Omicron", Greek),
        ("sym.Pi", Greek),
        ("sym.Rho", Greek),
        ("sym.Sigma", Greek),
        ("sym.Tau", Greek),
        ("sym.Upsilon", Greek),
        ("sym.Phi", Greek),
        ("sym.Chi", Greek),
        ("sym.Psi", Greek),
        ("sym.Omega", Greek),
        ("sym.beta.alt", Greek),
        ("sym.epsilon", Greek),
        ("sym.kappa.alt", Greek),
        ("sym.phi", Greek),
        ("sym.pi.alt", Greek),
        ("sym.rho.alt", Greek),
        ("sym.sigma.alt", Greek),
        ("sym.theta.alt", Greek),
        ("sym.ell", Greek),
        ("sym.lrm", ControlOrSpace),
        ("sym.rlm", ControlOrSpace),
        ("sym.wj", ControlOrSpace),
        ("sym.zwj", ControlOrSpace),
        ("sym.zwnj", ControlOrSpace),
        ("sym.zws", ControlOrSpace),
        ("sym.space", ControlOrSpace),
        ("sym.space.nobreak", ControlOrSpace),
        ("sym.space.nobreak.narrow", ControlOrSpace),
        ("sym.space.en", ControlOrSpace),
        ("sym.space.quad", ControlOrSpace),
        ("sym.space.third", ControlOrSpace),
        ("sym.space.quarter", ControlOrSpace),
        ("sym.space.sixth", ControlOrSpace),
        ("sym.space.med", ControlOrSpace),
        ("sym.space.fig", ControlOrSpace),
        ("sym.space.punct", ControlOrSpace),
        ("sym.space.thin", ControlOrSpace),
        ("sym.space.hair", ControlOrSpace),
    ])
});

impl LanguageState {
    /// Get the all valid symbols
    pub async fn get_symbol_resources(snap: QuerySnap) -> LspResult<JsonValue> {
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

            let new_entry = EntryState::new_rootless(entry_path.clone())
                .ok_or_else(|| error_once!("cannot change entry"))
                .map_err(z_internal_error)?;

            let mut forked = snap.world.task(TaskInputs {
                entry: Some(new_entry),
                ..Default::default()
            });
            forked
                .map_shadow(&entry_path, math_shaping_text.into_bytes().into())
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
