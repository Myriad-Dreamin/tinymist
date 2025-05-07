use std::{path::Path, sync::Arc};

use reflexo_typst::{path::unix_slash, typst::prelude::EcoVec, LazyHash};
use rpds::RedBlackTreeMapSync;
use tinymist_std::{hash::FxHashMap, ImmutPath};
use typst::diag::EcoString;

use crate::project::{Id, LockFile, LspCompileSnapshot, ProjectPathMaterial, ProjectRoute};

#[derive(Default)]
pub struct ProjectRouteState {
    path_routes: FxHashMap<ImmutPath, RoutePathState>,
}

pub struct ProjectResolution {
    pub lock_dir: ImmutPath,
    pub project_id: Id,
}

impl ProjectRouteState {
    pub fn locate(&self, resolved: &ProjectResolution) -> Option<Arc<LockFile>> {
        let path_route = self.path_routes.get(&resolved.lock_dir)?;
        Some(path_route.lock.clone())
    }

    pub fn resolve(&mut self, leaf: &ImmutPath) -> Option<ProjectResolution> {
        for path in std::iter::successors(Some(leaf.as_ref()), |p| p.parent()) {
            if let Some(resolution) = self.resolve_at(path, leaf) {
                return Some(resolution);
            }
        }

        None
    }

    fn resolve_at(&mut self, lock_dir: &Path, leaf: &Path) -> Option<ProjectResolution> {
        log::debug!("resolve: {leaf:?} at {lock_dir:?}");
        let (lock_dir, project_id) = match self.path_routes.get_key_value(lock_dir) {
            Some((key, path_route)) => (key.clone(), path_route.routes.get(leaf)?.clone()),
            None => {
                let lock_dir: ImmutPath = lock_dir.into();
                let mut new_route = self.load_lock(&lock_dir).unwrap_or_default();

                let mut materials = RedBlackTreeMapSync::default();

                if let Some(cache_dir) = new_route.cache_dir.as_ref() {
                    let entries = walkdir::WalkDir::new(cache_dir)
                        .into_iter()
                        .filter_map(|entry| entry.ok())
                        .filter(|entry| entry.file_type().is_file());

                    for entry in entries {
                        let material = self.read_material(entry.path());
                        if let Some(material) = material {
                            let id = material.id.clone();
                            materials.insert_mut(id.clone(), material);
                        }
                    }
                }
                let materials = LazyHash::new(materials);
                new_route.routes = calculate_routes(new_route.lock.route.clone(), &materials);
                new_route.materials = materials;

                log::debug!("loaded routes at {lock_dir:?}, {:?}", new_route.routes);
                let project_id = new_route.routes.get(leaf)?.clone();

                self.path_routes.insert(lock_dir.clone(), new_route);
                (lock_dir, project_id)
            }
        };

        Some(ProjectResolution {
            lock_dir,
            project_id,
        })
    }

    pub fn update_lock(&mut self, lock_dir: ImmutPath, lock: LockFile) -> Option<()> {
        let path_route = self.path_routes.get_mut(&lock_dir)?;

        let lock_unchanged = path_route.lock.as_ref() == &lock;
        if lock_unchanged {
            return Some(());
        }

        path_route.lock = Arc::new(lock);
        path_route.routes = calculate_routes(path_route.lock.route.clone(), &path_route.materials);

        Some(())
    }

    pub fn update_existing_material(
        &mut self,
        lock_dir: ImmutPath,
        snap: &LspCompileSnapshot,
    ) -> Option<()> {
        let path_route = self.path_routes.get_mut(&lock_dir)?;

        let id = Id::from_world(&snap.world)?;
        let deps = snap.world.depended_fs_paths();
        let material = ProjectPathMaterial::from_deps(id, deps);

        let old = path_route.materials.get_mut(&material.id)?;
        if old == &material {
            return Some(());
        }

        path_route
            .materials
            .insert_mut(material.id.clone(), material);
        path_route.routes = calculate_routes(path_route.lock.route.clone(), &path_route.materials);

        Some(())
    }

    fn load_lock(&self, path: &Path) -> Option<RoutePathState> {
        let lock_data = Arc::new(match LockFile::read(path) {
            Ok(lock) => lock,
            Err(e) => {
                log::debug!("failed to load lock at {path:?}: {e:?}");
                return None;
            }
        });
        log::info!("loaded lock at {path:?}");

        let root: EcoString = unix_slash(path).into();
        let root_hash = tinymist_std::hash::hash128(&root);
        let cache_dir_base = dirs::cache_dir();
        let mut cache_dir = None;
        if let Some(cache_dir_base) = cache_dir_base {
            let root_lo = root_hash & 0xfff;
            let root_hi = root_hash >> 12;

            // let hash_str = format!("{root:016x}/{id:016x}");
            let project_state = format!("{root_lo:03x}/{root_hi:013x}");

            cache_dir = Some(
                cache_dir_base
                    .join("tinymist/projects")
                    .join(project_state)
                    .into(),
            );
        }

        Some(RoutePathState {
            lock: lock_data,
            materials: LazyHash::default(),
            routes: Arc::new(FxHashMap::default()),
            cache_dir,
        })
    }

    fn read_material(&self, entry_path: &Path) -> Option<ProjectPathMaterial> {
        log::info!("check material at {entry_path:?}");
        let name = entry_path.file_name().unwrap_or(entry_path.as_os_str());
        if name != "path-material.json" {
            return None;
        }

        let data = std::fs::read(entry_path).ok()?;

        let material = serde_json::from_slice::<ProjectPathMaterial>(&data).ok()?;
        Some(material)
    }
}

#[comemo::memoize]
fn calculate_routes(
    raw_routes: EcoVec<ProjectRoute>,
    materials: &LazyHash<rpds::RedBlackTreeMapSync<Id, ProjectPathMaterial>>,
) -> Arc<FxHashMap<ImmutPath, Id>> {
    let mut routes = FxHashMap::default();

    let mut priorities = FxHashMap::default();

    for route in raw_routes.iter() {
        if let Some(material) = materials.get(&route.id) {
            for file in material.files.iter() {
                routes.insert(file.as_path().into(), route.id.clone());
            }
        }

        priorities.insert(route.id.clone(), route.priority);
    }

    Arc::new(routes)
}

#[derive(Default)]
struct RoutePathState {
    lock: Arc<LockFile>,
    materials: LazyHash<rpds::RedBlackTreeMapSync<Id, ProjectPathMaterial>>,
    routes: Arc<FxHashMap<ImmutPath, Id>>,
    cache_dir: Option<ImmutPath>,
}
