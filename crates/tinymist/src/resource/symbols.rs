use std::sync::LazyLock;
use std::{path::Path, sync::Arc};

use reflexo_typst::TypstPagedDocument;
use reflexo_typst::{vector::font::GlyphId, TypstFont};
use reflexo_vec2svg::SvgGlyphBuilder;
use sync_ls::LspResult;
use tinymist_query::GLOBAL_STATS;
use typst::foundations::Bytes;
use typst::{syntax::VirtualPath, World};

use super::prelude::*;
use crate::project::LspComputeGraph;
use crate::world::{base::ShadowApi, EntryState, TaskInputs};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResourceSymbolResponse {
    symbols: Vec<ResourceSymbolItem>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResourceSymbolItem {
    id: String,
    category: SymCategory,
    value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    glyph: Option<String>,
}

#[derive(Debug)]
struct SymbolItem {
    id: String,
    category: SymCategory,
    /// The Unicode codepoint(s) representing this symbol.
    value: String,
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
    GameTheory,
    NumberTheory,
    Algebra,
    Geometry,
    Astronomical,
    Currency,
    Music,
    Shape,
    /// Arrows, harpoons, and tacks.
    Arrow,
    // Lowercase Greek and Uppercase Greek
    Greek,
    Cyrillic,
    Hebrew,
    DoubleStruck,
    Miscellany,
    /// Miscellaneous Technical, Miscellaneous and Miscellaneous letter-likes
    Misc,
}

static CAT_MAP: LazyLock<HashMap<&str, SymCategory>> = LazyLock::new(|| {
    macro_rules! build_symbols {
        (
           $($category:ident => $($symbol:ident)+; )+
        ) => {
            HashMap::from_iter([
                $(
                    $(
                        (stringify!($symbol), SymCategory::$category),
                    )+
                )+
            ])
        };
    }

    build_symbols! {
        Control => wj zwj zwnj zws lrm rlm;
        Space => space;
        Delimiter => paren brace bracket shell bag mustache bar fence chevron ceil floor corner;
        Punctuation => amp ast at backslash co colon comma dagger dash dot excl quest interrobang hash hyph numero percent permille permyriad pilcrow section semi slash dots tilde;
        Accent => acute breve caret caron hat diaer grave macron quote prime;
        Arithmetic => plus minus div times ratio;
        Relation => eq gt lt approx prec succ equiv smt lat prop original image asymp;
        SetTheory => emptyset nothing without complement in subset supset union inter sect;
        Calculus => infinity oo diff partial gradient nabla sum product integral laplace;
        Logic => forall exists top bot not and or xor models forces therefore because qed;
        FunctionAndCategoryTheory => mapsto compose convolve multimap;
        GameTheory => tiny miny;
        NumberTheory => divides;
        Algebra => wreath;
        Geometry => angle angzarr parallel perp;
        Astronomical => earth jupiter mars mercury neptune saturn sun uranus venus;
        Currency => afghani baht bitcoin cedi cent currency dollar dong dorome dram euro franc guarani hryvnia kip lari lira manat naira pataca peso pound riel ruble rupee shekel som taka taman tenge togrog won yen yuan;
        Music => note rest natural flat sharp;
        Shape => bullet circle ellipse triangle square rect penta hexa diamond lozenge parallelogram star;
        Arrow => arrow arrows arrowhead harpoon harpoons tack;
        Greek => alpha beta chi delta digamma epsilon eta gamma iota kai kappa lambda mu nu omega omicron phi pi psi rho sigma tau theta upsilon xi zeta Alpha Beta Chi Delta Digamma Epsilon Eta Gamma Iota Kai Kappa Lambda Mu Nu Omega Omicron Phi Pi Psi Rho Sigma Tau Theta Upsilon Xi Zeta;
        Cyrillic => sha Sha;
        Hebrew => aleph alef beth bet gimel gimmel daleth dalet shin;
        DoubleStruck => AA BB CC DD EE FF GG HH II JJ KK LL MM NN OO PP QQ RR SS TT UU VV WW XX YY ZZ;
        Miscellany => die errorbar gender;
        Misc => diameter interleave join hourglass degree smash power smile frown ballot checkmark crossmark floral refmark cc copyright copyleft trademark maltese suit angstrom ell planck Re Im dotless;
    }
});

impl ServerState {
    /// Get the all valid symbols
    pub async fn get_symbol_resources(snap: LspComputeGraph) -> LspResult<JsonValue> {
        let symbols = collect_symbols(&snap)?;

        let glyph_mapping = render_symbols(&snap, &symbols)?;

        let symbols = render_glyphs(&symbols, &glyph_mapping)?;

        let resp = ResourceSymbolResponse { symbols };

        serde_json::to_value(resp)
            .context("cannot serialize response")
            .map_err(internal_error)
    }
}

fn collect_symbols(snap: &LspComputeGraph) -> LspResult<Vec<SymbolItem>> {
    let std = snap
        .library()
        .std
        .read()
        .scope()
        .ok_or_else(|| internal_error("cannot get std scope"))?;
    let sym = std
        .get("sym")
        .ok_or_else(|| internal_error("cannot get sym"))?;

    let mut symbols = Vec::new();
    if let Some(scope) = sym.read().scope() {
        populate_scope(scope, "sym", SymCategory::Misc, &mut symbols);
    }
    // todo: disabling emoji module, as there is performant issue on emojis
    // let _ = emoji;
    // populate_scope(emoji().scope(), "emoji", SymCategory::Emoji, &mut symbols);

    Ok(symbols)
}

fn populate_scope(
    sym: &Scope,
    mod_name: &str,
    fallback_cat: SymCategory,
    out: &mut Vec<SymbolItem>,
) {
    for (k, b) in sym.iter() {
        let Value::Symbol(sym) = b.read() else {
            continue;
        };

        populate(sym, mod_name, k, fallback_cat, out)
    }
}

fn populate(
    sym: &Symbol,
    mod_name: &str,
    sym_name: &str,
    fallback_cat: SymCategory,
    out: &mut Vec<SymbolItem>,
) {
    for (modifier_name, ch, _) in sym.variants() {
        let mut name = String::with_capacity(
            mod_name.len() + sym_name.len() + modifier_name.as_str().len() + 2,
        );

        name.push_str(mod_name);
        name.push('.');
        name.push_str(sym_name);

        if !modifier_name.is_empty() {
            name.push('.');
            name.push_str(modifier_name.as_str());
        }

        let category = CAT_MAP.get(sym_name).cloned().unwrap_or(fallback_cat);
        out.push(SymbolItem {
            id: name,
            category,
            value: ch.into(),
        });
    }
}

fn render_symbols(
    snap: &LspComputeGraph,
    symbols: &[SymbolItem],
) -> LspResult<HashMap<String, (TypstFont, GlyphId)>> {
    const PRELUDE: &str = r#"#show math.equation: set text(font: (
  "New Computer Modern Math",
  "Latin Modern Math",
  "STIX Two Math",
  "Cambria Math",
  "New Computer Modern",
  "Cambria",
))
"#;

    let math_shaping_text = symbols.iter().fold(PRELUDE.to_owned(), |mut o, it| {
        use std::fmt::Write;
        writeln!(o, "$#{}$/* {} */#pagebreak()", it.id, it.value).ok();
        o
    });
    log::debug!("math shaping text: {math_shaping_text}");

    let entry_path: Arc<Path> = Path::new("/._sym_.typ").into();

    let new_entry = EntryState::new_rootless(VirtualPath::new(&entry_path));

    let mut forked = snap.world().task(TaskInputs {
        entry: Some(new_entry),
        ..TaskInputs::default()
    });

    let _guard = GLOBAL_STATS.stat(forked.main(), "render_symbols");
    forked
        .map_shadow_by_id(forked.main(), Bytes::from_string(math_shaping_text))
        .map_err(|e| error_once!("cannot map shadow", err: e))
        .map_err(internal_error)?;

    let sym_doc = typst::compile::<TypstPagedDocument>(&forked)
        .output
        .map_err(|e| error_once!("cannot compile symbols", err: format!("{e:?}")))
        .map_err(internal_error)?;

    log::debug!("sym doc: {sym_doc:?}");

    let res = extract_rendered_symbols(&sym_doc, symbols.iter().map(|it| &it.id));

    Ok(res)
}

fn extract_rendered_symbols<'a>(
    doc: &TypstPagedDocument,
    symbols: impl Iterator<Item = &'a String>,
) -> HashMap<String, (TypstFont, GlyphId)> {
    use typst::layout::Frame;
    use typst::layout::FrameItem;

    struct Worker {
        res: HashMap<String, (TypstFont, GlyphId)>,
    }

    impl Worker {
        fn work<'a>(
            &mut self,
            paged_doc: &TypstPagedDocument,
            symbols: impl Iterator<Item = &'a String>,
        ) {
            for (pg, s) in paged_doc.pages.iter().zip(symbols) {
                self.work_frame(&pg.frame, s);
            }
        }

        fn work_frame(&mut self, k: &Frame, active: &str) {
            for (_, item) in k.items() {
                let text = match item {
                    FrameItem::Group(g) => {
                        self.work_frame(&g.frame, active);
                        continue;
                    }
                    FrameItem::Text(text) => text,
                    _ => continue,
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
                    log::debug!("glyph: {active} => {ch} ({chc:x})", chc = ch as u32);
                    self.res
                        .insert(active.to_owned(), (font.clone(), GlyphId(g.id)));
                }
            }
        }
    }

    let mut worker = Worker {
        res: HashMap::new(),
    };
    worker.work(doc, symbols);
    worker.res
}

fn render_glyphs(
    symbols: &[SymbolItem],
    glyph_mapping: &HashMap<String, (TypstFont, GlyphId)>,
) -> LspResult<Vec<ResourceSymbolItem>> {
    let glyph_provider = reflexo_vec2svg::GlyphProvider::default();
    let glyph_pass = reflexo_typst::vector::pass::ConvertInnerImpl::new(glyph_provider, false);

    let mut builder = SvgGlyphBuilder::new();

    let mut render_sym = |u| {
        let (font, id) = glyph_mapping.get(u)?.clone();

        let glyph = glyph_pass.must_flat_glyph(&GlyphItem::Raw(font.clone(), id))?;

        let rendered = builder.render_glyph("", &glyph)?; // the glyph_id does not matter here

        Some(create_display_svg(&font, id, &rendered))
    };

    let rendered_symbols = symbols
        .iter()
        .map(|it| ResourceSymbolItem {
            id: it.id.clone(),
            category: it.category,
            value: it.value.clone(),
            glyph: render_sym(&it.id),
        })
        .collect();

    Ok(rendered_symbols)
}

fn create_display_svg(font: &TypstFont, gid: GlyphId, svg_path: &str) -> String {
    let face = font.ttf();

    let (x_min, x_max) = face
        .glyph_bounding_box(gid)
        .map(|bbox| (bbox.x_min as f32, bbox.x_max as f32))
        .unwrap_or_default();

    // Font-wide metrics
    let units_per_em = font.metrics().units_per_em as f32;
    let ascender = font.metrics().ascender.get() as f32 * units_per_em;
    let descender = font.metrics().descender.get() as f32 * units_per_em; // usually negative

    // Horizontal advance (fallback to em)
    let x_advance = face
        .glyph_hor_advance(gid)
        .map(f32::from)
        .unwrap_or(units_per_em);

    // Start viewBox.x at left-most ink or 0, whichever is smaller (to include left
    // overhang)
    let view_x = x_min.min(0.0);

    // Start view width as the advance; enlarge if ink extends past that
    let view_w = x_advance.max(x_max - view_x);

    // Vertical viewBox uses font ascender/descent so baseline is at y=0
    let view_y = -ascender;
    let view_h = ascender - descender; // ascender - (negative descender) -> total height

    let svg_content = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{view_x} {view_y} {view_w} {view_h}" preserveAspectRatio="xMidYMid meet">
<g transform="scale(1 -1)">{svg_path}</g>
</svg>"#
    );

    svg_content
}
