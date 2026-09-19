use eframe::egui::{self, Color32, Key, Pos2, Rect, RichText, ScrollArea, Sense, Ui, Vec2};
use crate::search::{SearchQuery, SearchResultMatch, SearchStatus};
use super::icons::{paint_icon, Icon};
use super::status_bar::format_number;

pub enum SearchBarAction {
    FindAll,
    FindNext,
    FindPrev,
    Cancel,
    Close,
}

pub fn render_search_bar(
    ui: &mut Ui,
    query: &mut SearchQuery,
    status: &SearchStatus,
    current_match_idx: Option<usize>,
    stored_matches: usize,
    actual_matches: usize,
    request_focus: bool,
) -> Option<SearchBarAction> {
    let mut action = None;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;

        // Search icon
        let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
        paint_icon(ui.painter(), icon_rect, Icon::Search, Color32::from_rgb(56, 189, 248));

        let input_resp = ui.add(
            egui::TextEdit::singleline(&mut query.pattern)
                .desired_width(220.0)
                .hint_text("Search in file...")
        );

        if request_focus {
            input_resp.request_focus();
        }

        let enter_pressed = (input_resp.has_focus() || input_resp.lost_focus())
            && ui.input(|i| i.key_pressed(Key::Enter));

        if enter_pressed {
            if ui.input(|i| i.modifiers.shift) {
                action = Some(SearchBarAction::FindPrev);
            } else {
                action = Some(SearchBarAction::FindNext);
            }
        }

        // Toggles
        let case_bg = if query.case_sensitive { Color32::from_rgb(16, 40, 68) } else { Color32::from_rgb(15, 21, 32) };
        let case_fg = if query.case_sensitive { Color32::from_rgb(56, 189, 248) } else { Color32::from_rgb(130, 140, 155) };
        let (c_rect, c_resp) = ui.allocate_exact_size(Vec2::new(26.0, 24.0), Sense::click());
        ui.painter().rect_filled(c_rect, 3.0, case_bg);
        ui.painter().text(c_rect.center(), egui::Align2::CENTER_CENTER, "Aa", egui::FontId::proportional(11.0), case_fg);
        if c_resp.on_hover_text("Match Case").clicked() {
            query.case_sensitive = !query.case_sensitive;
        }

        let regex_bg = if query.is_regex { Color32::from_rgb(16, 40, 68) } else { Color32::from_rgb(15, 21, 32) };
        let regex_fg = if query.is_regex { Color32::from_rgb(56, 189, 248) } else { Color32::from_rgb(130, 140, 155) };
        let (r_rect, r_resp) = ui.allocate_exact_size(Vec2::new(26.0, 24.0), Sense::click());
        ui.painter().rect_filled(r_rect, 3.0, regex_bg);
        ui.painter().text(r_rect.center(), egui::Align2::CENTER_CENTER, ".*", egui::FontId::proportional(11.0), regex_fg);
        if r_resp.on_hover_text("Regular Expression").clicked() {
            query.is_regex = !query.is_regex;
        }

        ui.checkbox(&mut query.whole_word, "Whole word");

        // Actual Match count indicator with formatted commas
        if actual_matches > 0 {
            let count_text = if let Some(idx) = current_match_idx {
                if stored_matches < actual_matches {
                    format!(
                        "{}/{} matches ({} navigable)",
                        format_number((idx + 1) as u64),
                        format_number(actual_matches as u64),
                        format_number(stored_matches as u64),
                    )
                } else {
                    format!(
                        "{}/{} matches",
                        format_number((idx + 1) as u64),
                        format_number(actual_matches as u64)
                    )
                }
            } else {
                format!("{} matches", format_number(actual_matches as u64))
            };
            ui.label(RichText::new(count_text).size(11.5).strong().color(Color32::from_rgb(180, 195, 215)));
        }

        // Prev / Next arrow buttons
        let (prev_rect, prev_resp) = ui.allocate_exact_size(Vec2::new(24.0, 24.0), Sense::click());
        let prev_bg = if prev_resp.hovered() { Color32::from_rgb(26, 36, 52) } else { Color32::from_rgb(16, 22, 34) };
        ui.painter().rect_filled(prev_rect, 3.0, prev_bg);
        let p_chev = Rect::from_center_size(prev_rect.center(), Vec2::splat(12.0));
        paint_icon(ui.painter(), p_chev, Icon::ChevronDown, Color32::from_rgb(180, 195, 215)); // or up/down
        if prev_resp.on_hover_text("Find Previous (Shift+Enter)").clicked() {
            action = Some(SearchBarAction::FindPrev);
        }

        let (next_rect, next_resp) = ui.allocate_exact_size(Vec2::new(24.0, 24.0), Sense::click());
        let next_bg = if next_resp.hovered() { Color32::from_rgb(26, 36, 52) } else { Color32::from_rgb(16, 22, 34) };
        ui.painter().rect_filled(next_rect, 3.0, next_bg);
        let n_chev = Rect::from_center_size(next_rect.center(), Vec2::splat(12.0));
        paint_icon(ui.painter(), n_chev, Icon::ChevronRight, Color32::from_rgb(180, 195, 215));
        if next_resp.on_hover_text("Find Next (Enter)").clicked() {
            action = Some(SearchBarAction::FindNext);
        }

        // Vibrant Blue Search Button
        let (s_btn_rect, s_btn_resp) = ui.allocate_exact_size(Vec2::new(65.0, 24.0), Sense::click());
        let s_btn_bg = if s_btn_resp.hovered() { Color32::from_rgb(16, 130, 228) } else { Color32::from_rgb(0, 120, 212) };
        ui.painter().rect_filled(s_btn_rect, 4.0, s_btn_bg);
        ui.painter().text(
            s_btn_rect.center(),
            egui::Align2::CENTER_CENTER,
            "Search",
            egui::FontId::proportional(11.5),
            Color32::WHITE,
        );
        if s_btn_resp.on_hover_text("Find All Occurrences").clicked() {
            action = Some(SearchBarAction::FindAll);
        }

        // Status & Progress display
        match status {
            SearchStatus::Searching { progress_pct, matches_found, speed_mb_s } => {
                let speed_str = if *speed_mb_s > 1024 {
                    format!("{:.1} GB/s", *speed_mb_s as f64 / 1024.0)
                } else {
                    format!("{} MB/s", speed_mb_s)
                };
                ui.colored_label(
                    Color32::from_rgb(245, 158, 11),
                    format!("Searching: {:.1}% ({} matches, {})", progress_pct, matches_found, speed_str)
                );
                if ui.button("Cancel").clicked() {
                    action = Some(SearchBarAction::Cancel);
                }
            }
            SearchStatus::Completed { matches_found, elapsed_secs } => {
                if *matches_found == 0 {
                    ui.colored_label(Color32::from_rgb(248, 113, 113), "No matches found");
                } else if current_match_idx.is_none() {
                    ui.label(format!("({:.2}s)", elapsed_secs));
                }
            }
            SearchStatus::Cancelled { matches_found } => {
                ui.colored_label(Color32::from_rgb(245, 158, 11), format!("Cancelled ({} matches)", format_number(*matches_found as u64)));
            }
            SearchStatus::Error(err) => {
                ui.colored_label(Color32::from_rgb(248, 113, 113), format!("Error: {}", err));
            }
            SearchStatus::Idle => {}
        }

        // Close button
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (close_rect, close_resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
            let close_hovered = close_resp.hovered();
            if close_hovered {
                ui.painter().rect_filled(close_rect, 3.0, Color32::from_rgb(45, 25, 30));
            }
            let close_color = if close_hovered { Color32::from_rgb(248, 113, 113) } else { Color32::from_rgb(130, 140, 155) };
            paint_icon(ui.painter(), close_rect.shrink(4.0), Icon::Close, close_color);
            if close_resp.on_hover_text("Close Search (Esc)").clicked() {
                action = Some(SearchBarAction::Close);
            }
        });
    });

    action
}

pub fn render_search_results_panel(
    ui: &mut Ui,
    matches: &[SearchResultMatch],
    actual_matches: usize,
    active_idx: Option<usize>,
    on_select: &mut Option<usize>,
    on_close: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;

        let results_label = if matches.len() < actual_matches {
            format!("Search Results ({} of {})", format_number(matches.len() as u64), format_number(actual_matches as u64))
        } else {
            format!("Search Results ({})", format_number(actual_matches as u64))
        };

        // Active tab pill
        let (tab_rect, _) = ui.allocate_exact_size(Vec2::new(160.0, 24.0), Sense::hover());
        ui.painter().rect_filled(tab_rect, 4.0, Color32::from_rgb(16, 26, 42));
        ui.painter().rect_stroke(
            tab_rect,
            4.0,
            egui::Stroke::new(1.0_f32, Color32::from_rgb(30, 50, 80)),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            Pos2::new(tab_rect.left() + 10.0, tab_rect.center().y),
            egui::Align2::LEFT_CENTER,
            results_label,
            egui::FontId::proportional(11.5),
            Color32::from_rgb(240, 245, 255),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (close_rect, close_resp) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
            if close_resp.hovered() {
                ui.painter().rect_filled(close_rect, 3.0, Color32::from_rgb(45, 25, 30));
            }
            paint_icon(ui.painter(), close_rect.shrink(3.0), Icon::Close, Color32::from_rgb(130, 140, 155));
            if close_resp.on_hover_text("Close Results Panel").clicked() {
                *on_close = true;
            }
        });
    });

    ui.add_space(4.0);

    // Results table headers
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let header_color = Color32::from_rgb(110, 125, 145);
        ui.label(RichText::new("#").size(11.0).color(header_color).strong());
        ui.allocate_ui_with_layout(Vec2::new(60.0, 16.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new("Line").size(11.0).color(header_color).strong());
        });
        ui.label(RichText::new("Preview").size(11.0).color(header_color).strong());
    });
    ui.separator();

    let text_font = egui::FontId::monospace(12.0);
    let line_num_font = egui::FontId::monospace(12.0);

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (idx, m) in matches.iter().enumerate().take(1000) {
                let is_active = Some(idx) == active_idx;
                let bg_color = if is_active {
                    Color32::from_rgb(19, 38, 65) // active match blue
                } else if idx % 2 == 1 {
                    Color32::from_rgb(10, 14, 22)
                } else {
                    Color32::TRANSPARENT
                };

                let response = egui::Frame::NONE
                    .fill(bg_color)
                    .corner_radius(2.0)
                    .inner_margin(egui::Margin::symmetric(4, 2))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            // Index
                            ui.label(RichText::new(format!("{}", idx + 1)).font(line_num_font.clone()).color(Color32::from_rgb(90, 100, 115)));

                            // Line Number
                            ui.allocate_ui_with_layout(Vec2::new(60.0, 16.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(format!("{}", m.line_number)).font(line_num_font.clone()).color(Color32::from_rgb(56, 189, 248)));
                            });

                            // Snippet Preview
                            ui.label(RichText::new(&m.snippet).font(text_font.clone()).color(Color32::from_rgb(220, 230, 242)));
                        });
                    }).response;

                if response.interact(egui::Sense::click()).clicked() {
                    *on_select = Some(idx);
                }
            }
            if matches.len() > 1000 {
                ui.label(RichText::new(format!("... and {} more matches (showing first 1,000)", matches.len() - 1000)).weak().italics());
            }
        });
}
