//! Tinymist Analysis Statistics

use std::fmt::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock};

use parking_lot::Mutex;
use tinymist_std::hash::FxDashMap;
use tinymist_std::time::Duration;
use typst::syntax::FileId;

/// Statistics about the allocation

#[derive(Debug, Default)]
pub struct AllocStats {
    /// The number of allocated objects.
    pub allocated: AtomicUsize,
    /// The number of dropped objects.
    pub dropped: AtomicUsize,
}

impl AllocStats {
    /// increment the statistics.
    pub fn increment(&self) {
        self.allocated.fetch_add(1, Ordering::Relaxed);
    }

    /// decrement the statistics.
    pub fn decrement(&self) {
        self.dropped.fetch_add(1, Ordering::Relaxed);
    }

    /// Report the statistics of the allocation.
    pub fn report() -> String {
        let maps = crate::adt::interner::MAPS.lock().clone();
        let mut data = Vec::new();
        for (name, sz, map) in maps {
            let allocated = map.allocated.load(std::sync::atomic::Ordering::Relaxed);
            let dropped = map.dropped.load(std::sync::atomic::Ordering::Relaxed);
            let alive = allocated.saturating_sub(dropped);
            data.push((name, sz * alive, allocated, dropped, alive));
        }

        // sort by total
        data.sort_by(|x, y| y.4.cmp(&x.4));

        // format to html

        let mut html = String::new();
        html.push_str(r#"<div>
<style>
table.alloc-stats { width: 100%; border-collapse: collapse; }
table.alloc-stats th, table.alloc-stats td { border: 1px solid black; padding: 8px; text-align: center; }
table.alloc-stats th.name-column, table.alloc-stats td.name-column { text-align: left; }
table.alloc-stats tr:nth-child(odd) { background-color: rgba(242, 242, 242, 0.8); }
@media (prefers-color-scheme: dark) {
    table.alloc-stats tr:nth-child(odd) { background-color: rgba(50, 50, 50, 0.8); }
}
</style>
<table class="alloc-stats"><tr><th class="name-column">Name</th><th>Alive</th><th>Allocated</th><th>Dropped</th><th>Size</th></tr>"#);

        for (name, sz, allocated, dropped, alive) in data {
            html.push_str("<tr>");
            html.push_str(&format!(r#"<td class="name-column">{name}</td>"#));
            html.push_str(&format!("<td>{alive}</td>"));
            html.push_str(&format!("<td>{allocated}</td>"));
            html.push_str(&format!("<td>{dropped}</td>"));
            html.push_str(&format!("<td>{}</td>", human_size(sz)));
            html.push_str("</tr>");
        }
        html.push_str("</table>");
        html.push_str("</div>");

        html
    }
}

/// The data of the query statistic.
#[derive(Clone)]
pub struct QueryStatBucketData {
    pub(crate) query: u64,
    pub(crate) missing: u64,
    pub(crate) total: Duration,
    pub(crate) min: Duration,
    pub(crate) max: Duration,
}

impl Default for QueryStatBucketData {
    fn default() -> Self {
        Self {
            query: 0,
            missing: 0,
            total: Duration::from_secs(0),
            min: Duration::from_secs(u64::MAX),
            max: Duration::from_secs(0),
        }
    }
}

/// Statistics about some query
#[derive(Default, Clone)]
pub struct QueryStatBucket {
    /// The data of the query statistic.
    pub data: Arc<Mutex<QueryStatBucketData>>,
}

impl QueryStatBucket {
    /// Increment the query statistic.
    pub fn increment(&self, elapsed: Duration) {
        let mut data = self.data.lock();
        data.query += 1;
        data.total += elapsed;
        data.min = data.min.min(elapsed);
        data.max = data.max.max(elapsed);
    }
}

/// A guard for the query statistic.
pub struct QueryStatGuard {
    /// The bucket of the query statistic for any file.
    pub bucket_any: Option<QueryStatBucket>,
    /// The bucket of the query statistic.
    pub bucket: QueryStatBucket,
    /// The start time of the query.
    pub since: tinymist_std::time::Instant,
}

impl Drop for QueryStatGuard {
    fn drop(&mut self) {
        let elapsed = self.since.elapsed();
        self.bucket.increment(elapsed);
        if let Some(bucket) = self.bucket_any.as_ref() {
            bucket.increment(elapsed);
        }
    }
}

impl QueryStatGuard {
    /// Increment the missing count.
    pub fn miss(&self) {
        let mut data = self.bucket.data.lock();
        data.missing += 1;
    }
}

/// Statistics about the analyzers
#[derive(Default)]
pub struct AnalysisStats {
    /// The query statistics.
    pub query_stats: Arc<FxDashMap<Option<FileId>, FxDashMap<&'static str, QueryStatBucket>>>,
}

impl AnalysisStats {
    /// Gets a statistic guard for a query.
    pub fn stat(&self, id: Option<FileId>, query: &'static str) -> QueryStatGuard {
        let stats = &self.query_stats;
        let get = |v| {
            stats
                .entry(v)
                .or_default()
                .entry(query)
                .or_default()
                .clone()
        };
        QueryStatGuard {
            bucket_any: if id.is_some() { Some(get(None)) } else { None },
            bucket: get(id),
            since: tinymist_std::time::Instant::now(),
        }
    }

    /// Reports the statistics of the analysis.
    pub fn report(&self) -> String {
        let stats = &self.query_stats;
        let mut data = Vec::new();
        for refs in stats.iter() {
            let id = refs.key();
            let queries = refs.value();
            for refs2 in queries.iter() {
                let query = refs2.key();
                let bucket = refs2.value().data.lock().clone();
                let name = match id {
                    Some(id) => format!("{id:?}:{query}"),
                    None => query.to_string(),
                };
                let name = name.replace('\\', "/");
                data.push((name, bucket));
            }
        }

        // sort by query duration
        data.sort_by(|x, y| y.1.max.cmp(&x.1.max));

        // format to html

        let mut html = String::new();
        html.push_str(r#"<div>
<style>
table.analysis-stats { width: 100%; border-collapse: collapse; }
table.analysis-stats th, table.analysis-stats td { border: 1px solid black; padding: 8px; text-align: center; }
table.analysis-stats th.name-column, table.analysis-stats td.name-column { text-align: left; }
table.analysis-stats tr:nth-child(odd) { background-color: rgba(242, 242, 242, 0.8); }
@media (prefers-color-scheme: dark) {
    table.analysis-stats tr:nth-child(odd) { background-color: rgba(50, 50, 50, 0.8); }
}
</style>
<table class="analysis-stats"><tr><th class="query-column">Name</th><th>Count</th><th>Missing</th><th>Total</th><th>Min</th><th>Max</th></tr>"#);

        for (name, bucket) in data {
            let _ = write!(
                &mut html,
                "<tr><td class=\"query-column\">{name}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td></tr>",
                bucket.query, bucket.missing, bucket.total, bucket.min, bucket.max
            );
        }
        html.push_str("</table>");
        html.push_str("</div>");

        html
    }
}

/// The global statistics about the analyzers.
pub static GLOBAL_STATS: LazyLock<AnalysisStats> = LazyLock::new(AnalysisStats::default);

fn human_size(size: usize) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut unit = 0;
    let mut size = size as f64;
    while size >= 768.0 && unit < units.len() {
        size /= 1024.0;
        unit += 1;
    }
    format!("{:.2} {}", size, units[unit])
}
