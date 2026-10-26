use std::sync::OnceLock;

use ab_glyph::{point, Font, FontArc, PxScale, ScaleFont};
use egui::{Color32, Pos2, Rect, Vec2};
use image::RgbaImage;

use crate::annotate::object::{Annotation, Kind};
use crate::capture::screen::CapturedFrame;

static FONT: OnceLock<Option<FontArc>> = OnceLock::new();

fn load_font() -> Option<&'static FontArc> {
    FONT.get_or_init(|| {
        for path in [
            "C:/Windows/Fonts/msyh.ttc",
            "C:/Windows/Fonts/simhei.ttf",
            "C:/Windows/Fonts/Deng.ttf",
            "C:/Windows/Fonts/simsun.ttc",
            "C:/Windows/Fonts/simkai.ttf",
        ] {
            if let Ok(bytes) = std::fs::read(path) {
                if let Ok(f) = FontArc::try_from_vec(bytes) {
                    log::info!("compose font loaded: {path}");
                    return Some(f);
                }
            }
        }
        log::warn!("compose: no CJK font loaded, text will be skipped");
        None
    })
    .as_ref()
}

/// 将 `items` 里的标注烤入按 `sel` 裁剪后的图像
pub fn compose(
    frame: &CapturedFrame,
    sel: Rect,
    screen_size: Vec2,
    items: &[Annotation],
    mosaic_block: u32,
) -> RgbaImage {
    let sx = frame.width as f32 / screen_size.x.max(1.0);
    let sy = frame.height as f32 / screen_size.y.max(1.0);

    let crop_x = (sel.min.x * sx).clamp(0.0, frame.width as f32 - 1.0);
    let crop_y = (sel.min.y * sy).clamp(0.0, frame.height as f32 - 1.0);
    let crop_w = ((sel.width() * sx).round().max(1.0) as u32)
        .min(frame.width.saturating_sub(crop_x as u32))
        .max(1);
    let crop_h = ((sel.height() * sy).round().max(1.0) as u32)
        .min(frame.height.saturating_sub(crop_y as u32))
        .max(1);

    let mut out = RgbaImage::new(crop_w, crop_h);
    for y in 0..crop_h {
        for x in 0..crop_w {
            let px = frame.pixels.get_pixel(crop_x as u32 + x, crop_y as u32 + y);
            out.put_pixel(x, y, *px);
        }
    }

    let to_px = |p: Pos2| (p.x * sx - crop_x, p.y * sy - crop_y);
    let font = load_font();

    for a in items {
        draw_annotation(
            &mut out,
            a,
            &to_px,
            sx,
            sy,
            frame,
            crop_x,
            crop_y,
            mosaic_block.max(2),
            font,
        );
    }

    out
}

#[allow(clippy::too_many_arguments)]
fn draw_annotation(
    img: &mut RgbaImage,
    a: &Annotation,
    to_px: &impl Fn(Pos2) -> (f32, f32),
    sx: f32,
    sy: f32,
    frame: &CapturedFrame,
    crop_x: f32,
    crop_y: f32,
    mosaic_block: u32,
    font: Option<&FontArc>,
) {
    let scale = sx.max(sy);
    let stroke_w = (a.style.stroke_width * scale).max(1.0);
    let color = a.style.stroke;

    match &a.kind {
        Kind::Rect { rect, fill } => {
            let (x0, y0) = to_px(rect.min);
            let (x1, y1) = to_px(rect.max);
            if let Some(f) = fill {
                fill_rect(img, x0, y0, x1 - x0, y1 - y0, *f);
            }
            stroke_rect(img, x0, y0, x1 - x0, y1 - y0, color, stroke_w);
        }
        Kind::Ellipse { rect, fill } => {
            let (cx, cy) = to_px(rect.center());
            let rx = rect.width() * 0.5 * sx;
            let ry = rect.height() * 0.5 * sy;
            if let Some(f) = fill {
                fill_ellipse(img, cx, cy, rx, ry, *f);
            }
            stroke_ellipse(img, cx, cy, rx, ry, color, stroke_w);
        }
        Kind::Arrow { from, to } => {
            let (x0, y0) = to_px(*from);
            let (x1, y1) = to_px(*to);
            draw_line(img, x0, y0, x1, y1, color, stroke_w);
            draw_arrow_head(img, x0, y0, x1, y1, color, stroke_w, scale);
        }
        Kind::Line { from, to } => {
            let (x0, y0) = to_px(*from);
            let (x1, y1) = to_px(*to);
            draw_line(img, x0, y0, x1, y1, color, stroke_w);
        }
        Kind::Pen { points } => {
            for w in points.windows(2) {
                let (x0, y0) = to_px(w[0]);
                let (x1, y1) = to_px(w[1]);
                draw_line(img, x0, y0, x1, y1, color, stroke_w);
            }
        }
        Kind::Text { pos, text, size } => {
            if let Some(f) = font {
                let (x, y) = to_px(*pos);
                let px = (size * scale).max(6.0);
                draw_text(img, f, px, x, y, text, color);
            }
        }
        Kind::Step {
            center,
            radius,
            number,
        } => {
            let (cx, cy) = to_px(*center);
            let r = radius * scale;
            fill_ellipse(img, cx, cy, r, r, color);
            if let Some(f) = font {
                let text = number.to_string();
                let px = r * 1.2;
                let scaled = f.as_scaled(PxScale::from(px));
                let text_w = scaled.h_advance(scaled.scaled_glyph('0').id)
                    * text.chars().count() as f32;
                let text_h = scaled.ascent() - scaled.descent();
                draw_text(
                    img,
                    f,
                    px,
                    cx - text_w * 0.5,
                    cy - text_h * 0.5,
                    &text,
                    Color32::WHITE,
                );
            }
        }
        Kind::Mosaic { rect } => {
            let (x0, y0) = to_px(rect.min);
            let (x1, y1) = to_px(rect.max);
            mosaic_rect(img, frame, crop_x, crop_y, x0, y0, x1, y1, mosaic_block, sx);
        }
    }
}

// ---- 像素级绘制 ----

fn blend_pixel(img: &mut RgbaImage, x: u32, y: u32, c: Color32) {
    if x >= img.width() || y >= img.height() {
        return;
    }
    let px = img.get_pixel_mut(x, y);
    let a = c.a() as f32 / 255.0;
    let ia = 1.0 - a;
    px[0] = (c.r() as f32 * a + px[0] as f32 * ia) as u8;
    px[1] = (c.g() as f32 * a + px[1] as f32 * ia) as u8;
    px[2] = (c.b() as f32 * a + px[2] as f32 * ia) as u8;
    px[3] = 255;
}

fn fill_rect(img: &mut RgbaImage, x0: f32, y0: f32, w: f32, h: f32, c: Color32) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let gx0 = x0.floor().max(0.0) as u32;
    let gy0 = y0.floor().max(0.0) as u32;
    let gx1 = ((x0 + w).ceil() as u32).min(img.width());
    let gy1 = ((y0 + h).ceil() as u32).min(img.height());
    for y in gy0..gy1 {
        for x in gx0..gx1 {
            blend_pixel(img, x, y, c);
        }
    }
}

fn stroke_rect(img: &mut RgbaImage, x0: f32, y0: f32, w: f32, h: f32, c: Color32, sw: f32) {
    let half = sw * 0.5;
    fill_rect(img, x0 - half, y0 - half, w + sw, sw, c);
    fill_rect(img, x0 - half, y0 + h - half, w + sw, sw, c);
    fill_rect(img, x0 - half, y0 - half, sw, h + sw, c);
    fill_rect(img, x0 + w - half, y0 - half, sw, h + sw, c);
}

fn fill_ellipse(img: &mut RgbaImage, cx: f32, cy: f32, rx: f32, ry: f32, c: Color32) {
    if rx < 0.5 || ry < 0.5 {
        return;
    }
    let x0 = (cx - rx).floor().max(0.0) as u32;
    let y0 = (cy - ry).floor().max(0.0) as u32;
    let x1 = ((cx + rx).ceil() as u32).min(img.width());
    let y1 = ((cy + ry).ceil() as u32).min(img.height());
    let irx2 = 1.0 / (rx * rx);
    let iry2 = 1.0 / (ry * ry);
    for y in y0..y1 {
        for x in x0..x1 {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            if dx * dx * irx2 + dy * dy * iry2 <= 1.0 {
                blend_pixel(img, x, y, c);
            }
        }
    }
}

fn stroke_ellipse(img: &mut RgbaImage, cx: f32, cy: f32, rx: f32, ry: f32, c: Color32, sw: f32) {
    let steps = 128;
    let mut prev: Option<(f32, f32)> = None;
    for i in 0..=steps {
        let t = i as f32 / steps as f32 * std::f32::consts::TAU;
        let x = cx + rx * t.cos();
        let y = cy + ry * t.sin();
        if let Some((px, py)) = prev {
            draw_line(img, px, py, x, y, c, sw);
        }
        prev = Some((x, y));
    }
}

fn draw_line(img: &mut RgbaImage, x0: f32, y0: f32, x1: f32, y1: f32, c: Color32, sw: f32) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = (dx * dx + dy * dy).sqrt();
    let r = (sw * 0.5).max(0.5);
    if len < 0.5 {
        fill_ellipse(img, x0, y0, r, r, c);
        return;
    }
    let steps = (len * 2.0).ceil().max(1.0) as u32;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = x0 + dx * t;
        let y = y0 + dy * t;
        fill_ellipse(img, x, y, r, r, c);
    }
}

fn draw_arrow_head(
    img: &mut RgbaImage,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    c: Color32,
    sw: f32,
    scale: f32,
) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return;
    }
    let ux = dx / len;
    let uy = dy / len;
    let head_len = (sw * 4.0).max(12.0 * scale).min(len * 0.4);
    let head_w = head_len * 0.6;
    let px = -uy;
    let py = ux;
    let bx = x1 - ux * head_len;
    let by = y1 - uy * head_len;
    fill_triangle(
        img,
        (x1, y1),
        (bx + px * head_w, by + py * head_w),
        (bx - px * head_w, by - py * head_w),
        c,
    );
}

fn fill_triangle(
    img: &mut RgbaImage,
    a: (f32, f32),
    b: (f32, f32),
    c: (f32, f32),
    color: Color32,
) {
    let min_x = a.0.min(b.0).min(c.0).max(0.0) as u32;
    let max_x = ((a.0.max(b.0).max(c.0)) + 1.0) as u32;
    let min_y = a.1.min(b.1).min(c.1).max(0.0) as u32;
    let max_y = ((a.1.max(b.1).max(c.1)) + 1.0) as u32;
    let max_x = max_x.min(img.width());
    let max_y = max_y.min(img.height());
    let sign = |p1: (f32, f32), p2: (f32, f32), p3: (f32, f32)| {
        (p1.0 - p3.0) * (p2.1 - p3.1) - (p2.0 - p3.0) * (p1.1 - p3.1)
    };
    let d1 = sign(a, b, c);
    if d1.abs() < 1e-6 {
        return;
    }
    for y in min_y..max_y {
        for x in min_x..max_x {
            let p = (x as f32 + 0.5, y as f32 + 0.5);
            let b1 = sign(p, a, b) * d1 >= 0.0;
            let b2 = sign(p, b, c) * d1 >= 0.0;
            let b3 = sign(p, c, a) * d1 >= 0.0;
            if b1 && b2 && b3 {
                blend_pixel(img, x, y, color);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_text(
    img: &mut RgbaImage,
    font: &FontArc,
    size_px: f32,
    x: f32,
    y: f32,
    text: &str,
    color: Color32,
) {
    let scaled = font.as_scaled(PxScale::from(size_px));
    let ascent = scaled.ascent();
    let mut caret = point(x, y + ascent);
    for ch in text.chars() {
        if ch == '\n' {
            caret.x = x;
            caret.y += scaled.height() + scaled.line_gap();
            continue;
        }

        // 关键修复：把字形位置设为 caret，文字才会落在点击位置
        let mut glyph = scaled.scaled_glyph(ch);
        glyph.position = caret;

        if let Some(outline) = scaled.outline_glyph(glyph.clone()) {
            let bounds = outline.px_bounds();
            outline.draw(|gx, gy, coverage| {
                let px = (bounds.min.x as i32 + gx as i32).max(0) as u32;
                let py = (bounds.min.y as i32 + gy as i32).max(0) as u32;
                let cov = (coverage * 255.0) as u32;
                if cov == 0 {
                    return;
                }
                let a = (cov * (color.a() as u32)) / 255;
                let c =
                    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a as u8);
                blend_pixel(img, px, py, c);
            });
        }
        caret.x += scaled.h_advance(glyph.id);
    }
}

#[allow(clippy::too_many_arguments)]
fn mosaic_rect(
    img: &mut RgbaImage,
    frame: &CapturedFrame,
    crop_x: f32,
    crop_y: f32,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    block: u32,
    sx: f32,
) {
    let bw = (block as f32 * sx).max(2.0);
    let mut yy = y0;
    while yy < y1 {
        let mut xx = x0;
        while xx < x1 {
            let bx1 = (xx + bw).min(x1);
            let by1 = (yy + bw).min(y1);

            let gx0 = (xx + crop_x).clamp(0.0, frame.width as f32 - 1.0) as u32;
            let gy0 = (yy + crop_y).clamp(0.0, frame.height as f32 - 1.0) as u32;
            let gx1 = ((xx + crop_x + bw) as u32).clamp(gx0 + 1, frame.width);
            let gy1 = ((yy + crop_y + bw) as u32).clamp(gy0 + 1, frame.height);

            let avg = avg_rect(&frame.pixels, gx0, gy0, gx1, gy1);
            fill_rect(img, xx, yy, bx1 - xx, by1 - yy, avg);
            xx += bw;
        }
        yy += bw;
    }
}

fn avg_rect(img: &RgbaImage, x0: u32, y0: u32, x1: u32, y1: u32) -> Color32 {
    let mut r: u64 = 0;
    let mut g: u64 = 0;
    let mut b: u64 = 0;
    let mut n: u64 = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = img.get_pixel(x, y);
            r += p[0] as u64;
            g += p[1] as u64;
            b += p[2] as u64;
            n += 1;
        }
    }
    if n == 0 {
        return Color32::BLACK;
    }
    Color32::from_rgb((r / n) as u8, (g / n) as u8, (b / n) as u8)
}