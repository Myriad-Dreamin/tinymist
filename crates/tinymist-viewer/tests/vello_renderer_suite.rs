#![allow(missing_docs)]

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io::Cursor;
use std::num::NonZeroUsize;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, OnceLock};

use anyhow::{Context as _, Result, anyhow, bail};
use comemo::Tracked;
use image::{GenericImage, ImageFormat, Rgba, RgbaImage};
use parking_lot::Mutex;
use reflexo::vector::incr::IncrDocClient;
use reflexo::vector::stream::BytesModuleStream;
use reflexo_vec2svg::IncrSvgDocServer;
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use tinymist_preview::protocol::DIFF_V1_PREFIX;
use tinymist_std::typst::TypstDocument;
use tinymist_viewer::incr::IncrVelloDocClient;
use tinymist_viewer::{SvgResource, SvgResourceFormat, SvgResourceResolver};
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

const OUTPUT_ROOT: &str = "target/tinymist-viewer/vello-renderer";
const RENDERER_DIFF_ARTIFACT: &str = "renderer-diff-vello";
const RENDERER_DIFF_MANIFEST: &str = "renderer-diff-manifest.json";
const HASH_BITS: usize = 16;
const OFFICIAL_GROUP: &str = "official";
const VELLO_GROUP: &str = "vello";

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
    let artifact_root = output_root.join(RENDERER_DIFF_ARTIFACT);
    let cases = all_ref_cases(&tests_root)?;
    let collected = Box::new(collect_tests(&tests_root)?);
    let make_bundle = env_flag("TINYMIST_RENDERER_DIFF");

    fs::create_dir_all(&output_root)?;
    if make_bundle {
        if artifact_root.exists() {
            fs::remove_dir_all(&artifact_root).with_context(|| {
                format!(
                    "failed to remove previous renderer diff bundle {}",
                    artifact_root.display()
                )
            })?;
        }
        fs::create_dir_all(&artifact_root)?;
    }

    let mut rasterizer = Box::new(SceneRasterizer::new());
    let mut failures = String::new();
    let mut hash_refs = String::new();
    let mut bundle_failures = String::new();
    let mut manifest_cases = vec![];

    for name in cases {
        let ref_png = tests_root.join("ref").join(format!("{name}.png"));
        if !ref_png.exists() {
            let failure = format!("{name}: missing upstream PNG ref at {}", ref_png.display());
            writeln!(failures, "{failure}")?;
            if make_bundle {
                let png = placeholder_png(None);
                write_group_png(&artifact_root, VELLO_GROUP, &name, &png)?;
                writeln!(bundle_failures, "{failure}")?;
            }
            continue;
        }

        let official_asset = if make_bundle {
            Some(copy_group_png(
                &artifact_root,
                OFFICIAL_GROUP,
                &name,
                &ref_png,
            )?)
        } else {
            let ref_output = output_png_path(&output_root, "ref", &name);
            if let Some(parent) = ref_output.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&ref_png, &ref_output)
                .with_context(|| format!("failed to copy upstream ref {}", ref_png.display()))?;
            None
        };

        let render_result =
            render_suite_case_by_name(&name, &collected, &tests_root, &mut rasterizer);
        let (png, render_error) = match render_result {
            Ok(png) => (png, None),
            Err(err) => {
                writeln!(failures, "{name}: {err:#}")?;
                if make_bundle {
                    let png = placeholder_png(Some(&ref_png));
                    let vello_asset = write_group_png(&artifact_root, VELLO_GROUP, &name, &png)?;
                    let message = format!("{err:#}");
                    writeln!(bundle_failures, "{name}: {message}")?;
                    let official_asset = official_asset.expect("official PNG should be loaded");
                    let case = renderer_diff_case(
                        &name,
                        "render-error",
                        official_asset,
                        vello_asset,
                        Some(message),
                    );
                    manifest_cases.push(case);
                }
                continue;
            }
        };

        let vello_asset = if make_bundle {
            Some(write_group_png(&artifact_root, VELLO_GROUP, &name, &png)?)
        } else {
            let vello_png = output_png_path(&output_root, VELLO_GROUP, &name);
            write_png(&vello_png, &png)?;
            None
        };

        let hash = format!("ihash16:{}", block_hash(&png, HASH_BITS));
        writeln!(hash_refs, "{name} {hash}")?;

        if make_bundle {
            let official_asset = official_asset.expect("official PNG should be loaded");
            let vello_asset = vello_asset.expect("Vello PNG should be written");
            let case =
                renderer_diff_case(&name, "pending", official_asset, vello_asset, render_error);
            manifest_cases.push(case);
        }
    }

    if make_bundle && !manifest_cases.is_empty() {
        if !bundle_failures.is_empty() {
            fs::write(artifact_root.join("failures.txt"), &bundle_failures)?;
        }
        write_renderer_diff_manifest(&artifact_root, &tests_root, manifest_cases)?;
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
    collected: &BTreeMap<String, SuiteTest>,
    tests_root: &Path,
    rasterizer: &mut SceneRasterizer,
) -> Result<RgbaImage> {
    let test = collected
        .get(name)
        .ok_or_else(|| anyhow!("{name}: no such Typst suite section"))?;

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
    Some(Header { name })
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
    let world = Box::new(RenderWorld::new(
        test.source.clone(),
        typst_root.to_path_buf(),
        Some(tests_root.join("packages")),
    ));
    let compiled = compile_paged(world.as_ref())
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
    client.set_svg_resource_resolver(Some(suite_svg_resource_resolver()));
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

        let mut scene = Box::new(vello::Scene::new());
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
    fs::write(path, encode_png(image)?)
        .with_context(|| format!("failed to write PNG {}", path.display()))
}

fn output_png_path(root: &Path, kind: &str, name: &str) -> PathBuf {
    root.join(kind).join(format!("{name}.png"))
}

fn placeholder_png(ref_png: Option<&Path>) -> RgbaImage {
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

    image
}

fn encode_png(image: &RgbaImage) -> Result<Vec<u8>> {
    let mut cursor = Cursor::new(Vec::new());
    image.write_to(&mut cursor, ImageFormat::Png)?;
    Ok(cursor.into_inner())
}

fn copy_group_png(
    root: &Path,
    group: &str,
    name: &str,
    source: &Path,
) -> Result<RendererDiffAsset> {
    let bytes =
        fs::read(source).with_context(|| format!("failed to read PNG {}", source.display()))?;
    let image = image::load_from_memory(&bytes)
        .with_context(|| format!("failed to decode PNG {}", source.display()))?
        .to_rgba8();
    let png_path = output_png_path(root, group, name);
    write_group_png_bytes(root, group, name, image, bytes, &png_path)
}

fn write_group_png(
    root: &Path,
    group: &str,
    name: &str,
    image: &RgbaImage,
) -> Result<RendererDiffAsset> {
    let png_path = output_png_path(root, group, name);
    write_group_png_bytes(
        root,
        group,
        name,
        image.clone(),
        encode_png(image)?,
        &png_path,
    )
}

fn write_group_png_bytes(
    root: &Path,
    group: &str,
    name: &str,
    image: RgbaImage,
    bytes: Vec<u8>,
    png_path: &Path,
) -> Result<RendererDiffAsset> {
    if let Some(parent) = png_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(png_path, &bytes)
        .with_context(|| format!("failed to write PNG {}", png_path.display()))?;

    let perceptual_hash = format!("ihash16:{}", block_hash(&image, HASH_BITS));
    let sha256 = format!("sha256:{:x}", Sha256::digest(&bytes));
    let hash_path = group_asset_path(root, group, name, "hash");
    let sha256_path = group_asset_path(root, group, name, "sha256");
    fs::write(&hash_path, format!("{perceptual_hash}\n"))
        .with_context(|| format!("failed to write hash {}", hash_path.display()))?;
    fs::write(&sha256_path, format!("{sha256}\n"))
        .with_context(|| format!("failed to write sha256 {}", sha256_path.display()))?;

    Ok(RendererDiffAsset {
        png: output_group_rel_path(group, name, "png"),
        hash: output_group_rel_path(group, name, "hash"),
        sha256: output_group_rel_path(group, name, "sha256"),
        width: image.width(),
        height: image.height(),
        perceptual_hash,
        sha256_digest: sha256,
        image: Box::new(image),
    })
}

fn group_asset_path(root: &Path, group: &str, name: &str, extension: &str) -> PathBuf {
    root.join(group).join(format!("{name}.{extension}"))
}

fn output_group_rel_path(group: &str, name: &str, extension: &str) -> String {
    format!("{group}/{name}.{extension}")
}

fn renderer_diff_case(
    name: &str,
    status: &str,
    official: RendererDiffAsset,
    vello: RendererDiffAsset,
    error: Option<String>,
) -> RendererDiffCase {
    let comparison = compare_images(
        &official.image,
        &vello.image,
        &official.perceptual_hash,
        &vello.perceptual_hash,
    );

    let status = match status {
        "render-error" => "render-error",
        _ if comparison.pixel_mismatch_count == 0 => "matched",
        _ => "different",
    };

    let mut assets = BTreeMap::new();
    assets.insert(OFFICIAL_GROUP.to_owned(), official);
    assets.insert(VELLO_GROUP.to_owned(), vello);

    RendererDiffCase {
        name: name.to_owned(),
        status: status.to_owned(),
        assets,
        comparisons: vec![RendererDiffComparison {
            lhs: OFFICIAL_GROUP.to_owned(),
            rhs: VELLO_GROUP.to_owned(),
            status: status.to_owned(),
            metrics: comparison,
        }],
        error,
    }
}

fn compare_images(
    official: &RgbaImage,
    actual: &RgbaImage,
    official_hash: &str,
    actual_hash: &str,
) -> RendererDiffMetrics {
    let width = official.width().max(actual.width()).max(1);
    let height = official.height().max(actual.height()).max(1);
    let total_pixels = u64::from(width) * u64::from(height);

    let mut pixel_mismatch_count = 0u64;
    let mut max_channel_delta = 0u8;
    let mut total_abs_delta = 0u64;

    for y in 0..height {
        for x in 0..width {
            let lhs = sample_pixel(official, x, y);
            let rhs = sample_pixel(actual, x, y);
            let mut pixel_delta = 0u8;

            for channel in 0..3 {
                let delta = lhs[channel].abs_diff(rhs[channel]);
                pixel_delta = pixel_delta.max(delta);
                max_channel_delta = max_channel_delta.max(delta);
                total_abs_delta += u64::from(delta);
            }

            if pixel_delta > 0 || lhs[3] != rhs[3] {
                pixel_mismatch_count += 1;
            }
        }
    }

    let pixel_mismatch_ratio = pixel_mismatch_count as f64 / total_pixels as f64;
    let mean_absolute_error = total_abs_delta as f64 / (total_pixels as f64 * 3.0 * 255.0);

    RendererDiffMetrics {
        perceptual_hash_distance: perceptual_hash_distance(official_hash, actual_hash),
        pixel_mismatch_count,
        pixel_mismatch_ratio,
        mean_absolute_error,
        max_channel_delta,
    }
}

fn sample_pixel(image: &RgbaImage, x: u32, y: u32) -> Rgba<u8> {
    if x < image.width() && y < image.height() {
        *image.get_pixel(x, y)
    } else {
        Rgba([0, 0, 0, 0])
    }
}

fn perceptual_hash_distance(lhs: &str, rhs: &str) -> u32 {
    let lhs = lhs.strip_prefix("ihash16:").unwrap_or(lhs);
    let rhs = rhs.strip_prefix("ihash16:").unwrap_or(rhs);
    let mut distance = 0;

    for (lhs, rhs) in lhs.chars().zip(rhs.chars()) {
        let lhs = lhs.to_digit(16).unwrap_or(0);
        let rhs = rhs.to_digit(16).unwrap_or(0);
        distance += (lhs ^ rhs).count_ones();
    }

    let extra = lhs.len().abs_diff(rhs.len()) as u32;
    distance + extra * 4
}

fn write_renderer_diff_manifest(
    output_root: &Path,
    tests_root: &Path,
    cases: Vec<RendererDiffCase>,
) -> Result<()> {
    let summary = RendererDiffSummary::from_cases(&cases);
    let manifest = RendererDiffManifest {
        schema_version: 1,
        artifact_name: RENDERER_DIFF_ARTIFACT.to_owned(),
        groups: vec![
            RendererDiffGroup {
                id: OFFICIAL_GROUP.to_owned(),
                label: "Typst official renderer".to_owned(),
                kind: "baseline".to_owned(),
                source: Some("typst/typst tests/ref".to_owned()),
            },
            RendererDiffGroup {
                id: VELLO_GROUP.to_owned(),
                label: "Vello".to_owned(),
                kind: "renderer".to_owned(),
                source: None,
            },
        ],
        source: RendererDiffSource {
            suite: "typst renderer suite".to_owned(),
            typst_tests: tests_root.display().to_string(),
            typst_ref: std::env::var("TINYMIST_TYPST_REF").ok(),
            github_run_id: std::env::var("GITHUB_RUN_ID").ok(),
            github_sha: std::env::var("GITHUB_SHA").ok(),
        },
        hash: RendererDiffHashInfo {
            algorithm: "blockhash".to_owned(),
            bits: HASH_BITS,
            format: "ihash16:<hex>".to_owned(),
            distance: "hamming".to_owned(),
        },
        summary,
        cases,
    };
    let manifest = serde_json::to_string_pretty(&manifest)?;
    fs::write(output_root.join(RENDERER_DIFF_MANIFEST), manifest)
        .with_context(|| format!("failed to write {RENDERER_DIFF_MANIFEST}"))
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

fn env_flag(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| !value.is_empty() && value != "0")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RendererDiffManifest {
    schema_version: u32,
    artifact_name: String,
    groups: Vec<RendererDiffGroup>,
    source: RendererDiffSource,
    hash: RendererDiffHashInfo,
    summary: RendererDiffSummary,
    cases: Vec<RendererDiffCase>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RendererDiffGroup {
    id: String,
    label: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RendererDiffSource {
    suite: String,
    typst_tests: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    typst_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    github_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    github_sha: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RendererDiffHashInfo {
    algorithm: String,
    bits: usize,
    format: String,
    distance: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RendererDiffSummary {
    total: usize,
    matched: usize,
    different: usize,
    render_errors: usize,
}

impl RendererDiffSummary {
    fn from_cases(cases: &[RendererDiffCase]) -> Self {
        let mut summary = Self {
            total: cases.len(),
            matched: 0,
            different: 0,
            render_errors: 0,
        };

        for case in cases {
            match case.status.as_str() {
                "matched" => summary.matched += 1,
                "render-error" => summary.render_errors += 1,
                _ => summary.different += 1,
            }
        }

        summary
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RendererDiffCase {
    name: String,
    status: String,
    assets: BTreeMap<String, RendererDiffAsset>,
    comparisons: Vec<RendererDiffComparison>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RendererDiffAsset {
    png: String,
    hash: String,
    sha256: String,
    width: u32,
    height: u32,
    perceptual_hash: String,
    sha256_digest: String,
    #[serde(skip_serializing)]
    image: Box<RgbaImage>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RendererDiffComparison {
    lhs: String,
    rhs: String,
    status: String,
    metrics: RendererDiffMetrics,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RendererDiffMetrics {
    perceptual_hash_distance: u32,
    pixel_mismatch_count: u64,
    pixel_mismatch_ratio: f64,
    mean_absolute_error: f64,
    max_channel_delta: u8,
}

struct Header {
    name: String,
}

struct IndexedLine<'a> {
    start: usize,
    next_start: usize,
    text: &'a str,
}

struct SuiteTest {
    name: String,
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

fn suite_svg_resource_resolver() -> Arc<dyn SvgResourceResolver> {
    static RESOLVER: OnceLock<Arc<SuiteSvgResourceResolver>> = OnceLock::new();
    let resolver: Arc<dyn SvgResourceResolver> = RESOLVER
        .get_or_init(|| {
            let mut resolver = SuiteSvgResourceResolver::default();
            resolver.add_dev_asset_svg_base("images/linked.svg");
            Arc::new(resolver)
        })
        .clone();
    resolver
}

#[derive(Default)]
struct SuiteSvgResourceResolver {
    svg_bases: BTreeMap<Vec<u8>, PathBuf>,
}

impl SuiteSvgResourceResolver {
    fn add_dev_asset_svg_base(&mut self, asset: &str) {
        let Some(data) = typst_dev_assets::get(asset) else {
            return;
        };
        let Some(base) = Path::new(asset).parent() else {
            return;
        };

        self.svg_bases
            .insert(Sha256::digest(data).to_vec(), base.to_path_buf());
    }

    fn resolve_dev_asset(&self, path: &Path) -> Option<SvgResource> {
        let asset = normalized_relative_path(path)?;
        let format = svg_resource_format(Path::new(&asset))?;
        let data = typst_dev_assets::get(&asset)?;
        Some(SvgResource::new(format, data.to_vec()))
    }
}

impl SvgResourceResolver for SuiteSvgResourceResolver {
    fn resolve_svg_resource(&self, svg_data: &[u8], href: &str) -> Option<SvgResource> {
        if href.starts_with('/') || has_url_scheme(href) {
            return None;
        }

        if let Some(asset) = href_assets_suffix(href)
            && let Some(resource) = self.resolve_dev_asset(&asset)
        {
            return Some(resource);
        }

        let hash = Sha256::digest(svg_data).to_vec();
        let base = self.svg_bases.get(&hash)?;
        self.resolve_dev_asset(&base.join(href))
    }
}

fn href_assets_suffix(href: &str) -> Option<PathBuf> {
    let mut in_assets = false;
    let mut asset = PathBuf::new();

    for component in Path::new(href).components() {
        match component {
            Component::Normal(part) if !in_assets && part.to_str() == Some("assets") => {
                in_assets = true;
            }
            Component::Normal(part) if in_assets => asset.push(part),
            Component::CurDir if in_assets => {}
            Component::ParentDir if in_assets => {
                asset.pop();
            }
            Component::Prefix(_) | Component::RootDir => return None,
            _ => {}
        }
    }

    (in_assets && !asset.as_os_str().is_empty()).then_some(asset)
}

fn normalized_relative_path(path: &Path) -> Option<String> {
    let mut parts = vec![];
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str()?.to_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::Prefix(_) | Component::RootDir => return None,
        }
    }

    (!parts.is_empty()).then(|| parts.join("/"))
}

fn svg_resource_format(path: &Path) -> Option<SvgResourceFormat> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => Some(SvgResourceFormat::Jpeg),
        "png" => Some(SvgResourceFormat::Png),
        "gif" => Some(SvgResourceFormat::Gif),
        "webp" => Some(SvgResourceFormat::Webp),
        _ => None,
    }
}

fn has_url_scheme(href: &str) -> bool {
    let Some((scheme, _)) = href.split_once("://") else {
        return false;
    };

    !scheme.is_empty()
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
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
