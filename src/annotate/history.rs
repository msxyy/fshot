use crate::annotate::object::{Annotation, Kind};

pub struct History {
    pub items: Vec<Annotation>,
    undo_stack: Vec<Vec<Annotation>>,
    redo_stack: Vec<Vec<Annotation>>,
    next_id: u64,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            next_id: 1,
        }
    }

    pub fn push(&mut self, mut a: Annotation) -> u64 {
        a.id = self.next_id;
        let id = a.id;
        self.next_id += 1;
        self.undo_stack.push(self.items.clone());
        self.redo_stack.clear();
        self.items.push(a);
        id
    }

    pub fn undo(&mut self) -> bool {
        if let Some(prev) = self.undo_stack.pop() {
            let cur = std::mem::replace(&mut self.items, prev);
            self.redo_stack.push(cur);
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.redo_stack.pop() {
            let cur = std::mem::replace(&mut self.items, next);
            self.undo_stack.push(cur);
            true
        } else {
            false
        }
    }

    pub fn remove(&mut self, id: u64) {
        if self.items.iter().any(|a| a.id == id) {
            self.undo_stack.push(self.items.clone());
            self.redo_stack.clear();
            self.items.retain(|a| a.id != id);
        }
    }

    /// 记录一次"修改"前推入撤销栈，但不改动 items；调用方随后自行修改。
    pub fn snapshot(&mut self) {
        self.undo_stack.push(self.items.clone());
        self.redo_stack.clear();
    }

    /// 修改文本标注内容；内容未变化时不产生撤销记录。
    pub fn set_text(&mut self, id: u64, new_text: String) -> bool {
        let Some(idx) = self.items.iter().position(|a| a.id == id) else {
            return false;
        };
        let current = match &self.items[idx].kind {
            Kind::Text { text, .. } => text.clone(),
            _ => return false,
        };
        if current == new_text {
            return false;
        }
        self.undo_stack.push(self.items.clone());
        self.redo_stack.clear();
        if let Kind::Text { text, .. } = &mut self.items[idx].kind {
            *text = new_text;
        }
        true
    }

    /// 当前最大序号（用于撤销/删除后回退序号计数）
    pub fn max_step_number(&self) -> u32 {
        self.items
            .iter()
            .filter_map(|a| match &a.kind {
                Kind::Step { number, .. } => Some(*number),
                _ => None,
            })
            .max()
            .unwrap_or(0)
    }

    pub fn find_mut(&mut self, id: u64) -> Option<&mut Annotation> {
        self.items.iter_mut().find(|a| a.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotate::object::{Kind, Style};
    use egui::Pos2;

    fn text(id: u64, content: &str) -> Annotation {
        Annotation {
            id,
            kind: Kind::Text {
                pos: Pos2::ZERO,
                text: content.to_string(),
                size: 18.0,
            },
            style: Style::default(),
        }
    }

    fn text_of(a: &Annotation) -> &str {
        match &a.kind {
            Kind::Text { text, .. } => text,
            _ => panic!("not a text annotation"),
        }
    }

    #[test]
    fn set_text_records_undo_only_when_changed() {
        let mut h = History::new();
        let id = h.push(text(0, "old"));
        let undo_len = h.undo_stack.len();

        assert!(!h.set_text(id, "old".to_string()));
        assert_eq!(h.undo_stack.len(), undo_len);

        assert!(h.set_text(id, "new".to_string()));
        assert_eq!(h.undo_stack.len(), undo_len + 1);
        assert_eq!(text_of(&h.items[0]), "new");
        assert!(h.undo());
        assert_eq!(text_of(&h.items[0]), "old");
    }

    #[test]
    fn max_step_number_tracks_remaining_steps() {
        let mut h = History::new();
        assert_eq!(h.max_step_number(), 0);
        h.push(Annotation {
            id: 0,
            kind: Kind::Step {
                center: Pos2::ZERO,
                radius: 14.0,
                number: 1,
            },
            style: Style::default(),
        });
        h.push(Annotation {
            id: 0,
            kind: Kind::Step {
                center: Pos2::ZERO,
                radius: 14.0,
                number: 2,
            },
            style: Style::default(),
        });
        assert_eq!(h.max_step_number(), 2);
        h.undo();
        assert_eq!(h.max_step_number(), 1);
    }
}