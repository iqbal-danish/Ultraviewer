use eframe::egui::{Color32, Pos2, Rect, RichText, Sense, Ui, Vec2};
use crate::file_engine::Encoding;
use crate::formats::FileType;
use std::time::Duration;

pub enum StatusBarAction {
    ResetZoom,
}

pub struct StatusBarProps<'a> {
    pub file_name: Option<&'a str>,
    pub file_size: u64,
    pub encoding: Option<Encoding>,
    pub file_type: Option<FileType>,
    pub visible_lines_count: usize,
    pub current_line: usize,
    pub current_offset: u64,
    pub memory_rss_bytes: u64,
    pub open_latency: Option<Duration>,
    pub indexing_pct: Option<f32>,
    pub is_indexing_complete: bool,
    pub indexing_speed_mb: u64,
    pub total_indexed_lines: usize,
    pub is_dirty: bool,
    pub edit_count: usize,
    pub is_edit_mode: bool,
    pub font_size: f32,
}

pub fn render_status_bar(ui: &mut Ui, props: StatusBarProps) -> Option<StatusBarAction> {
    let mut action = None;
    let sep_color = Color32::from_rgb(44, 49, 58); // #2C313A
    let muted_color = Color32::from_rgb(171, 178, 191); // #ABB2BF

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;

        // File name & Size
        if let Some(name) = props.file_name {
            let display_name = if props.is_dirty { format!("{} ●", name) } else { name.to_string() };
            let name_color = if props.is_dirty { Color32::from_rgb(229, 192, 123) } else { Color32::from_rgb(220, 225, 235) };
            ui.label(RichText::new(display_name).strong().size(11.5).color(name_color));

            let size_str = format_bytes(props.file_size);
            render_status_sep(ui, sep_color);
            ui.label(RichText::new(size_str).size(11.0).color(muted_color));
        } else {
            ui.label(RichText::new("UltraViewer Ready").size(11.0).color(muted_color));
        }

        if let Some(enc) = props.encoding {
            render_status_sep(ui, sep_color);
            ui.label(RichText::new(enc.name()).size(11.0).color(muted_color));
        }

        if let Some(ft) = props.file_type {
            render_status_sep(ui, sep_color);
            let ft_color = match ft {
                FileType::Xml => Color32::from_rgb(224, 108, 117),
                FileType::Json => Color32::from_rgb(229, 192, 123),
                _ => Color32::from_rgb(97, 175, 239),
            };
            ui.label(RichText::new(ft.name()).size(11.0).color(ft_color));
        }

        if props.file_size > 0 {
            render_status_sep(ui, sep_color);
            ui.label(
                RichText::new(format!("{} lines", format_number(props.total_indexed_lines as u64)))
                    .size(11.0)
                    .color(muted_color),
            );

            render_status_sep(ui, sep_color);
            ui.label(
                RichText::new(format!("Ln {}", format_number(props.current_line as u64)))
                    .size(11.0)
                    .color(Color32::from_rgb(210, 220, 235)),
            );
        }

        // Right-aligned status indicators: Indexing speed, RAM, and Ready status
        ui.with_layout(eframe::egui::Layout::right_to_left(eframe::egui::Align::Center), |ui| {
            ui.add_space(8.0);

            // Ready Status indicator with glowing green circle (no emoji!)
            let (ready_rect, _) = ui.allocate_exact_size(Vec2::new(56.0, 16.0), Sense::hover());
            let dot_center = Pos2::new(ready_rect.left() + 6.0, ready_rect.center().y);
            // Glow ring
            ui.painter().circle_filled(dot_center, 4.0, Color32::from_rgba_premultiplied(74, 222, 128, 60));
            // Core dot
            ui.painter().circle_filled(dot_center, 2.5, Color32::from_rgb(52, 211, 153));
            ui.painter().text(
                Pos2::new(ready_rect.left() + 16.0, ready_rect.center().y),
                eframe::egui::Align2::LEFT_CENTER,
                "Ready",
                eframe::egui::FontId::proportional(11.0),
                Color32::from_rgb(200, 215, 230),
            );

            // Zoom indicator badge (e.g. "100%")
            let zoom_pct = ((props.font_size / 14.0) * 100.0).round() as u32;
            render_status_sep(ui, sep_color);
            let zoom_resp = ui.selectable_label(
                false,
                RichText::new(format!("{}%", zoom_pct)).size(11.0).color(muted_color),
            );
            if zoom_resp.on_hover_text(format!("Zoom: {}% (Ctrl+Scroll or Ctrl+/- to adjust, click to reset)", zoom_pct)).clicked() {
                action = Some(StatusBarAction::ResetZoom);
            }

            render_status_sep(ui, sep_color);

            // Memory RSS badge
            let ram_str = format_bytes(props.memory_rss_bytes);
            ui.label(RichText::new(format!("{} RAM", ram_str)).size(11.0).color(muted_color));

            // Indexing throughput bar
            if props.file_size > 0 {
                if !props.is_indexing_complete {
                    if let Some(pct) = props.indexing_pct {
                        let speed_str = if props.indexing_speed_mb > 1024 {
                            format!("{:.1} GB/s", props.indexing_speed_mb as f64 / 1024.0)
                        } else {
                            format!("{} MB/s", props.indexing_speed_mb)
                        };

                        render_status_sep(ui, sep_color);
                        ui.label(
                            RichText::new(speed_str)
                                .size(11.0)
                                .color(Color32::from_rgb(56, 189, 248)),
                        );

                        let (rect, _) = ui.allocate_exact_size(Vec2::new(80.0, 7.0), Sense::hover());
                        ui.painter().rect_filled(rect, 3.5, Color32::from_rgb(20, 30, 45));
                        let fill_w = rect.width() * (pct / 100.0).clamp(0.0, 1.0);
                        let fill_rect = Rect::from_min_size(rect.min, Vec2::new(fill_w, 7.0));
                        ui.painter().rect_filled(fill_rect, 3.5, Color32::from_rgb(0, 120, 212));

                        ui.label(
                            RichText::new(format!("Indexing {:.0}%", pct))
                                .size(11.0)
                                .color(Color32::from_rgb(200, 215, 235)),
                        );
                    }
                }
            }
        });
    });

    action
}

fn render_status_sep(ui: &mut Ui, color: Color32) {
    ui.painter().vline(
        ui.cursor().min.x,
        (ui.cursor().min.y + 4.0)..=(ui.cursor().max.y - 4.0),
        eframe::egui::Stroke::new(1.0_f32, color),
    );
    ui.add_space(2.0);
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;

    let b = bytes as f64;
    if b >= TB {
        format!("{:.2} TB", b / TB)
    } else if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.2} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

pub fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let len = s.len();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result
}
