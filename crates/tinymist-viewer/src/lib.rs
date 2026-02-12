//! # Typst Vello
//!
//! A Vello scene drawer for Typst's frames.
//!
//! Some code are borrowed from [velyst](https://github.com/voxell-tech/velyst).

use std::fmt;

use ecow::EcoVec;
use reflexo::hash::Fingerprint;
use std::sync::Arc;
use vello::kurbo::{self, Affine};
use vello::peniko;

pub mod doc;
pub mod incr;
mod render;

/// A vello scene corresponding to a typst page.
#[derive(Debug, Clone)]
pub struct VecPage {
    size: kurbo::Vec2,
    elem: Arc<VecScene>,
    content_hash: Fingerprint,
}

/// A scene that can be rendered to a [`vello::Scene`].
#[derive(Clone)]
pub enum VecScene {
    /// A group of scenes that are rendered together.
    Group(GroupScene),
    /// A path that is rendered to a scene.
    Path(kurbo::BezPath, peniko::Color),
    /// A scene that is rendered to a scene.
    Scene(Box<vello::Scene>, Option<kurbo::Affine>),
}

impl fmt::Debug for VecScene {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VecScene::Group(..) => write!(f, "GroupScene"),
            VecScene::Path(..) => write!(f, "Path"),
            VecScene::Scene(..) => write!(f, "Scene"),
        }
    }
}

impl VecScene {
    /// Renders the scene to a [`vello::Scene`].
    pub fn render(&self, scene: &mut vello::Scene) {
        match self {
            VecScene::Group(group) => group.render(scene),
            VecScene::Path(path, color) => {
                scene.fill(
                    peniko::Fill::NonZero,
                    Affine::IDENTITY,
                    &peniko::Brush::Solid(*color),
                    None,
                    path,
                );
            }
            VecScene::Scene(sub, transform) => scene.append(sub, *transform),
        }
    }
}

/// A group of scenes that are rendered together.
#[derive(Debug, Clone)]
pub struct GroupScene {
    clip: Option<kurbo::BezPath>,
    ts: Affine,
    scenes: EcoVec<(kurbo::Vec2, Arc<VecScene>)>,
}

impl GroupScene {
    /// Renders the group to a [`vello::Scene`].
    pub fn render(&self, scene: &mut vello::Scene) {
        if let Some(clip) = &self.clip {
            scene.push_clip_layer(peniko::Fill::NonZero, self.ts, clip);
        }
        let ts = self.ts;
        for (pos, elem) in self.scenes.iter() {
            let ts = ts.pre_translate(*pos);
            let mut sub_scene = vello::Scene::new();
            elem.render(&mut sub_scene);
            scene.append(&sub_scene, Some(ts));
        }
        if self.clip.is_some() {
            scene.pop_layer();
        }
    }
}
