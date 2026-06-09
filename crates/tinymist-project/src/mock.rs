//! Mock project compiler support for Tinymist tests.
//!
//! This module intentionally lives in `tinymist-project` so project-runtime
//! tests can drive compiler interrupts without depending on aggregate
//! test-support crates. Enable the `mock` feature from downstream
//! test-support crates when this module is needed as a dependency.

use tinymist_world::{
    mock::{MockCompilerFeat, MockWorldBuilder},
    vfs::{mock::MockChange, notify::NotifyMessage},
};
use tokio::sync::mpsc;

use crate::{CompileServerOpts, Interrupt, ProjectCompiler};

/// A project compiler backed by mock VFS and world components.
pub type MockProjectCompiler<Ext = ()> = ProjectCompiler<MockCompilerFeat, Ext>;

/// Extension helpers for building project compilers from mock world builders.
pub trait MockProjectBuilderExt {
    /// Builds a syntax-only project compiler and its notify receiver.
    fn project_compiler<Ext>(
        &self,
    ) -> typst::diag::FileResult<(
        MockProjectCompiler<Ext>,
        mpsc::UnboundedReceiver<NotifyMessage>,
    )>
    where
        Ext: Default + 'static;

    /// Builds a project compiler with custom options and its notify receiver.
    fn project_compiler_with_opts<Ext>(
        &self,
        opts: CompileServerOpts<MockCompilerFeat, Ext>,
    ) -> typst::diag::FileResult<(
        MockProjectCompiler<Ext>,
        mpsc::UnboundedReceiver<NotifyMessage>,
    )>
    where
        Ext: Default + 'static;
}

impl MockProjectBuilderExt for MockWorldBuilder {
    fn project_compiler<Ext>(
        &self,
    ) -> typst::diag::FileResult<(
        MockProjectCompiler<Ext>,
        mpsc::UnboundedReceiver<NotifyMessage>,
    )>
    where
        Ext: Default + 'static,
    {
        self.project_compiler_with_opts(CompileServerOpts {
            syntax_only: true,
            ..Default::default()
        })
    }

    fn project_compiler_with_opts<Ext>(
        &self,
        opts: CompileServerOpts<MockCompilerFeat, Ext>,
    ) -> typst::diag::FileResult<(
        MockProjectCompiler<Ext>,
        mpsc::UnboundedReceiver<NotifyMessage>,
    )>
    where
        Ext: Default + 'static,
    {
        let (tx, rx) = mpsc::unbounded_channel();
        Ok((ProjectCompiler::new(self.build_universe()?, tx, opts), rx))
    }
}

/// Applies VFS mock changes to project compilers.
pub trait MockProjectChangeExt {
    /// Applies this change to a project compiler as a filesystem event.
    fn apply_as_fs_to_project<F, Ext>(&self, compiler: &mut ProjectCompiler<F, Ext>, is_sync: bool)
    where
        F: tinymist_world::CompilerFeat + Send + Sync + 'static,
        Ext: Default + 'static;

    /// Applies this change to a project compiler as a memory event.
    fn apply_as_memory_to_project<F, Ext>(&self, compiler: &mut ProjectCompiler<F, Ext>)
    where
        F: tinymist_world::CompilerFeat + Send + Sync + 'static,
        Ext: Default + 'static;
}

impl MockProjectChangeExt for MockChange {
    fn apply_as_fs_to_project<F, Ext>(&self, compiler: &mut ProjectCompiler<F, Ext>, is_sync: bool)
    where
        F: tinymist_world::CompilerFeat + Send + Sync + 'static,
        Ext: Default + 'static,
    {
        compiler.process(Interrupt::Fs(self.filesystem_event(is_sync)));
    }

    fn apply_as_memory_to_project<F, Ext>(&self, compiler: &mut ProjectCompiler<F, Ext>)
    where
        F: tinymist_world::CompilerFeat + Send + Sync + 'static,
        Ext: Default + 'static,
    {
        compiler.process(Interrupt::Memory(self.memory_event()));
    }
}
