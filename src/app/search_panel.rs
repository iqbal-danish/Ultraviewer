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
    ToggleReplace,
    ReplaceNext,
    ReplaceAll,
}

pub fn render_search_bar(
    ui: &mut Ui,
    query: &mut SearchQuery,
    _status: &SearchStatus,
    current_match_idx: Option<usize>,
    _stored_matches: usize,
    actual_matches: usize,
    request_focus: bool,
    show_replace: bool,
    replace_text: &mut String,
    request_replace_focus: bool,
) -> Option<SearchBarAction> {
    let mut action = None;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);

        // 1. Left Expand/Collapse Chevron Button (Vertical Pill)
        let tog_h = if show_replace { 48.0 } else { 24.0 };
        let (tog_rect, tog_resp) = ui.allocate_exact_size(Vec2::new(18.0, tog_h), Sense::click());
        let tog_hovered = tog_resp.hovered();
        let tog_bg = if tog_hovered { Color32::from_rgb(44, 49, 58) } else { Color32::from_rgb(33, 37, 43) };
        ui.painter().rect_filled(tog_rect, 3.0, tog_bg);
        ui.painter().rect_stroke(tog_rect, 3.0, egui::Stroke::new(1.0_f32, Color32::from_rgb(40, 44, 52)), egui::StrokeKind::Inside);
        let tog_icon = if show_replace { Icon::ChevronDown } else { Icon::ChevronRight };
        let tog_color = if tog_hovered { Color32::WHITE } else { Color32::from_rgb(171, 178, 191) };
        paint_icon(ui.painter(), Rect::from_center_size(tog_rect.center(), Vec2::splat(11.0)), tog_icon, tog_color);
        if tog_resp.on_hover_text("Toggle Replace (Ctrl+H)").clicked() {
            action = Some(SearchBarAction::ToggleReplace);
        }

        // 2. Rows container
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 3.0);

            // ── Row 1: [ Find input + Aa ab .* ]  "1 of 4"   ↑  ↓  ≡  ✕ ──
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;

                // Find input box with embedded filter toggles
                egui::Frame::NONE
                    .fill(Color32::from_rgb(27, 30, 36))  // #1B1E24 — sunken input well
                    .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(40, 44, 52))) // #282C34
                    .corner_radius(3.0)
                    .inner_margin(egui::Margin::symmetric(4, 2))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;

                            let input_resp = ui.add(
                                egui::TextEdit::singleline(&mut query.pattern)
                                    .desired_width(135.0)
                                    .font(egui::FontId::proportional(13.0))
                                    .hint_text("Find")
                                    .frame(false)
                            );
                            if request_focus {
                                input_resp.request_focus();
                            }
                            if (input_resp.has_focus() || input_resp.lost_focus()) && ui.input(|i| i.key_pressed(Key::Enter)) {
                                if ui.input(|i| i.modifiers.shift) {
                                    action = Some(SearchBarAction::FindPrev);
                                } else {
                                    action = Some(SearchBarAction::FindNext);
                                }
                            }

                            // Aa Toggle (Match Case)
                            let aa_active = query.case_sensitive;
                            let (aa_rect, aa_resp) = ui.allocate_exact_size(Vec2::new(20.0, 20.0), Sense::click());
                            if aa_active {
                                ui.painter().rect_filled(aa_rect, 2.0, Color32::from_rgb(44, 107, 180));
                                ui.painter().rect_stroke(aa_rect, 2.0, egui::Stroke::new(1.0_f32, Color32::from_rgb(97, 175, 239)), egui::StrokeKind::Inside);
                            } else if aa_resp.hovered() {
                                ui.painter().rect_filled(aa_rect, 2.0, Color32::from_rgb(44, 49, 58));
                            }
                            let aa_color = if aa_active || aa_resp.hovered() { Color32::WHITE } else { Color32::from_rgb(171, 178, 191) };
                            ui.painter().text(aa_rect.center(), egui::Align2::CENTER_CENTER, "Aa", egui::FontId::proportional(11.0), aa_color);
                            if aa_resp.on_hover_text("Match Case (Alt+C)").clicked() {
                                query.case_sensitive = !query.case_sensitive;
                            }

                            // ab Toggle (Match Whole Word with Underline)
                            let ab_active = query.whole_word;
                            let (ab_rect, ab_resp) = ui.allocate_exact_size(Vec2::new(20.0, 20.0), Sense::click());
                            if ab_active {
                                ui.painter().rect_filled(ab_rect, 2.0, Color32::from_rgb(44, 107, 180));
                                ui.painter().rect_stroke(ab_rect, 2.0, egui::Stroke::new(1.0_f32, Color32::from_rgb(97, 175, 239)), egui::StrokeKind::Inside);
                            } else if ab_resp.hovered() {
                                ui.painter().rect_filled(ab_rect, 2.0, Color32::from_rgb(44, 49, 58));
                            }
                            let ab_color = if ab_active || ab_resp.hovered() { Color32::WHITE } else { Color32::from_rgb(171, 178, 191) };
                            ui.painter().text(Pos2::new(ab_rect.center().x, ab_rect.center().y - 1.5), egui::Align2::CENTER_CENTER, "ab", egui::FontId::proportional(11.0), ab_color);
                            ui.painter().line_segment(
                                [Pos2::new(ab_rect.left() + 3.5, ab_rect.bottom() - 3.0), Pos2::new(ab_rect.right() - 3.5, ab_rect.bottom() - 3.0)],
                                egui::Stroke::new(1.2_f32, ab_color),
                            );
                            if ab_resp.on_hover_text("Match Whole Word (Alt+W)").clicked() {
                                query.whole_word = !query.whole_word;
                            }

                            // .* Toggle (Use Regular Expression)
                            let rx_active = query.is_regex;
                            let (rx_rect, rx_resp) = ui.allocate_exact_size(Vec2::new(20.0, 20.0), Sense::click());
                            if rx_active {
                                ui.painter().rect_filled(rx_rect, 2.0, Color32::from_rgb(44, 107, 180));
                                ui.painter().rect_stroke(rx_rect, 2.0, egui::Stroke::new(1.0_f32, Color32::from_rgb(97, 175, 239)), egui::StrokeKind::Inside);
                            } else if rx_resp.hovered() {
                                ui.painter().rect_filled(rx_rect, 2.0, Color32::from_rgb(44, 49, 58));
                            }
                            let rx_color = if rx_active || rx_resp.hovered() { Color32::WHITE } else { Color32::from_rgb(171, 178, 191) };
                            ui.painter().text(rx_rect.center(), egui::Align2::CENTER_CENTER, ".*", egui::FontId::proportional(11.0), rx_color);
                            if rx_resp.on_hover_text("Use Regular Expression (Alt+R)").clicked() {
                                query.is_regex = !query.is_regex;
                            }
                        });
                    });

                // Match count indicator e.g. "1 of 4" or "No results"
                let (count_text, count_color) = if query.is_empty() {
                    ("".to_string(), Color32::from_rgb(171, 178, 191))
                } else if actual_matches == 0 {
                    ("No results".to_string(), Color32::from_rgb(224, 108, 117)) // #E06C75 red
                } else if let Some(idx) = current_match_idx {
                    (
                        format!("{} of {}", format_number((idx + 1) as u64), format_number(actual_matches as u64)),
                        Color32::from_rgb(152, 195, 121), // #98C379 green
                    )
                } else {
                    (
                        format!("{}", format_number(actual_matches as u64)),
                        Color32::from_rgb(152, 195, 121),
                    )
                };

                let count_w = if count_text.is_empty() { 6.0 } else { 56.0 };
                ui.allocate_ui_with_layout(Vec2::new(count_w, 24.0), egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                    ui.label(RichText::new(count_text).size(11.5).color(count_color));
                });

                // Previous Match (↑)
                let (up_rect, up_resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
                if up_resp.hovered() {
                    ui.painter().rect_filled(up_rect, 3.0, Color32::from_rgb(44, 49, 58));
                }
                let up_color = if up_resp.hovered() { Color32::WHITE } else { Color32::from_rgb(171, 178, 191) };
                paint_icon(ui.painter(), up_rect.shrink(3.0), Icon::ArrowUp, up_color);
                if up_resp.on_hover_text("Previous Match (Shift+Enter)").clicked() {
                    action = Some(SearchBarAction::FindPrev);
                }

                // Next Match (↓)
                let (down_rect, down_resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
                if down_resp.hovered() {
                    ui.painter().rect_filled(down_rect, 3.0, Color32::from_rgb(44, 49, 58));
                }
                let down_color = if down_resp.hovered() { Color32::WHITE } else { Color32::from_rgb(171, 178, 191) };
                paint_icon(ui.painter(), down_rect.shrink(3.0), Icon::ArrowDown, down_color);
                if down_resp.on_hover_text("Next Match (Enter)").clicked() {
                    action = Some(SearchBarAction::FindNext);
                }

                // Find in Selection / All Matches (≡)
                let (sel_rect, sel_resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
                if sel_resp.hovered() {
                    ui.painter().rect_filled(sel_rect, 3.0, Color32::from_rgb(44, 49, 58));
                }
                let sel_color = if sel_resp.hovered() { Color32::WHITE } else { Color32::from_rgb(171, 178, 191) };
                paint_icon(ui.painter(), sel_rect.shrink(3.0), Icon::SelectionLines, sel_color);
                if sel_resp.on_hover_text("Find All Occurrences (Bottom Panel)").clicked() {
                    action = Some(SearchBarAction::FindAll);
                }

                // Close (✕)
                let (cl_rect, cl_resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
                if cl_resp.hovered() {
                    ui.painter().rect_filled(cl_rect, 3.0, Color32::from_rgb(44, 49, 58));
                }
                let cl_color = if cl_resp.hovered() { Color32::WHITE } else { Color32::from_rgb(171, 178, 191) };
                paint_icon(ui.painter(), cl_rect.shrink(3.0), Icon::Close, cl_color);
                if cl_resp.on_hover_text("Close (Esc)").clicked() {
                    action = Some(SearchBarAction::Close);
                }
            });

            // ── Row 2: [ Replace input + AB ]  [ReplaceOne] [ReplaceAll] ──
            if show_replace {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;

                    // Replace input box with AB toggle
                    egui::Frame::NONE
                        .fill(Color32::from_rgb(27, 30, 36))  // #1B1E24 — sunken input well
                        .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(40, 44, 52))) // #282C34
                        .corner_radius(3.0)
                        .inner_margin(egui::Margin::symmetric(4, 2))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;

                                let rep_resp = ui.add(
                                    egui::TextEdit::singleline(replace_text)
                                        .desired_width(175.0)
                                        .font(egui::FontId::proportional(13.0))
                                        .hint_text("Replace")
                                        .frame(false)
                                );
                                if request_replace_focus {
                                    rep_resp.request_focus();
                                }
                                if (rep_resp.has_focus() || rep_resp.lost_focus()) && ui.input(|i| i.key_pressed(Key::Enter)) {
                                    if ui.input(|i| i.modifiers.alt || i.modifiers.command) {
                                        action = Some(SearchBarAction::ReplaceAll);
                                    } else {
                                        action = Some(SearchBarAction::ReplaceNext);
                                    }
                                }

                                // AB (Preserve Case) button
                                let (ab_rect, ab_resp) = ui.allocate_exact_size(Vec2::new(24.0, 20.0), Sense::click());
                                if ab_resp.hovered() {
                                    ui.painter().rect_filled(ab_rect, 2.0, Color32::from_rgb(44, 49, 58));
                                }
                                let ab_c = if ab_resp.hovered() { Color32::WHITE } else { Color32::from_rgb(171, 178, 191) };
                                ui.painter().text(ab_rect.center(), egui::Align2::CENTER_CENTER, "AB", egui::FontId::proportional(11.0), ab_c);
                                if ab_resp.on_hover_text("Preserve Case (Alt+P)").clicked() {
                                    // Optional preserve case toggle
                                }
                            });
                        });

                    // Replace One Match button
                    let (r1_rect, r1_resp) = ui.allocate_exact_size(Vec2::new(20.0, 20.0), Sense::click());
                    if r1_resp.hovered() {
                        ui.painter().rect_filled(r1_rect, 3.0, Color32::from_rgb(44, 49, 58));
                    }
                    let r1_color = if r1_resp.hovered() { Color32::WHITE } else { Color32::from_rgb(171, 178, 191) };
                    paint_icon(ui.painter(), r1_rect.shrink(2.0), Icon::ReplaceOne, r1_color);
                    if r1_resp.on_hover_text("Replace (Enter)").clicked() {
                        action = Some(SearchBarAction::ReplaceNext);
                    }

                    // Replace All button
                    let (ra_rect, ra_resp) = ui.allocate_exact_size(Vec2::new(22.0, 20.0), Sense::click());
                    if ra_resp.hovered() {
                        ui.painter().rect_filled(ra_rect, 3.0, Color32::from_rgb(44, 49, 58));
                    }
                    let ra_color = if ra_resp.hovered() { Color32::WHITE } else { Color32::from_rgb(171, 178, 191) };
                    paint_icon(ui.painter(), ra_rect.shrink(2.0), Icon::ReplaceAll, ra_color);
                    if ra_resp.on_hover_text("Replace All (Ctrl+Alt+Enter)").clicked() {
                        action = Some(SearchBarAction::ReplaceAll);
                    }
                });
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
        ui.painter().rect_filled(tab_rect, 4.0, Color32::from_rgb(40, 44, 52));
        ui.painter().rect_stroke(
            tab_rect,
            4.0,
            egui::Stroke::new(1.0_f32, Color32::from_rgb(62, 68, 81)),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            Pos2::new(tab_rect.left() + 10.0, tab_rect.center().y),
            egui::Align2::LEFT_CENTER,
            results_label,
            egui::FontId::proportional(11.5),
            Color32::from_rgb(220, 225, 235),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (close_rect, close_resp) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
            if close_resp.hovered() {
                ui.painter().rect_filled(close_rect, 3.0, Color32::from_rgb(56, 62, 74));
            }
            paint_icon(ui.painter(), close_rect.shrink(3.0), Icon::Close, Color32::from_rgb(171, 178, 191));
            if close_resp.on_hover_text("Close Results Panel").clicked() {
                *on_close = true;
            }
        });
    });

    ui.add_space(4.0);

    // Results table headers
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let header_color = Color32::from_rgb(120, 130, 145);
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
                    Color32::from_rgb(44, 60, 85) // active match One Dark blue
                } else if idx % 2 == 1 {
                    Color32::from_rgb(27, 30, 35)
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
                            ui.label(RichText::new(format!("{}", idx + 1)).font(line_num_font.clone()).color(Color32::from_rgb(92, 99, 112)));

                            // Line Number
                            ui.allocate_ui_with_layout(Vec2::new(60.0, 16.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(format!("{}", m.line_number)).font(line_num_font.clone()).color(Color32::from_rgb(97, 175, 239)));
                            });

                            // Snippet Preview
                            ui.label(RichText::new(&m.snippet).font(text_font.clone()).color(Color32::from_rgb(171, 178, 191)));
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
