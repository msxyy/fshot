use egui::{Color32, Pos2};

use crate::annotate::object::{Style, Tool};
use crate::config::ToolPrefs;

#[derive(Clone, Debug)]
pub enum Draft {
    Shape { start: Pos2, current: Pos2 },
    Pen(Vec<Pos2>),
    Mosaic { start: Pos2, current: Pos2 },
}

pub struct ToolState {
    pub active: Tool,
    pub style: Style,
    pub fill: bool,
    pub draft: Option<Draft>,
    pub text_size: f32,
    pub mosaic_block: u32,
    /// 正在编辑的文本：(annotation_id, 当前草稿文本)
    pub text_edit: Option<(u64, String, Pos2)>,
    pub next_step: u32,
}

impl Default for ToolState {
    fn default() -> Self {
        Self {
            active: Tool::Rect,
            style: Style::default(),
            fill: false,
            draft: None,
            text_size: 18.0,
            mosaic_block: 16,
            text_edit: None,
            next_step: 1,
        }
    }
}

impl ToolState {
    /// 从持久化的偏好构造
    pub fn from_prefs(p: &ToolPrefs) -> Self {
        let active = match p.active.as_str() {
            "pointer" => Tool::Pointer,
            "ellipse" => Tool::Ellipse,
            "arrow" => Tool::Arrow,
            "line" => Tool::Line,
            "pen" => Tool::Pen,
            "text" => Tool::Text,
            "step" => Tool::Step,
            "mosaic" => Tool::Mosaic,
            _ => Tool::Rect,
        };
        let [r, g, b, a] = p.stroke;
        Self {
            active,
            style: Style {
                stroke: Color32::from_rgba_unmultiplied(r, g, b, a),
                stroke_width: p.stroke_width,
            },
            fill: p.fill,
            draft: None,
            text_size: p.text_size,
            mosaic_block: p.mosaic_block.max(2),
            text_edit: None,
            next_step: 1,
        }
    }

    /// 导出为持久化偏好
    pub fn to_prefs(&self) -> ToolPrefs {
        let active = match self.active {
            Tool::Pointer => "pointer",
            Tool::Rect => "rect",
            Tool::Ellipse => "ellipse",
            Tool::Arrow => "arrow",
            Tool::Line => "line",
            Tool::Pen => "pen",
            Tool::Text => "text",
            Tool::Step => "step",
            Tool::Mosaic => "mosaic",
        };
        let c = self.style.stroke;
        ToolPrefs {
            active: active.to_string(),
            stroke: [c.r(), c.g(), c.b(), c.a()],
            stroke_width: self.style.stroke_width,
            fill: self.fill,
            text_size: self.text_size,
            mosaic_block: self.mosaic_block,
        }
    }
}