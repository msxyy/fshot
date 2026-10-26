use egui::{Pos2, Rect};

/// 选区控制点容忍半径（逻辑点）
pub const HANDLE_HIT: f32 = 9.0;
/// 选区边框拖动容忍宽度（逻辑点）
pub const BORDER_HIT: f32 = 4.0;

/// 选区可拖拽的 8 个控制点
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    Nw,
    N,
    Ne,
    E,
    Se,
    S,
    Sw,
    W,
}

impl Handle {
    pub const ALL: [Handle; 8] = [
        Handle::Nw,
        Handle::N,
        Handle::Ne,
        Handle::E,
        Handle::Se,
        Handle::S,
        Handle::Sw,
        Handle::W,
    ];

    pub fn point(self, r: Rect) -> Pos2 {
        let c = r.center();
        match self {
            Handle::Nw => r.min,
            Handle::N => Pos2::new(c.x, r.min.y),
            Handle::Ne => Pos2::new(r.max.x, r.min.y),
            Handle::E => Pos2::new(r.max.x, c.y),
            Handle::Se => r.max,
            Handle::S => Pos2::new(c.x, r.max.y),
            Handle::Sw => Pos2::new(r.min.x, r.max.y),
            Handle::W => Pos2::new(r.min.x, c.y),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Drag {
    /// 重新框选
    New { start: Pos2 },
    /// 整体平移（拖边框）
    Move { grab: Pos2, orig: Rect },
    /// 拖控制点缩放
    Resize { handle: Handle, orig: Rect },
}

pub struct Selection {
    pub rect: Option<Rect>,
    drag: Option<Drag>,
}

impl Selection {
    pub fn new() -> Self {
        Self {
            rect: None,
            drag: None,
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// 是否正在「重新框选」（区别于平移/缩放）
    pub fn is_new_drag(&self) -> bool {
        matches!(self.drag, Some(Drag::New { .. }))
    }

    /// 是否正在整体平移选区
    pub fn is_moving(&self) -> bool {
        matches!(self.drag, Some(Drag::Move { .. }))
    }

    /// 正在缩放时返回当前控制点
    pub fn active_handle(&self) -> Option<Handle> {
        match self.drag {
            Some(Drag::Resize { handle, .. }) => Some(handle),
            _ => None,
        }
    }

    pub fn clear(&mut self) {
        self.rect = None;
        self.drag = None;
    }

    pub fn start_new(&mut self, pos: Pos2) {
        self.drag = Some(Drag::New { start: pos });
        self.rect = Some(Rect::from_min_max(pos, pos));
    }

    pub fn start_move(&mut self, pos: Pos2) {
        if let Some(orig) = self.rect {
            self.drag = Some(Drag::Move { grab: pos, orig });
        }
    }

    pub fn start_resize(&mut self, handle: Handle) {
        if let Some(orig) = self.rect {
            self.drag = Some(Drag::Resize { handle, orig });
        }
    }

    pub fn drag_to(&mut self, pos: Pos2) {
        match self.drag {
            Some(Drag::New { start }) => {
                self.rect = Some(Rect::from_two_pos(start, pos));
            }
            Some(Drag::Move { grab, orig }) => {
                self.rect = Some(orig.translate(pos - grab));
            }
            Some(Drag::Resize { handle, orig }) => {
                let mut min = orig.min;
                let mut max = orig.max;
                if matches!(handle, Handle::Nw | Handle::W | Handle::Sw) {
                    min.x = pos.x;
                }
                if matches!(handle, Handle::Ne | Handle::E | Handle::Se) {
                    max.x = pos.x;
                }
                if matches!(handle, Handle::Nw | Handle::N | Handle::Ne) {
                    min.y = pos.y;
                }
                if matches!(handle, Handle::Sw | Handle::S | Handle::Se) {
                    max.y = pos.y;
                }
                // from_two_pos 负责处理拖过头时的翻转
                self.rect = Some(Rect::from_two_pos(min, max));
            }
            None => {}
        }
    }

    pub fn end_drag(&mut self) {
        if self.drag.take().is_some() {
            if let Some(r) = self.rect {
                if r.width() < 3.0 || r.height() < 3.0 {
                    self.rect = None;
                }
            }
        }
    }

    /// 命中 8 个控制点之一
    pub fn hit_handle(&self, pos: Pos2, tol: f32) -> Option<Handle> {
        let r = self.rect?;
        Handle::ALL.into_iter().find(|h| {
            Rect::from_center_size(h.point(r), egui::vec2(tol * 2.0, tol * 2.0)).contains(pos)
        })
    }

    /// 命中选区边框（用于整体平移）
    pub fn hit_border(&self, pos: Pos2, tol: f32) -> bool {
        let Some(r) = self.rect else {
            return false;
        };
        if r.width() < 3.0 || r.height() < 3.0 {
            return false;
        }
        if !r.expand(tol).contains(pos) {
            return false;
        }
        let inner = r.shrink(tol);
        !(inner.width() > 0.0 && inner.height() > 0.0 && inner.contains(pos))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection(a: (f32, f32), b: (f32, f32)) -> Selection {
        let mut s = Selection::new();
        s.start_new(Pos2::new(a.0, a.1));
        s.drag_to(Pos2::new(b.0, b.1));
        s.end_drag();
        s
    }

    #[test]
    fn new_selection_normalizes_reverse_drag() {
        let s = selection((100.0, 120.0), (40.0, 50.0));
        assert_eq!(
            s.rect,
            Some(Rect::from_min_max(
                Pos2::new(40.0, 50.0),
                Pos2::new(100.0, 120.0)
            ))
        );
    }

    #[test]
    fn tiny_selection_is_discarded() {
        let s = selection((10.0, 10.0), (10.5, 10.5));
        assert!(s.rect.is_none());
    }

    #[test]
    fn resize_corner_keeps_opposite_corner() {
        let mut s = selection((10.0, 10.0), (110.0, 110.0));
        s.start_resize(Handle::Nw);
        s.drag_to(Pos2::new(30.0, 40.0));
        s.end_drag();
        assert_eq!(
            s.rect,
            Some(Rect::from_min_max(
                Pos2::new(30.0, 40.0),
                Pos2::new(110.0, 110.0)
            ))
        );
    }

    #[test]
    fn resize_past_opposite_edge_flips() {
        let mut s = selection((10.0, 10.0), (110.0, 110.0));
        s.start_resize(Handle::E);
        s.drag_to(Pos2::new(0.0, 0.0));
        s.end_drag();
        assert_eq!(
            s.rect,
            Some(Rect::from_min_max(
                Pos2::new(0.0, 10.0),
                Pos2::new(10.0, 110.0)
            ))
        );
    }

    #[test]
    fn move_keeps_size_and_relative_position() {
        let mut s = selection((10.0, 10.0), (110.0, 60.0));
        s.start_move(Pos2::new(50.0, 30.0));
        s.drag_to(Pos2::new(70.0, 50.0));
        s.end_drag();
        let r = s.rect.unwrap();
        assert_eq!(r.size(), egui::vec2(100.0, 50.0));
        assert_eq!(
            r,
            Rect::from_min_max(Pos2::new(30.0, 30.0), Pos2::new(130.0, 80.0))
        );
    }

    #[test]
    fn handle_hit_takes_precedence_over_border() {
        let s = selection((10.0, 10.0), (110.0, 110.0));
        assert_eq!(
            s.hit_handle(Pos2::new(12.0, 12.0), HANDLE_HIT),
            Some(Handle::Nw)
        );
        assert_eq!(s.hit_handle(Pos2::new(60.0, 60.0), HANDLE_HIT), None);
        assert!(s.hit_border(Pos2::new(60.0, 10.0), BORDER_HIT));
        assert!(!s.hit_border(Pos2::new(60.0, 60.0), BORDER_HIT));
    }
}
