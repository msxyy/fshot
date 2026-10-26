use egui::{Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, TextureId};

use crate::annotate::object::{Annotation, Kind, Style, Tool};
use crate::annotate::tool::Draft;

/// 渲染全部已有标注；`mosaic_tex` 为马赛克纹理（可选），`screen` 为整屏矩形
pub fn draw_all(
    painter: &Painter,
    items: &[Annotation],
    mosaic_tex: Option<TextureId>,
    screen: Rect,
) {
    for a in items {
        draw_one(painter, a, mosaic_tex, screen);
    }
}

pub fn draw_one(
    painter: &Painter,
    a: &Annotation,
    mosaic_tex: Option<TextureId>,
    screen: Rect,
) {
    let stroke = Stroke::new(a.style.stroke_width, a.style.stroke);
    match &a.kind {
        Kind::Rect { rect, fill } => {
            if let Some(f) = fill {
                painter.rect_filled(*rect, 0.0_f32, *f);
            }
            painter.rect_stroke(*rect, 0.0_f32, stroke);
        }
        Kind::Ellipse { rect, fill } => {
            let pts = ellipse_points(*rect, 64);
            let fill_color = fill.unwrap_or(Color32::TRANSPARENT);
            painter.add(Shape::convex_polygon(pts, fill_color, stroke));
        }
        Kind::Arrow { from, to } => {
            painter.arrow(*from, *to - *from, stroke);
        }
        Kind::Line { from, to } => {
            painter.line_segment([*from, *to], stroke);
        }
        Kind::Pen { points } => {
            if points.len() >= 2 {
                painter.add(Shape::line(points.clone(), stroke));
            }
        }
        Kind::Text { pos, text, size } => {
            painter.text(
                *pos,
                egui::Align2::LEFT_TOP,
                text,
                FontId::proportional(*size),
                a.style.stroke,
            );
        }
        Kind::Step { center, radius, number } => {
            painter.circle_filled(*center, *radius, a.style.stroke);
            painter.text(
                *center,
                egui::Align2::CENTER_CENTER,
                number.to_string(),
                FontId::proportional(*radius * 1.2),
                Color32::WHITE,
            );
        }
        Kind::Mosaic { rect } => {
            if let Some(tex) = mosaic_tex {
                // 马赛克纹理覆盖整个屏幕，UV 必须以整屏为基准，而不是以裁剪区为基准，
                // 否则选区不是全屏时预览会采样到错误的屏幕区域。
                let uv = mosaic_uv(*rect, screen);
                painter.image(tex, *rect, uv, Color32::WHITE);
            } else {
                painter.rect_filled(*rect, 0.0_f32, Color32::from_gray(80));
            }
        }
    }
}

pub fn draw_draft(
    painter: &Painter,
    draft: &Draft,
    active: Tool,
    style: &Style,
    mosaic_tex: Option<TextureId>,
    screen: Rect,
) {
    let stroke = Stroke::new(style.stroke_width, style.stroke);
    match draft {
        Draft::Shape { start, current } => match active {
            Tool::Rect => {
                painter.rect_stroke(Rect::from_two_pos(*start, *current), 0.0_f32, stroke);
            }
            Tool::Ellipse => {
                let r = Rect::from_two_pos(*start, *current);
                painter.add(Shape::closed_line(ellipse_points(r, 64), stroke));
            }
            Tool::Arrow => {
                painter.arrow(*start, *current - *start, stroke);
            }
            Tool::Line => {
                painter.line_segment([*start, *current], stroke);
            }
            _ => {}
        },
        Draft::Pen(points) => {
            if points.len() >= 2 {
                painter.add(Shape::line(points.clone(), stroke));
            }
        }
        Draft::Mosaic { start, current } => {
            let r = Rect::from_two_pos(*start, *current);
            if let Some(tex) = mosaic_tex {
                painter.image(tex, r, mosaic_uv(r, screen), Color32::WHITE);
            } else {
                painter.rect_filled(r, 0.0_f32, Color32::from_gray(80));
            }
            painter.rect_stroke(
                r,
                0.0_f32,
                Stroke::new(1.0_f32, Color32::from_rgb(0, 170, 255)),
            );
        }
    }
}

/// 绘制选中态：虚线框 + 四角手柄
pub fn draw_selection_frame(painter: &Painter, bounds: Rect) {
    let stroke = Stroke::new(1.0_f32, Color32::from_rgb(0, 200, 255));
    // 外框
    painter.rect_stroke(bounds, 0.0_f32, stroke);
    // 四角小手柄
    let h = 4.0_f32;
    for c in [
        bounds.min,
        Pos2::new(bounds.max.x, bounds.min.y),
        Pos2::new(bounds.min.x, bounds.max.y),
        bounds.max,
    ] {
        painter.rect_filled(
            Rect::from_center_size(c, egui::vec2(h * 2.0, h * 2.0)),
            0.0_f32,
            Color32::from_rgb(0, 200, 255),
        );
    }
}

fn ellipse_points(rect: Rect, n: usize) -> Vec<Pos2> {
    let c = rect.center();
    let rx = rect.width() * 0.5;
    let ry = rect.height() * 0.5;
    (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            Pos2::new(c.x + rx * a.cos(), c.y + ry * a.sin())
        })
        .collect()
}

/// 把屏幕坐标的矩形映射为整屏纹理上的 UV
fn mosaic_uv(rect: Rect, screen: Rect) -> Rect {
    let sw = screen.width().max(1.0);
    let sh = screen.height().max(1.0);
    Rect::from_min_max(
        Pos2::new(
            (rect.min.x - screen.min.x) / sw,
            (rect.min.y - screen.min.y) / sh,
        ),
        Pos2::new(
            (rect.max.x - screen.min.x) / sw,
            (rect.max.y - screen.min.y) / sh,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::vec2;

    #[test]
    fn mosaic_uv_is_relative_to_screen_not_clip() {
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 500.0));
        let rect = Rect::from_min_max(Pos2::new(250.0, 100.0), Pos2::new(500.0, 200.0));
        assert_eq!(
            mosaic_uv(rect, screen),
            Rect::from_min_max(Pos2::new(0.25, 0.2), Pos2::new(0.5, 0.4))
        );
    }
}