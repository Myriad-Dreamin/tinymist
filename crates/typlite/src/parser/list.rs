//! HTML list parsing module, handling conversion of ordered and unordered lists

use cmark_writer::ast::{ListItem, Node};
use typst::html::{tag, HtmlElement, HtmlNode};

use crate::attributes::{ListItemAttr, TypliteAttrsParser};
use crate::Result;

use super::core::HtmlToAstParser;

/// List parser
pub struct ListParser;

impl ListParser {
    /// Convert HTML list to ListItem vector
    pub fn convert_list(
        parser: &mut HtmlToAstParser,
        element: &HtmlElement,
    ) -> Result<Vec<ListItem>> {
        let mut all_items = Vec::new();
        let prev_buffer = std::mem::take(&mut parser.inline_buffer);
        let is_ordered = element.tag == tag::ol;

        for child in &element.children {
            if let HtmlNode::Element(li) = child {
                if li.tag == tag::li {
                    let attrs = ListItemAttr::parse(&li.attrs)?;
                    let mut item_content = Vec::new();

                    for li_child in &li.children {
                        match li_child {
                            HtmlNode::Text(text, _) => {
                                parser
                                    .inline_buffer
                                    .push(Node::Text(text.as_str().to_string()));
                            }
                            HtmlNode::Element(child_elem) => {
                                if child_elem.tag == tag::ul || child_elem.tag == tag::ol {
                                    // Handle nested lists
                                    if !parser.inline_buffer.is_empty() {
                                        item_content.push(Node::Paragraph(std::mem::take(
                                            &mut parser.inline_buffer,
                                        )));
                                    }

                                    let items = Self::convert_list(parser, child_elem)?;
                                    if child_elem.tag == tag::ul {
                                        item_content.push(Node::UnorderedList(items));
                                    } else {
                                        item_content.push(Node::OrderedList { start: 1, items });
                                    }
                                } else {
                                    parser.convert_element(child_elem)?;
                                }
                            }
                            _ => {}
                        }
                    }

                    if !parser.inline_buffer.is_empty() {
                        item_content
                            .push(Node::Paragraph(std::mem::take(&mut parser.inline_buffer)));
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
        }

        parser.inline_buffer = prev_buffer;
        Ok(all_items)
    }
}
