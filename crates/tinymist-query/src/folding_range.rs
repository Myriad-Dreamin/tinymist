use crate::{
    analysis::{get_lexical_hierarchy, LexicalHierarchy, LexicalKind, LexicalScopeKind},
    prelude::*,
};

#[derive(Debug, Clone)]
pub struct FoldingRangeRequest {
    pub path: PathBuf,
    pub line_folding_only: bool,
}

impl FoldingRangeRequest {
    pub fn request(
        self,
        world: &TypstSystemWorld,
        position_encoding: PositionEncoding,
    ) -> Option<Vec<FoldingRange>> {
        let line_folding_only = self.line_folding_only;

        let source = get_suitable_source_in_workspace(world, &self.path).ok()?;

        let symbols = get_lexical_hierarchy(source.clone(), LexicalScopeKind::Block)?;

        let mut results = vec![];
        let LspPosition { line, character } =
            typst_to_lsp::offset_to_position(source.text().len(), position_encoding, &source);
        let loc = (line, Some(character));

        calc_folding_range(
            &symbols,
            &source,
            position_encoding,
            line_folding_only,
            loc,
            loc,
            true,
            &mut results,
        );
        if false {
            trace!("FoldingRangeRequest(line_folding_only={line_folding_only}) symbols: {symbols:#?} results: {results:#?}");
        }

        Some(results)
    }
}

type LoC = (u32, Option<u32>);

#[allow(clippy::too_many_arguments)]
#[allow(deprecated)]
fn calc_folding_range(
    symbols: &[LexicalHierarchy],
    source: &Source,
    position_encoding: PositionEncoding,
    line_folding_only: bool,
    parent_last_loc: LoC,
    last_loc: LoC,
    is_last_range: bool,
    ranges: &mut Vec<FoldingRange>,
) {
    for (i, e) in symbols.iter().enumerate() {
        let rng = typst_to_lsp::range(e.info.range.clone(), source, position_encoding).raw_range;
        let is_not_last_range = i + 1 < symbols.len();
        let is_not_final_last_range = !is_last_range || is_not_last_range;

        let mut range = FoldingRange {
            start_line: rng.start.line,
            start_character: Some(rng.start.character),
            end_line: rng.end.line,
            end_character: line_folding_only.then_some(rng.end.character),
            kind: None,
            collapsed_text: Some(e.info.name.clone()),
        };

        let next_start = if is_not_last_range {
            let next = &symbols[i + 1];
            let next_rng =
                typst_to_lsp::range(next.info.range.clone(), source, position_encoding).raw_range;
            (next_rng.start.line, Some(next_rng.start.character))
        } else if is_not_final_last_range {
            parent_last_loc
        } else {
            last_loc
        };

        if matches!(e.info.kind, LexicalKind::Namespace(..)) {
            range.end_line = range.end_line.max(if is_not_last_range {
                next_start.0.saturating_sub(1)
            } else {
                next_start.0
            });
        }

        if let Some(ch) = &e.children {
            let parent_last_loc = if is_not_last_range {
                (rng.end.line, Some(rng.end.character))
            } else {
                parent_last_loc
            };

            calc_folding_range(
                ch,
                source,
                position_encoding,
                line_folding_only,
                parent_last_loc,
                last_loc,
                !is_not_final_last_range,
                ranges,
            );
        }

        ranges.push(range);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test_folding_range_request() {
        run_with_source("#let a = 1;", |world, path| {
            let request = FoldingRangeRequest {
                path,
                line_folding_only: true,
            };
            let result = request.request(world, PositionEncoding::Utf16);
            assert_snapshot!(JsonRepr::new_pure(result.unwrap()), @"[]");
        });
        let t = r#"#let a = {
  let b = {
  
  }
}"#;
        run_with_source(t, |world, path| {
            let request = FoldingRangeRequest {
                path,
                line_folding_only: true,
            };
            let result = request.request(world, PositionEncoding::Utf16);
            assert_snapshot!(JsonRepr::new_pure(result.unwrap()), @r###"
            [
             {
              "collapsedText": "",
              "endLine": 0,
              "startCharacter": 9,
              "startLine": 0
             },
             {
              "collapsedText": "",
              "endLine": 3,
              "startCharacter": 10,
              "startLine": 1
             }
            ]
            "###);
        });
    }
}
