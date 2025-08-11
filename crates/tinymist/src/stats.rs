//! Statistics about the analyzers

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use parking_lot::Mutex;
use reflexo::{hash::FxDashMap, path::unix_slash};

#[derive(Clone)]
pub(crate) struct QueryStatBucketData {
    pub query: u64,
    pub total: Duration,
    pub snap: Duration,
    pub min: Duration,
    pub max: Duration,
}

impl Default for QueryStatBucketData {
    fn default() -> Self {
        Self {
            query: 0,
            total: Duration::from_secs(0),
            snap: Duration::from_secs(0),
            min: Duration::from_secs(u64::MAX),
            max: Duration::from_secs(0),
        }
    }
}

/// Statistics about some query
#[derive(Default, Clone)]
pub struct QueryStatBucket {
    pub(crate) data: Arc<Mutex<QueryStatBucketData>>,
}

pub struct QueryStatGuard {
    pub bucket: QueryStatBucket,
    pub since: std::time::SystemTime,
    pub snap_since: OnceLock<std::time::Duration>,
}

impl Drop for QueryStatGuard {
    fn drop(&mut self) {
        let elapsed = self.since.elapsed().unwrap_or_default();
        let mut data = self.bucket.data.lock();
        data.query += 1;
        data.total += elapsed;
        data.snap += self.snap_since.get().cloned().unwrap_or_default();
        data.min = data.min.min(elapsed);
        data.max = data.max.max(elapsed);
    }
}

impl QueryStatGuard {
    pub(crate) fn snap(&self) {
        self.snap_since
            .get_or_init(|| self.since.elapsed().unwrap_or_default());
    }
}

/// Statistics about the analyzers
#[derive(Default)]
pub struct CompilerQueryStats {
    pub(crate) query_stats: FxDashMap<PathBuf, FxDashMap<&'static str, QueryStatBucket>>,
}

impl CompilerQueryStats {
    /// Record a query.
    pub(crate) fn query_stat(&self, path: Option<&Path>, name: &'static str) -> QueryStatGuard {
        let stats = &self.query_stats;
        // let refs = stats.entry(path.clone()).or_default();
        let refs = stats
            .entry(path.unwrap_or_else(|| Path::new("")).to_path_buf())
            .or_default();
        let refs2 = refs.entry(name).or_default();
        QueryStatGuard {
            bucket: refs2.clone(),
            since: std::time::SystemTime::now(),
            snap_since: OnceLock::new(),
        }
    }

    /// Report the statistics of the analysis.
    pub fn report(&self) -> String {
        let stats = &self.query_stats;
        let mut data = Vec::new();
        for refs in stats.iter() {
            let id = unix_slash(refs.key());
            let queries = refs.value();
            for refs2 in queries.iter() {
                let query = refs2.key();
                let bucket = refs2.value().data.lock().clone();
                let name = format!("{id:?}:{query}").replace('\\', "/");
                data.push((name, bucket));
            }
        }

        // sort by query duration
        data.sort_by(|x, y| y.1.max.cmp(&x.1.max));

        // format to html

        let mut html = String::new();
        html.push_str(r#"<div>
<style>
table.query-stats { width: 100%; border-collapse: collapse; }
table.query-stats th, table.query-stats td { border: 1px solid black; padding: 8px; text-align: center; }
table.query-stats th.name-column, table.query-stats td.name-column { text-align: left; }
table.query-stats tr:nth-child(odd) { background-color: rgba(242, 242, 242, 0.8); }
@media (prefers-color-scheme: dark) {
    table.query-stats tr:nth-child(odd) { background-color: rgba(50, 50, 50, 0.8); }
}
</style>
<table class="query-stats"><tr><th class="query-column">Query</th><th>Count</th><th>Total</th><th>Snap</th><th>Min</th><th>Max</th></tr>"#);

        for (name, bucket) in data {
            html.push_str("<tr>");
            html.push_str(&format!(r#"<td class="query-column">{name}</td>"#));
            html.push_str(&format!("<td>{}</td>", bucket.query));
            html.push_str(&format!("<td>{:?}</td>", bucket.total));
            html.push_str(&format!("<td>{:?}</td>", bucket.snap));
            html.push_str(&format!("<td>{:?}</td>", bucket.min));
            html.push_str(&format!("<td>{:?}</td>", bucket.max));
            html.push_str("</tr>");
        }
        html.push_str("</table>");
        html.push_str("</div>");

        html
    }
}
