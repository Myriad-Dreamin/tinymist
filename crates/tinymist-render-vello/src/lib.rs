//! # Typst Vello
//!
//! A Vello scene drawer for Typst's frames.
//!
//! Some code are borrowed from [velyst](https://github.com/voxell-tech/velyst).

use std::collections::HashMap;

use image::{ImageScene, render_image};
use shape::{ShapeScene, convert_curve, render_shape};
use smallvec::SmallVec;
use text::{TextScene, render_text};
use typst_library::foundations::Label;
use typst_library::layout::{Frame, FrameItem, FrameKind, GroupItem, Point, Size, Transform};
use utils::convert_transform;
use vello::kurbo;

pub mod image;
pub mod shape;
pub mod text;
pub mod utils;

/// Every group is layouted in a flat list.
/// Each group will have a parent index associated with it.
#[derive(Default)]
pub struct TypstScene {
    size: kurbo::Vec2,
    group_scenes: Vec<TypstGroupScene>,
    group_map: HashMap<Label, SmallVec<[usize; 1]>>,
}

impl TypstScene {
    pub fn from_frame(frame: &Frame) -> Self {
        let size = kurbo::Vec2::new(frame.size().x.to_pt(), frame.size().y.to_pt());
        let mut typst_scene = TypstScene {
            size,
            ..Default::default()
        };

        let group_paths = TypstGroup {
            size,
            ..Default::default()
        };
        typst_scene.append_group(group_paths);
        typst_scene.handle_frame(
            frame,
            RenderState::new(frame.size(), Transform::identity()),
            0,
        );

        typst_scene
    }

    /// Render [`TypstScene`] into a [`vello::Scene`].
    pub fn render(&mut self) -> vello::Scene {
        let mut scene = vello::Scene::new();
        let mut computed_transforms = Vec::with_capacity(self.group_scenes.len());

        for group_scene in self.group_scenes.iter_mut() {
            let group = &mut group_scene.group;

            // Calculate accumulated transform from the group hierarchy.
            let transform = match group.parent {
                Some(parent_index) => {
                    let transform = computed_transforms[parent_index] * group.transform;
                    computed_transforms.push(transform);
                    transform
                }
                None => {
                    computed_transforms.push(group.transform);
                    group.transform
                }
            };
            let transform = (transform != kurbo::Affine::IDENTITY).then_some(transform);

            let mut pushed_clip = false;
            if let Some(clip_path) = &group.clip_path {
                scene.push_clip_layer(group.transform, clip_path);

                pushed_clip = true;
            }

            if group_scene.updated {
                // Use the rendered group scene.
                scene.append(&group_scene.scene, transform);
            } else {
                let new_scene = group.render();
                // Scene needs to be re-rendered if it's not updated.
                scene.append(&new_scene, transform);
                // Update group scene to the newly rendered scene.
                group_scene.scene = new_scene;
            }

            if pushed_clip {
                scene.pop_layer();
            }

            // Flag the current group scene as updated.
            group_scene.updated = true;
        }

        scene
    }

    /// Populate [`TypstGroup`] with items inside the [`Frame`] and recursively
    /// populate the [`TypstScene`] itself if the frame contains any groups.
    fn handle_frame(&mut self, frame: &Frame, state: RenderState, group_index: usize) {
        for (pos, item) in frame.items() {
            let pos = *pos;
            let local_transform = Transform::translate(pos.x, pos.y);

            match item {
                FrameItem::Group(group) => {
                    self.handle_group(
                        group,
                        state.pre_translate(pos),
                        local_transform,
                        Some(group_index),
                    );
                }
                FrameItem::Text(text) => {
                    let scenes = &mut self.get_group_mut(group_index).scenes;
                    scenes.push(SceneKind::Text(render_text(
                        text,
                        state.pre_translate(pos),
                        (!local_transform.is_identity()).then_some(local_transform),
                    )));
                }
                FrameItem::Shape(shape, _) => {
                    let scenes = &mut self.get_group_mut(group_index).scenes;
                    scenes.push(SceneKind::Shape(render_shape(
                        shape,
                        state.pre_translate(pos),
                        local_transform,
                    )));
                }
                FrameItem::Image(image, size, _) => {
                    if size.any(|p| p.to_pt() == 0.0) {
                        // Image size invalid!
                        continue;
                    }

                    let scenes = &mut self.get_group_mut(group_index).scenes;
                    scenes.push(SceneKind::Image(render_image(
                        image,
                        *size,
                        local_transform,
                    )));
                }
                // TODO: Support links
                FrameItem::Link(_, _) => {}
                FrameItem::Tag(_) => {}
            }
        }
    }

    /// Convert [`GroupItem`] into [`TypstGroup`] and append it.
    fn handle_group(
        &mut self,
        group: &GroupItem,
        state: RenderState,
        local_transform: Transform,
        parent: Option<usize>,
    ) {
        // Generate TypstGroup for the underlying frame.
        let group_paths = TypstGroup {
            size: kurbo::Vec2::new(group.frame.size().x.to_pt(), group.frame.size().y.to_pt()),
            transform: convert_transform(local_transform.pre_concat(group.transform)),
            parent,
            clip_path: group.clip.as_ref().map(convert_curve),
            label: group.label,
            ..Default::default()
        };

        // Update state based on group frame.
        let state = match group.frame.kind() {
            FrameKind::Soft => state.pre_concat(group.transform),
            FrameKind::Hard => state
                .with_transform(Transform::identity())
                .with_size(group.frame.size()),
        };

        let group_index = self.group_scenes.len();
        self.append_group(group_paths);
        self.handle_frame(&group.frame, state, group_index);
    }

    /// Add a group to the [group list][Self::group_scenes].
    fn append_group(&mut self, group: TypstGroup) {
        if let Some(label) = group.label {
            let index = self.group_scenes.len();
            match self.group_map.get_mut(&label) {
                Some(map) => {
                    map.push(index);
                }
                None => {
                    self.group_map.insert(label, SmallVec::from_buf([index]));
                }
            }
        }
        self.group_scenes.push(TypstGroupScene::new(group));
    }
}

impl TypstScene {
    pub fn query(&self, label: Label) -> SmallVec<[usize; 1]> {
        self.group_map[&label].clone()
    }

    pub fn get_group(&mut self, index: usize) -> &TypstGroup {
        &self.group_scenes[index].group
    }

    pub fn get_group_mut(&mut self, index: usize) -> &mut TypstGroup {
        self.group_scenes[index].updated = false;
        &mut self.group_scenes[index].group
    }

    pub fn iter_groups(&self) -> impl Iterator<Item = &TypstGroup> {
        self.group_scenes
            .iter()
            .map(|group_scene| &group_scene.group)
    }

    pub fn iter_groups_mut(&mut self) -> impl Iterator<Item = &mut TypstGroup> {
        self.group_scenes
            .iter_mut()
            .map(|group_scene| &mut group_scene.group)
    }

    /// Number of groups in the scene.
    pub fn groups_len(&self) -> usize {
        self.group_scenes.len()
    }

    /// Width and height of the entire scene.
    pub fn size(&self) -> kurbo::Vec2 {
        self.size
    }
}

#[derive(Default)]
pub struct TypstGroupScene {
    group: TypstGroup,
    scene: vello::Scene,
    updated: bool,
}

impl TypstGroupScene {
    pub fn new(group: TypstGroup) -> Self {
        Self {
            group,
            ..Default::default()
        }
    }
}

impl std::fmt::Debug for TypstGroupScene {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypstGroupScene")
            .field("group", &self.group)
            .field("updated", &self.updated)
            .finish()
    }
}

#[derive(Default, Debug)]
pub struct TypstGroup {
    size: kurbo::Vec2,
    transform: kurbo::Affine,
    scenes: Vec<SceneKind>,
    parent: Option<usize>,
    clip_path: Option<kurbo::BezPath>,
    label: Option<Label>,
}

impl TypstGroup {
    /// Create [`TypstGroup`] from a single [`SceneKind`].
    pub fn from_scene(scene: SceneKind, parent: Option<usize>) -> Self {
        Self {
            scenes: vec![scene],
            parent,
            ..Default::default()
        }
    }

    pub fn render(&self) -> vello::Scene {
        let mut scene = vello::Scene::new();

        for shape in self.scenes.iter() {
            shape.render(&mut scene);
        }

        scene
    }

    pub fn size(&self) -> kurbo::Vec2 {
        self.size
    }

    pub fn transform(&self) -> kurbo::Affine {
        self.transform
    }

    pub fn parent(&self) -> Option<usize> {
        self.parent
    }

    pub fn clip_path(&self) -> Option<&kurbo::BezPath> {
        self.clip_path.as_ref()
    }

    pub fn label(&self) -> Option<Label> {
        self.label
    }
}

#[derive(Debug, Clone)]
pub enum SceneKind {
    Shape(ShapeScene),
    Text(TextScene),
    Image(ImageScene),
}

impl SceneKind {
    pub fn render(&self, scene: &mut vello::Scene) {
        match self {
            SceneKind::Shape(shape) => shape.render(scene),
            SceneKind::Text(text) => text.render(scene),
            SceneKind::Image(image) => image.render(scene),
        };
    }
}

/// Contextual information for rendering.
#[derive(Default, Debug, Clone, Copy)]
pub struct RenderState {
    /// The transform of the current item.
    transform: Transform,
    /// The size of the first hard frame in the hierarchy.
    size: Size,
}

impl RenderState {
    pub fn new(size: Size, transform: Transform) -> Self {
        Self { size, transform }
    }

    /// Pre translate the current item's transform.
    pub fn pre_translate(self, pos: Point) -> Self {
        self.pre_concat(Transform::translate(pos.x, pos.y))
    }

    /// Pre concat the current item's transform.
    pub fn pre_concat(self, transform: Transform) -> Self {
        Self {
            transform: self.transform.pre_concat(transform),
            ..self
        }
    }

    /// Sets the size of the first hard frame in the hierarchy.
    pub fn with_size(self, size: Size) -> Self {
        Self { size, ..self }
    }

    /// Sets the current item's transform.
    pub fn with_transform(self, transform: Transform) -> Self {
        Self { transform, ..self }
    }
}
