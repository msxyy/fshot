use egui::{Color32, Pos2, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Pointer,
    Rect,
    Ellipse,
    Arrow,
    Line,
    Pen,
    Text,
    Step,
    Mosaic,
}

#[derive(Clone, Debug)]
pub struct Style {
    pub stroke: Color32,
    pub stroke_width: f32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            stroke: Color32::from_rgb(230, 60, 60),
            stroke_width: 2.0,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Kind {
    Rect {
        rect: Rect,
        fill: Option<Color32>,
    },
    Ellipse {
        rect: Rect,
        fill: Option<Color32>,
    },
    Arrow {
        from: Pos2,
        to: Pos2,
    },
    Line {
        from: Pos2,
        to: Pos2,
    },
    Pen {
        points: Vec<Pos2>,
    },
    Text {
        pos: Pos2,
        text: String,
        size: f32,
    },
    Step {
        center: Pos2,
        radius: f32,
        number: u32,
    },
    Mosaic {
        rect: Rect,
    },
}

#[derive(Clone, Debug)]
pub struct Annotation {
    pub id: u64,
    pub kind: Kind,
    pub style: Style,
}

impl Annotation {
    /// 边界矩形（用于 hit-test 与选中高亮）
    pub fn bounds(&self) -> Rect {
        match &self.kind {
            Kind::Rect { rect, .. }
            | Kind::Ellipse { rect, .. }
            | Kind::Mosaic { rect } => *rect,
            Kind::Arrow { from, to } | Kind::Line { from, to } => {
                Rect::from_two_pos(*from, *to)
            }
            Kind::Pen { points } => {
                if points.is_empty() {
                    Rect::NOTHING
                } else {
                    let mut r = Rect::from_min_max(points[0], points[0]);
                    for p in points {
                        r = r.union(Rect::from_min_max(*p, *p));
                    }
                    r
                }
            }
            Kind::Text { pos, text, size } => {
                let w = text.chars().count() as f32 * size * 0.6;
                Rect::from_min_size(*pos, egui::vec2(w.max(20.0), *size * 1.4))
            }
            Kind::Step { center, radius, .. } => {
                Rect::from_center_size(*center, egui::vec2(*radius * 2.0, *radius * 2.0))
            }
        }
    }

    /// 点是否命中（含容差）
    pub fn hit_test(&self, p: Pos2) -> bool {
        let tol = self.style.stroke_width.max(4.0);
        match &self.kind {
            Kind::Rect { rect, .. }
            | Kind::Ellipse { rect, .. }
            | Kind::Mosaic { rect } => rect.expand(tol).contains(p),
            Kind::Arrow { from, to } | Kind::Line { from, to } => {
                point_seg_dist(p, *from, *to) <= tol + 2.0
            }
            Kind::Pen { points } => {
                if points.len() < 2 {
                    return false;
                }
                for w in points.windows(2) {
                    if point_seg_dist(p, w[0], w[1]) <= tol + 2.0 {
                        return true;
                    }
                }
                false
            }
            Kind::Text { .. } | Kind::Step { .. } => self.bounds().contains(p),
        }
    }

    pub fn translate(&mut self, delta: egui::Vec2) {
        match &mut self.kind {
            Kind::Rect { rect, .. } | Kind::Ellipse { rect, .. } | Kind::Mosaic { rect } => {
                rect.min += delta;
                rect.max += delta;
            }
            Kind::Arrow { from, to } | Kind::Line { from, to } => {
                *from += delta;
                *to += delta;
            }
            Kind::Pen { points } => {
                for p in points {
                    *p += delta;
                }
            }
            Kind::Text { pos, .. } => {
                *pos += delta;
            }
            Kind::Step { center, .. } => {
                *center += delta;
            }
        }
    }
}

fn point_seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let ap = p - a;
    let t = if ab.length_sq() < 1e-6 {
        0.0
    } else {
        (ap.dot(ab) / ab.length_sq()).clamp(0.0, 1.0)
    };
    (a + ab * t).distance(p)
}