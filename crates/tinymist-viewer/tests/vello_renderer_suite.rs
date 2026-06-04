#![allow(missing_docs)]

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::io::Cursor;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, OnceLock};

use anyhow::{Context as _, Result, anyhow, bail};
use comemo::Tracked;
use image::{GenericImage, ImageFormat, Rgba, RgbaImage};
use parking_lot::Mutex;
use reflexo::vector::incr::IncrDocClient;
use reflexo::vector::stream::BytesModuleStream;
use reflexo_vec2svg::IncrSvgDocServer;
use tinymist_preview::protocol::DIFF_V1_PREFIX;
use tinymist_std::typst::TypstDocument;
use tinymist_viewer::incr::IncrVelloDocClient;
use typst::diag::{At, FileError, FileResult, SourceResult, StrResult, Warned, bail as typst_bail};
use typst::engine::Engine;
use typst::foundations::{
    Array, Bytes, Context, Datetime, IntoValue, NoneValue, Repr, Smart, Value, func,
};
use typst::layout::{Abs, Margin, PageElem, PagedDocument};
use typst::model::{Numbering, NumberingPattern};
use typst::syntax::{FileId, Source, Span, VirtualPath};
use typst::text::{Font, FontBook, TextElem, TextSize};
use typst::utils::LazyHash;
use typst::visualize::Color as TypstColor;
use typst::{Feature, Library, LibraryExt, World};
use vello::kurbo::{Affine, Rect, Size};
use vello::peniko::{Color, Fill};
use vello::util::RenderContext;
use vello::wgpu::{
    BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, MapMode,
    TexelCopyBufferInfo, TexelCopyBufferLayout, TextureDescriptor, TextureDimension, TextureFormat,
    TextureUsages, TextureViewDescriptor,
};

const CASES_PATH: &str = "tests/vello-renderer/cases.txt";
const OUTPUT_ROOT: &str = "target/tinymist-viewer/vello-renderer";

#[test]
fn typst_suite_vello_renderer_hashes() -> Result<()> {
    let Some(tests_root) = typst_tests_root() else {
        eprintln!(
            "skipping vello renderer suite: set TINYMIST_TYPST_TESTS to a Typst tests/ checkout"
        );
        return Ok(());
    };

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .context("viewer crate should live under <workspace>/crates/tinymist-viewer")?;
    let output_root = workspace_root.join(OUTPUT_ROOT);
    let hash_cases = selected_cases(&manifest_dir)?;
    let hash_case_set = hash_cases.iter().cloned().collect::<BTreeSet<_>>();
    let collected = collect_tests(&tests_root)?;
    let make_pdf = env_flag("TINYMIST_VELLO_PDF");
    let render_cases = if make_pdf {
        all_ref_cases(&tests_root)?
    } else {
        hash_cases.clone()
    };

    fs::create_dir_all(&output_root)?;

    let mut rasterizer = SceneRasterizer::new();
    let mut failures = String::new();
    let mut hash_results = BTreeMap::new();
    let mut pdf_failures = String::new();
    let mut rendered_cases = vec![];

    for name in render_cases {
        let is_hash_case = hash_case_set.contains(&name);
        let ref_png = tests_root.join("ref").join(format!("{name}.png"));
        if !ref_png.exists() {
            let failure = format!("{name}: missing upstream PNG ref at {}", ref_png.display());
            if is_hash_case {
                writeln!(failures, "{failure}")?;
            } else if make_pdf {
                let vello_png = output_png_path(&output_root, "vello", &name);
                write_placeholder_png(&vello_png, None)?;
                writeln!(pdf_failures, "{failure}")?;
                rendered_cases.push(name);
            }
            continue;
        }

        let ref_output = output_png_path(&output_root, "ref", &name);
        if let Some(parent) = ref_output.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&ref_png, &ref_output)
            .with_context(|| format!("failed to copy upstream ref {}", ref_png.display()))?;

        let render_result = render_suite_case_by_name(
            &name,
            is_hash_case,
            &collected,
            &tests_root,
            &mut rasterizer,
        );
        let png = match render_result {
            Ok(png) => png,
            Err(err) => {
                if is_hash_case {
                    writeln!(failures, "{name}: {err:#}")?;
                } else if make_pdf {
                    let vello_png = output_png_path(&output_root, "vello", &name);
                    write_placeholder_png(&vello_png, Some(&ref_png))?;
                    writeln!(pdf_failures, "{name}: {err:#}")?;
                    rendered_cases.push(name);
                }
                continue;
            }
        };

        let vello_png = output_png_path(&output_root, "vello", &name);
        write_png(&vello_png, &png)?;

        if is_hash_case {
            let hash = format!("ihash16:{}", block_hash(&png, 16));
            hash_results.insert(name.clone(), hash);
        }

        rendered_cases.push(name);
    }

    if make_pdf && !rendered_cases.is_empty() {
        if !pdf_failures.is_empty() {
            fs::write(
                output_root.join("vello-renderer-failures.txt"),
                &pdf_failures,
            )?;
        }
        write_comparison_pdf(&output_root, &rendered_cases)?;
    }

    let mut hash_refs = String::new();
    for name in &hash_cases {
        let Some(hash) = hash_results.get(name) else {
            writeln!(failures, "{name}: did not render a hash reference")?;
            continue;
        };
        writeln!(hash_refs, "{name} {hash}")?;
    }

    if !failures.is_empty() {
        bail!("vello renderer suite failed:\n{failures}");
    }

    insta::assert_snapshot!("vello_renderer_hashes", hash_refs);

    Ok(())
}

fn typst_tests_root() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("TINYMIST_TYPST_TESTS").map(PathBuf::from)
        && path.join("suite").is_dir()
        && path.join("ref").is_dir()
    {
        return Some(path);
    }

    let home = std::env::var_os("HOME")?;
    let path = PathBuf::from(home).join("work/rust/typst/tests");
    (path.join("suite").is_dir() && path.join("ref").is_dir()).then_some(path)
}

fn selected_cases(manifest_dir: &Path) -> Result<Vec<String>> {
    if let Ok(raw) = std::env::var("TINYMIST_VELLO_CASES")
        && !raw.trim().is_empty()
    {
        return Ok(raw
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .collect());
    }

    let path = manifest_dir.join(CASES_PATH);
    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed to read case list {}", path.display()))?;
    let cases = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    if cases.is_empty() {
        bail!("case list {} is empty", path.display());
    }

    Ok(cases)
}

fn all_ref_cases(tests_root: &Path) -> Result<Vec<String>> {
    let ref_root = tests_root.join("ref");
    let mut cases = vec![];
    for entry in fs::read_dir(&ref_root)
        .with_context(|| format!("failed to read Typst ref root {}", ref_root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "png") {
            continue;
        }

        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        cases.push(stem.to_owned());
    }

    cases.sort();
    if cases.is_empty() {
        bail!("Typst ref root {} contains no PNG refs", ref_root.display());
    }

    Ok(cases)
}

fn collect_tests(tests_root: &Path) -> Result<BTreeMap<String, SuiteTest>> {
    let typst_root = tests_root
        .parent()
        .context("Typst tests path should be <typst>/tests")?;
    let suite_root = tests_root.join("suite");
    let mut tests = BTreeMap::new();

    for entry in walkdir::WalkDir::new(&suite_root).sort_by_file_name() {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "typ") {
            continue;
        }

        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read suite file {}", path.display()))?;
        if text.starts_with("// SKIP") {
            continue;
        }

        parse_suite_file(path, typst_root, &text, &mut tests)?;
    }

    Ok(tests)
}

fn render_suite_case_by_name(
    name: &str,
    require_render_attr: bool,
    collected: &BTreeMap<String, SuiteTest>,
    tests_root: &Path,
    rasterizer: &mut SceneRasterizer,
) -> Result<RgbaImage> {
    let test = collected
        .get(name)
        .ok_or_else(|| anyhow!("{name}: no such Typst suite section"))?;

    if require_render_attr && !test.attrs.render {
        bail!("{name}: suite section is not a render target");
    }

    render_suite_case(test, tests_root, rasterizer)
}

fn parse_suite_file(
    path: &Path,
    typst_root: &Path,
    text: &str,
    tests: &mut BTreeMap<String, SuiteTest>,
) -> Result<()> {
    let lines = indexed_lines(text);
    let mut headers = vec![];
    for (line_idx, line) in lines.iter().enumerate() {
        if let Some(header) = parse_header(line.text) {
            headers.push((line_idx, header));
        }
    }

    let rel_path = path.strip_prefix(typst_root).with_context(|| {
        format!(
            "suite path {} is not under {}",
            path.display(),
            typst_root.display()
        )
    })?;

    for (idx, (line_idx, header)) in headers.iter().enumerate() {
        let source_start = lines[*line_idx].next_start;
        let source_end = headers
            .get(idx + 1)
            .map(|(next_line, _)| lines[*next_line].start)
            .unwrap_or(text.len());
        let source_text = text[source_start..source_end].to_owned();
        let file_id = FileId::new(None, VirtualPath::new(rel_path));
        let source = Source::new(file_id, source_text);

        let old = tests.insert(
            header.name.clone(),
            SuiteTest {
                name: header.name.clone(),
                attrs: header.attrs,
                source,
            },
        );

        if old.is_some() {
            bail!("duplicate Typst suite section {}", header.name);
        }
    }

    Ok(())
}

fn parse_header(line: &str) -> Option<Header> {
    let trimmed = line.trim_end_matches('\r');
    let inner = trimmed.strip_prefix("---")?.strip_suffix("---")?.trim();
    if inner.is_empty() {
        return None;
    }

    let mut parts = inner.split_whitespace();
    let name = parts.next()?.to_owned();
    let attrs = Attrs::from_flags(parts);
    Some(Header { name, attrs })
}

fn indexed_lines(text: &str) -> Vec<IndexedLine<'_>> {
    let mut lines = vec![];
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        let next_start = start + line.len();
        let text = line.trim_end_matches('\n');
        lines.push(IndexedLine {
            start,
            next_start,
            text,
        });
        start = next_start;
    }
    if start < text.len() {
        lines.push(IndexedLine {
            start,
            next_start: text.len(),
            text: &text[start..],
        });
    }
    lines
}

fn render_suite_case(
    test: &SuiteTest,
    tests_root: &Path,
    rasterizer: &mut SceneRasterizer,
) -> Result<RgbaImage> {
    let typst_root = tests_root
        .parent()
        .context("Typst tests path should be <typst>/tests")?;
    let world = RenderWorld::new(
        test.source.clone(),
        typst_root.to_path_buf(),
        Some(tests_root.join("packages")),
    );
    let compiled = compile_paged(&world)
        .with_context(|| format!("failed to compile Typst suite case {}", test.name))?;
    let document = TypstDocument::Paged(Arc::new(compiled));

    let mut renderer = IncrSvgDocServer::default();
    let frame = renderer.pack_delta(&document);
    if !frame.starts_with(DIFF_V1_PREFIX) {
        bail!("preview frame for {} did not use diff-v1", test.name);
    }

    let delta = BytesModuleStream::from_slice(&frame[DIFF_V1_PREFIX.len()..]).checkout_owned();
    let mut doc = IncrDocClient::default();
    doc.merge_delta(delta);

    let mut client = IncrVelloDocClient::default();
    let background = client.background_color().unwrap_or(Color::WHITE);
    let pages = client
        .render_pages(&mut doc)
        .with_context(|| format!("failed to lower {} to Vello scenes", test.name))?;
    if pages.is_empty() {
        bail!("{} rendered no pages", test.name);
    }

    let mut rendered = vec![];
    for (scene, size) in pages {
        let width = size.width.ceil().max(1.0) as u32;
        let height = size.height.ceil().max(1.0) as u32;
        rendered.push(rasterizer.render_page(&scene, size, width, height, background)?);
    }

    Ok(merge_pages(&rendered))
}

fn compile_paged(world: &dyn World) -> Result<PagedDocument> {
    let Warned {
        output,
        warnings: _,
    } = typst::compile::<PagedDocument>(world);
    output.map_err(|errors| anyhow!("{}", format_diagnostics(&errors)))
}

fn format_diagnostics(errors: &[typst::diag::SourceDiagnostic]) -> String {
    let mut out = String::new();
    for error in errors {
        let _ = writeln!(out, "{}", error.message);
        for hint in &error.hints {
            let _ = writeln!(out, "  hint: {hint}");
        }
    }
    out
}

struct SceneRasterizer {
    context: RenderContext,
    renderer: Option<vello::Renderer>,
    device_id: Option<usize>,
}

impl SceneRasterizer {
    fn new() -> Self {
        Self {
            context: RenderContext::new(),
            renderer: None,
            device_id: None,
        }
    }

    fn render_page(
        &mut self,
        page_scene: &vello::Scene,
        page_size: Size,
        width: u32,
        height: u32,
        background: Color,
    ) -> Result<RgbaImage> {
        let device_id = match self.device_id {
            Some(device_id) => device_id,
            None => {
                let device_id = pollster::block_on(self.context.device(None))
                    .context("no compatible WGPU device found for Vello renderer suite")?;
                self.device_id = Some(device_id);
                device_id
            }
        };

        let device_handle = &mut self.context.devices[device_id];
        let device = &device_handle.device;
        let queue = &device_handle.queue;

        let mut renderer = match self.renderer.take() {
            Some(renderer) => renderer,
            None => vello::Renderer::new(
                device,
                vello::RendererOptions {
                    use_cpu: true,
                    num_init_threads: NonZeroUsize::new(1),
                    antialiasing_support: vello::AaSupport::area_only(),
                    ..Default::default()
                },
            )
            .map_err(|err| anyhow!("failed to create Vello renderer: {err}"))?,
        };

        let mut scene = vello::Scene::new();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            background,
            None,
            &Rect::new(0.0, 0.0, page_size.width, page_size.height),
        );
        scene.append(page_scene, None);

        let size = Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let target = device.create_texture(&TextureDescriptor {
            label: Some("tinymist-vello-suite-target"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::STORAGE_BINDING | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&TextureViewDescriptor::default());
        let render_params = vello::RenderParams {
            base_color: background,
            width,
            height,
            antialiasing_method: vello::AaConfig::Area,
        };

        renderer
            .render_to_texture(device, queue, &scene, &view, &render_params)
            .map_err(|err| anyhow!("failed to render Vello scene: {err}"))?;

        let padded_byte_width = (width * 4).next_multiple_of(256);
        let buffer_size = u64::from(padded_byte_width) * u64::from(height);
        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some("tinymist-vello-suite-readback"),
            size: buffer_size,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("tinymist-vello-suite-copy"),
        });
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            TexelCopyBufferInfo {
                buffer: &buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_byte_width),
                    rows_per_image: None,
                },
            },
            size,
        );
        queue.submit([encoder.finish()]);

        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        device.poll(vello::wgpu::PollType::wait_indefinitely())?;
        receiver
            .recv()
            .context("Vello readback channel closed")?
            .map_err(|err| anyhow!("failed to map Vello readback buffer: {err}"))?;

        let data = slice.get_mapped_range();
        let mut result = Vec::with_capacity((width * height * 4) as usize);
        for row in 0..height {
            let start = (row * padded_byte_width) as usize;
            result.extend_from_slice(&data[start..start + (width * 4) as usize]);
        }
        drop(data);
        buffer.unmap();

        self.renderer = Some(renderer);

        RgbaImage::from_vec(width, height, result).context("failed to create Vello PNG image")
    }
}

fn merge_pages(pages: &[RgbaImage]) -> RgbaImage {
    const GAP: u32 = 1;

    let width = pages.iter().map(RgbaImage::width).max().unwrap_or(1);
    let page_height: u32 = pages.iter().map(RgbaImage::height).sum();
    let gap_height = GAP * pages.len().saturating_sub(1) as u32;
    let height = (page_height + gap_height).max(1);
    let mut merged = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 0]));

    let mut y = 0;
    for (idx, page) in pages.iter().enumerate() {
        merged
            .copy_from(page, 0, y)
            .expect("page should fit merged image");
        y += page.height();
        if idx + 1 != pages.len() {
            for gap_y in y..(y + GAP) {
                for x in 0..width {
                    merged.put_pixel(x, gap_y, Rgba([0, 0, 0, 255]));
                }
            }
            y += GAP;
        }
    }

    merged
}

fn write_png(path: &Path, image: &RgbaImage) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut cursor = Cursor::new(Vec::new());
    image.write_to(&mut cursor, ImageFormat::Png)?;
    fs::write(path, cursor.into_inner())
        .with_context(|| format!("failed to write PNG {}", path.display()))
}

fn output_png_path(root: &Path, kind: &str, name: &str) -> PathBuf {
    root.join(kind).join(format!("{name}.png"))
}

fn write_placeholder_png(path: &Path, ref_png: Option<&Path>) -> Result<()> {
    let (width, height) = ref_png
        .and_then(|path| image::image_dimensions(path).ok())
        .unwrap_or((256, 144));
    let width = width.max(64);
    let height = height.max(64);
    let mut image = RgbaImage::from_pixel(width, height, Rgba([255, 245, 245, 255]));

    for y in 0..height {
        for x in 0..width {
            if ((x + y) / 18).is_multiple_of(2) {
                image.put_pixel(x, y, Rgba([255, 225, 225, 255]));
            }
        }
    }

    for x in 0..width {
        image.put_pixel(x, 0, Rgba([190, 0, 0, 255]));
        image.put_pixel(x, height - 1, Rgba([190, 0, 0, 255]));
    }
    for y in 0..height {
        image.put_pixel(0, y, Rgba([190, 0, 0, 255]));
        image.put_pixel(width - 1, y, Rgba([190, 0, 0, 255]));
    }

    write_png(path, &image)
}

fn block_hash(image: &RgbaImage, bits: usize) -> String {
    let width = image.width() as usize;
    let height = image.height() as usize;
    let even_x = width.is_multiple_of(bits);
    let even_y = height.is_multiple_of(bits);

    let mut blocks = if even_x && even_y {
        block_hash_even(image, bits)
    } else {
        block_hash_precise(image, bits)
    };
    translate_blocks_to_bits(
        &mut blocks,
        width as f64 * height as f64 / (bits * bits) as f64,
    );
    bits_to_hex(&blocks)
}

fn block_hash_even(image: &RgbaImage, bits: usize) -> Vec<f64> {
    let block_size_x = image.width() as usize / bits;
    let block_size_y = image.height() as usize / bits;
    let mut result = vec![];

    for y in 0..bits {
        for x in 0..bits {
            let mut total = 0.0;
            for iy in 0..block_size_y {
                for ix in 0..block_size_x {
                    let cx = x * block_size_x + ix;
                    let cy = y * block_size_y + iy;
                    total += pixel_value(image.get_pixel(cx as u32, cy as u32));
                }
            }
            result.push(total);
        }
    }

    result
}

fn block_hash_precise(image: &RgbaImage, bits: usize) -> Vec<f64> {
    let width = image.width() as usize;
    let height = image.height() as usize;
    let block_width = width as f64 / bits as f64;
    let block_height = height as f64 / bits as f64;
    let even_x = width.is_multiple_of(bits);
    let even_y = height.is_multiple_of(bits);
    let mut blocks = vec![vec![0.0; bits]; bits];

    for y in 0..height {
        let (block_top, block_bottom, weight_top, weight_bottom) =
            block_position(y, height, block_height, even_y);
        for x in 0..width {
            let (block_left, block_right, weight_left, weight_right) =
                block_position(x, width, block_width, even_x);
            let value = pixel_value(image.get_pixel(x as u32, y as u32));

            blocks[block_top][block_left] += value * weight_top * weight_left;
            blocks[block_top][block_right] += value * weight_top * weight_right;
            blocks[block_bottom][block_left] += value * weight_bottom * weight_left;
            blocks[block_bottom][block_right] += value * weight_bottom * weight_right;
        }
    }

    blocks.into_iter().flatten().collect()
}

fn block_position(pos: usize, max: usize, block_size: f64, even: bool) -> (usize, usize, f64, f64) {
    if even {
        let block = (pos as f64 / block_size).floor() as usize;
        return (block, block, 1.0, 0.0);
    }

    let pos_mod = ((pos + 1) as f64) % block_size;
    let pos_frac = pos_mod - pos_mod.floor();
    let pos_int = pos_mod - pos_frac;
    let weight_first = 1.0 - pos_frac;
    let weight_second = pos_frac;
    let (first, second) = if pos_int > 0.0 || pos + 1 == max {
        let block = (pos as f64 / block_size).floor() as usize;
        (block, block)
    } else {
        (
            (pos as f64 / block_size).floor() as usize,
            (pos as f64 / block_size).ceil() as usize,
        )
    };

    (first, second, weight_first, weight_second)
}

fn translate_blocks_to_bits(blocks: &mut [f64], pixels_per_block: f64) {
    let half_block_value = pixels_per_block * 256.0 * 3.0 / 2.0;
    let band_size = blocks.len() / 4;

    for band in 0..4 {
        let start = band * band_size;
        let end = start + band_size;
        let median = median(&blocks[start..end]);
        for value in &mut blocks[start..end] {
            let bit =
                *value > median || ((*value - median).abs() < 1.0 && median > half_block_value);
            *value = f64::from(bit);
        }
    }
}

fn median(data: &[f64]) -> f64 {
    let mut sorted = data.to_vec();
    sorted.sort_by(f64::total_cmp);
    if sorted.len().is_multiple_of(2) {
        (sorted[sorted.len() / 2] + sorted[sorted.len() / 2 + 1]) / 2.0
    } else {
        sorted[sorted.len() / 2]
    }
}

fn bits_to_hex(bits: &[f64]) -> String {
    let mut hex = String::new();
    for nibble in bits.chunks(4) {
        let mut value = 0u8;
        for bit in nibble {
            value = (value << 1) | u8::from(*bit != 0.0);
        }
        write!(hex, "{value:x}").expect("writing into a string cannot fail");
    }
    hex
}

fn pixel_value(pixel: &Rgba<u8>) -> f64 {
    if pixel[3] == 0 {
        765.0
    } else {
        f64::from(pixel[0]) + f64::from(pixel[1]) + f64::from(pixel[2])
    }
}

fn write_comparison_pdf(output_root: &Path, names: &[String]) -> Result<()> {
    let points = serde_json::to_string_pretty(names)?;
    fs::write(output_root.join("test-points.json"), points)?;

    let source = r#"
#set page(height: auto)

#let points = json("test-points.json");

#set heading(numbering: "1.")

#outline()

#for v in points [
  #page[
    = #v
    #table(
      columns: (1fr, 1fr),
      table.header("Typst Ref", "Vello"),
      image("ref/" + v + ".png"), image("vello/" + v + ".png"),
    )
  ]
]
"#;
    fs::write(output_root.join("index.typ"), source)?;

    let file_id = FileId::new(None, VirtualPath::new("index.typ"));
    let world = RenderWorld::new(
        Source::new(file_id, source.to_owned()),
        output_root.to_path_buf(),
        None,
    );
    let doc = compile_paged(&world).context("failed to compile vello renderer comparison PDF")?;
    let pdf = typst_pdf::pdf(&doc, &typst_pdf::PdfOptions::default())
        .map_err(|errors| anyhow!("{}", format_diagnostics(&errors)))?;
    fs::write(output_root.join("vello-renderer-comparison.pdf"), pdf)?;
    Ok(())
}

fn env_flag(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| !value.is_empty() && value != "0")
}

#[derive(Clone, Copy)]
struct Attrs {
    render: bool,
}

impl Attrs {
    fn from_flags<'a>(flags: impl Iterator<Item = &'a str>) -> Self {
        let mut has_render = false;
        let mut has_non_render = false;
        for flag in flags {
            match flag {
                "render" => has_render = true,
                "html" | "pdftags" => has_non_render = true,
                "large" | "nopdfua" => {}
                _ => {}
            }
        }

        Self {
            render: has_render || !has_non_render,
        }
    }
}

struct Header {
    name: String,
    attrs: Attrs,
}

struct IndexedLine<'a> {
    start: usize,
    next_start: usize,
    text: &'a str,
}

struct SuiteTest {
    name: String,
    attrs: Attrs,
    source: Source,
}

struct RenderWorld {
    main: Source,
    root: PathBuf,
    package_root: Option<PathBuf>,
    slots: Mutex<BTreeMap<FileId, FileSlot>>,
    base: &'static RenderBase,
}

impl RenderWorld {
    fn new(main: Source, root: PathBuf, package_root: Option<PathBuf>) -> Self {
        Self {
            main,
            root,
            package_root,
            slots: Mutex::new(BTreeMap::new()),
            base: render_base(),
        }
    }

    fn slot<F, T>(&self, id: FileId, f: F) -> T
    where
        F: FnOnce(&mut FileSlot) -> T,
    {
        let mut slots = self.slots.lock();
        f(slots.entry(id).or_insert_with(|| FileSlot::new(id)))
    }

    fn system_path(&self, id: FileId) -> FileResult<SystemPath> {
        let base = match id.package() {
            Some(spec) => {
                let package_root = self.package_root.as_ref().ok_or(FileError::AccessDenied)?;
                PathBuf::from("tests/packages")
                    .join(format!("{}-{}", spec.name, spec.version))
                    .strip_prefix("tests/packages")
                    .map(|path| package_root.join(path))
                    .map_err(|_| FileError::AccessDenied)?
            }
            None => PathBuf::new(),
        };

        let path = id.vpath().resolve(&base).ok_or(FileError::AccessDenied)?;
        if let Ok(asset) = path.strip_prefix("assets") {
            return Ok(SystemPath::Asset(asset.to_path_buf()));
        }

        if path.is_absolute() {
            Ok(SystemPath::File(path))
        } else {
            Ok(SystemPath::File(self.root.join(path)))
        }
    }

    fn read_system_path(&self, path: &SystemPath) -> FileResult<Cow<'static, [u8]>> {
        match path {
            SystemPath::Asset(asset) => {
                let asset = asset.to_string_lossy();
                typst_dev_assets::get(&asset)
                    .map(Cow::Borrowed)
                    .ok_or_else(|| FileError::NotFound(PathBuf::from("assets").join(&*asset)))
            }
            SystemPath::File(path) => {
                let f = |e| FileError::from_io(e, path);
                if fs::metadata(path).map_err(f)?.is_dir() {
                    Err(FileError::IsDirectory)
                } else {
                    fs::read(path).map(Cow::Owned).map_err(f)
                }
            }
        }
    }
}

impl World for RenderWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.base.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.base.book
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            Ok(self.main.clone())
        } else {
            self.slot(id, |slot| slot.source(self))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.slot(id, |slot| slot.file(self))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.base.fonts.get(index).cloned()
    }

    fn today(&self, _: Option<i64>) -> Option<Datetime> {
        Some(Datetime::from_ymd(1970, 1, 1).unwrap())
    }
}

enum SystemPath {
    Asset(PathBuf),
    File(PathBuf),
}

struct FileSlot {
    id: FileId,
    source: OnceLock<FileResult<Source>>,
    file: OnceLock<FileResult<Bytes>>,
}

impl FileSlot {
    fn new(id: FileId) -> Self {
        Self {
            id,
            source: OnceLock::new(),
            file: OnceLock::new(),
        }
    }

    fn source(&mut self, world: &RenderWorld) -> FileResult<Source> {
        self.source
            .get_or_init(|| {
                let path = world.system_path(self.id)?;
                let data = world.read_system_path(&path)?;
                let text = String::from_utf8(data.into_owned())?;
                Ok(Source::new(self.id, text))
            })
            .clone()
    }

    fn file(&mut self, world: &RenderWorld) -> FileResult<Bytes> {
        self.file
            .get_or_init(|| {
                let path = world.system_path(self.id)?;
                world.read_system_path(&path).map(|data| match data {
                    Cow::Owned(data) => Bytes::new(data),
                    Cow::Borrowed(data) => Bytes::new(data),
                })
            })
            .clone()
    }
}

struct RenderBase {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
}

fn render_base() -> &'static RenderBase {
    static BASE: OnceLock<RenderBase> = OnceLock::new();
    BASE.get_or_init(RenderBase::new)
}

impl RenderBase {
    fn new() -> Self {
        let fonts: Vec<_> = typst_assets::fonts()
            .chain(typst_dev_assets::fonts())
            .flat_map(|data| Font::iter(Bytes::new(data)))
            .collect();

        Self {
            library: LazyHash::new(test_library()),
            book: LazyHash::new(FontBook::from_fonts(&fonts)),
            fonts,
        }
    }
}

fn test_library() -> Library {
    let mut lib = Library::builder()
        .with_features([Feature::Html, Feature::A11yExtras].into_iter().collect())
        .build();

    lib.global.scope_mut().define_func::<test>();
    lib.global.scope_mut().define_func::<test_repr>();
    lib.global.scope_mut().define_func::<print>();
    lib.global.scope_mut().define_func::<lines>();
    lib.global
        .scope_mut()
        .define("conifer", TypstColor::from_u8(0x9f, 0xEB, 0x52, 0xFF));
    lib.global
        .scope_mut()
        .define("forest", TypstColor::from_u8(0x43, 0xA1, 0x27, 0xFF));

    lib.styles
        .set(PageElem::width, Smart::Custom(Abs::pt(120.0).into()));
    lib.styles.set(PageElem::height, Smart::Auto);
    lib.styles.set(
        PageElem::margin,
        Margin::splat(Some(Smart::Custom(Abs::pt(10.0).into()))),
    );
    lib.styles
        .set(TextElem::size, TextSize(Abs::pt(10.0).into()));

    lib
}

#[func]
fn test(lhs: Value, rhs: Value) -> StrResult<NoneValue> {
    if lhs != rhs {
        typst_bail!("Assertion failed: {} != {}", lhs.repr(), rhs.repr());
    }
    Ok(NoneValue)
}

#[func]
fn test_repr(lhs: Value, rhs: Value) -> StrResult<NoneValue> {
    if lhs.repr() != rhs.repr() {
        typst_bail!("Assertion failed: {} != {}", lhs.repr(), rhs.repr());
    }
    Ok(NoneValue)
}

#[func]
fn print(#[variadic] values: Vec<Value>) -> NoneValue {
    let mut line = String::from("> ");
    for (idx, value) in values.into_iter().enumerate() {
        if idx > 0 {
            line.push_str(", ");
        }
        let _ = write!(line, "{value:?}");
    }
    eprintln!("{line}");
    NoneValue
}

#[func]
fn lines(
    engine: &mut Engine,
    context: Tracked<Context>,
    span: Span,
    count: u64,
    #[default(Numbering::Pattern(NumberingPattern::from_str("A").unwrap()))] numbering: Numbering,
) -> SourceResult<Value> {
    (1..=count)
        .map(|n| numbering.apply(engine, context, &[n]))
        .collect::<SourceResult<Array>>()?
        .join(Some('\n'.into_value()), None, None)
        .at(span)
}
