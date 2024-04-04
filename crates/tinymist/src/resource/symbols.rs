pub use super::prelude::*;

#[derive(Debug, Serialize, Deserialize)]
struct ResourceSymbolResponse {
    symbols: HashMap<String, ResourceSymbolItem>,
    #[serde(rename = "fontSelects")]
    font_selects: Vec<FontItem>,
    #[serde(rename = "glyphDefs")]
    glyph_defs: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ResourceSymbolItem {
    category: SymCategory,
    unicode: u32,
    glyphs: Vec<ResourceGlyphDesc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum SymCategory {
    #[serde(rename = "accent")]
    Accent,
    #[serde(rename = "greek")]
    Greek,
    #[serde(rename = "misc")]
    Misc,
}

#[derive(Debug, Serialize, Deserialize)]
struct ResourceGlyphDesc {
    #[serde(rename = "fontIndex")]
    font_index: u32,
    #[serde(rename = "xAdvance")]
    x_advance: Option<u16>,
    #[serde(rename = "yAdvance")]
    y_advance: Option<u16>,
    #[serde(rename = "xMin")]
    x_min: Option<i16>,
    #[serde(rename = "xMax")]
    x_max: Option<i16>,
    #[serde(rename = "yMin")]
    y_min: Option<i16>,
    #[serde(rename = "yMax")]
    y_max: Option<i16>,
    name: Option<String>,
    shape: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FontItem {
    family: String,
    #[serde(rename = "capHeight")]
    cap_height: f32,
    ascender: f32,
    descender: f32,
    #[serde(rename = "unitsPerEm")]
    units_per_em: f32,
    // vertical: bool,
}

type ResourceSymbolMap = HashMap<String, ResourceSymbolItem>;

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
    ])
});

impl TypstLanguageServer {
    /// Get the all valid symbols
    pub fn get_symbol_resources(&self) -> ZResult<JsonValue> {
        let mut symbols = ResourceSymbolMap::new();
        populate_scope(typst::symbols::sym().scope(), "sym", &mut symbols);
        // currently we don't have plan on emoji
        // populate_scope(typst::symbols::emoji().scope(), "emoji", &mut symbols);

        let chars = symbols
            .values()
            .map(|e| char::from_u32(e.unicode).unwrap())
            .collect::<String>();

        let font = self
            .primary()
            .steal(move |e| {
                use typst::text::FontVariant;
                use typst::World;
                let book = e.compiler.world().book();

                // todo: bad font fallback

                let fonts = &["New Computer Modern Math", "Latin Modern Math", "Cambria"];

                let preferred_fonts = fonts
                    .iter()
                    .flat_map(|f| {
                        let f = f.to_lowercase();
                        book.select(&f, FontVariant::default())
                            .and_then(|i| e.compiler.world().font(i))
                    })
                    .collect::<Vec<_>>();

                let mut last_hit: Option<typst_ts_core::TypstFont> = None;

                log::info!("font init: {hit:?}", hit = last_hit);

                let fonts = chars
                    .chars()
                    .map(|c| {
                        for font in &preferred_fonts {
                            if font.info().coverage.contains(c as u32) {
                                return Some(font.clone());
                            }
                        }

                        if let Some(last_hit) = &last_hit {
                            if last_hit.info().coverage.contains(c as u32) {
                                return Some(last_hit.clone());
                            }
                        }

                        let hit =
                            book.select_fallback(None, FontVariant::default(), &c.to_string())?;
                        last_hit = e.compiler.world().font(hit);

                        log::info!("font hit: {hit:?}", hit = last_hit);

                        last_hit.clone()
                    })
                    .collect::<Vec<_>>();

                fonts
            })
            .ok();

        let mut glyph_def = String::new();

        let mut collected_fonts = None;

        if let Some(fonts) = font.clone() {
            let glyph_provider = typst_ts_core::font::GlyphProvider::default();
            let glyph_pass =
                typst_ts_core::vector::pass::ConvertInnerImpl::new(glyph_provider, false);

            let mut glyph_renderer = Svg::default();
            let mut glyphs = vec![];

            let font_collected = fonts
                .iter()
                .flatten()
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .cloned()
                .collect::<Vec<_>>();

            let mut render_char = |font: Option<&typst_ts_core::TypstFont>, u| {
                let font = font?;
                let font_index = font_collected.iter().position(|e| e == font).unwrap() as u32;

                let id = font.ttf().glyph_index(char::from_u32(u).unwrap())?;
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

            for ((_, v), font) in symbols.iter_mut().zip(fonts.iter()) {
                let Some(desc) = render_char(font.as_ref(), v.unicode) else {
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

        serde_json::to_value(resp).context("cannot serialize response")
    }
}

fn populate(sym: &Symbol, mod_name: &str, sym_name: &str, out: &mut ResourceSymbolMap) {
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

        let category = CAT_MAP
            .get(name.as_str())
            .cloned()
            .unwrap_or(SymCategory::Misc);
        out.insert(
            name,
            ResourceSymbolItem {
                category,
                unicode: ch as u32,
                glyphs: vec![],
            },
        );
    }
}

fn populate_scope(sym: &Scope, mod_name: &str, out: &mut ResourceSymbolMap) {
    for (k, v) in sym.iter() {
        let Value::Symbol(sym) = v else {
            continue;
        };

        populate(sym, mod_name, k, out)
    }
}
