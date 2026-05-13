//! Mock world support for Tinymist tests.
//!
//! This module intentionally lives in `tinymist-world` so world-level tests can
//! use deterministic compiler worlds without depending on higher-level project
//! crates. Enable the `mock` feature from downstream test-support crates when
//! this module is needed as a dependency.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
};

use tinymist_vfs::{
    RootResolver, Vfs,
    mock::{MockChange, MockPathAccess, MockWorkspace},
};
use typst::{
    Features,
    diag::FileResult,
    foundations::{Bytes, Dict},
    syntax::VirtualPath,
    utils::LazyHash,
};

use crate::{
    CompilerFeat, CompilerUniverse, CompilerWorld, EntryState,
    font::{FontResolverImpl, memory::MemoryFontSearcher},
    package::{RegistryPathMapper, registry::DummyRegistry},
};

/// A compiler feature set for mock-backed Tinymist worlds.
#[derive(Debug, Clone, Copy)]
pub struct MockCompilerFeat;

impl CompilerFeat for MockCompilerFeat {
    type FontResolver = FontResolverImpl;
    type AccessModel = MockPathAccess;
    type Registry = DummyRegistry;
}

/// A compiler universe backed by [`MockWorkspace`].
pub type MockUniverse = CompilerUniverse<MockCompilerFeat>;

/// A compiler world backed by [`MockWorkspace`].
pub type MockWorld = CompilerWorld<MockCompilerFeat>;

/// Extension helpers for using a VFS mock workspace at world level.
pub trait MockWorkspaceWorldExt {
    /// Creates an entry state rooted at this workspace.
    fn entry_state(&self, entry: impl AsRef<Path>) -> FileResult<EntryState>;

    /// Creates a world builder for this workspace and entry file.
    fn world(&self, entry: impl Into<PathBuf>) -> MockWorldBuilder;
}

impl MockWorkspaceWorldExt for MockWorkspace {
    fn entry_state(&self, entry: impl AsRef<Path>) -> FileResult<EntryState> {
        Ok(EntryState::new_rooted(
            self.root_path(),
            Some(VirtualPath::new(
                self.path(entry)
                    .strip_prefix(self.root())
                    .map_err(|_| typst::diag::FileError::AccessDenied)?,
            )),
        ))
    }

    fn world(&self, entry: impl Into<PathBuf>) -> MockWorldBuilder {
        MockWorldBuilder::new(self.clone(), entry)
    }
}

/// Applies VFS mock changes to world-level runtime structures.
pub trait MockWorldChangeExt {
    /// Applies this change to a compiler universe through the VFS revision path.
    fn apply_to_universe<F>(&self, universe: &mut CompilerUniverse<F>)
    where
        F: CompilerFeat;
}

impl MockWorldChangeExt for MockChange {
    fn apply_to_universe<F>(&self, universe: &mut CompilerUniverse<F>)
    where
        F: CompilerFeat,
    {
        universe.increment_revision(|universe| {
            universe.vfs().notify_fs_changes(self.changeset().clone());
        });
    }
}

/// Builder for mock-backed compiler worlds.
#[derive(Debug, Clone)]
pub struct MockWorldBuilder {
    workspace: MockWorkspace,
    entry: PathBuf,
    features: Features,
    inputs: Option<Arc<LazyHash<Dict>>>,
    font_resolver: Option<Arc<FontResolverImpl>>,
    creation_timestamp: Option<i64>,
}

impl MockWorldBuilder {
    /// Creates a mock world builder.
    pub fn new(workspace: MockWorkspace, entry: impl Into<PathBuf>) -> Self {
        Self {
            workspace,
            entry: entry.into(),
            features: Features::default(),
            inputs: None,
            font_resolver: None,
            creation_timestamp: None,
        }
    }

    /// Sets the Typst feature flags for the universe.
    pub fn with_features(mut self, features: Features) -> Self {
        self.features = features;
        self
    }

    /// Sets Typst input values for the universe.
    pub fn with_inputs(mut self, inputs: Dict) -> Self {
        self.inputs = Some(Arc::new(LazyHash::new(inputs)));
        self
    }

    /// Sets pre-hashed Typst input values for the universe.
    pub fn with_lazy_inputs(mut self, inputs: Arc<LazyHash<Dict>>) -> Self {
        self.inputs = Some(inputs);
        self
    }

    /// Sets the font resolver for the universe.
    pub fn with_font_resolver(mut self, resolver: Arc<FontResolverImpl>) -> Self {
        self.font_resolver = Some(resolver);
        self
    }

    /// Sets a deterministic creation timestamp for the universe.
    pub fn with_creation_timestamp(mut self, timestamp: Option<i64>) -> Self {
        self.creation_timestamp = timestamp;
        self
    }

    /// Builds a compiler universe.
    pub fn build_universe(&self) -> FileResult<MockUniverse> {
        let registry = Arc::new(DummyRegistry);
        let resolver: Arc<dyn RootResolver + Send + Sync> =
            Arc::new(RegistryPathMapper::new(registry.clone()));

        Ok(CompilerUniverse::new_raw(
            self.workspace.entry_state(&self.entry)?,
            self.features.clone(),
            self.inputs.clone(),
            Vfs::new(resolver, self.workspace.access_model()),
            registry,
            self.font_resolver
                .clone()
                .unwrap_or_else(embedded_font_resolver),
            self.creation_timestamp,
        ))
    }

    /// Builds a compiler world snapshot.
    pub fn build_world(&self) -> FileResult<MockWorld> {
        Ok(self.build_universe()?.snapshot())
    }
}

/// Returns a deterministic font resolver using Typst's embedded fonts.
pub fn embedded_font_resolver() -> Arc<FontResolverImpl> {
    static FONT_RESOLVER: LazyLock<Arc<FontResolverImpl>> = LazyLock::new(|| {
        let mut searcher = MemoryFontSearcher::new();
        for font in typst_assets::fonts() {
            searcher.add_memory_font(Bytes::new(font));
        }
        Arc::new(searcher.build())
    });

    FONT_RESOLVER.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_world_reads_follow_up_updates() {
        let workspace = MockWorkspace::default_builder()
            .file("main.typ", "#import \"content.typ\": value\n#value")
            .file("content.typ", "#let value = [before]")
            .build();
        let mut universe = workspace.world("main.typ").build_universe().unwrap();
        let content_path = workspace.path("content.typ");

        assert_eq!(
            universe
                .snapshot()
                .source_by_path(&content_path)
                .unwrap()
                .text(),
            "#let value = [before]"
        );

        workspace
            .update_source("content.typ", "#let value = [after]")
            .apply_to_universe(&mut universe);

        assert_eq!(
            universe
                .snapshot()
                .source_by_path(&content_path)
                .unwrap()
                .text(),
            "#let value = [after]"
        );
    }

    #[test]
    fn mock_world_handles_rename_remove_sequence() {
        let workspace = MockWorkspace::default_builder()
            .file("main.typ", "#import \"content.typ\": value\n#value")
            .file("content.typ", "#let value = [before]")
            .build();
        let mut universe = workspace.world("main.typ").build_universe().unwrap();
        let content_path = workspace.path("content.typ");
        let renamed_path = workspace.path("renamed.typ");
        let main_path = workspace.path("main.typ");

        workspace
            .rename("content.typ", "renamed.typ")
            .unwrap()
            .apply_to_universe(&mut universe);
        workspace
            .update_source("main.typ", "#import \"renamed.typ\": value\n#value")
            .apply_to_universe(&mut universe);

        let world = universe.snapshot();
        assert!(world.source_by_path(&content_path).is_err());
        assert_eq!(
            world.source_by_path(&renamed_path).unwrap().text(),
            "#let value = [before]"
        );

        workspace
            .remove("renamed.typ")
            .unwrap()
            .apply_to_universe(&mut universe);
        workspace
            .update_source("main.typ", "#let value = [inline]\n#value")
            .apply_to_universe(&mut universe);

        let world = universe.snapshot();
        assert!(world.source_by_path(&renamed_path).is_err());
        assert_eq!(
            world.source_by_path(&main_path).unwrap().text(),
            "#let value = [inline]\n#value"
        );
    }
}
