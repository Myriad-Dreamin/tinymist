use crate::Result;
use crate::ir::{self, IrNode, Table, TableRowKind};

use super::emit::IrMarkdownEmitter;
use super::escape::indent_multiline;
use super::render_html::render_ir_table_as_html;

impl IrMarkdownEmitter {
    pub(super) fn render_table(&mut self, table: &Table, indent: usize) -> Result<String> {
        if should_render_table_as_html(table) {
            let html = render_ir_table_as_html(table)?;
            return Ok(indent_multiline(&html, indent)
                .trim_end_matches('\n')
                .to_string());
        }

        let mut out = String::new();
        let prefix = " ".repeat(indent);

        if table.rows.is_empty() {
            return Ok(out);
        }

        let head_row_idx = table
            .rows
            .iter()
            .position(|r| matches!(r.kind, TableRowKind::Head))
            .unwrap_or(0);
        let header_row = &table.rows[head_row_idx];

        out.push_str(&prefix);
        out.push_str(&render_gfm_row(&header_row.cells, self)?);
        out.push('\n');

        out.push_str(&prefix);
        out.push('|');
        for _ in 0..table.columns {
            out.push_str(" --- |");
        }
        out.push('\n');

        for (idx, row) in table.rows.iter().enumerate() {
            if idx == head_row_idx && matches!(row.kind, TableRowKind::Head) {
                continue;
            }
            if matches!(row.kind, TableRowKind::Head) {
                continue;
            }
            out.push_str(&prefix);
            out.push_str(&render_gfm_row(&row.cells, self)?);
            out.push('\n');
        }

        Ok(out.trim_end_matches('\n').to_string())
    }
}

fn render_gfm_row(cells: &[ir::TableCell], emitter: &mut IrMarkdownEmitter) -> Result<String> {
    let mut row = String::new();
    row.push('|');
    for cell in cells {
        row.push(' ');
        let text = render_table_cell_inline(cell, emitter)?;
        row.push_str(&text);
        row.push(' ');
        row.push('|');
    }
    Ok(row)
}

fn render_table_cell_inline(
    cell: &ir::TableCell,
    emitter: &mut IrMarkdownEmitter,
) -> Result<String> {
    let mut out = String::new();
    for node in &cell.content {
        match node {
            IrNode::Inline(inline) => {
                out.push_str(&emitter.render_inlines(std::slice::from_ref(inline))?);
            }
            IrNode::Block(_) => {
                // Shouldn't happen in GFM table.
            }
        }
    }
    Ok(out)
}

fn should_render_table_as_html(table: &Table) -> bool {
    for row in &table.rows {
        for cell in &row.cells {
            if cell.colspan != 1 || cell.rowspan != 1 {
                return true;
            }
            if cell.content.iter().any(|n| matches!(n, IrNode::Block(_))) {
                return true;
            }
        }
    }
    false
}
