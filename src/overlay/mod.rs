pub mod selection;
pub mod toolbar;

use egui::{
    Color32, ColorImage, CursorIcon, LayerId, Order, Pos2, Rect, TextureHandle, TextureOptions, Vec2,
};
use image::{Rgba, RgbaImage};

use crate::annotate::history::History;
use crate::annotate::render;
use crate::annotate::tool::ToolState;
use crate::capture::screen::CapturedFrame;
use crate::config::ToolPrefs;
use selection::{Handle, Selection};

pub struct OverlayState {
    pub frame: CapturedFrame,
    pub texture: Option<TextureHandle>,
    pub mosaic_texture: Option<TextureHandle>,
    /// 当前马赛克纹理使用的物理块大小（像素），用于检测改动并重建
    pub mosaic_block_px: u32,
    pub selection: Selection,
    pub hovering: Option<Pos2>,
    pub screen_size: Vec2,
    pub focused_once: bool,
    pub history: History,
    pub tool: ToolState,
    pub toolbar_rect: Rect,
    pub selected_id: Option<u64>,
    pub drag_annotation: Option<(u64, Pos2)>,
    /// 本次拖动标注是否已经记录撤销快照
    pub drag_dirty: bool,
}

impl OverlayState {
    pub fn new(frame: CapturedFrame, prefs: &ToolPrefs) -> Self {
        Self {
            frame,
            texture: None,
            mosaic_texture: None,
            mosaic_block_px: 0,
            selection: Selection::new(),
            hovering: None,
            screen_size: Vec2::ZERO,
            focused_once: false,
            history: History::new(),
            tool: ToolState::from_prefs(prefs),
            toolbar_rect: Rect::NOTHING,
            selected_id: None,
            drag_annotation: None,
            drag_dirty: false,
        }
    }
}

pub fn draw(ctx: &egui::Context, state: &mut OverlayState, view_w: f32, view_h: f32) {
    if state.texture.is_none() {
        let img = ColorImage::from_rgba_unmultiplied(
            [state.frame.width as usize, state.frame.height as usize],
            state.frame.pixels.as_raw(),
        );
        state.texture = Some(ctx.load_texture(
            "fshot-overlay-src",
            img,
            TextureOptions::NEAREST,
        ));
    }
    // 工具栏里的「块」是逻辑点；换算成截图像素后再生成马赛克纹理，
    // 保证预览块大小与导出（compose 里按 scale 放大）一致。
    let block_px = mosaic_block_px(state.tool.mosaic_block, state.frame.width, view_w);
    if state.mosaic_texture.is_none() || state.mosaic_block_px != block_px {
        // 块大小改变时重建整屏马赛克纹理；旧 TextureHandle 被替换时自动释放
        let small = make_mosaic(&state.frame.pixels, block_px);
        let ci = ColorImage::from_rgba_unmultiplied(
            [small.width() as usize, small.height() as usize],
            small.as_raw(),
        );
        state.mosaic_texture = Some(ctx.load_texture(
            format!("fshot-overlay-mosaic-{block_px}"),
            ci,
            TextureOptions::NEAREST,
        ));
        state.mosaic_block_px = block_px;
    }
    let tex_id = state.texture.as_ref().unwrap().id();
    let mosaic_id = state.mosaic_texture.as_ref().map(|t| t.id());

    let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(view_w, view_h));
    state.screen_size = screen.size();

    let painter =
        ctx.layer_painter(LayerId::new(Order::Background, egui::Id::new("fshot-overlay")));

    let full_uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0_f32, 1.0_f32));
    painter.image(tex_id, screen, full_uv, Color32::WHITE);

    painter.rect_filled(screen, 0.0_f32, Color32::from_black_alpha(150));

    let mut selection_rect: Option<Rect> = None;
    if let Some(sel) = state.selection.rect {
        if sel.width() > 1.0_f32 && sel.height() > 1.0_f32 {
            selection_rect = Some(sel);
            let sw = screen.width().max(1.0_f32);
            let sh = screen.height().max(1.0_f32);
            let uv = Rect::from_min_max(
                Pos2::new(sel.min.x / sw, sel.min.y / sh),
                Pos2::new(sel.max.x / sw, sel.max.y / sh),
            );
            painter.image(tex_id, sel, uv, Color32::WHITE);
            painter.rect_stroke(
                sel,
                0.0_f32,
                egui::Stroke::new(1.0_f32, Color32::from_rgb(0, 170, 255)),
            );

            if state.selection.is_dragging() {
                let label = format!("{:.0} × {:.0}", sel.width(), sel.height());
                let pos = Pos2::new(sel.min.x, (sel.min.y - 6.0_f32).max(4.0_f32));
                painter.text(
                    pos,
                    egui::Align2::LEFT_BOTTOM,
                    label,
                    egui::FontId::monospace(12.0_f32),
                    Color32::WHITE,
                );
            }
        }
    }

    if let Some(sel) = selection_rect {
        // 重新框选时隐藏已有标注；平移/缩放时保留，方便对齐
        if !state.selection.is_new_drag() {
            let p_ann = painter.with_clip_rect(sel);
            render::draw_all(&p_ann, &state.history.items, mosaic_id, screen);
            if let Some(draft) = &state.tool.draft {
                render::draw_draft(
                    &p_ann,
                    draft,
                    state.tool.active,
                    &state.tool.style,
                    mosaic_id,
                    screen,
                );
            }
            if let Some(id) = state.selected_id {
                if let Some(a) = state.history.items.iter().find(|a| a.id == id) {
                    render::draw_selection_frame(&p_ann, a.bounds());
                }
            }
        }
        if !state.selection.is_dragging() {
            draw_handles(&painter, sel);
        }
    }

    state.toolbar_rect = Rect::NOTHING;
    if let Some(sel) = selection_rect {
        if !state.selection.is_dragging() && state.tool.text_edit.is_none() {
            let mut reselect = false;
            let r = toolbar::draw(
                ctx,
                sel,
                screen,
                &mut state.tool.active,
                &mut state.tool.style,
                &mut state.tool.fill,
                &mut state.tool.text_size,
                &mut state.tool.mosaic_block,
                &mut reselect,
            );
            state.toolbar_rect = r;
            if reselect {
                state.selection.clear();
                state.selected_id = None;
                state.tool.draft = None;
                // 立刻再画一帧，避免选区/手柄残留到下一次鼠标事件
                ctx.request_repaint();
            }
        }
    }

    if let Some((_id, buf, pos)) = state.tool.text_edit.as_mut() {
        egui::Area::new(egui::Id::new("fshot-text-edit"))
            .fixed_pos(*pos)
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(Color32::from_black_alpha(235))
                    .show(ui, |ui| {
                        ui.label("文本（Enter 确认 / Esc 取消）");
                        let r = ui.add(
                            egui::TextEdit::singleline(buf)
                                .desired_width(260.0_f32)
                                .hint_text("输入文字…"),
                        );
                        r.request_focus();
                    });
            });
    }

    // 光标反馈：控制点/边框用缩放或移动光标，空白处用十字准星；
    // 指针位于工具栏、文本输入框或弹出层上时交给 egui 自己管理。
    if let Some(pos) = state.hovering {
        let over_ui = state.toolbar_rect.contains(pos)
            || state.tool.text_edit.is_some()
            || ctx.memory(|m| m.any_popup_open());
        if !over_ui {
            ctx.set_cursor_icon(cursor_for(state, pos));
        }
    }
}

fn draw_handles(painter: &egui::Painter, sel: Rect) {
    let stroke = egui::Stroke::new(1.0_f32, Color32::from_rgb(0, 120, 200));
    for handle in Handle::ALL {
        let r = Rect::from_center_size(handle.point(sel), Vec2::splat(7.0));
        painter.rect_filled(r, 1.0_f32, Color32::WHITE);
        painter.rect_stroke(r, 1.0_f32, stroke);
    }
}

fn cursor_for(state: &OverlayState, pos: Pos2) -> CursorIcon {
    if let Some(handle) = state.selection.active_handle() {
        return handle_cursor(handle);
    }
    if state.selection.is_moving() {
        return CursorIcon::Grabbing;
    }
    if state.selection.is_dragging() {
        return CursorIcon::Crosshair;
    }
    if let Some(handle) = state.selection.hit_handle(pos, selection::HANDLE_HIT) {
        return handle_cursor(handle);
    }
    if state.selection.hit_border(pos, selection::BORDER_HIT) {
        return CursorIcon::Move;
    }
    CursorIcon::Crosshair
}

fn handle_cursor(handle: Handle) -> CursorIcon {
    match handle {
        Handle::Nw | Handle::Se => CursorIcon::ResizeNwSe,
        Handle::Ne | Handle::Sw => CursorIcon::ResizeNeSw,
        Handle::N | Handle::S => CursorIcon::ResizeVertical,
        Handle::E | Handle::W => CursorIcon::ResizeHorizontal,
    }
}

/// 生成低分辨率马赛克图（每 block×block 区平均一色）
fn make_mosaic(src: &RgbaImage, block: u32) -> RgbaImage {
    let (w, h) = src.dimensions();
    let block = block.max(2);
    let mw = (w / block).max(1);
    let mh = (h / block).max(1);
    let mut out = RgbaImage::new(mw, mh);
    for my in 0..mh {
        for mx in 0..mw {
            let mut r: u64 = 0;
            let mut g: u64 = 0;
            let mut b: u64 = 0;
            let mut a: u64 = 0;
            let mut n: u64 = 0;
            for y in 0..block {
                for x in 0..block {
                    let px = mx * block + x;
                    let py = my * block + y;
                    if px < w && py < h {
                        let p = src.get_pixel(px, py);
                        r += p[0] as u64;
                        g += p[1] as u64;
                        b += p[2] as u64;
                        a += p[3] as u64;
                        n += 1;
                    }
                }
            }
            if n > 0 {
                out.put_pixel(
                    mx,
                    my,
                    Rgba([(r / n) as u8, (g / n) as u8, (b / n) as u8, (a / n) as u8]),
                );
            }
        }
    }
    out
}

/// 逻辑块大小 → 截图物理像素块大小（与 compose 里的 `block * sx` 保持一致）
fn mosaic_block_px(block: u32, frame_w: u32, view_w: f32) -> u32 {
    let scale = frame_w as f32 / view_w.max(1.0);
    ((block.max(2) as f32) * scale).round().max(2.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_frame() -> CapturedFrame {
        CapturedFrame {
            width: 320,
            height: 200,
            pixels: RgbaImage::from_pixel(320, 200, Rgba([30, 40, 50, 255])),
            monitor_x: 0,
            monitor_y: 0,
            scale: 1.0,
        }
    }

    #[test]
    fn mosaic_block_scales_with_dpi() {
        assert_eq!(mosaic_block_px(16, 1920, 1920.0), 16);
        assert_eq!(mosaic_block_px(16, 3840, 1920.0), 32);
        assert_eq!(mosaic_block_px(16, 2400, 1920.0), 20);
        assert_eq!(mosaic_block_px(1, 1920, 1920.0), 2);
    }

    #[test]
    fn draw_builds_textures_and_rebuilds_mosaic_on_block_change() {
        let ctx = egui::Context::default();
        let mut state = OverlayState::new(test_frame(), &crate::config::ToolPrefs::default());
        state.selection.start_new(Pos2::new(20.0, 20.0));
        state.selection.drag_to(Pos2::new(220.0, 160.0));
        state.selection.end_drag();
        state.hovering = Some(Pos2::new(219.0, 159.0));

        let run = |ctx: &egui::Context, state: &mut OverlayState| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(320.0, 200.0))),
                ..Default::default()
            };
            ctx.run(input, |c| draw(c, state, 320.0, 200.0))
        };

        let out = run(&ctx, &mut state);
        assert!(!out.shapes.is_empty());
        assert!(state.texture.is_some());
        assert!(state.mosaic_texture.is_some());
        let first = state.mosaic_texture.as_ref().unwrap().id();

        state.tool.mosaic_block = 32;
        let _ = run(&ctx, &mut state);
        let second = state.mosaic_texture.as_ref().unwrap().id();
        assert_ne!(first, second, "block change should rebuild mosaic texture");
    }
}