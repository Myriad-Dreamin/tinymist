use crate::Result;
use crate::ir;
use crate::writer::html::{self, HtmlRenderOptions};

pub(super) fn render_ir_table_as_html(table: &ir::Table) -> Result<String> {
    html::render_table_as_html(
        table,
        &HtmlRenderOptions {
            strict: false,
            ..Default::default()
        },
    )
}

pub(super) fn render_ir_html_element_as_html(element: &ir::HtmlElement) -> Result<String> {
    html::render_html_element(
        element,
        &HtmlRenderOptions {
            strict: false,
            ..Default::default()
        },
    )
}

pub(super) fn render_ir_html_element_inline(element: &ir::HtmlElement) -> Result<String> {
    // Inline HTML should not include trailing newlines.
    let html = render_ir_html_element_as_html(element)?;
    Ok(html.trim_end_matches('\n').to_string())
}
