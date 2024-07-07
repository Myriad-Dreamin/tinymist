#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum CompileStatus {
    Compiling,
    CompileSuccess,
    CompileError,
}

pub trait CompilationHandle: Send + 'static {
    fn status(&self, status: CompileStatus);
    fn notify_compile(
        &self,
        res: Result<std::sync::Arc<typst_ts_core::TypstDocument>, CompileStatus>,
    );
}
