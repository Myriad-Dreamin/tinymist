//! HTML list parsing module, handling conversion of ordered and unordered lists

use cmark_writer::ast::{ListItem, Node};
use ecow::eco_format;
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
        parser.list_level += 1;

        let prev_buffer = std::mem::take(&mut parser.inline_buffer);
        let is_ordered = element.tag == tag::ol;
        let mut all_items = Vec::new();

        for child in &element.children {
            if let HtmlNode::Element(li) = child {
                if li.tag == tag::li {
                    let attrs = ListItemAttr::parse(&li.attrs)?;
                    let mut item_content = Vec::new();

                    let mut li_buffer = Vec::new();

                    if parser.feat.annotate_elem {
                        li_buffer.push(Node::Custom(Box::new(super::core::Comment(eco_format!(
                            "typlite:begin:list-item {}",
                            parser.list_level - 1
                        )))));
                    }

                    for li_child in &li.children {
                        match li_child {
                            HtmlNode::Text(text, _) => {
                                li_buffer.push(Node::Text(text.clone()));
                            }
                            HtmlNode::Element(child_elem) => {
                                let element_content =
                                    parser.process_list_item_element(child_elem)?;

                                if !element_content.is_empty() {
                                    li_buffer.extend(element_content);
                                }
                            }
                            _ => {}
                        }
                    }

                    if parser.feat.annotate_elem {
                        li_buffer.push(Node::Custom(Box::new(super::core::Comment(eco_format!(
                            "typlite:end:list-item {}",
                            parser.list_level - 1
                        )))));
                    }

                    if !li_buffer.is_empty() {
                        item_content.push(Node::Paragraph(li_buffer));
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
        parser.list_level -= 1;

        Ok(all_items)
    }
}
