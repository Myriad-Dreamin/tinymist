#![allow(missing_docs)]

use std::time::Duration;

use accesskit_consumer::{Node as ConsumerNode, Tree as ConsumerTree, TreeChangeHandler};
use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use masonry::accesskit::{
    Action, Node, NodeId, Rect, Role, TextDirection, Tree as AccessTree, TreeUpdate,
};

const PAGE_COUNTS: &[usize] = &[32, 256, 2048];
const MAX_FULL_TREE_PAGES: usize = 256;
const VISIBLE_PAGE_WINDOW: usize = 9;
const PARAGRAPHS_PER_PAGE: usize = 8;
const RUNS_PER_PARAGRAPH: usize = 4;
const RUN_TEXT_CHARS: usize = 64;

const DOC_ID: NodeId = NodeId(1);
const PAGE_ID_BASE: u64 = 1_000_000;
const PARAGRAPH_ID_BASE: u64 = 2_000_000;
const RUN_ID_BASE: u64 = 3_000_000;

fn bench_synthetic_accesskit_tree(c: &mut Criterion) {
    print_shapes();

    let mut group = c.benchmark_group("viewer_a11y_synthetic_tree");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(100));
    group.measurement_time(Duration::from_millis(500));

    for &page_count in PAGE_COUNTS {
        let shape = Shape::new(page_count);

        group.throughput(Throughput::Elements(
            shape.adaptive_initial_nodes(VISIBLE_PAGE_WINDOW) as u64,
        ));

        group.bench_with_input(
            BenchmarkId::new("construct_initial_adaptive_tree", page_count),
            &shape,
            |b, shape| {
                b.iter(|| {
                    let update = initial_adaptive_update(
                        *shape,
                        shape.scroll_start_page(),
                        VISIBLE_PAGE_WINDOW,
                    );
                    criterion::black_box(update);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("apply_initial_adaptive_tree", page_count),
            &shape,
            |b, shape| {
                b.iter_batched(
                    || {
                        initial_adaptive_update(
                            *shape,
                            shape.scroll_start_page(),
                            VISIBLE_PAGE_WINDOW,
                        )
                    },
                    |update| {
                        let tree = ConsumerTree::new(update, true);
                        criterion::black_box(tree);
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        if !shape.uses_full_tree() {
            group.throughput(Throughput::Elements(shape.scroll_update_nodes() as u64));

            group.bench_with_input(
                BenchmarkId::new("construct_incremental_scroll_adaptive_tree", page_count),
                &shape,
                |b, shape| {
                    let base_start = shape.scroll_start_page();
                    let mut start = base_start;
                    b.iter(|| {
                        let old_start = start;
                        start = if start == base_start {
                            base_start + 1
                        } else {
                            base_start
                        };
                        let update = page_stubs_visible_scroll_update(
                            *shape,
                            old_start,
                            start,
                            VISIBLE_PAGE_WINDOW,
                        );
                        criterion::black_box(update);
                    });
                },
            );

            group.bench_with_input(
                BenchmarkId::new("apply_incremental_scroll_adaptive_tree", page_count),
                &shape,
                |b, shape| {
                    let base_start = shape.scroll_start_page();
                    let mut tree = ConsumerTree::new(
                        initial_adaptive_update(*shape, base_start, VISIBLE_PAGE_WINDOW),
                        true,
                    );
                    let mut changes = NoopChanges;
                    let mut start = base_start;
                    b.iter(|| {
                        let old_start = start;
                        start = if start == base_start {
                            base_start + 1
                        } else {
                            base_start
                        };
                        tree.update_and_process_changes(
                            page_stubs_visible_scroll_update(
                                *shape,
                                old_start,
                                start,
                                VISIBLE_PAGE_WINDOW,
                            ),
                            &mut changes,
                        );
                    });
                },
            );
        }
    }

    group.finish();
}

#[derive(Clone, Copy)]
struct Shape {
    page_count: usize,
    paragraphs: usize,
    runs: usize,
}

impl Shape {
    fn new(page_count: usize) -> Self {
        let paragraphs = page_count * PARAGRAPHS_PER_PAGE;
        let runs = paragraphs * RUNS_PER_PARAGRAPH;
        Self {
            page_count,
            paragraphs,
            runs,
        }
    }

    fn uses_full_tree(self) -> bool {
        self.page_count <= MAX_FULL_TREE_PAGES
    }

    fn scroll_start_page(self) -> usize {
        100.min(self.page_count.saturating_sub(VISIBLE_PAGE_WINDOW + 1))
    }

    fn expanded_page_nodes(self) -> usize {
        1 + self.expanded_page_child_nodes()
    }

    fn expanded_page_child_nodes(self) -> usize {
        PARAGRAPHS_PER_PAGE + PARAGRAPHS_PER_PAGE * RUNS_PER_PARAGRAPH
    }

    fn full_nodes(self) -> usize {
        1 + self.page_count * self.expanded_page_nodes()
    }

    fn page_stubs_visible_nodes(self, materialized_page_count: usize) -> usize {
        1 + self.page_count + materialized_page_count * self.expanded_page_child_nodes()
    }

    fn adaptive_initial_nodes(self, materialized_page_count: usize) -> usize {
        if self.uses_full_tree() {
            self.full_nodes()
        } else {
            self.page_stubs_visible_nodes(materialized_page_count)
        }
    }

    fn scroll_update_nodes(self) -> usize {
        1 + self.expanded_page_nodes()
    }
}

fn print_shapes() {
    for &page_count in PAGE_COUNTS {
        let shape = Shape::new(page_count);
        let strategy = if shape.uses_full_tree() {
            "full"
        } else {
            "page-stubs-visible"
        };
        println!(
            "synthetic-a11y-adaptive-shape pages={} strategy={} nodes(initial={}, scroll-update={}) paragraphs={} text-runs={}",
            page_count,
            strategy,
            shape.adaptive_initial_nodes(VISIBLE_PAGE_WINDOW),
            if shape.uses_full_tree() {
                0
            } else {
                shape.scroll_update_nodes()
            },
            shape.paragraphs,
            shape.runs
        );
    }
}

fn initial_adaptive_update(shape: Shape, start_page: usize, page_count: usize) -> TreeUpdate {
    if shape.uses_full_tree() {
        initial_full_update(shape)
    } else {
        initial_page_stubs_visible_update(shape, start_page, page_count)
    }
}

fn initial_full_update(shape: Shape) -> TreeUpdate {
    let mut nodes = Vec::with_capacity(shape.full_nodes());
    nodes.push((
        DOC_ID,
        document_node_with_children(shape, (0..shape.page_count).map(page_id).collect()),
    ));

    for page in 0..shape.page_count {
        push_expanded_page_subtree(&mut nodes, shape, page);
    }

    TreeUpdate {
        nodes,
        tree: Some(AccessTree {
            root: DOC_ID,
            toolkit_name: Some("tinymist-viewer synthetic a11y bench".to_owned()),
            toolkit_version: None,
        }),
        focus: DOC_ID,
    }
}

fn initial_page_stubs_visible_update(
    shape: Shape,
    start_page: usize,
    page_count: usize,
) -> TreeUpdate {
    let mut nodes = Vec::with_capacity(shape.page_stubs_visible_nodes(page_count));
    nodes.push((
        DOC_ID,
        document_node_with_children(shape, (0..shape.page_count).map(page_id).collect()),
    ));

    let end_page = start_page + page_count;
    for page in 0..shape.page_count {
        if (start_page..end_page).contains(&page) {
            push_expanded_page_subtree(&mut nodes, shape, page);
        } else {
            nodes.push((page_id(page), page_index_node(shape, page, Vec::new())));
        }
    }

    TreeUpdate {
        nodes,
        tree: Some(AccessTree {
            root: DOC_ID,
            toolkit_name: Some("tinymist-viewer synthetic a11y bench".to_owned()),
            toolkit_version: None,
        }),
        focus: DOC_ID,
    }
}

fn page_stubs_visible_scroll_update(
    shape: Shape,
    old_start_page: usize,
    start_page: usize,
    page_count: usize,
) -> TreeUpdate {
    let (departed_page, added_page) = if start_page > old_start_page {
        (old_start_page, start_page + page_count - 1)
    } else {
        (old_start_page + page_count - 1, start_page)
    };

    let mut nodes = Vec::with_capacity(shape.scroll_update_nodes());
    nodes.push((
        page_id(departed_page),
        page_index_node(shape, departed_page, Vec::new()),
    ));
    push_expanded_page_subtree(&mut nodes, shape, added_page);

    TreeUpdate {
        nodes,
        tree: None,
        focus: DOC_ID,
    }
}

fn push_expanded_page_subtree(nodes: &mut Vec<(NodeId, Node)>, shape: Shape, page: usize) {
    nodes.push((
        page_id(page),
        page_index_node(
            shape,
            page,
            (0..PARAGRAPHS_PER_PAGE)
                .map(|paragraph| paragraph_id(page, paragraph))
                .collect(),
        ),
    ));
    for paragraph in 0..PARAGRAPHS_PER_PAGE {
        nodes.push((
            paragraph_id(page, paragraph),
            paragraph_node(page, paragraph),
        ));
        for run in 0..RUNS_PER_PARAGRAPH {
            nodes.push((
                run_id(page, paragraph, run),
                text_run_node(page, paragraph, run),
            ));
        }
    }
}

fn document_node_with_children(shape: Shape, children: Vec<NodeId>) -> Node {
    let mut node = Node::new(Role::Document);
    node.set_bounds(Rect {
        x0: 0.0,
        y0: 0.0,
        x1: 800.0,
        y1: shape.page_count as f64 * 1000.0,
    });
    node.set_children(children);
    node
}

fn page_index_node(shape: Shape, page: usize, children: Vec<NodeId>) -> Node {
    let mut node = Node::new(Role::Region);
    node.set_label(format!("Page {}", page + 1));
    node.set_bounds(Rect {
        x0: 0.0,
        y0: page as f64 * 1000.0,
        x1: 800.0,
        y1: (page + 1) as f64 * 1000.0,
    });
    node.set_position_in_set(page + 1);
    node.set_size_of_set(shape.page_count);
    node.add_action(Action::ScrollIntoView);
    node.set_children(children);
    node
}

fn paragraph_node(page: usize, paragraph: usize) -> Node {
    let y0 = page as f64 * 1000.0 + paragraph as f64 * 96.0;
    let mut node = Node::new(Role::Paragraph);
    node.set_bounds(Rect {
        x0: 48.0,
        y0,
        x1: 752.0,
        y1: y0 + 72.0,
    });
    node.set_children(
        (0..RUNS_PER_PARAGRAPH)
            .map(|run| run_id(page, paragraph, run))
            .collect::<Vec<_>>(),
    );
    node
}

fn text_run_node(page: usize, paragraph: usize, run: usize) -> Node {
    let y0 = page as f64 * 1000.0 + paragraph as f64 * 96.0 + run as f64 * 18.0;
    let x0 = 48.0 + (run % 2) as f64 * 320.0;

    let mut node = Node::new(Role::TextRun);
    node.set_bounds(Rect {
        x0,
        y0,
        x1: x0 + 300.0,
        y1: y0 + 16.0,
    });
    node.set_text_direction(TextDirection::LeftToRight);
    node.set_value(run_text(page, paragraph, run));
    node.set_character_lengths(vec![1; RUN_TEXT_CHARS]);
    node.set_character_positions(
        (0..RUN_TEXT_CHARS)
            .map(|idx| idx as f32 * 7.0)
            .collect::<Vec<_>>(),
    );
    node.set_character_widths(vec![7.0; RUN_TEXT_CHARS]);
    node
}

fn run_text(page: usize, paragraph: usize, run: usize) -> String {
    let prefix = format!("p{page:04} para{paragraph:02} run{run:02} stable ");
    let fill = RUN_TEXT_CHARS.saturating_sub(prefix.len());
    format!("{prefix}{}", "x".repeat(fill))
}

fn page_id(page: usize) -> NodeId {
    NodeId(PAGE_ID_BASE + page as u64)
}

fn paragraph_id(page: usize, paragraph: usize) -> NodeId {
    NodeId(PARAGRAPH_ID_BASE + (page * PARAGRAPHS_PER_PAGE + paragraph) as u64)
}

fn run_id(page: usize, paragraph: usize, run: usize) -> NodeId {
    NodeId(
        RUN_ID_BASE + ((page * PARAGRAPHS_PER_PAGE + paragraph) * RUNS_PER_PARAGRAPH + run) as u64,
    )
}

#[derive(Default)]
struct NoopChanges;

impl TreeChangeHandler for NoopChanges {
    fn node_added(&mut self, _node: &ConsumerNode<'_>) {}

    fn node_updated(&mut self, _old_node: &ConsumerNode<'_>, _new_node: &ConsumerNode<'_>) {}

    fn focus_moved(
        &mut self,
        _old_node: Option<&ConsumerNode<'_>>,
        _new_node: Option<&ConsumerNode<'_>>,
    ) {
    }

    fn node_removed(&mut self, _node: &ConsumerNode<'_>) {}
}

criterion_group!(benches, bench_synthetic_accesskit_tree);
criterion_main!(benches);
