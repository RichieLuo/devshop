use eframe::egui::{self, IconData, TextureHandle};
use egui::emath::GuiRounding;
use std::sync::OnceLock;

#[path = "branding_icon.rs"]
mod branding_icon;

pub fn icon_data() -> IconData {
    let image = branding_icon::resize_icon(icon_source(), 256);
    IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    }
}

fn icon_source() -> &'static image::RgbaImage {
    static SOURCE: OnceLock<image::RgbaImage> = OnceLock::new();
    SOURCE.get_or_init(branding_icon::icon_image)
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub enum IconShape {
    RoundedSquare,
    Circle,
}

pub fn texture(ctx: &egui::Context, points: f32, shape: IconShape) -> TextureHandle {
    let size = (points * ctx.pixels_per_point()).round().clamp(16.0, 512.0) as u32;
    let id = egui::Id::new(("devshop-icon-pixels", size, shape));
    if let Some(texture) = ctx.data(|data| data.get_temp::<TextureHandle>(id)) {
        return texture;
    }
    let image = if shape == IconShape::Circle {
        let mut source = icon_source().clone();
        let radius = source.width() as f32 / 2.0;
        for (x, y, pixel) in source.enumerate_pixels_mut() {
            let distance = (x as f32 + 0.5 - radius).hypot(y as f32 + 0.5 - radius);
            let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0);
            pixel[3] = (f32::from(pixel[3]) * coverage).round() as u8;
        }
        branding_icon::resize_icon(&source, size)
    } else {
        branding_icon::resize_icon(icon_source(), size)
    };
    let color_image = egui::ColorImage::from_rgba_unmultiplied([size as usize; 2], image.as_raw());
    let texture = ctx.load_texture(
        format!("devshop-icon-{size}-{}", shape == IconShape::Circle),
        color_image,
        egui::TextureOptions::LINEAR.with_mipmap_mode(Some(egui::TextureFilter::Linear)),
    );
    ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
    texture
}

pub fn show_icon(ui: &mut egui::Ui, points: f32) -> egui::Response {
    let texture = texture(ui.ctx(), points, IconShape::RoundedSquare);
    let (rect, response) = ui.allocate_exact_size(egui::Vec2::splat(points), egui::Sense::hover());
    let rect = rect.round_to_pixels(ui.pixels_per_point());
    egui::Image::new(&texture).paint_at(ui, rect);
    response
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "DevShop-CJK".into(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/NotoSansCJKsc-Regular.otf"
        ))),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .get_mut(&family)
            .unwrap()
            .push("DevShop-CJK".into());
    }
    ctx.set_fonts(fonts);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_icons_follow_viewport_scale_and_reuse_the_matching_texture() {
        let ctx = egui::Context::default();
        for scale in [1.0, 1.25, 1.5, 2.0] {
            ctx.set_pixels_per_point(scale);
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                for points in [28.0, 48.0, 74.0] {
                    let icon = texture(ctx, points, IconShape::RoundedSquare);
                    assert_eq!(icon.size(), [(points * scale).round() as usize; 2]);
                    assert_eq!(
                        icon.id(),
                        texture(ctx, points, IconShape::RoundedSquare).id()
                    );
                }
            });
        }
        let circle = texture(&ctx, 48.0, IconShape::Circle);
        assert_ne!(
            circle.id(),
            texture(&ctx, 48.0, IconShape::RoundedSquare).id()
        );
    }
}
