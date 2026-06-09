#![allow(missing_docs)]

use std::sync::Arc;
use std::time::Duration;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use masonry::core::{CollectionWidget, NewWidget};
use masonry::kurbo::Axis;
use masonry::layout::Length;
use masonry::peniko::Color;
use masonry::theme::default_property_set;
use masonry::widgets::{Flex, Portal, SizedBox};
use masonry_testing::{TestHarness, TestHarnessParams};
use reflexo::vector::incr::IncrDocClient;
use reflexo::vector::stream::BytesModuleStream;
use reflexo_vec2svg::IncrSvgDocServer;
use tinymist_preview::protocol::DIFF_V1_PREFIX;
use tinymist_std::typst::TypstDocument;
use tinymist_viewer::doc::PageCanvas;
use tinymist_viewer::incr::IncrVelloDocClient;
use vello::Scene;
use vello::kurbo::Size;

const PAGE_COUNTS: &[usize] = &[32, 256, 2048];
const VIEWPORT_SIZE: Size = Size::new(800., 800.);

fn bench_page_incremental_rendering(c: &mut Criterion) {
    let frames = PAGE_COUNTS
        .iter()
        .map(|&page_count| PageFrames::compile(page_count))
        .collect::<Vec<_>>();

    let mut group = c.benchmark_group("viewer_page_incremental_rendering");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(100));
    group.measurement_time(Duration::from_millis(500));

    for frames in &frames {
        let page_count = frames.page_count;
        group.throughput(Throughput::Elements(page_count as u64));
        group.bench_with_input(
            BenchmarkId::new("initial_full_render", page_count),
            &frames.initial,
            |b, initial| {
                b.iter_batched(
                    || doc_from_frame(initial),
                    |mut doc| {
                        let mut vello = IncrVelloDocClient::default();
                        let pages = vello
                            .render_pages(&mut doc)
                            .expect("benchmark initial render should succeed");
                        assert_eq!(pages.len(), page_count);
                        criterion::black_box(pages);
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("incremental_noop_render", page_count),
            &frames.initial,
            |b, initial| {
                b.iter_batched(
                    || {
                        let mut doc = doc_from_frame(initial);
                        let mut vello = IncrVelloDocClient::default();
                        let pages = vello
                            .render_pages(&mut doc)
                            .expect("benchmark warm render should succeed");
                        assert_eq!(pages.len(), page_count);
                        (doc, vello)
                    },
                    |(mut doc, mut vello)| {
                        let pages = vello
                            .render_pages(&mut doc)
                            .expect("benchmark incremental render should succeed");
                        assert_eq!(pages.len(), page_count);
                        criterion::black_box(pages);
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("incremental_single_page_render", page_count),
            frames,
            |b, frames| {
                b.iter_batched(
                    || {
                        let mut doc = doc_from_frame(&frames.initial);
                        let mut vello = IncrVelloDocClient::default();
                        let pages = vello
                            .render_pages(&mut doc)
                            .expect("benchmark warm render should succeed");
                        assert_eq!(pages.len(), page_count);
                        (doc, vello)
                    },
                    |(mut doc, mut vello)| {
                        merge_frame(&mut doc, &frames.changed_middle);
                        let pages = vello
                            .render_pages(&mut doc)
                            .expect("benchmark changed render should succeed");
                        assert_eq!(pages.len(), page_count);
                        criterion::black_box(pages);
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_page_widget_harness(c: &mut Criterion) {
    let frames = PAGE_COUNTS
        .iter()
        .map(|&page_count| PageFrames::compile(page_count))
        .collect::<Vec<_>>();

    let mut group = c.benchmark_group("viewer_page_widget_harness");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(100));
    group.measurement_time(Duration::from_millis(500));

    for frames in &frames {
        let page_count = frames.page_count;
        group.throughput(Throughput::Elements(page_count as u64));

        group.bench_with_input(
            BenchmarkId::new("cached_visible_repaint", page_count),
            frames,
            |b, frames| {
                let mut viewer = ViewerHarness::new(&frames.initial);
                b.iter(|| {
                    let image = viewer.render();
                    criterion::black_box(image);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("incremental_noop_update_render", page_count),
            frames,
            |b, frames| {
                let mut viewer = ViewerHarness::new(&frames.initial);
                b.iter(|| {
                    viewer.apply_frame(&frames.initial);
                    let image = viewer.render();
                    criterion::black_box(image);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("incremental_noop_widget_update", page_count),
            frames,
            |b, frames| {
                let mut viewer = ViewerHarness::new(&frames.initial);
                b.iter(|| {
                    viewer.apply_frame(&frames.initial);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("incremental_visible_page_update_render", page_count),
            frames,
            |b, frames| {
                let mut viewer = ViewerHarness::new(&frames.initial);
                let mut use_changed = true;
                b.iter(|| {
                    let frame = if use_changed {
                        &frames.changed_visible
                    } else {
                        &frames.initial
                    };
                    use_changed = !use_changed;
                    viewer.apply_frame(frame);
                    let image = viewer.render();
                    criterion::black_box(image);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("incremental_visible_page_widget_update", page_count),
            frames,
            |b, frames| {
                let mut viewer = ViewerHarness::new(&frames.initial);
                let mut use_changed = true;
                b.iter(|| {
                    let frame = if use_changed {
                        &frames.changed_visible
                    } else {
                        &frames.initial
                    };
                    use_changed = !use_changed;
                    viewer.apply_frame(frame);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("incremental_middle_page_update_render", page_count),
            frames,
            |b, frames| {
                let mut viewer = ViewerHarness::new(&frames.initial);
                let mut use_changed = true;
                b.iter(|| {
                    let frame = if use_changed {
                        &frames.changed_middle
                    } else {
                        &frames.initial
                    };
                    use_changed = !use_changed;
                    viewer.apply_frame(frame);
                    let image = viewer.render();
                    criterion::black_box(image);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("incremental_middle_page_widget_update", page_count),
            frames,
            |b, frames| {
                let mut viewer = ViewerHarness::new(&frames.initial);
                let mut use_changed = true;
                b.iter(|| {
                    let frame = if use_changed {
                        &frames.changed_middle
                    } else {
                        &frames.initial
                    };
                    use_changed = !use_changed;
                    viewer.apply_frame(frame);
                });
            },
        );
    }

    group.finish();
}

struct ViewerHarness {
    doc: IncrDocClient,
    vello: IncrVelloDocClient,
    harness: TestHarness<Portal<Flex>>,
}

impl ViewerHarness {
    fn new(frame: &[u8]) -> Self {
        let mut doc = IncrDocClient::default();
        let mut vello = IncrVelloDocClient::default();
        merge_frame(&mut doc, frame);
        let pages = vello
            .render_pages(&mut doc)
            .expect("initial viewer harness render should succeed");
        let background_color = vello.background_color();

        let mut params = TestHarnessParams::default();
        params.window_size = VIEWPORT_SIZE;
        params.background_color = Color::from_rgb8(0x29, 0x29, 0x29);

        let mut harness = TestHarness::create_with(
            default_property_set(),
            NewWidget::new(page_portal(&pages, background_color)),
            params,
        );
        let _ = harness.render();

        Self {
            doc,
            vello,
            harness,
        }
    }

    fn apply_frame(&mut self, frame: &[u8]) {
        merge_frame(&mut self.doc, frame);
        let pages = self
            .vello
            .render_pages(&mut self.doc)
            .expect("incremental viewer harness render should succeed");
        let background_color = self.vello.background_color();
        self.harness.edit_root_widget(|mut root| {
            update_page_list(&mut root, &pages, background_color);
        });
    }

    fn render(&mut self) -> image::RgbaImage {
        self.harness.render()
    }
}

fn page_portal(pages: &[(Arc<Scene>, Size)], background_color: Option<Color>) -> Portal<Flex> {
    Portal::new(NewWidget::new(page_list(pages, background_color)))
}

fn page_list(pages: &[(Arc<Scene>, Size)], background_color: Option<Color>) -> Flex {
    let mut list = Flex::column();
    for (page_scene, scene_size) in pages {
        let (elem_width, elem_height, elem_scale) = page_layout(*scene_size);
        let page = PageCanvas::new(page_scene.clone(), elem_scale, background_color);
        let page = SizedBox::new(NewWidget::new(page))
            .width(Length::const_px(elem_width))
            .height(Length::const_px(elem_height));
        list = list.with_fixed(NewWidget::new(page));
    }

    list
}

fn update_page_list(
    root: &mut masonry::core::WidgetMut<'_, Portal<Flex>>,
    pages: &[(Arc<Scene>, Size)],
    background_color: Option<Color>,
) {
    let page_count_matches = {
        let list = Portal::child_mut(root);
        list.widget.len() == pages.len()
    };
    if !page_count_matches {
        Portal::set_child(root, NewWidget::new(page_list(pages, background_color)));
        return;
    }

    let mut list = Portal::child_mut(root);
    for (idx, (page_scene, scene_size)) in pages.iter().enumerate() {
        let (elem_width, elem_height, elem_scale) = page_layout(*scene_size);
        let width = Length::const_px(elem_width);
        let height = Length::const_px(elem_height);

        let mut page_child = Flex::get_mut(&mut list, idx);
        let mut page_box = page_child.downcast::<SizedBox>();
        if page_box.widget.length(Axis::Horizontal) != Some(width) {
            SizedBox::set_width(&mut page_box, width);
        }
        if page_box.widget.length(Axis::Vertical) != Some(height) {
            SizedBox::set_height(&mut page_box, height);
        }

        let Some(mut page) = SizedBox::child_mut(&mut page_box) else {
            SizedBox::set_child(
                &mut page_box,
                NewWidget::new(PageCanvas::new(
                    page_scene.clone(),
                    elem_scale,
                    background_color,
                )),
            );
            continue;
        };
        let mut page = page.downcast::<PageCanvas>();
        PageCanvas::request_render(&mut page, page_scene.clone(), elem_scale, background_color);
    }
}

fn page_layout(scene_size: Size) -> (f64, f64, f64) {
    let elem_width = VIEWPORT_SIZE.width - 0.5;
    let elem_scale = if scene_size.width > 0. {
        elem_width / scene_size.width
    } else {
        1.0
    };
    let elem_height = elem_scale * scene_size.height;
    (elem_width, elem_height, elem_scale)
}

struct PageFrames {
    page_count: usize,
    initial: Vec<u8>,
    changed_visible: Vec<u8>,
    changed_middle: Vec<u8>,
}

impl PageFrames {
    fn compile(page_count: usize) -> Self {
        let initial_document = compile_document(page_count, None);
        let changed_visible_document = compile_document(page_count, Some(0));
        let changed_middle_document = compile_document(page_count, Some(page_count / 2));

        let mut renderer = IncrSvgDocServer::default();
        let initial = renderer.pack_delta(&initial_document);
        let changed_visible =
            changed_frame_from_initial(&initial_document, &changed_visible_document);
        let changed_middle =
            changed_frame_from_initial(&initial_document, &changed_middle_document);

        assert!(
            initial.starts_with(DIFF_V1_PREFIX),
            "initial benchmark frame should be diff-v1"
        );
        assert!(
            changed_visible.starts_with(DIFF_V1_PREFIX),
            "visible-page benchmark frame should be diff-v1"
        );
        assert!(
            changed_middle.starts_with(DIFF_V1_PREFIX),
            "middle-page benchmark frame should be diff-v1"
        );

        Self {
            page_count,
            initial,
            changed_visible,
            changed_middle,
        }
    }
}

fn changed_frame_from_initial(initial: &TypstDocument, changed: &TypstDocument) -> Vec<u8> {
    let mut renderer = IncrSvgDocServer::default();
    let initial_frame = renderer.pack_delta(initial);
    assert!(
        initial_frame.starts_with(DIFF_V1_PREFIX),
        "initial seed frame should be diff-v1"
    );
    renderer.pack_delta(changed)
}

fn compile_document(page_count: usize, changed_page: Option<usize>) -> TypstDocument {
    let source = page_fixture(page_count, changed_page);

    tinymist_tests::run_with_sources(&source, |verse, _| {
        let world = verse.snapshot();
        let doc = typst::compile::<typst::layout::PagedDocument>(&world)
            .output
            .expect("large-page benchmark fixture should compile");
        TypstDocument::Paged(Arc::new(doc))
    })
}

fn doc_from_frame(frame: &[u8]) -> IncrDocClient {
    let delta = BytesModuleStream::from_slice(&frame[DIFF_V1_PREFIX.len()..]).checkout_owned();
    let mut doc = IncrDocClient::default();
    doc.merge_delta(delta);
    doc
}

fn merge_frame(doc: &mut IncrDocClient, frame: &[u8]) {
    let delta = BytesModuleStream::from_slice(&frame[DIFF_V1_PREFIX.len()..]).checkout_owned();
    doc.merge_delta(delta);
}

fn page_fixture(page_count: usize, changed_page: Option<usize>) -> String {
    assert!(page_count > 0);

    let mut source = String::from("#set page(width: 32pt, height: 32pt, margin: 0pt)\n");
    for page in 0..page_count {
        let fill = if Some(page) == changed_page {
            "rgb(220, 60, 30)"
        } else if page % 2 == 0 {
            "black"
        } else {
            "rgb(30, 120, 220)"
        };
        source.push_str(&format!("#rect(width: 16pt, height: 16pt, fill: {fill})\n"));
        if page + 1 != page_count {
            source.push_str("#pagebreak()\n");
        }
    }

    source
}

criterion_group!(
    benches,
    bench_page_incremental_rendering,
    bench_page_widget_harness
);
criterion_main!(benches);
