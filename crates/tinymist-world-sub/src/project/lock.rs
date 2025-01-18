use std::{path::Path, sync::Arc};

use reflexo_typst::{path::unix_slash, EntryReader, TypstFileId};
use typst::diag::EcoString;

use super::model::{Id, ProjectInput, ProjectMaterial, ProjectRoute, ProjectTask, ResourcePath};
use crate::LspWorld;

/// Make a new project lock updater.
pub fn update_lock(world: &LspWorld) -> Option<ProjectLockUpdater> {
    let root = world.workspace_root()?;
    Some(ProjectLockUpdater {
        root,
        updates: vec![],
    })
}

enum LockUpdate {
    Input(ProjectInput),
    Task(ProjectTask),
    Material(ProjectMaterial),
    Route(ProjectRoute),
}

pub struct ProjectLockUpdater {
    root: Arc<Path>,
    updates: Vec<LockUpdate>,
}

impl ProjectLockUpdater {
    pub fn compiled(&mut self, world: &LspWorld) -> Option<Id> {
        let entry = world.entry_state();
        log::info!("ProjectCompiler: record compile for {entry:?}");
        // todo: correct root
        let root = entry.workspace_root()?;
        let id = unix_slash(entry.main()?.vpath().as_rootless_path());
        log::info!("ProjectCompiler: record compile for id {id} at {root:?}");

        let path = &ResourcePath::from_user_sys(Path::new(&id));
        let id: Id = path.into();

        let root = ResourcePath::from_user_sys(Path::new("."));

        let font_resolver = &world.font_resolver;
        let font_paths = font_resolver
            .font_paths()
            .iter()
            .map(|p| ResourcePath::from_user_sys(p))
            .collect::<Vec<_>>();

        // let system_font = font_resolver.system_font();

        let registry = &world.registry;
        let package_path = registry
            .package_path()
            .map(|p| ResourcePath::from_user_sys(p));
        let package_cache_path = registry
            .package_cache_path()
            .map(|p| ResourcePath::from_user_sys(p));

        // todo: freeze the package paths
        let _ = package_cache_path;
        let _ = package_path;

        let input = ProjectInput {
            id: id.clone(),
            root: Some(root),
            font_paths,
            system_fonts: true, // !args.font.ignore_system_fonts,
            package_path: None,
            package_cache_path: None,
        };

        self.updates.push(LockUpdate::Input(input));

        Some(id)
    }

    pub fn task(&mut self, task: ProjectTask) {
        self.updates.push(LockUpdate::Task(task));
    }

    pub fn update_materials(&mut self, doc_id: Id, ids: Vec<TypstFileId>) {
        let mut files = ids
            .into_iter()
            .map(ResourcePath::from_file_id)
            .collect::<Vec<_>>();
        files.sort();
        self.updates.push(LockUpdate::Material(ProjectMaterial {
            root: EcoString::default(),
            id: doc_id,
            files,
        }));
    }

    pub fn route(&mut self, doc_id: Id, priority: u32) {
        self.updates.push(LockUpdate::Route(ProjectRoute {
            id: doc_id,
            priority,
        }));
    }

    pub fn commit(self) {
        let err = super::LockFile::update(&self.root, |l| {
            let root: EcoString = unix_slash(&self.root).into();
            let root_hash = reflexo_typst::hash::hash128(&root);
            for update in self.updates {
                match update {
                    LockUpdate::Input(input) => {
                        l.replace_document(input);
                    }
                    LockUpdate::Task(task) => {
                        l.replace_task(task);
                    }
                    LockUpdate::Material(mut mat) => {
                        mat.root = root.clone();
                        let cache_dir = dirs::cache_dir();
                        if let Some(cache_dir) = cache_dir {
                            let id = reflexo_typst::hash::hash128(&mat.id);
                            let lower4096 = root_hash & 0xfff;
                            let upper4096 = root_hash >> 12;

                            // let hash_str = format!("{root:016x}/{id:016x}");
                            let hash_str = format!("{lower4096:03x}/{upper4096:013x}/{id:016x}");

                            let cache_dir = cache_dir.join("tinymist/projects").join(hash_str);
                            let _ = std::fs::create_dir_all(&cache_dir);

                            let data = serde_json::to_string(&mat).unwrap();
                            let path = cache_dir.join("material.json");
                            let result = tinymist_fs::paths::write_atomic(path, data);
                            if let Err(err) = result {
                                log::error!("ProjectCompiler: write material error: {err}");
                            }

                            // todo: clean up old cache
                        }
                        // l.replace_material(mat);
                    }
                    LockUpdate::Route(route) => {
                        l.replace_route(route);
                    }
                }
            }

            Ok(())
        });
        if let Err(err) = err {
            log::error!("ProjectCompiler: lock file error: {err}");
        }
    }
}
