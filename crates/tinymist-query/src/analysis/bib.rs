use typst::foundations::Bytes;
use yaml_rust2::{parser::Event, parser::MarkedEventReceiver, scanner::Marker};

use super::prelude::*;

pub(crate) fn bib_info(files: EcoVec<(TypstFileId, Bytes)>) -> Option<Arc<BibInfo>> {
    let mut worker = BibWorker {
        info: BibInfo::default(),
    };

    // We might have multiple bib/yaml files
    for (file_id, content) in files.clone() {
        worker.analyze_path(file_id, content);
    }

    let info = Arc::new(worker.info);

    crate::log_debug_ct!("bib analysis: {files:?} -> {info:?}");
    Some(info)
}

/// The bibliography information.
#[derive(Debug, Default)]
pub struct BibInfo {
    /// The bibliography entries.
    pub entries: indexmap::IndexMap<String, BibEntry>,
}

#[derive(Debug, Clone)]
pub struct BibEntry {
    pub file_id: TypstFileId,
    pub name_range: Range<usize>,
    pub range: Range<usize>,
}

struct BibWorker {
    info: BibInfo,
}

impl BibWorker {
    fn analyze_path(&mut self, file_id: TypstFileId, content: Bytes) -> Option<()> {
        let file_extension = file_id.vpath().as_rooted_path().extension()?.to_str()?;
        let content = std::str::from_utf8(&content).ok()?;

        match file_extension.to_lowercase().as_str() {
            "yml" | "yaml" => self.yaml_bib(file_id, content),
            "bib" => {
                let bibliography = biblatex::RawBibliography::parse(content).ok()?;
                self.tex_bib(file_id, bibliography)
            }
            _ => return None,
        };

        Some(())
    }

    fn yaml_bib(&mut self, file_id: TypstFileId, content: &str) {
        let yaml = YamlBib::from_content(content, file_id);
        self.info.entries.extend(yaml.entries);
    }

    fn tex_bib(&mut self, file_id: TypstFileId, bibliography: biblatex::RawBibliography) {
        for entry in bibliography.entries {
            let name = entry.v.key;
            let entry = BibEntry {
                file_id,
                name_range: name.span,
                range: entry.span,
            };
            self.info.entries.insert(name.v.to_owned(), entry);
        }
    }
}

#[derive(Debug, Clone)]
struct BibSpanned<T> {
    value: T,
    range: Range<usize>,
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
                    self.start = self.key.take();
                }
                self.depth += 1;
            }
            Event::Scalar(s, ..) => {
                if self.depth == 1 {
                    self.key = Some(BibSpanned {
                        value: s.to_owned(),
                        range: mark.index()..mark.index() + s.chars().count(),
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
                    let span = start.range.start..end;
                    self.content.push((start, span));
                }
            }
            _ => {}
        }
    }
}

#[derive(Debug)]
struct YamlBib {
    entries: Vec<(String, BibEntry)>,
}

impl YamlBib {
    fn from_content(content: &str, file_id: TypstFileId) -> Self {
        let mut parser = yaml_rust2::parser::Parser::new(content.chars());
        let mut loader = YamlBibLoader::default();
        parser.load(&mut loader, true).ok();

        // Resolves char offsets because yaml2 only provides char indices
        let mut char_offsets = loader
            .content
            .iter()
            .flat_map(|(name, span)| [name.range.start, name.range.end, span.start, span.end])
            .map(|offset| (offset, None))
            .collect::<Vec<_>>();
        char_offsets.sort_by_key(|(offset, _)| *offset);
        char_offsets.dedup_by_key(|(offset, _)| *offset);
        let mut cursor = 0;
        let mut utf8_offset = 0;
        for (ch_idx, ch_offset) in content.chars().chain(Some('\0')).enumerate() {
            if cursor < char_offsets.len() {
                let (idx, offset) = &mut char_offsets[cursor];
                if ch_idx == *idx {
                    *offset = Some(utf8_offset);
                    cursor += 1;
                }
            }
            utf8_offset += ch_offset.len_utf8();
        }

        // Maps the a char index to a char offset
        let char_map = char_offsets
            .into_iter()
            .filter_map(|(start, end)| end.map(|end| (start, end)))
            .collect::<HashMap<_, _>>();
        let map_range = |range: Range<usize>| {
            // The valid utf8 lower bound at the range.start
            let start = char_map.get(&range.start).copied()?;
            // The valid utf8 upper bound at the range.end
            let end = char_map.get(&range.end).copied()?;
            Some(start..end)
        };
        let to_entry = |(name, range): (BibSpanned<String>, Range<usize>)| {
            let name_range = map_range(name.range)?;
            let range = map_range(range)?;
            let entry = BibEntry {
                file_id,
                name_range,
                range,
            };
            Some((name.value, entry))
        };

        let entries = loader.content.into_iter().filter_map(to_entry).collect();
        Self { entries }
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
        let bib = super::YamlBib::from_content(
            content,
            FileId::new_fake(VirtualPath::new(Path::new("test.yml"))),
        );
        assert_eq!(bib.entries.len(), 2);
        insta::assert_snapshot!(format!("{:?}", bib.entries[0]), @r###"("Euclid", BibEntry { file_id: /test.yml, name_range: 1..7, range: 1..63 })"###);
        insta::assert_snapshot!(format!("{:?}", bib.entries[1]), @r###"("Euclid2", BibEntry { file_id: /test.yml, name_range: 63..70, range: 63..126 })"###);
    }

    #[test]
    fn yaml_bib_incomplete() {
        let content = r#"
Euclid:
  type: article
  title: '{Elements, {V}ols.\ 1--13}'
Euclid3
"#;
        let file_id = FileId::new_fake(VirtualPath::new(Path::new("test.yml")));
        super::YamlBib::from_content(content, file_id);
    }
}
