use std::collections::{BTreeMap, BTreeSet, HashMap};

use comemo::Track;
use tinymist_std::typst::TypstPagedDocument;
use typst::World;
use typst::engine::{Engine, Route, Sink, Traced};
use typst::foundations::{Content, NativeElement, Value};
use typst::introspection::{Introspector, Location, PagedPosition};
use typst::model::{BibliographyElem, FigureCaption, FootnoteElem, Works};
use typst::syntax::Span;
use typst::utils::Protected;

const FRAME_PREFIX: &[u8] = b"link-hover,";

pub(crate) fn frame(document: &TypstPagedDocument, world: Option<&dyn World>) -> Vec<u8> {
    let mut frame = FRAME_PREFIX.to_vec();
    serde_json::to_writer(&mut frame, &descriptions(document, world))
        .expect("link hover map serialization cannot fail");
    frame
}

fn descriptions(
    document: &TypstPagedDocument,
    world: Option<&dyn World>,
) -> BTreeMap<String, BTreeSet<String>> {
    let introspector = document.introspector();
    let targets = introspector.frame_link_targets();
    if targets.is_empty() {
        return BTreeMap::new();
    }

    let mut text = HashMap::new();
    for elem in introspector.query(&FootnoteElem::ELEM.select()) {
        if let Some(location) = elem.location()
            && targets.contains(&location.variant(1))
            && let Some(body) = elem
                .to_packed::<FootnoteElem>()
                .and_then(|note| note.body_content())
        {
            // Typst places the footnote entry at the first variant of the marker.
            text.insert(location.variant(1), body.clone());
        }
    }

    if let Some(world) = world {
        collect_bibliography_text(
            world,
            introspector.as_ref(),
            |location| targets.contains(location),
            &mut text,
        );
    }

    let mut descriptions = BTreeMap::<_, BTreeSet<_>>::new();
    for &location in targets {
        let Some(position) = introspector.position(location) else {
            continue;
        };
        let content = text
            .get(&location)
            .or_else(|| introspector.elements().get_by_loc(&location));
        if let Some(description) = content.and_then(description) {
            descriptions
                .entry(position_key(position))
                .or_default()
                .insert(description);
        }
    }
    descriptions
}

fn collect_bibliography_text(
    world: &dyn World,
    introspector: &dyn Introspector,
    is_target: impl Fn(&Location) -> bool,
    text: &mut HashMap<Location, Content>,
) {
    let bibliographies = introspector.query(&BibliographyElem::ELEM.select());
    if bibliographies.is_empty() {
        return;
    }

    let traced = Traced::default();
    let mut sink = Sink::new();
    let mut engine = Engine {
        library: world.library(),
        world: world.track(),
        route: Route::default(),
        introspector: Protected::new(introspector.track()),
        traced: traced.track(),
        sink: sink.track_mut(),
    };
    let Ok(works) = Works::generate(&mut engine, Span::detached()) else {
        return;
    };
    for elem in bibliographies {
        if let Some(location) = elem.location()
            && let Ok(bibliography) = works.bibliography(location, elem.span())
        {
            for entry in &bibliography.entries {
                if is_target(&entry.backlink) {
                    text.insert(entry.backlink, entry.body.clone());
                }
            }
        }
    }
}

fn description(content: &Content) -> Option<String> {
    // Prefer a figure caption or heading body to a whole element’s contents.
    let plain = match content
        .get_by_name("caption")
        .or_else(|_| content.get_by_name("body"))
    {
        Ok(Value::Content(body)) => match body.to_packed::<FigureCaption>() {
            Some(caption) => caption.body.plain_text(),
            None => body.plain_text(),
        },
        _ => content.plain_text(),
    };
    let normalized = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    (!normalized.is_empty()).then_some(normalized)
}

fn position_key(PagedPosition { page, point }: PagedPosition) -> String {
    // Match the coordinates serialized by Reflexo in handleTypstLocation.
    format!(
        "{page}, {}, {}",
        point.x.to_pt() as f32,
        point.y.to_pt() as f32
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_descriptions_match_rendered_link_destinations() {
        tinymist_tests::run_with_sources(
            r#"
            #set heading(numbering: "1.")
            See @intro, @diagram, @note, and @alpha @beta.#footnote[Footnote body] <note>

            = Destination heading <intro>
            #figure(rect(width: 1cm, height: 1cm), caption: [Figure caption]) <diagram>

            #bibliography(bytes("@book{alpha, author={Ada Example}, title={First Work}, year={2024}, publisher={Example Press}}\n@book{beta, author={Bea Example}, title={Second Work}, year={2025}, publisher={Example Press}}"), style: "ieee")
            "#,
            |verse, _| {
                let world = verse.snapshot();
                let document = typst::compile::<TypstPagedDocument>(&world)
                    .output
                    .expect("links fixture should compile");
                let metadata = frame(&document, Some(&world));
                let descriptions: BTreeMap<String, BTreeSet<String>> = serde_json::from_slice(
                    metadata
                        .strip_prefix(FRAME_PREFIX)
                        .expect("hover frame prefix"),
                )
                .expect("hover frame should contain a map");

                let svg = reflexo_vec2svg::render_svg(&document);
                let destination_for = |expected: &str| {
                    let (position, _) = descriptions
                        .iter()
                        .find(|(_, texts)| texts.iter().any(|text| text.contains(expected)))
                        .unwrap_or_else(|| panic!("missing {expected:?} in {descriptions:?}"));
                    assert!(svg.contains(&format!("handleTypstLocation(this, {position})")));
                    position
                };

                destination_for("Destination heading");
                let figure = destination_for("Figure caption");
                assert!(descriptions[figure].contains("Figure caption"));
                let footnote_positions = descriptions
                    .iter()
                    .filter(|(_, texts)| texts.contains("Footnote body"))
                    .map(|(position, _)| {
                        assert!(svg.contains(&format!("handleTypstLocation(this, {position})")));
                        position
                    })
                    .count();
                assert_eq!(
                    footnote_positions, 2,
                    "both footnote marker ref and footnote entry should resolve to Footnote body"
                );
                assert_ne!(
                    destination_for("A. Example, First Work. Example Press, 2024"),
                    destination_for("B. Example, Second Work. Example Press, 2025")
                );
            },
        );
    }
}
