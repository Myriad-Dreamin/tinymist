use std::sync::Arc;

use typst_library::layout::{Size, Transform};
use typst_library::visualize as viz;
use vello::{kurbo, peniko};
use vello_svg::usvg;

use crate::utils::convert_transform;

#[derive(Default, Clone)]
pub struct ImageScene {
    pub transform: kurbo::Affine,
    pub scene: vello::Scene,
}

impl ImageScene {
    pub fn render(&self, scene: &mut vello::Scene) {
        scene.append(&self.scene, Some(self.transform));
    }
}

pub fn render_image(image: &viz::Image, size: Size, local_transform: Transform) -> ImageScene {
    // Size cannot be 0.
    debug_assert!(size.all(|p| p.to_pt() != 0.0));

    // TODO: The plan is to load it using bevy assets!

    match image.kind() {
        viz::ImageKind::Raster(raster) => {
            let mut scene = vello::Scene::new();

            let width = raster.width();
            let height = raster.height();

            let image_data = peniko::ImageData {
                data: peniko::Blob::new(Arc::new(raster.dynamic().to_rgba8().into_vec())),
                format: peniko::ImageFormat::Rgba8,
                alpha_type: peniko::ImageAlphaType::Alpha,
                width,
                height,
            };

            let brush = peniko::ImageBrush::new(image_data);
            scene.draw_image(&brush, kurbo::Affine::IDENTITY);

            let transform = convert_transform(local_transform).pre_scale_non_uniform(
                size.x.to_pt() / width as f64,
                size.y.to_pt() / height as f64,
            );

            ImageScene { transform, scene }
        }
        // TODO: Support paths in svg for animation.. (maybe a SvgScene?)
        viz::ImageKind::Svg(svg) => {
            let transform = convert_transform(local_transform)
                .pre_scale_non_uniform(size.x.to_pt() / svg.width(), size.y.to_pt() / svg.height());

            // FIXME: This is needed because the svg versions are different.
            let scene =
                match usvg::Tree::from_data(svg.data().as_slice(), &usvg::Options::default()) {
                    Ok(tree) => vello_svg::render_tree(&tree),
                    _ => vello::Scene::new(),
                };

            ImageScene { transform, scene }
        }
        viz::ImageKind::Pdf(pdf_img) => {
            let page = match pdf_img.document().pdf().pages().get(pdf_img.page_index()) {
                Some(p) => p,
                None => return ImageScene::default(),
            };

            let mut scene = vello::Scene::new();

            let target_px_w = (size.x.to_pt().ceil() as u32).max(1);
            let target_px_h = (size.y.to_pt().ceil() as u32).max(1);

            let settings = hayro::RenderSettings {
                width: Some(target_px_w.try_into().unwrap_or(u16::MAX)),
                height: Some(target_px_h.try_into().unwrap_or(u16::MAX)),
                ..Default::default()
            };

            let pixmap = hayro::render(page, &hayro::InterpreterSettings::default(), &settings);
            let rgba = pixmap.clone().take_u8();

            let image_data = peniko::ImageData {
                data: peniko::Blob::new(Arc::new(rgba)),
                format: peniko::ImageFormat::Rgba8,
                alpha_type: peniko::ImageAlphaType::Alpha,
                width: pixmap.width().into(),
                height: pixmap.height().into(),
            };

            let brush = peniko::ImageBrush::new(image_data);
            scene.draw_image(&brush, kurbo::Affine::IDENTITY);

            let (page_w, page_h) = page.render_dimensions();
            let transform = convert_transform(local_transform).pre_scale_non_uniform(
                size.x.to_pt() / page_w as f64,
                size.y.to_pt() / page_h as f64,
            );

            ImageScene { transform, scene }
        }
    }
}

impl std::fmt::Debug for ImageScene {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FixedScene")
            .field("transform", &self.transform)
            .finish()
    }
}
