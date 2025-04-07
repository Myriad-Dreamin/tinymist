//! Tinymist Analysis Statistics

use std::sync::atomic::{AtomicUsize, Ordering};

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
