use eframe::egui::{self, Color32, RichText, Sense, Ui, Vec2};
use crate::editor::Viewport;

pub struct CsvGridState {
    pub is_enabled: bool,
    pub delimiter: char,
    pub headers: Vec<String>,
}

impl Default for CsvGridState {
    fn default() -> Self {
        Self {
            is_enabled: false,
            delimiter: ',',
            headers: Vec::new(),
        }
    }
}

pub fn render_csv_grid(
    ui: &mut Ui,
    viewport: &Viewport,
    font_size: f32,
    delimiter: char,
    cached_headers: &[String],
) {
    if viewport.lines.is_empty() {
        return;
    }

    let text_font = egui::FontId::monospace(font_size);
    let line_num_font = egui::FontId::monospace((font_size * 0.9).max(10.0));
    let line_num_color = Color32::from_rgb(92, 99, 112);

    let headers: Vec<String> = if !cached_headers.is_empty() {
        cached_headers.to_vec()
    } else {
        viewport.lines.first()
            .map(|l| split_delimited(&l.text, delimiter))
            .unwrap_or_default()
    };
    let col_count = headers.len().max(1);

    let available_w = ui.available_width().max(400.0);
    let col_w = (available_w / col_count as f32).clamp(120.0, 320.0);
    let total_table_w = 70.0 + (col_count as f32 * col_w);

    egui::ScrollArea::horizontal()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(total_table_w);

            // Table Header Row (Sticky at top)
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(egui::vec2(60.0, 24.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("#").font(line_num_font.clone()).color(line_num_color));
                });

                for (_i, col_name) in headers.iter().enumerate() {
                    let (h_rect, _) = ui.allocate_exact_size(Vec2::new(col_w, 24.0), Sense::hover());
                    ui.painter().rect_filled(h_rect, 2.0, Color32::from_rgb(36, 42, 50));
                    ui.painter().rect_stroke(h_rect, 2.0, egui::Stroke::new(1.0_f32, Color32::from_rgb(24, 26, 31)), egui::StrokeKind::Inside);
                    ui.painter().text(
                        egui::Pos2::new(h_rect.left() + 6.0, h_rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        col_name,
                        egui::FontId::proportional(11.5),
                        Color32::from_rgb(229, 192, 123),
                    );
                }
            });

            ui.separator();

            // Data Rows (Vertical lines from viewport)
            ui.vertical(|ui| {
                for (r_idx, line) in viewport.lines.iter().enumerate() {
                    // Skip line 1 if it is the header row
                    if line.line_number == 1 && !cached_headers.is_empty() {
                        continue;
                    }

                    let cols = split_delimited(&line.text, delimiter);
                    let row_bg = if r_idx % 2 == 0 {
                        Color32::from_rgb(30, 34, 39)
                    } else {
                        Color32::from_rgb(26, 30, 35)
                    };

                    ui.horizontal(|ui| {
                        // Line Number
                        ui.allocate_ui_with_layout(egui::vec2(60.0, 22.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(line.line_number.to_string()).font(line_num_font.clone()).color(line_num_color));
                        });

                        for i in 0..col_count {
                            let val = cols.get(i).map(|s| s.as_str()).unwrap_or("");
                            let (c_rect, _) = ui.allocate_exact_size(Vec2::new(col_w, 22.0), Sense::hover());
                            ui.painter().rect_filled(c_rect, 0.0, row_bg);
                            ui.painter().rect_stroke(c_rect, 0.0, egui::Stroke::new(0.5_f32, Color32::from_rgb(24, 26, 31)), egui::StrokeKind::Inside);

                            ui.painter().text(
                                egui::Pos2::new(c_rect.left() + 6.0, c_rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                val,
                                text_font.clone(),
                                Color32::from_rgb(210, 220, 235),
                            );
                        }
                    });
                }
            });
        });
}

pub fn split_delimited(text: &str, delimiter: char) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;

    for ch in text.chars() {
        if ch == '"' {
            in_quotes = !in_quotes;
        } else if ch == delimiter && !in_quotes {
            fields.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(ch);
        }
    }
    fields.push(cur.trim().to_string());
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_delimited_csv() {
        let line = "Company,Count,\"City, State\",Website";
        let fields = split_delimited(line, ',');
        assert_eq!(fields, vec!["Company", "Count", "City, State", "Website"]);
    }

    #[test]
    fn test_split_delimited_tsv() {
        let line = "Apple\t100\tCupertino";
        let fields = split_delimited(line, '\t');
        assert_eq!(fields, vec!["Apple", "100", "Cupertino"]);
    }
}
