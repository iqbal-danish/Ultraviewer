use eframe::egui::{self, Color32, Layout, Pos2, Rect, RichText, Sense, Ui, Vec2};
use crate::formats::FileType;
use super::icons::{paint_icon, Icon};

pub struct TabBarProps<'a> {
    pub file_name: Option<&'a str>,
    pub file_type: Option<FileType>,
    pub file_size: u64,
    pub is_dirty: bool,
    pub is_edit_mode: bool,
    pub word_wrap: bool,
    pub current_line: usize,
    pub total_lines: usize,
    pub breadcrumb: Option<&'a str>,
}

pub enum TabBarAction {
    OpenFile,
    CloseFile,
    ToggleEditMode,
    SaveFile,
    ToggleWrap,
    ToggleSearch,
    ToggleTree,
    FormatBeautify,
    FormatMinify,
}

pub fn render_tab_bar(
    ui: &mut Ui,
    props: &TabBarProps,
) -> Option<TabBarAction> {
    let mut action = None;

    // Row 1: Document Tabs Bar
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;

        if let Some(file_name) = props.file_name {
            // Active Tab
            let tab_bg = Color32::from_rgb(12, 17, 26); // #0C111A sleek obsidian
            let tab_border = Color32::from_rgb(30, 42, 60); // #1E2A3C
            let active_blue = Color32::from_rgb(0, 120, 212); // #0078D4

            let (rect, _response) = ui.allocate_exact_size(Vec2::new(190.0, 32.0), Sense::hover());
            ui.painter().rect_filled(rect, 4.0, tab_bg);
            ui.painter().rect_stroke(
                rect,
                4.0,
                egui::Stroke::new(1.0_f32, tab_border),
                egui::StrokeKind::Inside,
            );

            // Active blue bottom indicator line (or top highlight line matching reference)
            let indicator_rect = Rect::from_min_size(
                Pos2::new(rect.left() + 4.0, rect.bottom() - 2.0),
                Vec2::new(rect.width() - 8.0, 2.0),
            );
            ui.painter().rect_filled(indicator_rect, 1.0, active_blue);

            // Tab contents inside
            let mut tab_ui = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            tab_ui.horizontal_centered(|ui| {
                ui.add_space(8.0);
                // Vector type icon
                let (type_icon, icon_color) = match props.file_type {
                    Some(FileType::Xml) => (Icon::XmlCode, Color32::from_rgb(56, 189, 248)),
                    Some(FileType::Json) => (Icon::JsonBraces, Color32::from_rgb(250, 204, 21)),
                    _ => (Icon::File, Color32::from_rgb(148, 163, 184)),
                };
                let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                paint_icon(ui.painter(), icon_rect, type_icon, icon_color);

                ui.add_space(4.0);

                let display_name = if file_name.len() > 17 {
                    format!("{}...", &file_name[..14])
                } else {
                    file_name.to_string()
                };

                let name_color = if props.is_dirty {
                    Color32::from_rgb(245, 158, 11)
                } else {
                    Color32::from_rgb(240, 246, 252)
                };
                ui.label(RichText::new(&display_name).strong().size(12.0).color(name_color));

                if props.is_dirty {
                    ui.painter().circle_filled(
                        Pos2::new(ui.cursor().min.x + 4.0, rect.center().y),
                        3.0,
                        Color32::from_rgb(245, 158, 11),
                    );
                    ui.add_space(8.0);
                }

                // Tab close button (vector cross)
                ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(6.0);
                    let (close_rect, close_resp) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::click());
                    let close_hovered = close_resp.hovered();
                    if close_hovered {
                        ui.painter().rect_filled(close_rect, 3.0, Color32::from_rgb(45, 25, 30));
                    }
                    let close_color = if close_hovered {
                        Color32::from_rgb(248, 113, 113)
                    } else {
                        Color32::from_rgb(120, 130, 145)
                    };
                    paint_icon(ui.painter(), close_rect.shrink(3.0), Icon::Close, close_color);

                    if close_resp.on_hover_text("Close Document (Ctrl+W)").clicked() {
                        action = Some(TabBarAction::CloseFile);
                    }
                });
            });
        }

        // Add File Tab button (+)
        let (new_tab_rect, new_tab_resp) = ui.allocate_exact_size(Vec2::new(28.0, 28.0), Sense::click());
        let new_tab_hovered = new_tab_resp.hovered();
        let new_tab_bg = if new_tab_hovered {
            Color32::from_rgb(26, 36, 52)
        } else {
            Color32::from_rgb(14, 20, 30)
        };
        ui.painter().rect_filled(new_tab_rect, 4.0, new_tab_bg);
        paint_icon(
            ui.painter(),
            new_tab_rect.shrink(7.0),
            Icon::Plus,
            if new_tab_hovered { Color32::WHITE } else { Color32::from_rgb(130, 140, 155) },
        );
        if new_tab_resp.on_hover_text("Open Another File (Ctrl+O)").clicked() {
            action = Some(TabBarAction::OpenFile);
        }

        // Right side quick actions
        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);

            if props.file_name.is_some() {
                // Edit / View Mode Toggle
                let (btn_rect, btn_resp) = ui.allocate_exact_size(Vec2::new(88.0, 26.0), Sense::click());
                let hovered = btn_resp.hovered();
                let (bg_color, text_color, icon) = if props.is_edit_mode {
                    (Color32::from_rgb(48, 32, 10), Color32::from_rgb(245, 158, 11), Icon::Pencil)
                } else {
                    (
                        if hovered { Color32::from_rgb(24, 32, 46) } else { Color32::from_rgb(15, 21, 32) },
                        Color32::from_rgb(140, 150, 165),
                        Icon::Lock,
                    )
                };
                ui.painter().rect_filled(btn_rect, 4.0, bg_color);
                let icon_box = Rect::from_min_size(Pos2::new(btn_rect.left() + 6.0, btn_rect.top() + 6.0), Vec2::splat(14.0));
                paint_icon(ui.painter(), icon_box, icon, text_color);
                let label_text = if props.is_edit_mode { "Edit Mode" } else { "Read Only" };
                ui.painter().text(
                    Pos2::new(btn_rect.left() + 24.0, btn_rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    label_text,
                    egui::FontId::proportional(11.0),
                    text_color,
                );
                if btn_resp.on_hover_text("Toggle In-Place Edit Mode").clicked() {
                    action = Some(TabBarAction::ToggleEditMode);
                }

                // Save button (enabled if dirty)
                if props.is_dirty {
                    let (save_rect, save_resp) = ui.allocate_exact_size(Vec2::new(65.0, 26.0), Sense::click());
                    let save_hovered = save_resp.hovered();
                    let save_bg = if save_hovered { Color32::from_rgb(0, 140, 240) } else { Color32::from_rgb(0, 120, 212) };
                    ui.painter().rect_filled(save_rect, 4.0, save_bg);
                    let s_icon_box = Rect::from_min_size(Pos2::new(save_rect.left() + 6.0, save_rect.top() + 6.0), Vec2::splat(14.0));
                    paint_icon(ui.painter(), s_icon_box, Icon::Save, Color32::WHITE);
                    ui.painter().text(
                        Pos2::new(save_rect.left() + 24.0, save_rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        "Save",
                        egui::FontId::proportional(11.0),
                        Color32::WHITE,
                    );
                    if save_resp.on_hover_text("Save File (Ctrl+S)").clicked() {
                        action = Some(TabBarAction::SaveFile);
                    }
                }

                // Word wrap button
                let (wrap_rect, wrap_resp) = ui.allocate_exact_size(Vec2::new(75.0, 26.0), Sense::click());
                let wrap_hovered = wrap_resp.hovered();
                let (w_bg, w_color) = if props.word_wrap {
                    (Color32::from_rgb(16, 40, 68), Color32::from_rgb(56, 189, 248))
                } else {
                    (
                        if wrap_hovered { Color32::from_rgb(24, 32, 46) } else { Color32::from_rgb(15, 21, 32) },
                        Color32::from_rgb(140, 150, 165),
                    )
                };
                ui.painter().rect_filled(wrap_rect, 4.0, w_bg);
                let w_icon_box = Rect::from_min_size(Pos2::new(wrap_rect.left() + 6.0, wrap_rect.top() + 6.0), Vec2::splat(14.0));
                paint_icon(ui.painter(), w_icon_box, Icon::Wrap, w_color);
                let w_label = if props.word_wrap { "Wrap ON" } else { "Wrap" };
                ui.painter().text(
                    Pos2::new(wrap_rect.left() + 24.0, wrap_rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    w_label,
                    egui::FontId::proportional(11.0),
                    w_color,
                );
                if wrap_resp.on_hover_text("Toggle Word Wrap (Alt+Z)").clicked() {
                    action = Some(TabBarAction::ToggleWrap);
                }

                // Find button
                let (find_rect, find_resp) = ui.allocate_exact_size(Vec2::new(65.0, 26.0), Sense::click());
                let find_hovered = find_resp.hovered();
                let f_bg = if find_hovered { Color32::from_rgb(24, 32, 46) } else { Color32::from_rgb(15, 21, 32) };
                ui.painter().rect_filled(find_rect, 4.0, f_bg);
                let f_icon_box = Rect::from_min_size(Pos2::new(find_rect.left() + 6.0, find_rect.top() + 6.0), Vec2::splat(14.0));
                paint_icon(ui.painter(), f_icon_box, Icon::Search, Color32::from_rgb(140, 150, 165));
                ui.painter().text(
                    Pos2::new(find_rect.left() + 24.0, find_rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    "Find",
                    egui::FontId::proportional(11.0),
                    Color32::from_rgb(180, 190, 205),
                );
                if find_resp.on_hover_text("Find in Document (Ctrl+F)").clicked() {
                    action = Some(TabBarAction::ToggleSearch);
                }

                // Format dropdown / quick action if XML/JSON
                if matches!(props.file_type, Some(FileType::Xml | FileType::Json)) {
                    ui.menu_button(RichText::new("Format").size(11.0).color(Color32::from_rgb(180, 190, 205)), |ui| {
                        if ui.button("Beautify (Format)").clicked() {
                            action = Some(TabBarAction::FormatBeautify);
                            ui.close_menu();
                        }
                        if ui.button("Minify (Compact)").clicked() {
                            action = Some(TabBarAction::FormatMinify);
                            ui.close_menu();
                        }
                    });
                }
            }
        });
    });

    // Row 2: Breadcrumb strip & metadata badges
    if let Some(file_name) = props.file_name {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;

            // Breadcrumb path
            let breadcrumb_text = if let Some(bc) = props.breadcrumb {
                format!("{}  >  {}", file_name, bc)
            } else {
                format!("{}  >  Ln {}", file_name, props.current_line)
            };

            ui.label(
                RichText::new(breadcrumb_text)
                    .size(11.0)
                    .color(Color32::from_rgb(100, 115, 135)),
            );

            // Badges on right side of breadcrumb
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);

                // Read-Only badge
                let (ro_rect, _) = ui.allocate_exact_size(Vec2::new(76.0, 18.0), Sense::hover());
                ui.painter().rect_filled(ro_rect, 3.0, Color32::from_rgb(16, 24, 38));
                let ro_icon_box = Rect::from_min_size(Pos2::new(ro_rect.left() + 4.0, ro_rect.top() + 3.0), Vec2::splat(12.0));
                paint_icon(ui.painter(), ro_icon_box, Icon::Lock, Color32::from_rgb(56, 189, 248));
                ui.painter().text(
                    Pos2::new(ro_rect.left() + 19.0, ro_rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    "Read Only",
                    egui::FontId::proportional(10.0),
                    Color32::from_rgb(180, 195, 215),
                );

                // File size badge
                let size_mb = (props.file_size as f64) / (1024.0 * 1024.0);
                let size_text = if size_mb >= 1024.0 {
                    format!("{:.2} GB", size_mb / 1024.0)
                } else {
                    format!("{:.1} MB", size_mb)
                };
                render_pill_badge(ui, &size_text, Color32::from_rgb(20, 28, 42), Color32::from_rgb(140, 160, 190));

                // File Type Badge
                if let Some(ft) = props.file_type {
                    let type_str = format!("{:?}", ft).to_uppercase();
                    render_pill_badge(ui, &type_str, Color32::from_rgb(16, 36, 58), Color32::from_rgb(56, 189, 248));
                }
            });
        });
        ui.add_space(2.0);
    }

    action
}

fn render_pill_badge(ui: &mut Ui, text: &str, bg: Color32, fg: Color32) {
    let frame = egui::Frame::NONE
        .fill(bg)
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(6, 1));
    frame.show(ui, |ui| {
        ui.label(RichText::new(text).size(10.0).color(fg).strong());
    });
}
