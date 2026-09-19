use eframe::egui::{self, Color32, Pos2, Rect, Sense, Ui, Vec2};
use super::icons::{paint_icon, Icon};

pub enum MenuAction {
    OpenFile,
    CloseFile,
    SaveFile,
    SaveFileAs,
    Exit,
    Undo,
    Redo,
    ToggleEditMode,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ToggleTheme,
    GoToLine,
    Find,
    XmlValidate,
    XmlToggleTree,
    XmlToggleHighlight,
    JsonValidate,
    JsonToggleTree,
    JsonToggleHighlight,
    FormatBeautify2,
    FormatBeautify4,
    FormatMinify,
    FormatSaveAs,
    AnalyzeFields,
    About,
}

pub fn render_menu_bar(
    ui: &mut Ui,
    file_is_open: bool,
    is_xml: bool,
    is_json: bool,
    is_dirty: bool,
    can_undo: bool,
    can_redo: bool,
    is_edit_mode: bool,
) -> Option<MenuAction> {
    let mut action = None;

    egui::menu::bar(ui, |ui| {
        ui.menu_button("File", |ui| {
            if ui.button("Open File... (Ctrl+O)").clicked() {
                action = Some(MenuAction::OpenFile);
                ui.close_menu();
            }

            if file_is_open {
                ui.add_enabled_ui(is_dirty, |ui| {
                    if ui.button("Save (Ctrl+S)").clicked() {
                        action = Some(MenuAction::SaveFile);
                        ui.close_menu();
                    }
                });

                if ui.button("Save As... (Ctrl+Shift+S)").clicked() {
                    action = Some(MenuAction::SaveFileAs);
                    ui.close_menu();
                }

                ui.separator();

                if ui.button("Close File").clicked() {
                    action = Some(MenuAction::CloseFile);
                    ui.close_menu();
                }
            }

            ui.separator();

            if ui.button("Exit").clicked() {
                action = Some(MenuAction::Exit);
                ui.close_menu();
            }
        });

        ui.menu_button("Edit", |ui| {
            ui.add_enabled_ui(file_is_open, |ui| {
                ui.add_enabled_ui(can_undo, |ui| {
                    if ui.button("Undo (Ctrl+Z)").clicked() {
                        action = Some(MenuAction::Undo);
                        ui.close_menu();
                    }
                });

                ui.add_enabled_ui(can_redo, |ui| {
                    if ui.button("Redo (Ctrl+Y)").clicked() {
                        action = Some(MenuAction::Redo);
                        ui.close_menu();
                    }
                });

                ui.separator();

                let edit_label = if is_edit_mode {
                    "Switch to View Mode (Ctrl+E)"
                } else {
                    "Switch to Edit Mode (Ctrl+E)"
                };
                if ui.button(edit_label).clicked() {
                    action = Some(MenuAction::ToggleEditMode);
                    ui.close_menu();
                }

                ui.separator();

                if ui.button("Find... (Ctrl+F)").clicked() {
                    action = Some(MenuAction::Find);
                    ui.close_menu();
                }
                if ui.button("Go to Line... (Ctrl+G)").clicked() {
                    action = Some(MenuAction::GoToLine);
                    ui.close_menu();
                }
            });
        });

        ui.menu_button("View", |ui| {
            if ui.button("Zoom In (Ctrl +)").clicked() {
                action = Some(MenuAction::ZoomIn);
                ui.close_menu();
            }
            if ui.button("Zoom Out (Ctrl -)").clicked() {
                action = Some(MenuAction::ZoomOut);
                ui.close_menu();
            }
            if ui.button("Reset Zoom (Ctrl 0)").clicked() {
                action = Some(MenuAction::ZoomReset);
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Toggle Dark / Light Theme").clicked() {
                action = Some(MenuAction::ToggleTheme);
                ui.close_menu();
            }
        });

        if is_xml {
            ui.menu_button("XML", |ui| {
                if ui.button("Validate XML Document").clicked() {
                    action = Some(MenuAction::XmlValidate);
                    ui.close_menu();
                }
                if ui.button("Toggle XML Structure Tree").clicked() {
                    action = Some(MenuAction::XmlToggleTree);
                    ui.close_menu();
                }
                let hl_label = "Toggle Syntax Highlighting";
                if ui.button(hl_label).clicked() {
                    action = Some(MenuAction::XmlToggleHighlight);
                    ui.close_menu();
                }
            });
        }

        if is_json {
            ui.menu_button("JSON", |ui| {
                if ui.button("Validate JSON Document").clicked() {
                    action = Some(MenuAction::JsonValidate);
                    ui.close_menu();
                }
                if ui.button("Toggle JSON Structure Tree").clicked() {
                    action = Some(MenuAction::JsonToggleTree);
                    ui.close_menu();
                }
                let hl_label = "Toggle Syntax Highlighting";
                if ui.button(hl_label).clicked() {
                    action = Some(MenuAction::JsonToggleHighlight);
                    ui.close_menu();
                }
            });
        }

        if is_xml || is_json {
            ui.menu_button("Format", |ui| {
                if ui.button("Beautify Document (2 Spaces)").clicked() {
                    action = Some(MenuAction::FormatBeautify2);
                    ui.close_menu();
                }
                if ui.button("Beautify Document (4 Spaces)").clicked() {
                    action = Some(MenuAction::FormatBeautify4);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Minify Document (Compact)").clicked() {
                    action = Some(MenuAction::FormatMinify);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Save Formatted Copy As...").clicked() {
                    action = Some(MenuAction::FormatSaveAs);
                    ui.close_menu();
                }
            });
        }

        ui.menu_button("Tools", |ui| {
            ui.add_enabled_ui(file_is_open, |ui| {
                if ui.button("Field Analyzer & Schema Profiler... (Ctrl+Shift+A)").clicked() {
                    action = Some(MenuAction::AnalyzeFields);
                    ui.close_menu();
                }
            });
        });

        ui.menu_button("Help", |ui| {
            if ui.button("About UltraViewer").clicked() {
                action = Some(MenuAction::About);
                ui.close_menu();
            }
        });

        // Right side: Quick Search Pill & Theme Toggle (matching reference image media_1789811265859.jpg)
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);

            // Theme toggle icon
            let (sun_rect, sun_resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
            let sun_hovered = sun_resp.hovered();
            let sun_color = if sun_hovered { Color32::WHITE } else { Color32::from_rgb(130, 140, 155) };
            paint_icon(ui.painter(), sun_rect.shrink(3.0), Icon::Sun, sun_color);
            if sun_resp.on_hover_text("Toggle Theme").clicked() {
                action = Some(MenuAction::ToggleTheme);
            }

            ui.add_space(8.0);

            // Quick Search Pill
            let (pill_rect, pill_resp) = ui.allocate_exact_size(Vec2::new(160.0, 22.0), Sense::click());
            let pill_hovered = pill_resp.hovered();
            let pill_bg = if pill_hovered { Color32::from_rgb(20, 28, 42) } else { Color32::from_rgb(14, 20, 30) };
            ui.painter().rect_filled(pill_rect, 4.0, pill_bg);
            ui.painter().rect_stroke(
                pill_rect,
                4.0,
                egui::Stroke::new(1.0_f32, Color32::from_rgb(28, 38, 54)),
                egui::StrokeKind::Inside,
            );

            let s_icon = Rect::from_min_size(Pos2::new(pill_rect.left() + 6.0, pill_rect.top() + 5.0), Vec2::splat(12.0));
            paint_icon(ui.painter(), s_icon, Icon::Search, Color32::from_rgb(110, 120, 135));

            ui.painter().text(
                Pos2::new(pill_rect.left() + 22.0, pill_rect.center().y),
                egui::Align2::LEFT_CENTER,
                "Search in file...",
                egui::FontId::proportional(11.0),
                Color32::from_rgb(110, 120, 135),
            );

            ui.painter().text(
                Pos2::new(pill_rect.right() - 6.0, pill_rect.center().y),
                egui::Align2::RIGHT_CENTER,
                "Ctrl+F",
                egui::FontId::proportional(10.0),
                Color32::from_rgb(70, 80, 95),
            );

            if pill_resp.on_hover_text("Quick Search (Ctrl+F)").clicked() {
                action = Some(MenuAction::Find);
            }
        });
    });

    action
}
