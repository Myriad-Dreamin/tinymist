use std::{path::Path, sync::Arc};

use reflexo::{hash::FxHashMap, ImmutPath};
use tinymist_project::Id;

#[derive(Default)]
pub struct ProjectRouteState {
    path_routes: FxHashMap<ImmutPath, RoutePathState>,
}

pub struct ProjectResolution {
    pub lock_dir: ImmutPath,
    pub project_id: Id,
}

impl ProjectRouteState {
    pub fn resolve(&self, path: &ImmutPath) -> Option<ProjectResolution> {
        for path in std::iter::successors(Some(path.as_ref()), |p| p.parent()) {
            if let Some(resolution) = self.resolve_at(path) {
                return Some(resolution);
            }
        }

        None
    }

    fn resolve_at(&self, path: &Path) -> Option<ProjectResolution> {
        let (key, path_route) = self.path_routes.get_key_value(path)?;
        let project_id = path_route.routes.get(path)?;

        Some(ProjectResolution {
            lock_dir: key.clone(),
            project_id: project_id.clone(),
        })
    }
}

struct RoutePathState {
    routes: Arc<FxHashMap<ImmutPath, Id>>,
}
