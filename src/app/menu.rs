use eframe::egui::{self, Color32, Pos2, Rect, Sense, Ui, Vec2};
use super::icons::{paint_icon, Icon};

pub enum MenuAction {
    OpenFile,
    CloseFile,
    SaveFile,
    SaveFileAs,
    Exit,
    Minimize,
    ToggleMaximize,
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
    OpenFolder,
    CommandPalette,
    ToggleCsvGrid,
    OpenDiffViewer,
    RegisterContextMenu,
    UnregisterContextMenu,
    SetTheme(super::theme::ColorTheme),
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
    font_size: f32,
    is_maximized: bool,
) -> Option<MenuAction> {
    let mut action = None;

    egui::menu::bar(ui, |ui| {
        // VS Code-style Brand Logo
        ui.add_space(2.0);
        let (logo_rect, logo_resp) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
        paint_icon(ui.painter(), logo_rect, Icon::Logo, Color32::from_rgb(0, 122, 204));
        if logo_resp.on_hover_text("UltraViewer - Ultra-Fast Large File Editor").clicked() {
            action = Some(MenuAction::About);
        }
        ui.add_space(6.0);

        ui.menu_button("File", |ui| {
            if ui.button("Open File... (Ctrl+O)").clicked() {
                action = Some(MenuAction::OpenFile);
                ui.close_menu();
            }

            if ui.button("Open Folder...").clicked() {
                action = Some(MenuAction::OpenFolder);
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
            let zoom_pct = ((font_size / 14.0) * 100.0).round() as u32;
            if ui.button("Zoom In (Ctrl +)").clicked() {
                action = Some(MenuAction::ZoomIn);
                ui.close_menu();
            }
            if ui.button("Zoom Out (Ctrl -)").clicked() {
                action = Some(MenuAction::ZoomOut);
                ui.close_menu();
            }
            if ui.button(format!("Reset Zoom (Ctrl 0) [{}%]", zoom_pct)).clicked() {
                action = Some(MenuAction::ZoomReset);
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Toggle CSV Grid View").clicked() {
                action = Some(MenuAction::ToggleCsvGrid);
                ui.close_menu();
            }
            ui.separator();
            ui.menu_button("Color Themes", |ui| {
                for theme in super::theme::ColorTheme::all() {
                    if ui.button(theme.name()).clicked() {
                        action = Some(MenuAction::SetTheme(*theme));
                        ui.close_menu();
                    }
                }
            });
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
            if ui.button("Command Palette... (Ctrl+Shift+P)").clicked() {
                action = Some(MenuAction::CommandPalette);
                ui.close_menu();
            }
            if ui.button("Compare Files (Diff)...").clicked() {
                action = Some(MenuAction::OpenDiffViewer);
                ui.close_menu();
            }
            ui.separator();
            ui.add_enabled_ui(file_is_open, |ui| {
                if ui.button("Field Analyzer & Schema Profiler... (Ctrl+Shift+A)").clicked() {
                    action = Some(MenuAction::AnalyzeFields);
                    ui.close_menu();
                }
            });
            ui.separator();
            if ui.button("Register 'Open with UltraViewer' Windows Context Menu").clicked() {
                action = Some(MenuAction::RegisterContextMenu);
                ui.close_menu();
            }
            if ui.button("Deregister / Remove 'Open with UltraViewer' Windows Context Menu").clicked() {
                action = Some(MenuAction::UnregisterContextMenu);
                ui.close_menu();
            }
        });

        ui.menu_button("Help", |ui| {
            if ui.button("About UltraViewer").clicked() {
                action = Some(MenuAction::About);
                ui.close_menu();
            }
        });

        // Right side: Window Controls, Theme Toggle, and Quick Search
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // 1. Close button (far right, 46px wide, turns bright red #E81123 on hover)
            let (close_rect, close_resp) = ui.allocate_exact_size(Vec2::new(46.0, 30.0), Sense::click());
            let close_hovered = close_resp.hovered();
            if close_hovered {
                ui.painter().rect_filled(close_rect, 0.0, Color32::from_rgb(232, 17, 35));
            }
            let close_icon_color = if close_hovered { Color32::WHITE } else { Color32::from_rgb(171, 178, 191) };
            let cc = close_rect.center();
            let d = 4.5;
            ui.painter().line_segment(
                [Pos2::new(cc.x - d, cc.y - d), Pos2::new(cc.x + d, cc.y + d)],
                egui::Stroke::new(1.0_f32, close_icon_color),
            );
            ui.painter().line_segment(
                [Pos2::new(cc.x - d, cc.y + d), Pos2::new(cc.x + d, cc.y - d)],
                egui::Stroke::new(1.0_f32, close_icon_color),
            );
            if close_resp.on_hover_text("Close").clicked() {
                action = Some(MenuAction::Exit);
            }

            // 2. Maximize / Restore button (46px wide)
            let (max_rect, max_resp) = ui.allocate_exact_size(Vec2::new(46.0, 30.0), Sense::click());
            let max_hovered = max_resp.hovered();
            if max_hovered {
                ui.painter().rect_filled(max_rect, 0.0, Color32::from_rgba_premultiplied(255, 255, 255, 20));
            }
            let max_icon_color = if max_hovered { Color32::WHITE } else { Color32::from_rgb(171, 178, 191) };
            let mc = max_rect.center();
            if is_maximized {
                // Restore icon: two overlapping squares
                let s = 4.0;
                // Back square
                ui.painter().rect_stroke(
                    Rect::from_center_size(Pos2::new(mc.x + 1.5, mc.y - 1.5), Vec2::splat(s * 2.0)),
                    0.0,
                    egui::Stroke::new(1.0_f32, max_icon_color),
                    egui::StrokeKind::Inside,
                );
                // Front square
                ui.painter().rect_filled(
                    Rect::from_center_size(Pos2::new(mc.x - 1.5, mc.y + 1.5), Vec2::splat(s * 2.0)),
                    0.0,
                    if max_hovered { Color32::from_rgb(44, 49, 58) } else { Color32::from_rgb(30, 34, 39) },
                );
                ui.painter().rect_stroke(
                    Rect::from_center_size(Pos2::new(mc.x - 1.5, mc.y + 1.5), Vec2::splat(s * 2.0)),
                    0.0,
                    egui::Stroke::new(1.0_f32, max_icon_color),
                    egui::StrokeKind::Inside,
                );
            } else {
                // Maximize icon: single square
                let s = 4.5;
                ui.painter().rect_stroke(
                    Rect::from_center_size(mc, Vec2::splat(s * 2.0)),
                    0.0,
                    egui::Stroke::new(1.0_f32, max_icon_color),
                    egui::StrokeKind::Inside,
                );
            }
            if max_resp.on_hover_text(if is_maximized { "Restore Down" } else { "Maximize" }).clicked() {
                action = Some(MenuAction::ToggleMaximize);
            }

            // 3. Minimize button (46px wide)
            let (min_rect, min_resp) = ui.allocate_exact_size(Vec2::new(46.0, 30.0), Sense::click());
            let min_hovered = min_resp.hovered();
            if min_hovered {
                ui.painter().rect_filled(min_rect, 0.0, Color32::from_rgba_premultiplied(255, 255, 255, 20));
            }
            let min_icon_color = if min_hovered { Color32::WHITE } else { Color32::from_rgb(171, 178, 191) };
            let min_c = min_rect.center();
            ui.painter().line_segment(
                [Pos2::new(min_c.x - 5.0, min_c.y), Pos2::new(min_c.x + 5.0, min_c.y)],
                egui::Stroke::new(1.0_f32, min_icon_color),
            );
            if min_resp.on_hover_text("Minimize").clicked() {
                action = Some(MenuAction::Minimize);
            }

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
            let (pill_rect, pill_resp) = ui.allocate_exact_size(Vec2::new(180.0, 22.0), Sense::click());
            let pill_hovered = pill_resp.hovered();
            let pill_bg = if pill_hovered { Color32::from_rgb(44, 49, 58) } else { Color32::from_rgb(26, 30, 35) };
            ui.painter().rect_filled(pill_rect, 4.0, pill_bg);
            ui.painter().rect_stroke(
                pill_rect,
                4.0,
                egui::Stroke::new(1.0_f32, Color32::from_rgb(20, 22, 26)),
                egui::StrokeKind::Inside,
            );

            let s_icon = Rect::from_min_size(Pos2::new(pill_rect.left() + 6.0, pill_rect.top() + 5.0), Vec2::splat(12.0));
            paint_icon(ui.painter(), s_icon, Icon::Search, Color32::from_rgb(140, 148, 160));

            ui.painter().text(
                Pos2::new(pill_rect.left() + 22.0, pill_rect.center().y),
                egui::Align2::LEFT_CENTER,
                "Command Palette...",
                egui::FontId::proportional(11.0),
                Color32::from_rgb(171, 178, 191),
            );

            ui.painter().text(
                Pos2::new(pill_rect.right() - 6.0, pill_rect.center().y),
                egui::Align2::RIGHT_CENTER,
                "Ctrl+Shift+P",
                egui::FontId::proportional(10.0),
                Color32::from_rgb(92, 99, 112),
            );

            if pill_resp.on_hover_text("Open Command Palette (Ctrl+Shift+P)").clicked() {
                action = Some(MenuAction::CommandPalette);
            }

            // Window drag handle in the remaining center empty space
            let remaining_rect = ui.available_rect_before_wrap();
            if remaining_rect.width() > 10.0 {
                let drag_resp = ui.interact(remaining_rect, ui.id().with("window_drag_area"), Sense::click_and_drag());
                if drag_resp.drag_started() || (drag_resp.hovered() && ui.input(|i| i.pointer.primary_pressed())) {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if drag_resp.double_clicked() {
                    action = Some(MenuAction::ToggleMaximize);
                }
            }
        });
    });

    action
}
