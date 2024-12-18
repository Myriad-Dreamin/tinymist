use std::ffi::OsStr;

use typst::foundations::Bytes;
use yaml_rust2::{parser::Event, parser::MarkedEventReceiver, scanner::Marker};

use super::prelude::*;

#[derive(Debug, Clone)]
struct BibSpanned<T> {
    value: T,
    span: Range<usize>,
}

#[derive(Default)]
struct YamlBibLoader {
    depth: usize,
    start: Option<BibSpanned<String>>,
    key: Option<BibSpanned<String>>,
    content: Vec<(BibSpanned<String>, Range<usize>)>,
}

impl MarkedEventReceiver for YamlBibLoader {
    fn on_event(&mut self, event: Event, mark: Marker) {
        match event {
            Event::MappingStart(..) => {
                if self.depth == 1 {
                    crate::log_debug_ct!("mapping start: {:?} {:?}", self.key, mark.index());
                    self.start = self.key.take();
                }
                self.depth += 1;
            }
            Event::Scalar(s, ..) => {
                crate::log_debug_ct!("scalar: {:?} {:?}", s, mark.index());
                if self.depth == 1 {
                    self.key = Some(BibSpanned {
                        value: s.to_owned(),
                        span: mark.index()..mark.index() + s.chars().count(),
                    });
                }
            }
            Event::MappingEnd => {
                self.depth -= 1;
                if self.depth == 1 {
                    let end = mark.index();
                    let start = self.start.take();
                    let Some(start) = start else {
                        return;
                    };
                    let span = start.span.start..end;
                    self.content.push((start, span));
                    crate::log_debug_ct!("mapping end: {:?} {:?}", self.key, mark.index());
                }
            }
            _ => {}
        }
    }
}

struct YamlBib {
    entries: Vec<(String, BibEntry)>,
}

impl YamlBib {
    fn from_content(content: &str, file_id: TypstFileId) -> Self {
        let mut parser = yaml_rust2::parser::Parser::new(content.chars());
        let mut loader = YamlBibLoader::default();
        parser.load(&mut loader, true).ok();

        let mut span_mapper = Vec::from_iter(
            loader
                .content
                .iter()
                .flat_map(|(name, span)| [name.span.start, name.span.end, span.start, span.end])
                .map(|offset| (offset, None)),
        );
        span_mapper.sort_by_key(|(offset, _)| *offset);
        span_mapper.dedup_by_key(|(offset, _)| *offset);
        let mut span_cursor = 0;
        let mut byte_offset = 0;
        for (off, ch) in content.chars().chain(Some('\0')).enumerate() {
            if span_cursor < span_mapper.len() {
                let (span, w) = &mut span_mapper[span_cursor];
                if off == *span {
                    *w = Some(byte_offset);
                    span_cursor += 1;
                }
            }
            byte_offset += ch.len_utf8();
        }

        let span_map = HashMap::<usize, usize>::from_iter(
            span_mapper
                .into_iter()
                .filter_map(|(span, offset)| offset.map(|offset| (span, offset))),
        );
        let map_span = |span: Range<usize>| {
            let start = span_map.get(&span.start).copied()?;
            let end = span_map.get(&span.end).copied()?;
            Some(start..end)
        };

        let entries = loader
            .content
            .into_iter()
            .filter_map(|(k, span)| {
                let k_span = map_span(k.span)?;
                let span = map_span(span)?;
                let entry = BibEntry {
                    file_id,
                    name_span: k_span.clone(),
                    span: span.clone(),
                };
                Some((k.value, entry))
            })
            .collect();

        Self { entries }
    }
}

#[derive(Debug, Clone)]
pub struct BibEntry {
    pub file_id: TypstFileId,
    pub name_span: Range<usize>,
    pub span: Range<usize>,
}

#[derive(Default)]
pub struct BibInfo {
    /// The bibliography entries.
    pub entries: indexmap::IndexMap<String, BibEntry>,
}

pub(crate) fn analyze_bib(paths: EcoVec<(TypstFileId, Bytes)>) -> Option<Arc<BibInfo>> {
    let mut worker = BibWorker {
        info: BibInfo::default(),
    };

    // We might have multiple bib/yaml files
    for (path, content) in paths.clone() {
        worker.analyze_path(path, content);
    }

    crate::log_debug_ct!(
        "bib analysis: {paths:?} -> {entries:?}",
        entries = worker.info.entries
    );
    Some(Arc::new(worker.info))
}

struct BibWorker {
    info: BibInfo,
}

impl BibWorker {
    fn analyze_path(&mut self, path: TypstFileId, content: Bytes) -> Option<()> {
        let content = std::str::from_utf8(&content).ok()?;

        let ext = path
            .vpath()
            .as_rootless_path()
            .extension()
            .and_then(OsStr::to_str)
            .unwrap_or_default();

        match ext.to_lowercase().as_str() {
            "yml" | "yaml" => {
                let yaml = YamlBib::from_content(content, path);
                self.info.entries.extend(yaml.entries);
            }
            "bib" => {
                let bibliography = biblatex::RawBibliography::parse(content).ok()?;
                for entry in bibliography.entries {
                    let name = entry.v.key;
                    let span = entry.span;
                    self.info.entries.insert(
                        name.v.to_owned(),
                        BibEntry {
                            file_id: path,
                            name_span: name.span,
                            span,
                        },
                    );
                }
            }
            _ => return None,
        };

        Some(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use typst::syntax::{FileId, VirtualPath};

    #[test]
    fn yaml_bib_test() {
        let content = r#"
Euclid:
  type: article
  title: '{Elements, {V}ols.\ 1--13}'
Euclid2:
  type: article
  title: '{Elements, {V}ols.\ 2--13}'
"#;
        let yaml = super::YamlBib::from_content(
            content,
            FileId::new_fake(VirtualPath::new(Path::new("test.yml"))),
        );
        assert_eq!(yaml.entries.len(), 2);
        assert_eq!(yaml.entries[0].0, "Euclid");
        assert_eq!(yaml.entries[1].0, "Euclid2");
    }

    #[test]
    fn yaml_bib_incomplete() {
        let content = r#"
Euclid:
  type: article
  title: '{Elements, {V}ols.\ 1--13}'
Euclid3
"#;
        super::YamlBib::from_content(
            content,
            FileId::new_fake(VirtualPath::new(Path::new("test.yml"))),
        );
    }
}
