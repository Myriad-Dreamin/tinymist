use crate::{get_lexical_hierarchy, prelude::*, LexicalScopeGranularity};

#[derive(Debug, Clone)]
pub struct FoldingRangeRequest {
    pub path: PathBuf,
}

impl FoldingRangeRequest {
    pub fn request(
        self,
        world: &TypstSystemWorld,
        position_encoding: PositionEncoding,
    ) -> Option<Vec<FoldingRange>> {
        let source = get_suitable_source_in_workspace(world, &self.path).ok()?;

        let symbols = get_lexical_hierarchy(source, LexicalScopeGranularity::Block)?;

        let _ = symbols;
        let _ = position_encoding;

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test_folding_range_request() {
        run_with_source("let a = 1;", |world, path| {
            let request = FoldingRangeRequest { path };
            let result = request.request(world, PositionEncoding::Utf16);
            assert_eq!(result, None);
        });
    }
}
