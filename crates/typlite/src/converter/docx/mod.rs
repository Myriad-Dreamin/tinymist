//! DOCX converter implementation using docx-rs
//!
//! This module is organized into several main components:
//! - Converter: Core functionality for converting HTML to intermediate DocxNode structure
//! - Writer: Functionality for rendering intermediate DocxNode structure to DOCX format
//! - Styles: Document style management
//! - Numbering: List numbering management
//! - Node structures: DocxNode and DocxInline representing document structure

mod converter;
mod numbering;
mod styles;
mod utils;

use base64::Engine;
use cmark_writer::ast::{ListItem, Node};
use docx_rs::*;
use ecow::EcoString;
use std::io::Cursor;

use crate::converter::FormatWriter;
use crate::Result;
use crate::TypliteFeat;

pub use converter::DocxConverter;
use utils::calculate_image_dimensions;

/// 直接从 AST 生成 DOCX 的转换器（不使用 DocxNode 中间表示）
pub struct DocxWriter {
    _feat: TypliteFeat,
    styles: styles::DocxStyles,
    numbering: numbering::DocxNumbering,
    list_level: usize,
    list_numbering_count: usize,
}

impl DocxWriter {
    pub fn new(feat: TypliteFeat) -> Self {
        Self {
            _feat: feat,
            styles: styles::DocxStyles::new(),
            numbering: numbering::DocxNumbering::new(),
            list_level: 0,
            list_numbering_count: 0,
        }
    }

    /// 将 SVG 数据转换为 PNG 格式
    fn convert_svg_to_png(&self, svg_data: &[u8]) -> Result<Vec<u8>> {
        // 检查数据是否为有效的 SVG
        let svg_str = match std::str::from_utf8(svg_data) {
            Ok(s) => s,
            Err(_) => return Err("无法将输入数据解析为 UTF-8 字符串".into()),
        };

        let dpi = 300.0;
        let scale_factor = dpi / 96.0;

        let opt = resvg::usvg::Options {
            dpi,
            ..resvg::usvg::Options::default()
        };

        // 解析 SVG
        let rtree = match resvg::usvg::Tree::from_str(svg_str, &opt) {
            Ok(tree) => tree,
            Err(e) => return Err(format!("SVG 解析错误：{:?}", e).into()),
        };

        let size = rtree.size().to_int_size();
        let width = (size.width() as f32 * scale_factor) as u32;
        let height = (size.height() as f32 * scale_factor) as u32;

        // 创建像素缓冲区
        let mut pixmap = match resvg::tiny_skia::Pixmap::new(width, height) {
            Some(pixmap) => pixmap,
            None => return Err("无法创建像素缓冲区".into()),
        };

        // 渲染 SVG 到像素缓冲区
        resvg::render(
            &rtree,
            resvg::tiny_skia::Transform::from_scale(scale_factor, scale_factor),
            &mut pixmap.as_mut(),
        );

        // 编码为 PNG
        pixmap
            .encode_png()
            .map_err(|e| format!("PNG 编码错误：{:?}", e).into())
    }

    /// 处理图像数据并添加到文档
    fn process_image_data(&self, docx: Docx, data: &[u8], alt_text: Option<&str>) -> Docx {
        // 添加图像格式验证
        match image::guess_format(data) {
            Ok(format) => {
                let (width, height) = calculate_image_dimensions(data, None);

                // 处理图像数据
                let pic = match format {
                    image::ImageFormat::Png => Pic::new(data).size(width, height),
                    image::ImageFormat::Jpeg => Pic::new(data).size(width, height),
                    _ => {
                        // 对于其他格式，尝试转换为 PNG
                        match image::load_from_memory(data) {
                            Ok(img) => {
                                let mut buffer = Vec::new();
                                if img
                                    .write_to(
                                        &mut Cursor::new(&mut buffer),
                                        image::ImageFormat::Png,
                                    )
                                    .is_ok()
                                {
                                    Pic::new(&buffer).size(width, height)
                                } else {
                                    // 如果转换失败，返回原始文档（不添加图片）
                                    let err_para = Paragraph::new().add_run(Run::new().add_text(
                                        "[图像处理错误：无法转换为支持的格式]".to_string(),
                                    ));
                                    return docx.add_paragraph(err_para);
                                }
                            }
                            Err(_) => {
                                // 如果无法加载图像，返回原始文档（不添加图片）
                                let err_para = Paragraph::new().add_run(
                                    Run::new().add_text("[图像处理错误：无法加载图像]".to_string()),
                                );
                                return docx.add_paragraph(err_para);
                            }
                        }
                    }
                };

                let img_para = Paragraph::new().add_run(Run::new().add_image(pic));
                let doc_with_img = docx.add_paragraph(img_para);

                if let Some(alt) = alt_text {
                    if !alt.is_empty() {
                        let caption_para = Paragraph::new()
                            .style("Caption")
                            .add_run(Run::new().add_text(alt));
                        doc_with_img.add_paragraph(caption_para)
                    } else {
                        doc_with_img
                    }
                } else {
                    doc_with_img
                }
            }
            Err(_) => {
                // 如果无法确定图像格式，返回原始文档（不添加图片）
                let err_para = Paragraph::new()
                    .add_run(Run::new().add_text("[图像处理错误：未知的图像格式]".to_string()));
                docx.add_paragraph(err_para)
            }
        }
    }

    /// 处理图像节点
    fn process_image(&self, docx: Docx, url: &str, alt_nodes: &[Node]) -> Result<Docx> {
        // 构建 alt 文本
        let alt_text = if !alt_nodes.is_empty() {
            let mut text = String::new();
            for node in alt_nodes {
                if let Node::Text(content) = node {
                    text.push_str(content);
                }
            }
            Some(text)
        } else {
            None
        };

        // 尝试读取图像文件
        if let Ok(img_data) = std::fs::read(url) {
            Ok(self.process_image_data(docx, &img_data, alt_text.as_deref()))
        } else {
            let placeholder = format!("[图像未找到：{}]", url);
            let para = Paragraph::new().add_run(Run::new().add_text(placeholder));
            Ok(docx.add_paragraph(para))
        }
    }

    /// 处理内联元素并添加到 Run
    fn process_inline_to_run(&self, mut run: Run, node: &Node) -> Result<Run> {
        match node {
            Node::Text(text) => {
                run = run.add_text(text);
            }
            Node::Strong(content) => {
                run = run.style("Strong");
                for child in content {
                    run = self.process_inline_to_run(run, child)?;
                }
            }
            Node::Emphasis(content) => {
                run = run.style("Emphasis");
                for child in content {
                    run = self.process_inline_to_run(run, child)?;
                }
            }
            Node::Strike(content) => {
                run = run.strike();
                for child in content {
                    run = self.process_inline_to_run(run, child)?;
                }
            }
            Node::Link {
                url: _,
                title: _,
                content,
            } => {
                // 超链接需要在段落级别处理，这里只处理内容
                run = run.style("Hyperlink");
                for child in content {
                    run = self.process_inline_to_run(run, child)?;
                }
            }
            Node::Image {
                url,
                title: _,
                alt: _,
            } => {
                if let Ok(img_data) = std::fs::read(url) {
                    // 添加图像格式验证
                    match image::guess_format(&img_data) {
                        Ok(format) => {
                            let (width, height) =
                                calculate_image_dimensions(&img_data, Some(96.0 / 300.0 / 2.0));

                            let pic = match format {
                                image::ImageFormat::Png | image::ImageFormat::Jpeg => {
                                    Pic::new(&img_data).size(width, height)
                                }
                                _ => {
                                    // 尝试转换为 PNG
                                    match image::load_from_memory(&img_data) {
                                        Ok(img) => {
                                            let mut buffer = Vec::new();
                                            if img
                                                .write_to(
                                                    &mut Cursor::new(&mut buffer),
                                                    image::ImageFormat::Png,
                                                )
                                                .is_ok()
                                            {
                                                Pic::new(&buffer).size(width, height)
                                            } else {
                                                run = run.add_text("[图像转换错误]");
                                                return Ok(run);
                                            }
                                        }
                                        Err(_) => {
                                            run = run.add_text("[图像加载错误]");
                                            return Ok(run);
                                        }
                                    }
                                }
                            };
                            run = run.add_image(pic);
                        }
                        Err(_) => {
                            run = run.add_text("[未知图像格式]");
                        }
                    }
                } else {
                    run = run.add_text(format!("[图像未找到：{}]", url));
                }
            }
            Node::HtmlElement(element) => {
                // 处理特殊的 HTML 元素
                if element.tag == "mark" {
                    run = run.style("Highlight");
                    for child in &element.children {
                        run = self.process_inline_to_run(run, child)?;
                    }
                } else if element.tag == "img" && element.self_closing {
                    let is_typst_block = element
                        .attributes
                        .iter()
                        .any(|a| a.name == "alt" && a.value == "typst-block");

                    let src = element
                        .attributes
                        .iter()
                        .find(|a| a.name == "src")
                        .map(|a| a.value.as_str())
                        .unwrap_or("");

                    if src.starts_with("data:image/") {
                        // 这可能是一个从 Frame 渲染的图像
                        if let Some(data_start) = src.find("base64,") {
                            let base64_data = &src[data_start + 7..];
                            if let Ok(img_data) =
                                base64::engine::general_purpose::STANDARD.decode(base64_data)
                            {
                                // 如果是 typst-block (SVG 数据)，需要特殊处理
                                if is_typst_block {
                                    // 使用 resvg 将 SVG 转换为 PNG
                                    if let Ok(png_data) = self.convert_svg_to_png(&img_data) {
                                        let (width, height) = calculate_image_dimensions(
                                            &png_data,
                                            Some(96.0 / 300.0 / 2.0),
                                        );
                                        run =
                                            run.add_image(Pic::new(&png_data).size(width, height));
                                    } else {
                                        run = run.add_text("[SVG 转换失败]");
                                    }
                                } else {
                                    // 普通图像处理
                                    let (width, height) = calculate_image_dimensions(
                                        &img_data,
                                        Some(96.0 / 300.0 / 2.0),
                                    );
                                    run = run.add_image(Pic::new(&img_data).size(width, height));
                                }
                            }
                        }
                    }
                } else {
                    // 常规处理元素内容
                    for child in &element.children {
                        run = self.process_inline_to_run(run, child)?;
                    }
                }
            }
            Node::InlineCode(code) => {
                run = run.style("CodeInline").add_text(code);
            }
            Node::HardBreak => {
                run = run.add_break(BreakType::TextWrapping);
            }
            Node::SoftBreak => {
                run = run.add_text(" ");
            }
            // 其他内联元素类型
            _ => {}
        }

        Ok(run)
    }

    /// 处理段落并添加到文档
    fn process_paragraph(
        &self,
        mut docx: Docx,
        content: &[Node],
        style: Option<&str>,
    ) -> Result<Docx> {
        let mut para = Paragraph::new();

        // 应用样式
        if let Some(style_name) = style {
            para = para.style(style_name);
        }

        // 提取所有链接节点
        let mut links = Vec::new();
        for (i, node) in content.iter().enumerate() {
            if let Node::Link {
                url,
                title: _,
                content: _,
            } = node
            {
                links.push((i, url.clone()));
            }
        }

        // 如果没有链接，正常处理段落
        if links.is_empty() {
            // 处理段落内容
            for node in content {
                let run = Run::new();
                let run = self.process_inline_to_run(run, node)?;
                if !run.children.is_empty() {
                    para = para.add_run(run);
                }
            }
        } else {
            // 如果有链接，我们需要分段处理
            let mut last_idx = 0;
            for (idx, url) in links {
                // 处理链接前的内容
                for item in content.iter().take(idx).skip(last_idx) {
                    let run = Run::new();
                    let run = self.process_inline_to_run(run, item)?;
                    if !run.children.is_empty() {
                        para = para.add_run(run);
                    }
                }

                // 处理链接
                if let Node::Link {
                    url: _,
                    title: _,
                    content: link_content,
                } = &content[idx]
                {
                    let mut hyperlink_run = Run::new().style("Hyperlink");
                    for child in link_content {
                        hyperlink_run = self.process_inline_to_run(hyperlink_run, child)?;
                    }

                    // 创建并添加超链接
                    if !hyperlink_run.children.is_empty() {
                        let hyperlink =
                            Hyperlink::new(&url, HyperlinkType::External).add_run(hyperlink_run);
                        para = para.add_hyperlink(hyperlink);
                    }
                }

                last_idx = idx + 1;
            }

            // 处理最后链接后的内容
            for item in content.iter().skip(last_idx) {
                let run = Run::new();
                let run = self.process_inline_to_run(run, item)?;
                if !run.children.is_empty() {
                    para = para.add_run(run);
                }
            }
        }

        // 只有当段落有内容时才添加
        if !para.children.is_empty() {
            docx = docx.add_paragraph(para);
        }

        Ok(docx)
    }

    /// 处理节点并添加到文档
    fn process_node(&mut self, mut docx: Docx, node: &Node) -> Result<Docx> {
        match node {
            Node::Document(blocks) => {
                for block in blocks {
                    docx = self.process_node(docx, block)?;
                }
            }
            Node::Paragraph(content) => {
                docx = self.process_paragraph(docx, content, None)?;
            }
            Node::Heading { level, content } => {
                // 确定标题样式名称
                let style_name = match level {
                    1 => "Heading1",
                    2 => "Heading2",
                    3 => "Heading3",
                    4 => "Heading4",
                    5 => "Heading5",
                    _ => "Heading6",
                };

                docx = self.process_paragraph(docx, content, Some(style_name))?;
            }
            Node::BlockQuote(content) => {
                for block in content {
                    if let Node::Paragraph(inline) = block {
                        docx = self.process_paragraph(docx, inline, Some("Blockquote"))?;
                    } else {
                        docx = self.process_node(docx, block)?;
                    }
                }
            }
            Node::CodeBlock { language, content } => {
                // 添加语言信息
                if let Some(lang) = language {
                    if !lang.is_empty() {
                        let lang_para = Paragraph::new()
                            .style("CodeBlock")
                            .add_run(Run::new().add_text(lang));
                        docx = docx.add_paragraph(lang_para);
                    }
                }

                // 逐行处理代码，确保保留换行
                let lines: Vec<&str> = content.split('\n').collect();
                for line in lines {
                    let code_para = Paragraph::new()
                        .style("CodeBlock")
                        .add_run(Run::new().add_text(line));
                    docx = docx.add_paragraph(code_para);
                }
            }
            Node::OrderedList { start: _, items } => {
                // 进入更深的列表级别
                self.list_level += 1;
                let current_level = self.list_level - 1;

                // 创建新的有序列表编号定义
                let (doc, num_id) = self.numbering.create_ordered_numbering(docx);
                docx = doc;

                // 处理列表项
                for item in items {
                    if let ListItem::Ordered { content, .. } = item {
                        for block in content {
                            match block {
                                Node::Paragraph(inline) => {
                                    let mut para = Paragraph::new().numbering(
                                        NumberingId::new(num_id),
                                        IndentLevel::new(current_level),
                                    );

                                    // 处理段落内容
                                    for node in inline {
                                        let run = Run::new();
                                        let run = self.process_inline_to_run(run, node)?;
                                        if !run.children.is_empty() {
                                            para = para.add_run(run);
                                        }
                                    }

                                    docx = docx.add_paragraph(para);
                                }
                                // 递归处理嵌套列表
                                Node::OrderedList { start: _, items: _ }
                                | Node::UnorderedList(_) => {
                                    docx = self.process_node(docx, block)?;
                                }
                                _ => {
                                    docx = self.process_node(docx, block)?;
                                }
                            }
                        }

                        // 如果列表项内容为空，添加空段落
                        if content.is_empty() {
                            let empty_para = Paragraph::new()
                                .numbering(
                                    NumberingId::new(num_id),
                                    IndentLevel::new(current_level),
                                )
                                .add_run(Run::new().add_text(""));
                            docx = docx.add_paragraph(empty_para);
                        }
                    }
                }

                // 离开列表级别
                self.list_level -= 1;
            }
            Node::UnorderedList(items) => {
                // 进入更深的列表级别
                self.list_level += 1;
                let current_level = self.list_level - 1;

                // 创建新的无序列表编号定义
                let (doc, num_id) = self.numbering.create_unordered_numbering(docx);
                docx = doc;

                // 处理列表项
                for item in items {
                    if let ListItem::Unordered { content } = item {
                        for block in content {
                            match block {
                                Node::Paragraph(inline) => {
                                    let mut para = Paragraph::new().numbering(
                                        NumberingId::new(num_id),
                                        IndentLevel::new(current_level),
                                    );

                                    // 处理段落内容
                                    for node in inline {
                                        let run = Run::new();
                                        let run = self.process_inline_to_run(run, node)?;
                                        if !run.children.is_empty() {
                                            para = para.add_run(run);
                                        }
                                    }

                                    docx = docx.add_paragraph(para);
                                }
                                // 递归处理嵌套列表
                                Node::OrderedList { start: _, items: _ }
                                | Node::UnorderedList(_) => {
                                    docx = self.process_node(docx, block)?;
                                }
                                _ => {
                                    docx = self.process_node(docx, block)?;
                                }
                            }
                        }

                        // 如果列表项内容为空，添加空段落
                        if content.is_empty() {
                            let empty_para = Paragraph::new()
                                .numbering(
                                    NumberingId::new(num_id),
                                    IndentLevel::new(current_level),
                                )
                                .add_run(Run::new().add_text(""));
                            docx = docx.add_paragraph(empty_para);
                        }
                    }
                }

                // 离开列表级别
                self.list_level -= 1;
            }
            Node::Table {
                headers,
                rows,
                alignments: _,
            } => {
                let mut table = Table::new(vec![]).style("Table");

                // 处理表头
                if !headers.is_empty() {
                    let mut cells = Vec::new();

                    for header_node in headers {
                        let mut table_cell = TableCell::new();
                        let mut para = Paragraph::new();

                        let run = Run::new();
                        let run = self.process_inline_to_run(run, header_node)?;
                        if !run.children.is_empty() {
                            para = para.add_run(run);
                        }

                        if !para.children.is_empty() {
                            table_cell = table_cell.add_paragraph(para);
                        }

                        cells.push(table_cell);
                    }

                    if !cells.is_empty() {
                        let header_row = TableRow::new(cells);
                        table = table.add_row(header_row);
                    }
                }

                // 处理表格行
                for row in rows {
                    let mut cells = Vec::new();

                    for cell_node in row {
                        let mut table_cell = TableCell::new();
                        let mut para = Paragraph::new();

                        let run = Run::new();
                        let run = self.process_inline_to_run(run, cell_node)?;
                        if !run.children.is_empty() {
                            para = para.add_run(run);
                        }

                        if !para.children.is_empty() {
                            table_cell = table_cell.add_paragraph(para);
                        }

                        cells.push(table_cell);
                    }

                    if !cells.is_empty() {
                        let data_row = TableRow::new(cells);
                        table = table.add_row(data_row);
                    }
                }

                // 添加表格到文档
                docx = docx.add_table(table);
            }
            Node::Image { url, title: _, alt } => {
                docx = self.process_image(docx, url, alt)?;
            }
            Node::ThematicBreak => {
                // 添加水平线作为特殊格式的段落
                let hr_para = Paragraph::new()
                    .style("HorizontalLine")
                    .add_run(Run::new().add_text(""));
                docx = docx.add_paragraph(hr_para);
            }
            // 内联元素不应该在这里单独处理
            _ => {}
        }

        Ok(docx)
    }

    /// 生成 DOCX 文档
    pub fn generate_docx(&mut self, doc: &Node) -> Result<Vec<u8>> {
        // 创建 DOCX 文档并初始化样式
        let mut docx = Docx::new();
        docx = self.styles.initialize_styles(docx);

        // 处理文档内容
        docx = self.process_node(docx, doc)?;

        // 初始化编号定义
        docx = self.numbering.initialize_numbering(docx);

        // 构建并打包文档
        let docx_built = docx.build();
        let mut buffer = Vec::new();
        docx_built
            .pack(&mut Cursor::new(&mut buffer))
            .map_err(|e| format!("Failed to pack DOCX: {}", e))?;

        Ok(buffer)
    }
}

impl FormatWriter for DocxWriter {
    fn write_vec(&mut self, document: &Node) -> Result<Vec<u8>> {
        self.list_level = 0;
        self.list_numbering_count = 0;
        self.generate_docx(document)
    }

    fn write_eco(&mut self, _document: &Node, _output: &mut EcoString) -> Result<()> {
        Err("DOCX format does not support EcoString output".into())
    }
}
