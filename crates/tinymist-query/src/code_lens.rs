use lsp_types::Command;

use crate::prelude::*;

#[derive(Debug, Clone)]
pub struct CodeLensRequest {
    pub path: PathBuf,
}

impl CodeLensRequest {
    pub fn request(
        self,
        world: &TypstSystemWorld,
        position_encoding: PositionEncoding,
    ) -> Option<Vec<CodeLens>> {
        let source = get_suitable_source_in_workspace(world, &self.path).ok()?;

        let doc_start = typst_to_lsp::range(0..0, &source, position_encoding);

        let mut res = vec![];

        let run_code_lens_cmd = |title: &str, args: Vec<JsonValue>| Command {
            title: title.to_string(),
            command: "tinymist.runCodeLens".to_string(),
            arguments: Some(args),
        };

        let doc_lens = |title: &str, args: Vec<JsonValue>| CodeLens {
            range: doc_start,
            command: Some(run_code_lens_cmd(title, args)),
            data: None,
        };

        res.push(doc_lens("Preview", vec!["preview".into()]));
        res.push(doc_lens("Preview in ..", vec!["preview-in".into()]));
        res.push(doc_lens("Export PDF", vec!["export-pdf".into()]));
        res.push(doc_lens("Export ..", vec!["export-as".into()]));

        Some(res)
    }
}
