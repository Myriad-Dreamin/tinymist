use std::{path::Path, sync::Arc};

use ecow::EcoVec;
use reflexo_typst::ImmutPath;
use tinymist_std::path::unix_slash;
use typst::diag::EcoString;
use typst::World;

use crate::model::{Id, ProjectInput, ProjectRoute, ProjectTask, ResourcePath};
use crate::{LspWorld, ProjectPathMaterial};

/// Make a new project lock updater.
pub fn update_lock(root: ImmutPath) -> ProjectLockUpdater {
    ProjectLockUpdater {
        root,
        updates: vec![],
    }
}

enum LockUpdate {
    Input(ProjectInput),
    Task(ProjectTask),
    Material(ProjectPathMaterial),
    Route(ProjectRoute),
}

pub struct ProjectLockUpdater {
    root: Arc<Path>,
    updates: Vec<LockUpdate>,
}

impl ProjectLockUpdater {
    pub fn compiled(&mut self, world: &LspWorld) -> Option<Id> {
        let id = Id::from_world(world)?;

        let root = ResourcePath::from_user_sys(Path::new("."));
        let main = ResourcePath::from_user_sys(world.path_for_id(world.main()).ok()?.as_path());

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
            main: Some(main),
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

    pub fn update_materials(&mut self, doc_id: Id, files: EcoVec<ImmutPath>) {
        self.updates
            .push(LockUpdate::Material(ProjectPathMaterial::from_deps(
                doc_id, files,
            )));
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
            let root_hash = tinymist_std::hash::hash128(&root);
            for update in self.updates {
                match update {
                    LockUpdate::Input(input) => {
                        l.replace_document(input);
                    }
                    LockUpdate::Task(task) => {
                        l.replace_task(task);
                    }
                    LockUpdate::Material(mut mat) => {
                        let root: EcoString = unix_slash(&self.root).into();
                        mat.root = root.clone();
                        let cache_dir = dirs::cache_dir();
                        if let Some(cache_dir) = cache_dir {
                            let id = tinymist_std::hash::hash128(&mat.id);
                            let root_lo = root_hash & 0xfff;
                            let root_hi = root_hash >> 12;
                            let id_lo = id & 0xfff;
                            let id_hi = id >> 12;

                            let hash_str =
                                format!("{root_lo:03x}/{root_hi:013x}/{id_lo:03x}/{id_hi:016x}");

                            let cache_dir = cache_dir.join("tinymist/projects").join(hash_str);
                            let _ = std::fs::create_dir_all(&cache_dir);

                            let data = serde_json::to_string(&mat).unwrap();
                            let path = cache_dir.join("path-material.json");
                            let result = tinymist_std::fs::paths::write_atomic(path, data);
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
