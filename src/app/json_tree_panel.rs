use eframe::egui::{self, Color32, RichText, ScrollArea, Sense, Ui, Vec2};
use crate::formats::JsonTreeNode;
use super::icons::{paint_icon, Icon};

pub fn render_json_tree_panel(
    ui: &mut Ui,
    root: Option<&JsonTreeNode>,
    is_building: bool,
    on_jump: &mut Option<u64>,
    on_close: &mut bool,
) {
    // Header with Title and Close button
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("JSON Structure")
                .strong()
                .size(13.0)
                .color(Color32::from_rgb(240, 245, 255)),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (c_rect, c_resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
            if c_resp.hovered() {
                ui.painter().rect_filled(c_rect, 3.0, Color32::from_rgb(45, 25, 30));
            }
            paint_icon(ui.painter(), c_rect.shrink(4.0), Icon::Close, Color32::from_rgb(140, 155, 175));
            if c_resp.on_hover_text("Close Panel").clicked() {
                *on_close = true;
            }
        });
    });

    ui.add_space(6.0);
    ui.separator();
    ui.add_space(6.0);

    if is_building {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(RichText::new("Indexing JSON structure...").size(12.0).color(Color32::from_rgb(56, 189, 248)));
        });
        ui.add_space(8.0);
    }

    if let Some(node) = root {
        // Full height scrollable hierarchy tree
        ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                render_json_node(ui, node, on_jump);
            });
    } else if !is_building {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("No JSON structure indexed yet.").weak().italics());
        });
    }
}

fn render_json_node(ui: &mut Ui, node: &JsonTreeNode, on_jump: &mut Option<u64>) {
    let has_children = !node.children.is_empty();

    let header_label = if let Some(ref sum) = node.summary {
        format!("{} ({})", node.name, sum)
    } else {
        node.name.clone()
    };

    let text_color = if node.is_array {
        Color32::from_rgb(250, 204, 21) // Gold
    } else if has_children {
        Color32::from_rgb(56, 189, 248) // Cyan
    } else {
        Color32::from_rgb(220, 230, 245)
    };

    if has_children {
        egui::CollapsingHeader::new(RichText::new(&header_label).color(text_color))
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.small_button("Jump").on_hover_text("Jump to element in document").clicked() {
                        *on_jump = Some(node.byte_offset);
                    }
                    ui.label(RichText::new(format!("offset: {}", node.byte_offset)).size(10.0).weak());
                });
                for child in &node.children {
                    render_json_node(ui, child, on_jump);
                }
            });
    } else {
        ui.horizontal(|ui| {
            let resp = ui.selectable_label(false, RichText::new(&header_label).color(text_color));
            if resp.on_hover_text("Click to jump to element").clicked() {
                *on_jump = Some(node.byte_offset);
            }
        });
    }
}
