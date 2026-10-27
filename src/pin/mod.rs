use egui::{
    Color32, ColorImage, CursorIcon, Frame, Id, Pos2, Rect, Sense, TextureHandle, TextureOptions,
};
use image::RgbaImage;
use winit::window::WindowId;

/// 贴图窗口的状态：原图 + 懒加载纹理。
pub struct PinState {
    pub image: RgbaImage,
    pub texture: Option<TextureHandle>,
}

impl PinState {
    pub fn new(image: RgbaImage) -> Self {
        Self {
            image,
            texture: None,
        }
    }
}

#[derive(Default)]
pub struct PinResponse {
    pub close: bool,
    pub drag: bool,
}

/// 绘制贴图窗口：左键拖动移动窗口，右键或 Esc 关闭。
pub fn draw(ctx: &egui::Context, state: &mut PinState, id: WindowId) -> PinResponse {
    let mut out = PinResponse::default();

    if state.texture.is_none() {
        let img = ColorImage::from_rgba_unmultiplied(
            [state.image.width() as usize, state.image.height() as usize],
            state.image.as_raw(),
        );
        state.texture =
            Some(ctx.load_texture(format!("fshot-pin-{id:?}"), img, TextureOptions::NEAREST));
    }
    let Some(tex_id) = state.texture.as_ref().map(|t| t.id()) else {
        return out;
    };

    egui::CentralPanel::default()
        .frame(Frame::none())
        .show(ctx, |ui| {
            let rect = ui.max_rect();
            let response = ui.interact(rect, Id::new(("fshot-pin", id)), Sense::click_and_drag());

            ui.painter().image(
                tex_id,
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0_f32, 1.0_f32)),
                Color32::WHITE,
            );
            ui.painter().rect_stroke(
                rect,
                0.0_f32,
                egui::Stroke::new(1.0_f32, Color32::from_rgb(0, 170, 255)),
            );

            if response.drag_started() {
                out.drag = true;
            }
            if response.secondary_clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                out.close = true;
            }
            if response.hovered() {
                ctx.set_cursor_icon(if response.dragged() {
                    CursorIcon::Grabbing
                } else {
                    CursorIcon::Grab
                });
            }
        });

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{RawInput, Rect, Vec2};
    use image::{Rgba, RgbaImage};
    use winit::window::WindowId;

    #[test]
    fn draw_loads_texture_and_paints_image() {
        let ctx = egui::Context::default();
        let mut state = PinState::new(RgbaImage::from_pixel(4, 3, Rgba([10, 20, 30, 255])));
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(4.0, 3.0))),
            ..Default::default()
        };
        let out = ctx.run(input, |c| {
            let resp = draw(c, &mut state, WindowId::dummy());
            assert!(!resp.close);
        });
        assert!(state.texture.is_some());
        assert!(!out.shapes.is_empty());
    }
}
