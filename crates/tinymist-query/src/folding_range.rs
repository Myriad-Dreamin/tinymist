use hashbrown::HashSet;

use crate::{
    prelude::*,
    syntax::{get_lexical_hierarchy, LexicalHierarchy, LexicalKind, LexicalScopeKind},
    SyntaxRequest,
};

/// The [`textDocument/foldingRange`] request is sent from the client to the
/// server to return all folding ranges found in a given text document.
///
/// [`textDocument/foldingRange`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_foldingRange
///
/// # Compatibility
///
/// This request was introduced in specification version 3.10.0.
#[derive(Debug, Clone)]
pub struct FoldingRangeRequest {
    /// The path of the document to get folding ranges for.
    pub path: PathBuf,
    /// If set, the client can only provide folding ranges that consist of whole
    /// lines.
    pub line_folding_only: bool,
}

impl SyntaxRequest for FoldingRangeRequest {
    type Response = Vec<FoldingRange>;

    fn request(
        self,
        source: &Source,
        position_encoding: PositionEncoding,
    ) -> Option<Self::Response> {
        let line_folding_only = self.line_folding_only;

        let hierarchy = get_lexical_hierarchy(source, LexicalScopeKind::Braced)?;

        let mut results = vec![];
        let LspPosition { line, character } =
            typst_to_lsp::offset_to_position(source.text().len(), position_encoding, source);
        let loc = (line, Some(character));

        calc_folding_range(
            &hierarchy,
            source,
            position_encoding,
            loc,
            loc,
            true,
            &mut results,
        );

        // Generally process of folding ranges with line_folding_only
        if line_folding_only {
            let mut max_line = 0;
            for r in &mut results {
                r.start_character = None;
                r.end_character = None;
                max_line = max_line.max(r.end_line);
            }
            let mut line_coverage = vec![false; max_line as usize + 1];
            let mut pair_coverage = HashSet::new();
            results.reverse();
            results.retain_mut(|r| {
                if pair_coverage.contains(&(r.start_line, r.end_line)) {
                    return false;
                }

                if line_coverage[r.start_line as usize] {
                    r.start_line += 1;
                }
                if line_coverage[r.end_line as usize] {
                    r.end_line = r.end_line.saturating_sub(1);
                }
                if r.start_line >= r.end_line {
                    return false;
                }

                line_coverage[r.start_line as usize] = true;
                pair_coverage.insert((r.start_line, r.end_line));
                true
            });
            results.reverse();
        }

        crate::log_debug_ct!("FoldingRangeRequest(line_folding_only={line_folding_only}) symbols: {hierarchy:#?} results: {results:#?}");

        Some(results)
    }
}

type LoC = (u32, Option<u32>);

fn calc_folding_range(
    hierarchy: &[LexicalHierarchy],
    source: &Source,
    position_encoding: PositionEncoding,
    parent_last_loc: LoC,
    last_loc: LoC,
    is_last_range: bool,
    folding_ranges: &mut Vec<FoldingRange>,
) {
    for (idx, child) in hierarchy.iter().enumerate() {
        let range = typst_to_lsp::range(child.info.range.clone(), source, position_encoding);
        let is_not_last_range = idx + 1 < hierarchy.len();
        let is_not_final_last_range = !is_last_range || is_not_last_range;

        let mut folding_range = FoldingRange {
            start_line: range.start.line,
            start_character: Some(range.start.character),
            end_line: range.end.line,
            end_character: Some(range.end.character),
            kind: None,
            collapsed_text: Some(child.info.name.to_string()),
        };

        let next_start = if is_not_last_range {
            let next = &hierarchy[idx + 1];
            let next_rng = typst_to_lsp::range(next.info.range.clone(), source, position_encoding);
            (next_rng.start.line, Some(next_rng.start.character))
        } else if is_not_final_last_range {
            parent_last_loc
        } else {
            last_loc
        };

        if matches!(child.info.kind, LexicalKind::Heading(..)) {
            folding_range.end_line = folding_range.end_line.max(if is_not_last_range {
                next_start.0.saturating_sub(1)
            } else {
                next_start.0
            });
        }

        if matches!(child.info.kind, LexicalKind::CommentGroup) {
            folding_range.kind = Some(lsp_types::FoldingRangeKind::Comment);
        }

        if let Some(ch) = &child.children {
            let parent_last_loc = if is_not_last_range {
                (range.end.line, Some(range.end.character))
            } else {
                parent_last_loc
            };

            calc_folding_range(
                ch,
                source,
                position_encoding,
                parent_last_loc,
                last_loc,
                !is_not_final_last_range,
                folding_ranges,
            );
        }
        folding_ranges.push(folding_range);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("folding_range", &|world, path| {
            let r = |line_folding_only| {
                let request = FoldingRangeRequest {
                    path: path.clone(),
                    line_folding_only,
                };

                let source = world.source_by_path(&path).unwrap();

                request.request(&source, PositionEncoding::Utf16)
            };

            let result_false = r(false);
            let result_true = r(true);
            assert_snapshot!(JsonRepr::new_pure(json!({
                "false": result_false,
                "true": result_true,
            })));
        });
    }
}
