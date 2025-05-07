//! Statistics about the analyzers

use std::{sync::Arc, time::Duration};

use parking_lot::Mutex;
use tinymist_std::hash::FxDashMap;
use typst::syntax::FileId;

#[derive(Clone)]
pub(crate) struct QueryStatBucketData {
    pub query: u64,
    pub missing: u64,
    pub total: Duration,
    pub min: Duration,
    pub max: Duration,
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
pub(crate) struct QueryStatBucket {
    pub data: Arc<Mutex<QueryStatBucketData>>,
}

pub(crate) struct QueryStatGuard {
    pub bucket: QueryStatBucket,
    pub since: std::time::SystemTime,
}

impl Drop for QueryStatGuard {
    fn drop(&mut self) {
        let elapsed = self.since.elapsed().unwrap_or_default();
        let mut data = self.bucket.data.lock();
        data.query += 1;
        data.total += elapsed;
        data.min = data.min.min(elapsed);
        data.max = data.max.max(elapsed);
    }
}

impl QueryStatGuard {
    pub(crate) fn miss(&self) {
        let mut data = self.bucket.data.lock();
        data.missing += 1;
    }
}

/// Statistics about the analyzers
#[derive(Default)]
pub struct AnalysisStats {
    pub(crate) query_stats: FxDashMap<FileId, FxDashMap<&'static str, QueryStatBucket>>,
}

impl AnalysisStats {
    /// Report the statistics of the analysis.
    pub fn report(&self) -> String {
        let stats = &self.query_stats;
        let mut data = Vec::new();
        for refs in stats.iter() {
            let id = refs.key();
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
table.analysis-stats { width: 100%; border-collapse: collapse; }
table.analysis-stats th, table.analysis-stats td { border: 1px solid black; padding: 8px; text-align: center; }
table.analysis-stats th.name-column, table.analysis-stats td.name-column { text-align: left; }
table.analysis-stats tr:nth-child(odd) { background-color: rgba(242, 242, 242, 0.8); }
@media (prefers-color-scheme: dark) {
    table.analysis-stats tr:nth-child(odd) { background-color: rgba(50, 50, 50, 0.8); }
}
</style>
<table class="analysis-stats"><tr><th class="query-column">Query</th><th>Count</th><th>Missing</th><th>Total</th><th>Min</th><th>Max</th></tr>"#);

        for (name, bucket) in data {
            html.push_str("<tr>");
            html.push_str(&format!(r#"<td class="query-column">{name}</td>"#));
            html.push_str(&format!("<td>{}</td>", bucket.query));
            html.push_str(&format!("<td>{}</td>", bucket.missing));
            html.push_str(&format!("<td>{:?}</td>", bucket.total));
            html.push_str(&format!("<td>{:?}</td>", bucket.min));
            html.push_str(&format!("<td>{:?}</td>", bucket.max));
            html.push_str("</tr>");
        }
        html.push_str("</table>");
        html.push_str("</div>");

        html
    }
}
