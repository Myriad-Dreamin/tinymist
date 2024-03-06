use std::sync::Arc;

use once_cell::sync::OnceCell;
use tokio::sync::{Mutex, RwLock};
pub use tower_lsp::Client as LspHost;
use typst::model::Document;

use crate::actor::typst::CompileCluster;
use crate::config::{Config, ConstConfig};

pub struct TypstServer {
    pub client: LspHost,
    pub document: Mutex<Arc<Document>>,
    // typst_thread: TypstThread,
    pub universe: OnceCell<CompileCluster>,
    pub config: Arc<RwLock<Config>>,
    pub const_config: OnceCell<ConstConfig>,
}

impl TypstServer {
    pub fn new(client: LspHost) -> Self {
        Self {
            // typst_thread: Default::default(),
            universe: Default::default(),
            config: Default::default(),
            const_config: Default::default(),
            client,
            document: Default::default(),
        }
    }

    pub fn const_config(&self) -> &ConstConfig {
        self.const_config
            .get()
            .expect("const config should be initialized")
    }

    pub fn universe(&self) -> &CompileCluster {
        self.universe.get().expect("universe should be initialized")
    }

    // pub fn typst_global_scopes(&self) -> typst::foundations::Scopes {
    //     typst::foundations::Scopes::new(Some(&TYPST_STDLIB))
    // }

    // pub async fn register_workspace_files(&self) -> FsResult<()> {
    //     let mut workspace = self.workspace().write().await;
    //     workspace.register_files()
    // }

    // async fn read_workspace(&self) -> RwLockReadGuard<Workspace> {
    //     self.workspace().read().await
    // }

    // async fn read_workspace_owned(&self) -> OwnedRwLockReadGuard<Workspace> {
    //     Arc::clone(self.workspace()).read_owned().await
    // }

    // pub async fn project_and_full_id(&self, uri: &Url) -> FsResult<(Project,
    // FullFileId)> {     let workspace = self.read_workspace_owned().await;
    //     let full_id = workspace.full_id(uri)?;
    //     let project = Project::new(full_id.package(), workspace);
    //     Ok((project, full_id))
    // }

    // pub async fn scope_with_source(&self, uri: &Url) -> FsResult<SourceScope> {
    //     let (project, _) = self.project_and_full_id(uri).await?;
    //     let source = project.read_source_by_uri(uri)?;
    //     Ok(SourceScope { project, source })
    // }

    // pub async fn thread_with_world(
    //     &self,
    //     builder: impl Into<WorldBuilder<'_>>,
    // ) -> FsResult<WorldThread> {
    //     let (main, project) =
    // builder.into().main_project(self.workspace()).await?;

    //     Ok(WorldThread {
    //         main,
    //         main_project: project,
    //         typst_thread: &self.typst_thread,
    //     })
    // }

    // /// Run the given function on the Typst thread, passing back its return
    // /// value.
    // pub async fn typst<T: Send + 'static>(
    //     &self,
    //     f: impl FnOnce(runtime::Handle) -> T + Send + 'static,
    // ) -> T {
    //     self.typst_thread.run(f).await
    // }
}

// pub struct SourceScope {
//     source: Source,
//     project: Project,
// }

// impl SourceScope {
//     pub fn run<T>(self, f: impl FnOnce(&Source, &Project) -> T) -> T {
//         f(&self.source, &self.project)
//     }

//     pub fn run2<T>(self, f: impl FnOnce(Source, Project) -> T) -> T {
//         f(self.source, self.project)
//     }
// }

// pub struct WorldThread<'a> {
//     main: Source,
//     main_project: Project,
//     typst_thread: &'a TypstThread,
// }

// impl<'a> WorldThread<'a> {
//     pub async fn run<T: Send + 'static>(
//         self,
//         f: impl FnOnce(ProjectWorld) -> T + Send + 'static,
//     ) -> T {
//         self.typst_thread
//             .run_with_world(self.main_project, self.main, f)
//             .await
//     }
// }

// pub enum WorldBuilder<'a> {
//     MainUri(&'a Url),
//     MainAndProject(Source, Project),
// }

// impl<'a> WorldBuilder<'a> {
//     async fn main_project(self, workspace: &Arc<RwLock<Workspace>>) ->
// FsResult<(Source, Project)> {         match self {
//             Self::MainUri(uri) => {
//                 let workspace = Arc::clone(workspace).read_owned().await;
//                 let full_id = workspace.full_id(uri)?;
//                 let source = workspace.read_source(uri)?;
//                 let project = Project::new(full_id.package(), workspace);
//                 Ok((source, project))
//             }
//             Self::MainAndProject(main, project) => Ok((main, project)),
//         }
//     }
// }

// impl<'a> From<&'a Url> for WorldBuilder<'a> {
//     fn from(uri: &'a Url) -> Self {
//         Self::MainUri(uri)
//     }
// }

// impl From<(Source, Project)> for WorldBuilder<'static> {
//     fn from((main, project): (Source, Project)) -> Self {
//         Self::MainAndProject(main, project)
//     }
// }
