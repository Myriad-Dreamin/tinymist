//! HTML table parsing module, processes the conversion of table elements.

use ecow::EcoString;
use typst_html::{HtmlElement, HtmlNode};

use crate::Result;
use crate::attributes::{TableAttr, TableCellAttr, TypliteAttrsParser};
use crate::ir::{
    Block, Inline, IrNode, Table, TableAlignment, TableCell, TableCellKind, TableRow, TableRowKind,
};
use crate::tags::md_tag;

use super::core::HtmlToIrParser;

/// Responsible for finding HTML table elements in the DOM structure.
pub struct TableStructureFinder;

impl TableStructureFinder {
    /// Locate the structured table element emitted from markdown.typ.
    pub fn find_structured_table(element: &HtmlElement) -> Option<&HtmlElement> {
        if element.tag == md_tag::table {
            Some(element)
        } else if element.tag == md_tag::grid {
            element.children.iter().find_map(|child| {
                if let HtmlNode::Element(table_elem) = child
                    && table_elem.tag == md_tag::table
                {
                    return Some(table_elem);
                }
                None
            })
        } else {
            None
        }
    }
}

/// Table parser.
pub struct TableParser;

impl TableParser {
    /// Convert structured table nodes to semantic IR.
    pub fn convert_table(
        parser: &mut HtmlToIrParser,
        element: &HtmlElement,
    ) -> Result<Option<Block>> {
        let Some(table) = TableStructureFinder::find_structured_table(element) else {
            return Ok(None);
        };

        Self::convert_structured_table(parser, table)
    }

    fn convert_structured_table(
        parser: &mut HtmlToIrParser,
        element: &HtmlElement,
    ) -> Result<Option<Block>> {
        let attrs = TableAttr::parse(&element.attrs)?;
        let columns = attrs.columns.unwrap_or(1).max(1);
        let alignments = Self::parse_table_alignments(attrs.align.as_ref(), columns);

        let mut header_rows: Vec<TableRow> = Vec::new();
        let mut body_rows: Vec<TableRow> = Vec::new();
        let mut footer_rows: Vec<TableRow> = Vec::new();
        let mut pending_body: Vec<TableCell> = Vec::new();
        let mut pending_width = 0;
        let mut header_allowed = true;

        for child in &element.children {
            if let HtmlNode::Element(child_elem) = child {
                if child_elem.tag == md_tag::header {
                    let row_kind = if header_allowed {
                        TableRowKind::Head
                    } else {
                        TableRowKind::Body
                    };
                    let row = Self::convert_structured_row(
                        parser,
                        child_elem,
                        row_kind,
                        TableCellKind::Header,
                        columns,
                    )?;
                    if header_allowed {
                        header_rows.push(row);
                    } else {
                        Self::flush_pending_row(
                            &mut pending_body,
                            &mut pending_width,
                            &mut body_rows,
                            columns,
                        );
                        body_rows.push(row);
                    }
                } else if child_elem.tag == md_tag::footer {
                    header_allowed = false;
                    Self::flush_pending_row(
                        &mut pending_body,
                        &mut pending_width,
                        &mut body_rows,
                        columns,
                    );
                    footer_rows.push(Self::convert_structured_row(
                        parser,
                        child_elem,
                        TableRowKind::Foot,
                        TableCellKind::Data,
                        columns,
                    )?);
                } else if child_elem.tag == md_tag::cell {
                    header_allowed = false;
                    let cell =
                        Self::convert_structured_cell(parser, child_elem, TableCellKind::Data)?;
                    pending_width += cell.colspan;
                    pending_body.push(cell);
                    if pending_width >= columns {
                        Self::pad_cells(&mut pending_body, columns, TableCellKind::Data);
                        body_rows.push(TableRow {
                            kind: TableRowKind::Body,
                            cells: std::mem::take(&mut pending_body),
                        });
                        pending_width = 0;
                    }
                }
            }
        }

        Self::flush_pending_row(
            &mut pending_body,
            &mut pending_width,
            &mut body_rows,
            columns,
        );

        if header_rows.is_empty() && !body_rows.is_empty() {
            if let Some(first) = body_rows.first_mut() {
                first.kind = TableRowKind::Head;
                for cell in &mut first.cells {
                    cell.kind = TableCellKind::Header;
                }
                header_rows.push(body_rows.remove(0));
            }
        }

        if header_rows.is_empty() && body_rows.is_empty() {
            return Ok(None);
        }

        let mut rows = Vec::new();
        rows.extend(header_rows);
        rows.extend(body_rows);
        rows.extend(footer_rows);

        Ok(Some(Block::Table(Table {
            columns,
            rows,
            alignments,
        })))
    }

    fn convert_structured_row(
        parser: &mut HtmlToIrParser,
        element: &HtmlElement,
        row_kind: TableRowKind,
        cell_kind: TableCellKind,
        columns: usize,
    ) -> Result<TableRow> {
        let mut cells = Vec::new();
        for child in &element.children {
            if let HtmlNode::Element(cell) = child
                && cell.tag == md_tag::cell
            {
                cells.push(Self::convert_structured_cell(
                    parser,
                    cell,
                    cell_kind.clone(),
                )?);
            }
        }
        Self::pad_cells(&mut cells, columns, cell_kind.clone());
        Ok(TableRow {
            kind: row_kind,
            cells,
        })
    }

    fn convert_structured_cell(
        parser: &mut HtmlToIrParser,
        cell: &HtmlElement,
        kind: TableCellKind,
    ) -> Result<TableCell> {
        let attrs = TableCellAttr::parse(&cell.attrs)?;
        let (inline_nodes, block_nodes) = parser.capture_children(cell)?;

        let mut content: Vec<IrNode> = Vec::new();
        for inline in inline_nodes {
            content.push(IrNode::Inline(inline));
        }
        for block in block_nodes {
            content.push(IrNode::Block(block));
        }

        let mut table_cell = TableCell {
            kind,
            colspan: attrs.colspan.unwrap_or(1).max(1),
            rowspan: attrs.rowspan.unwrap_or(1).max(1),
            content,
            align: Self::parse_cell_alignment(attrs.align.as_ref()),
        };

        if table_cell.content.is_empty() {
            table_cell
                .content
                .push(IrNode::Inline(Inline::Text(EcoString::new())));
        }

        Ok(table_cell)
    }

    fn flush_pending_row(
        pending_row: &mut Vec<TableCell>,
        pending_width: &mut usize,
        rows: &mut Vec<TableRow>,
        columns: usize,
    ) {
        if !pending_row.is_empty() {
            Self::pad_cells(pending_row, columns, TableCellKind::Data);
            rows.push(TableRow {
                kind: TableRowKind::Body,
                cells: std::mem::take(pending_row),
            });
            *pending_width = 0;
        }
    }

    fn pad_cells(cells: &mut Vec<TableCell>, columns: usize, default_kind: TableCellKind) {
        let mut width = 0;
        for cell in cells.iter() {
            width += cell.colspan;
        }
        while width < columns {
            cells.push(TableCell {
                kind: default_kind.clone(),
                colspan: 1,
                rowspan: 1,
                content: vec![IrNode::Inline(Inline::Text(EcoString::new()))],
                align: None,
            });
            width += 1;
        }
    }

    fn parse_table_alignments(value: Option<&EcoString>, columns: usize) -> Vec<TableAlignment> {
        let mut parsed: Vec<TableAlignment> = Vec::new();
        if let Some(value) = value {
            for token in value.split(',') {
                let token = token.trim();
                if token.is_empty() {
                    continue;
                }
                parsed.push(Self::parse_alignment_token(token));
            }
        }

        if parsed.is_empty() {
            vec![TableAlignment::None; columns]
        } else if parsed.len() < columns {
            let last = parsed.last().cloned().unwrap_or(TableAlignment::None);
            parsed.resize(columns, last);
            parsed
        } else {
            parsed.truncate(columns);
            parsed
        }
    }

    fn parse_cell_alignment(value: Option<&EcoString>) -> Option<TableAlignment> {
        value
            .map(Self::parse_alignment_token)
            .filter(|align| !matches!(align, TableAlignment::None))
    }

    fn parse_alignment_token(token: impl AsRef<str>) -> TableAlignment {
        let cleaned = token
            .as_ref()
            .trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .to_ascii_lowercase();

        match cleaned.as_str() {
            "left" => TableAlignment::Left,
            "right" => TableAlignment::Right,
            "center" => TableAlignment::Center,
            _ => TableAlignment::None,
        }
    }
}
