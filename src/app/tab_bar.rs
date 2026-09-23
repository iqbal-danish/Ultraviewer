use eframe::egui::{self, Color32, Layout, Pos2, Rect, RichText, Sense, Ui, Vec2};
use crate::formats::FileType;
use super::icons::{paint_icon, Icon};

#[derive(Debug, Clone)]
pub struct TabInfo<'a> {
    pub id: usize,
    pub name: &'a str,
    pub full_path: &'a str,
    pub file_type: Option<FileType>,
    pub is_dirty: bool,
    pub is_active: bool,
}

pub struct TabBarProps<'a> {
    pub tabs: Vec<TabInfo<'a>>,
    pub file_size: u64,
    pub is_dirty: bool,
    pub is_edit_mode: bool,
    pub word_wrap: bool,
    pub current_line: usize,
    pub total_lines: usize,
    pub breadcrumb: Option<&'a str>,
}

pub enum TabBarAction {
    NewBlankFile,
    SelectTab(usize),
    CloseTab(usize),
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

    // Clean Document Tabs Bar (VS Code Style)
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;

        for tab in &props.tabs {
            let tab_id = tab.id;
            let is_active = tab.is_active;

            let tab_bg = if is_active {
                Color32::from_rgb(30, 34, 39) // Active matches editor background
            } else {
                Color32::from_rgb(22, 25, 30) // Inactive tab background
            };
            let tab_border = Color32::from_rgb(24, 26, 31);
            let active_blue = Color32::from_rgb(97, 175, 239);

            let (rect, response) = ui.allocate_exact_size(Vec2::new(190.0, 36.0), Sense::click());
            let response = response.on_hover_text(tab.full_path);
            let is_hovered = response.hovered();

            let fill = if is_hovered && !is_active {
                Color32::from_rgb(34, 38, 45)
            } else {
                tab_bg
            };
            ui.painter().rect_filled(rect, 4.0, fill);
            ui.painter().rect_stroke(
                rect,
                4.0,
                egui::Stroke::new(1.0_f32, tab_border),
                egui::StrokeKind::Inside,
            );

            if is_active {
                // Active blue bottom indicator line
                let indicator_rect = Rect::from_min_size(
                    Pos2::new(rect.left() + 2.0, rect.bottom() - 2.0),
                    Vec2::new(rect.width() - 4.0, 2.0),
                );
                ui.painter().rect_filled(indicator_rect, 1.0, active_blue);
            }

            if response.clicked() && !is_active {
                action = Some(TabBarAction::SelectTab(tab_id));
            }

            // Tab contents inside
            let mut tab_ui = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            tab_ui.horizontal_centered(|ui| {
                ui.add_space(8.0);
                // Vector type icon
                let (type_icon, icon_color) = match tab.file_type {
                    Some(FileType::Xml) => (Icon::XmlCode, Color32::from_rgb(56, 189, 248)),
                    Some(FileType::Json) => (Icon::JsonBraces, Color32::from_rgb(250, 204, 21)),
                    _ => (Icon::File, Color32::from_rgb(148, 163, 184)),
                };
                let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                paint_icon(ui.painter(), icon_rect, type_icon, icon_color);

                ui.add_space(6.0);

                let display_name = if tab.name.len() > 18 {
                    format!("{}...", &tab.name[..15])
                } else {
                    tab.name.to_string()
                };

                let name_color = if tab.is_dirty {
                    Color32::from_rgb(245, 158, 11)
                } else if is_active {
                    Color32::from_rgb(240, 246, 252)
                } else {
                    Color32::from_rgb(150, 160, 175)
                };
                ui.label(RichText::new(&display_name).strong().size(14.0).color(name_color));

                if tab.is_dirty {
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
                    let (close_rect, close_resp) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
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

                    if close_resp.on_hover_text("Close Tab (Ctrl+W)").clicked() {
                        action = Some(TabBarAction::CloseTab(tab_id));
                    }
                });
            });
        }

        // Add File Tab button (+)
        let (new_tab_rect, new_tab_resp) = ui.allocate_exact_size(Vec2::new(32.0, 32.0), Sense::click());
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
        if new_tab_resp.on_hover_text("New Blank File (Ctrl+N)").clicked() {
            action = Some(TabBarAction::NewBlankFile);
        }

        // Right-aligned quick action buttons (Word Wrap toggle, etc.)
        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);
            let wrap_text = if props.word_wrap { "Wrap: ON" } else { "Wrap: OFF" };
            let wrap_color = if props.word_wrap {
                Color32::from_rgb(97, 175, 239)
            } else {
                Color32::from_rgb(140, 150, 165)
            };
            let (wrap_rect, wrap_resp) = ui.allocate_exact_size(Vec2::new(74.0, 26.0), Sense::click());
            let wrap_hovered = wrap_resp.hovered();
            let wrap_bg = if props.word_wrap {
                Color32::from_rgb(26, 42, 65)
            } else if wrap_hovered {
                Color32::from_rgb(34, 38, 45)
            } else {
                Color32::from_rgb(22, 25, 30)
            };
            let wrap_border = if props.word_wrap {
                Color32::from_rgb(56, 189, 248)
            } else {
                Color32::from_rgb(44, 49, 58)
            };
            ui.painter().rect_filled(wrap_rect, 4.0, wrap_bg);
            ui.painter().rect_stroke(wrap_rect, 4.0, egui::Stroke::new(1.0_f32, wrap_border), egui::StrokeKind::Inside);
            ui.painter().text(
                wrap_rect.center(),
                egui::Align2::CENTER_CENTER,
                wrap_text,
                egui::FontId::proportional(12.0),
                wrap_color,
            );
            if wrap_resp.on_hover_text("Toggle Word Wrap (Alt+Z)").clicked() {
                action = Some(TabBarAction::ToggleWrap);
            }
        });
    });

    action
}
