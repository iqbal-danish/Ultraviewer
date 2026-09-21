use std::path::PathBuf;
use eframe::egui::{self, Color32, RichText, Sense, Vec2};

pub struct DiffViewerState {
    pub is_open: bool,
    pub secondary_path: Option<PathBuf>,
    pub secondary_lines: Vec<String>,
}

impl Default for DiffViewerState {
    fn default() -> Self {
        Self {
            is_open: false,
            secondary_path: None,
            secondary_lines: Vec::new(),
        }
    }
}

pub enum DiffViewerAction {
    PickSecondaryFile,
    Close,
}

pub fn render_diff_modal(
    ctx: &egui::Context,
    state: &mut DiffViewerState,
    primary_lines: &[String],
    primary_name: &str,
) -> Option<DiffViewerAction> {
    if !state.is_open {
        return None;
    }

    let mut action = None;
    let screen_rect = ctx.screen_rect();
    let modal_w = (screen_rect.width() - 80.0).clamp(600.0, 1400.0);
    let modal_h = (screen_rect.height() - 80.0).clamp(400.0, 900.0);

    egui::Window::new("File Comparison & Diff")
        .collapsible(false)
        .resizable(true)
        .default_size(Vec2::new(modal_w, modal_h))
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Diff Comparison");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        action = Some(DiffViewerAction::Close);
                    }
                    if ui.button("Compare with File...").clicked() {
                        action = Some(DiffViewerAction::PickSecondaryFile);
                    }
                });
            });

            ui.separator();

            let sec_name = state.secondary_path.as_ref()
                .and_then(|p| p.file_name().and_then(|n| n.to_str()))
                .unwrap_or("No second file selected");

            ui.horizontal(|ui| {
                let pane_w = (ui.available_width() - 16.0) * 0.5;
                ui.allocate_ui_with_layout(Vec2::new(pane_w, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("Primary: {}", primary_name)).strong().color(Color32::from_rgb(97, 175, 239)));
                });
                ui.allocate_ui_with_layout(Vec2::new(pane_w, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("Compare: {}", sec_name)).strong().color(Color32::from_rgb(229, 192, 123)));
                });
            });

            ui.separator();

            let max_rows = primary_lines.len().max(state.secondary_lines.len());
            let mono_font = egui::FontId::monospace(11.5);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let pane_w = (ui.available_width() - 24.0) * 0.5;
                    for r in 0..max_rows {
                        let l1 = primary_lines.get(r).map(|s| s.as_str()).unwrap_or("");
                        let l2 = state.secondary_lines.get(r).map(|s| s.as_str()).unwrap_or("");
                        let is_diff = l1 != l2;

                        let (bg1, bg2) = if is_diff {
                            (Color32::from_rgba_premultiplied(224, 108, 117, 35), Color32::from_rgba_premultiplied(152, 195, 121, 35))
                        } else {
                            (Color32::TRANSPARENT, Color32::TRANSPARENT)
                        };

                        ui.horizontal(|ui| {
                            // Left Pane Line
                            let (r1, _) = ui.allocate_exact_size(Vec2::new(pane_w, 18.0), Sense::hover());
                            ui.painter().rect_filled(r1, 0.0, bg1);
                            ui.painter().text(
                                egui::Pos2::new(r1.left() + 4.0, r1.center().y),
                                egui::Align2::LEFT_CENTER,
                                format!("{:>4} | {}", r + 1, l1),
                                mono_font.clone(),
                                Color32::from_rgb(210, 220, 235),
                            );

                            ui.add_space(8.0);

                            // Right Pane Line
                            let (r2, _) = ui.allocate_exact_size(Vec2::new(pane_w, 18.0), Sense::hover());
                            ui.painter().rect_filled(r2, 0.0, bg2);
                            ui.painter().text(
                                egui::Pos2::new(r2.left() + 4.0, r2.center().y),
                                egui::Align2::LEFT_CENTER,
                                format!("{:>4} | {}", r + 1, l2),
                                mono_font.clone(),
                                Color32::from_rgb(210, 220, 235),
                            );
                        });
                    }
                });
        });

    action
}
