use eframe::egui::{self, Color32, Layout, Pos2, Rect, Sense, Ui, Vec2};
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
    pub is_hex_mode: bool,
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
    ToggleHexView,
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
        ui.spacing_mut().item_spacing.x = 0.0;

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

            let tab_w = 190.0_f32;
            let close_w = 28.0_f32;
            let body_w = tab_w - close_w;
            let tab_h = 36.0_f32;

            // Direct allocation of body and close button ensuring 100% reliable click detection
            let (body_rect, body_resp) = ui.allocate_exact_size(Vec2::new(body_w, tab_h), Sense::click());
            let (close_rect, close_resp) = ui.allocate_exact_size(Vec2::new(close_w, tab_h), Sense::click());

            let full_tab_rect = body_rect.union(close_rect);
            let body_hovered = body_resp.hovered();
            let close_hovered = close_resp.hovered();
            let is_hovered = body_hovered || close_hovered;

            let fill = if is_hovered && !is_active {
                Color32::from_rgb(34, 38, 45)
            } else {
                tab_bg
            };

            ui.painter().rect_filled(full_tab_rect, 4.0, fill);
            ui.painter().rect_stroke(
                full_tab_rect,
                4.0,
                egui::Stroke::new(1.0_f32, tab_border),
                egui::StrokeKind::Inside,
            );

            if is_active {
                // Active blue bottom indicator line
                let indicator_rect = Rect::from_min_size(
                    Pos2::new(full_tab_rect.left() + 2.0, full_tab_rect.bottom() - 2.0),
                    Vec2::new(full_tab_rect.width() - 4.0, 2.0),
                );
                ui.painter().rect_filled(indicator_rect, 1.0, active_blue);
            }

            // Render Tab Body Content (icon, label, dirty dot) directly via painter (zero interference with click events)
            let icon_size = 16.0;
            let icon_rect = Rect::from_center_size(
                Pos2::new(body_rect.left() + 16.0, body_rect.center().y),
                Vec2::splat(icon_size),
            );
            let (type_icon, icon_color) = match tab.file_type {
                Some(FileType::Xml) => (Icon::XmlCode, Color32::from_rgb(56, 189, 248)),
                Some(FileType::Json) => (Icon::JsonBraces, Color32::from_rgb(250, 204, 21)),
                _ => (Icon::File, Color32::from_rgb(148, 163, 184)),
            };
            paint_icon(ui.painter(), icon_rect, type_icon, icon_color);

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

            let text_pos = Pos2::new(icon_rect.right() + 6.0, body_rect.center().y);
            let font_id = egui::FontId::new(13.0, egui::FontFamily::Proportional);
            let galley = ui.painter().layout_no_wrap(display_name, font_id, name_color);
            let text_w = galley.size().x;
            ui.painter().galley(
                Pos2::new(text_pos.x, body_rect.center().y - galley.size().y / 2.0),
                galley,
                name_color,
            );

            if tab.is_dirty {
                ui.painter().circle_filled(
                    Pos2::new(text_pos.x + text_w + 6.0, body_rect.center().y),
                    3.0,
                    Color32::from_rgb(245, 158, 11),
                );
            }

            // Render Close Button inside close_rect
            let close_btn_box = Rect::from_center_size(close_rect.center(), Vec2::splat(18.0));
            if close_hovered {
                ui.painter().rect_filled(close_btn_box, 3.0, Color32::from_rgb(55, 28, 34));
            }
            let close_color = if close_hovered {
                Color32::from_rgb(248, 113, 113)
            } else {
                Color32::from_rgb(120, 130, 145)
            };
            paint_icon(ui.painter(), close_btn_box.shrink(3.0), Icon::Close, close_color);

            // Tooltips and click events
            let close_resp = close_resp.on_hover_text("Close Tab (Ctrl+W)");
            let body_resp = body_resp.on_hover_text(tab.full_path);

            if close_resp.clicked() || body_resp.middle_clicked() {
                action = Some(TabBarAction::CloseTab(tab_id));
            } else if body_resp.clicked() {
                action = Some(TabBarAction::SelectTab(tab_id));
            }

            ui.add_space(4.0);
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

        // Right-aligned quick action buttons (Word Wrap, Hex View toggle, etc.)
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

            ui.add_space(4.0);
            let hex_text = if props.is_hex_mode { "Hex: ON" } else { "Hex: OFF" };
            let hex_color = if props.is_hex_mode {
                Color32::from_rgb(152, 195, 121)
            } else {
                Color32::from_rgb(140, 150, 165)
            };
            let (hex_rect, hex_resp) = ui.allocate_exact_size(Vec2::new(70.0, 26.0), Sense::click());
            let hex_hovered = hex_resp.hovered();
            let hex_bg = if props.is_hex_mode {
                Color32::from_rgb(28, 48, 38)
            } else if hex_hovered {
                Color32::from_rgb(34, 38, 45)
            } else {
                Color32::from_rgb(22, 25, 30)
            };
            let hex_border = if props.is_hex_mode {
                Color32::from_rgb(74, 222, 128)
            } else {
                Color32::from_rgb(44, 49, 58)
            };
            ui.painter().rect_filled(hex_rect, 4.0, hex_bg);
            ui.painter().rect_stroke(hex_rect, 4.0, egui::Stroke::new(1.0_f32, hex_border), egui::StrokeKind::Inside);
            ui.painter().text(
                hex_rect.center(),
                egui::Align2::CENTER_CENTER,
                hex_text,
                egui::FontId::proportional(12.0),
                hex_color,
            );
            if hex_resp.on_hover_text("Toggle Zero-Copy Hex & Binary Inspector (Ctrl+Shift+H)").clicked() {
                action = Some(TabBarAction::ToggleHexView);
            }
        });
    });

    action
}
