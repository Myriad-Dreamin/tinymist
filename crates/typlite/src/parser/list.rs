//! HTML list parsing module, handling conversion of ordered and unordered lists.

use ecow::eco_format;
use typst_html::{HtmlElement, HtmlNode, tag};

use crate::Result;
use crate::attributes::{EnumAttr, ListAttr, ListItemAttr, TypliteAttrsParser};
use crate::ir::{Block, Inline, ListItem};
use crate::tags::md_tag;

use super::core::HtmlToIrParser;

/// List parser.
pub struct ListParser;

enum StructuredListKind {
    Unordered,
    Ordered { start: Option<u32>, reversed: bool },
}

impl ListParser {
    /// Convert HTML list to list items.
    pub fn convert_list(
        parser: &mut HtmlToIrParser,
        element: &HtmlElement,
    ) -> Result<Vec<ListItem>> {
        parser.list_level += 1;

        let prev_buffer = std::mem::take(&mut parser.inline_buffer);
        let is_ordered = element.tag == tag::ol;
        let mut all_items = Vec::new();

        for child in &element.children {
            if let HtmlNode::Element(li) = child
                && li.tag == tag::li
            {
                let attrs = ListItemAttr::parse(&li.attrs)?;
                let mut item_content = Vec::new();

                let mut li_buffer: Vec<Inline> = Vec::new();

                if parser.feat.annotate_elem {
                    li_buffer.push(Inline::Comment(eco_format!(
                        "typlite:begin:list-item {}",
                        parser.list_level - 1
                    )));
                }

                for li_child in &li.children {
                    match li_child {
                        HtmlNode::Text(text, _) => {
                            li_buffer.push(Inline::Text(text.clone()));
                        }
                        HtmlNode::Element(child_elem) => {
                            let element_content = parser.process_list_item_element(child_elem)?;
                            if !element_content.is_empty() {
                                li_buffer.extend(element_content);
                            }
                        }
                        _ => {}
                    }
                }

                if parser.feat.annotate_elem {
                    li_buffer.push(Inline::Comment(eco_format!(
                        "typlite:end:list-item {}",
                        parser.list_level - 1
                    )));
                }

                if !li_buffer.is_empty() {
                    item_content.push(Block::Paragraph(li_buffer));
                }

                if !item_content.is_empty() {
                    if is_ordered {
                        all_items.push(ListItem::Ordered {
                            number: attrs.value,
                            content: item_content,
                        });
                    } else {
                        all_items.push(ListItem::Unordered {
                            content: item_content,
                        });
                    }
                }
            }
        }

        parser.inline_buffer = prev_buffer;
        parser.list_level -= 1;

        Ok(all_items)
    }

    fn convert_structured_list(
        parser: &mut HtmlToIrParser,
        element: &HtmlElement,
        kind: StructuredListKind,
    ) -> Result<Block> {
        let ordered = matches!(kind, StructuredListKind::Ordered { .. });
        let mut items = Self::convert_structured_list_items(parser, element, ordered)?;

        if let StructuredListKind::Ordered { start, reversed } = kind {
            if reversed {
                let mut current = start.unwrap_or(items.len().max(1) as u32);
                for item in items.iter_mut() {
                    if let ListItem::Ordered { number, .. } = item {
                        *number = Some(current);
                        current = current.saturating_sub(1);
                    }
                }
                return Ok(Block::OrderedList {
                    start: start.unwrap_or(items.len().max(1) as u32),
                    items,
                });
            } else {
                return Ok(Block::OrderedList {
                    start: start.unwrap_or(1),
                    items,
                });
            }
        }

        Ok(Block::UnorderedList(items))
    }

    fn convert_structured_list_items(
        parser: &mut HtmlToIrParser,
        element: &HtmlElement,
        ordered: bool,
    ) -> Result<Vec<ListItem>> {
        parser.list_level += 1;
        let prev_buffer = std::mem::take(&mut parser.inline_buffer);
        let mut all_items = Vec::new();

        for child in &element.children {
            if let HtmlNode::Element(li) = child
                && li.tag == md_tag::item
            {
                let attrs = ListItemAttr::parse(&li.attrs)?;
                let mut item_content = Vec::new();
                let mut li_buffer: Vec<Inline> = Vec::new();

                if parser.feat.annotate_elem {
                    li_buffer.push(Inline::Comment(eco_format!(
                        "typlite:begin:list-item {}",
                        parser.list_level - 1
                    )));
                }

                for li_child in &li.children {
                    match li_child {
                        HtmlNode::Text(text, _) => li_buffer.push(Inline::Text(text.clone())),
                        HtmlNode::Element(child_elem) => {
                            let element_content = parser.process_list_item_element(child_elem)?;
                            if !element_content.is_empty() {
                                li_buffer.extend(element_content);
                            }
                        }
                        HtmlNode::Frame(frame) => {
                            li_buffer.push(parser.convert_frame(&frame.inner));
                        }
                        HtmlNode::Tag(..) => {}
                    }
                }

                if parser.feat.annotate_elem {
                    li_buffer.push(Inline::Comment(eco_format!(
                        "typlite:end:list-item {}",
                        parser.list_level - 1
                    )));
                }

                if !li_buffer.is_empty() {
                    item_content.push(Block::Paragraph(std::mem::take(&mut li_buffer)));
                }

                if !item_content.is_empty() {
                    if ordered {
                        all_items.push(ListItem::Ordered {
                            number: attrs.value,
                            content: item_content,
                        });
                    } else {
                        all_items.push(ListItem::Unordered {
                            content: item_content,
                        });
                    }
                }
            }
        }

        parser.list_level -= 1;
        parser.inline_buffer = prev_buffer;

        Ok(all_items)
    }

    pub fn convert_m1_list(
        parser: &mut HtmlToIrParser,
        element: &HtmlElement,
        attrs: &ListAttr,
    ) -> Result<Block> {
        let _ = attrs;
        Self::convert_structured_list(parser, element, StructuredListKind::Unordered)
    }

    pub fn convert_m1_enum(
        parser: &mut HtmlToIrParser,
        element: &HtmlElement,
        attrs: &EnumAttr,
    ) -> Result<Block> {
        Self::convert_structured_list(
            parser,
            element,
            StructuredListKind::Ordered {
                start: attrs.start,
                reversed: attrs.reversed,
            },
        )
    }
}
