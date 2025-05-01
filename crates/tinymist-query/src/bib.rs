use hayagriva::{
    BibliographyDriver, BibliographyRequest, BufWriteFormat, CitationItem, CitationRequest,
    ElemChildren,
};

use crate::analysis::BibInfo;

pub(crate) struct RenderedBibCitation {
    pub citation: String,
    pub bib_item: String,
}

/// Render the citation string in the bib with given CSL style.
pub(crate) fn render_citation_string(
    bib_info: &BibInfo,
    key: &str,
    support_html: bool,
) -> Option<RenderedBibCitation> {
    let entry = bib_info.entries.get(key)?;
    let raw_entry = entry.raw_entry.as_ref()?;

    let mut driver = BibliographyDriver::new();

    let locales = &[];
    driver.citation(CitationRequest::from_items(
        vec![CitationItem::with_entry(raw_entry)],
        bib_info.csl_style.get(),
        locales,
    ));

    let result = driver.finish(BibliographyRequest {
        style: bib_info.csl_style.get(),
        locale: None, // todo: get locale from CiteElem
        locale_files: locales,
    });
    let rendered_bib = result.bibliography?;

    let format_elem = |elem: &ElemChildren| {
        let mut buf = String::new();
        elem.write_buf(
            &mut buf,
            if support_html {
                BufWriteFormat::Html
            } else {
                BufWriteFormat::Plain
            },
        )
        .ok()?;
        Some(buf)
    };

    Some(RenderedBibCitation {
        citation: format_elem(&result.citations.first()?.citation)?,
        bib_item: format_elem(&rendered_bib.items.first()?.content)?,
    })
}
