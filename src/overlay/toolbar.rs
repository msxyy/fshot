use egui::{Align2, Color32, Context, Pos2, Rect};

use crate::annotate::object::{Style, Tool};

const TOOLS: &[(Tool, &str, &str)] = &[
    (Tool::Pointer, "▶", "指针 / 选中"),
    (Tool::Rect, "□", "矩形"),
    (Tool::Ellipse, "○", "椭圆"),
    (Tool::Arrow, "→", "箭头"),
    (Tool::Line, "／", "直线"),
    (Tool::Pen, "✎", "画笔"),
    (Tool::Text, "T", "文本"),
    (Tool::Step, "#", "序号"),
    (Tool::Mosaic, "▦", "马赛克"),
];

pub fn draw(
    ctx: &Context,
    anchor: Rect,
    screen: Rect,
    active: &mut Tool,
    style: &mut Style,
    fill: &mut bool,
    text_size: &mut f32,
    mosaic_block: &mut u32,
    reselect: &mut bool,
) -> Rect {
    let tb_h = 48.0_f32;
    let below_y = anchor.max.y + 10.0_f32;
    let above_y = anchor.min.y - 10.0_f32;
    let (y, pivot) = if below_y + tb_h < screen.max.y {
        (below_y, Align2::CENTER_TOP)
    } else {
        (above_y, Align2::CENTER_BOTTOM)
    };

    let mut result = Rect::NOTHING;
    egui::Area::new(egui::Id::new("fshot-toolbar"))
        .fixed_pos(Pos2::new(anchor.center().x, y))
        .pivot(pivot)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(Color32::from_black_alpha(235))
                .inner_margin(6.0_f32)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (tool, label, tip) in TOOLS {
                            let selected = *active == *tool;
                            if ui
                                .selectable_label(selected, *label)
                                .on_hover_text(*tip)
                                .clicked()
                            {
                                *active = *tool;
                            }
                        }
                        ui.separator();
                        ui.add(
                            egui::DragValue::new(&mut style.stroke_width)
                                .speed(0.1_f32)
                                .range(0.5_f32..=20.0_f32)
                                .prefix("线宽 "),
                        );
                        ui.add(
                            egui::DragValue::new(text_size)
                                .speed(0.5_f32)
                                .range(8.0_f32..=72.0_f32)
                                .prefix("字 "),
                        );
                        ui.add(
                            egui::DragValue::new(mosaic_block)
                                .speed(1.0_f32)
                                .range(4_u32..=64_u32)
                                .prefix("块 "),
                        );
                        ui.separator();
                        ui.color_edit_button_srgba(&mut style.stroke);
                        ui.checkbox(fill, "填充");
                        ui.separator();
                        if ui
                            .button("⟲ 重选")
                            .on_hover_text("清除当前选区，重新框选")
                            .clicked()
                        {
                            *reselect = true;
                        }
                    });
                });
            result = ui.min_rect();
        });
    result
}