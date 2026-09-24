use eframe::egui::{self, Color32, Key, Pos2, Rect, RichText, Sense, Ui, Vec2};
use super::icons::{paint_icon, Icon};
use crate::formats::{FileType, QuerySuggestion, SuggestionCategory, SuggestionKind};

pub enum BreadcrumbAction {
    CopyPath(String),
    ToggleQueryBar(String),
    ExecuteQuery(String),
    NextMatch,
    PrevMatch,
    ActivateSliceView,
    ExtractToCsv,
    CloseQuery,
}

pub struct BreadcrumbBarProps<'a> {
    pub file_type: Option<FileType>,
    pub file_name: Option<&'a str>,
    pub current_path: Option<&'a str>,
    pub dark_mode: bool,
    pub is_query_open: bool,
    pub query_text: &'a mut String,
    pub match_count: usize,
    pub current_match_idx: usize,
    pub is_searching: bool,
    pub is_slice_active: bool,
    pub suggestions: &'a [QuerySuggestion],
    pub show_suggestions: &'a mut bool,
}

pub fn render_breadcrumb_bar(ui: &mut Ui, props: BreadcrumbBarProps) -> Option<BreadcrumbAction> {
    let mut action = None;
    let path = props.current_path.unwrap_or("");

    let is_xml = matches!(props.file_type, Some(FileType::Xml));
    let is_json = matches!(props.file_type, Some(FileType::Json));

    if !is_xml && !is_json {
        return None;
    }

    // Cohesive One Dark background and border palette
    let bg_color = if props.dark_mode {
        Color32::from_rgb(33, 37, 43) // #21252B
    } else {
        Color32::from_rgb(240, 243, 246)
    };

    let border_color = if props.dark_mode {
        Color32::from_rgb(40, 44, 52) // #282C34
    } else {
        Color32::from_rgb(215, 220, 225)
    };

    let text_color = if props.dark_mode {
        Color32::from_rgb(220, 225, 232)
    } else {
        Color32::from_rgb(30, 35, 45)
    };

    let sep_color = if props.dark_mode {
        Color32::from_rgb(92, 99, 112) // #5C6370
    } else {
        Color32::from_rgb(140, 148, 160)
    };

    let label_color = if is_xml {
        Color32::from_rgb(224, 108, 117) // Coral Red for XML XPath
    } else {
        Color32::from_rgb(229, 192, 123) // Warm Gold for JSONPath
    };

    let type_label = if is_xml { "XPath" } else { "JSONPath" };

    egui::Frame::NONE
        .fill(bg_color)
        .stroke(egui::Stroke::new(1.0_f32, border_color))
        .inner_margin(egui::Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;

                if !props.is_query_open {
                    // Path Type Badge (pill styling matching query bar)
                    let type_badge = egui::Frame::NONE
                        .fill(label_color.gamma_multiply(0.18))
                        .stroke(egui::Stroke::new(1.0_f32, label_color.gamma_multiply(0.85)))
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(8, 4));

                    type_badge.show(ui, |ui| {
                        ui.label(
                            RichText::new(type_label)
                                .size(13.5)
                                .strong()
                                .color(label_color),
                        );
                    });

                    // ── Inactive Query State: Full Breadcrumb trail on left, Action buttons on right ──
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Query Bar Open Button
                        let (q_fill, q_stroke, q_text_color) = if props.dark_mode {
                            (
                                Color32::from_rgb(44, 49, 58),
                                Color32::from_rgb(60, 68, 80),
                                Color32::from_rgb(171, 178, 191),
                            )
                        } else {
                            (
                                Color32::from_rgb(225, 230, 238),
                                Color32::from_rgb(195, 205, 218),
                                Color32::from_rgb(60, 68, 80),
                            )
                        };

                        let query_btn = egui::Button::new(
                            RichText::new("🔍 Query")
                                .size(13.5)
                                .strong()
                                .color(q_text_color),
                        )
                        .min_size(Vec2::new(0.0, 28.0))
                        .fill(q_fill)
                        .stroke(egui::Stroke::new(1.0_f32, q_stroke))
                        .corner_radius(4.0);

                        if ui.add(query_btn).on_hover_text(format!("Open {} Query Bar (Ctrl+Shift+Q)", type_label)).clicked() {
                            action = Some(BreadcrumbAction::ToggleQueryBar(path.to_string()));
                        }

                        // Copy Path Button
                        let btn_text = format!("📋 Copy {}", type_label);
                        let copy_btn = egui::Button::new(
                            RichText::new(btn_text)
                                .size(13.5)
                                .color(Color32::from_rgb(220, 232, 245)),
                        )
                        .min_size(Vec2::new(0.0, 28.0))
                        .fill(if props.dark_mode { Color32::from_rgb(44, 49, 58) } else { Color32::from_rgb(225, 230, 238) })
                        .stroke(egui::Stroke::new(1.0_f32, if props.dark_mode { Color32::from_rgb(60, 68, 80) } else { Color32::from_rgb(195, 205, 218) }))
                        .corner_radius(4.0);

                        if ui.add(copy_btn).on_hover_text(format!("Copy exact {} to clipboard (Ctrl+Shift+C)", type_label)).clicked() {
                            action = Some(BreadcrumbAction::CopyPath(path.to_string()));
                        }

                        // Breadcrumb hierarchy in the remaining full width with smooth horizontal scrolling
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            egui::ScrollArea::horizontal()
                                .id_salt("breadcrumb_scroll_full")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    render_breadcrumb_segments(ui, props.file_name, path, is_xml, text_color, sep_color, &mut action);
                                });
                        });
                    });
                } else {
                    // ── Active Query State: Unified Header (Breadcrumbs + Inline Query Box + Controls) ──
                    // 1. Right-side controls (Close, CSV, Slice, Nav arrows, Match Count)
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Close Button (vector icon)
                        let (close_rect, close_resp) = ui.allocate_exact_size(Vec2::new(26.0, 26.0), egui::Sense::click());
                        if close_resp.hovered() {
                            ui.painter().rect_filled(close_rect, 4.0, Color32::from_rgb(55, 60, 70));
                        }
                        paint_icon(ui.painter(), close_rect, Icon::Close, if close_resp.hovered() { Color32::WHITE } else { Color32::from_rgb(170, 175, 185) });
                        if close_resp.on_hover_text("Close Query Bar (Esc)").clicked() {
                            action = Some(BreadcrumbAction::CloseQuery);
                        }

                        // Extract CSV Button
                        let csv_btn = egui::Button::new(RichText::new("Extract CSV").size(13.5).color(Color32::from_rgb(152, 195, 121)))
                            .min_size(Vec2::new(0.0, 28.0))
                            .corner_radius(4.0);
                        if ui.add(csv_btn)
                            .on_hover_text("Extract matching elements/fields to CSV or CSV Grid tab")
                            .clicked()
                        {
                            action = Some(BreadcrumbAction::ExtractToCsv);
                        }

                        // Slice View Button
                        let slice_btn_text = if props.is_slice_active {
                            "Exit Slice"
                        } else {
                            "Slice View"
                        };
                        let slice_color = if props.is_slice_active {
                            Color32::from_rgb(224, 108, 117)
                        } else {
                            Color32::from_rgb(97, 175, 239)
                        };

                        let slice_btn = egui::Button::new(RichText::new(slice_btn_text).size(13.5).strong().color(slice_color))
                            .min_size(Vec2::new(0.0, 28.0))
                            .corner_radius(4.0);
                        if ui.add(slice_btn)
                            .on_hover_text("Open instant virtual sub-file containing only matches without copying disk files")
                            .clicked()
                        {
                            action = Some(BreadcrumbAction::ActivateSliceView);
                        }

                        // Prev / Next Navigation Buttons (Vector icons to never render square box)
                        let (down_rect, down_resp) = ui.allocate_exact_size(Vec2::new(26.0, 28.0), egui::Sense::click());
                        if down_resp.hovered() {
                            ui.painter().rect_filled(down_rect, 4.0, Color32::from_rgb(50, 56, 66));
                        } else {
                            ui.painter().rect_filled(down_rect, 4.0, Color32::from_rgb(38, 42, 50));
                        }
                        ui.painter().rect_stroke(down_rect, 4.0, egui::Stroke::new(1.0_f32, Color32::from_rgb(55, 62, 74)), egui::StrokeKind::Inside);
                        paint_icon(ui.painter(), down_rect.shrink(7.0), Icon::ArrowDown, if down_resp.hovered() { Color32::WHITE } else { Color32::from_rgb(180, 190, 205) });
                        if down_resp.on_hover_text("Next match (Enter)").clicked() {
                            action = Some(BreadcrumbAction::NextMatch);
                        }

                        let (up_rect, up_resp) = ui.allocate_exact_size(Vec2::new(26.0, 28.0), egui::Sense::click());
                        if up_resp.hovered() {
                            ui.painter().rect_filled(up_rect, 4.0, Color32::from_rgb(50, 56, 66));
                        } else {
                            ui.painter().rect_filled(up_rect, 4.0, Color32::from_rgb(38, 42, 50));
                        }
                        ui.painter().rect_stroke(up_rect, 4.0, egui::Stroke::new(1.0_f32, Color32::from_rgb(55, 62, 74)), egui::StrokeKind::Inside);
                        paint_icon(ui.painter(), up_rect.shrink(7.0), Icon::ArrowUp, if up_resp.hovered() { Color32::WHITE } else { Color32::from_rgb(180, 190, 205) });
                        if up_resp.on_hover_text("Previous match (Shift+Enter)").clicked() {
                            action = Some(BreadcrumbAction::PrevMatch);
                        }

                        // Match count badge
                        if props.is_searching {
                            let searching_badge = egui::Frame::NONE
                                .fill(Color32::from_rgb(97, 175, 239).gamma_multiply(0.18))
                                .corner_radius(4.0)
                                .inner_margin(egui::Margin::symmetric(9, 4));
                            searching_badge.show(ui, |ui| {
                                ui.label(RichText::new("Scanning...").size(13.0).color(Color32::from_rgb(97, 175, 239)));
                            });
                        } else if props.match_count > 0 {
                            let count_badge = egui::Frame::NONE
                                .fill(Color32::from_rgb(152, 195, 121).gamma_multiply(0.18))
                                .corner_radius(4.0)
                                .inner_margin(egui::Margin::symmetric(9, 4));
                            count_badge.show(ui, |ui| {
                                let text = format!("{} of {}", props.current_match_idx.max(1), props.match_count);
                                ui.label(RichText::new(text).size(13.0).strong().color(Color32::from_rgb(152, 195, 121)));
                            });
                        } else if !props.query_text.trim().is_empty() {
                            let no_match_badge = egui::Frame::NONE
                                .fill(Color32::from_rgb(224, 108, 117).gamma_multiply(0.18))
                                .corner_radius(4.0)
                                .inner_margin(egui::Margin::symmetric(9, 4));
                            no_match_badge.show(ui, |ui| {
                                ui.label(RichText::new("No matches").size(13.0).color(Color32::from_rgb(224, 108, 117)));
                            });
                        }

                        // 2. Middle & Left: Query Mode Badge + Suggestions Toggle + Elastic Query Input
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            // Query Mode Indicator Badge (XPath / JSONPath)
                            let mode_name = if is_xml { "XPath" } else { "JSONPath" };
                            let mode_w = if is_xml { 60.0 } else { 80.0 };
                            let (mode_rect, mode_resp) = ui.allocate_exact_size(Vec2::new(mode_w, 28.0), Sense::hover());
                            let mode_bg = label_color.gamma_multiply(0.18);
                            let mode_border = label_color.gamma_multiply(0.85);
                            ui.painter().rect_filled(mode_rect, 4.0, mode_bg);
                            ui.painter().rect_stroke(mode_rect, 4.0, egui::Stroke::new(1.0_f32, mode_border), egui::StrokeKind::Inside);
                            ui.painter().text(
                                mode_rect.center(),
                                egui::Align2::CENTER_CENTER,
                                mode_name,
                                egui::FontId::proportional(13.0),
                                label_color,
                            );
                            mode_resp.on_hover_text(format!("Query language: {} (Press Esc or click Close to return to breadcrumbs)", mode_name));

                            ui.add_space(4.0);

                            // Suggestions toggle button: fixed width placed next to mode badge
                            let sug_count = props.suggestions.len();
                            let is_sug_open = *props.show_suggestions;
                            let sug_w = if sug_count > 0 { 136.0 } else { 115.0 };
                            let (sug_rect, sug_resp) = ui.allocate_exact_size(Vec2::new(sug_w, 28.0), egui::Sense::click());
                            let sug_hovered = sug_resp.hovered();

                            let sug_bg = if is_sug_open {
                                Color32::from_rgb(52, 60, 72)
                            } else if sug_hovered {
                                Color32::from_rgb(46, 52, 62)
                            } else {
                                Color32::from_rgb(36, 40, 48)
                            };
                            let sug_stroke = if is_sug_open {
                                Color32::from_rgb(97, 175, 239)
                            } else if sug_hovered {
                                Color32::from_rgb(70, 78, 92)
                            } else {
                                Color32::from_rgb(52, 58, 68)
                            };

                            ui.painter().rect_filled(sug_rect, 4.0, sug_bg);
                            ui.painter().rect_stroke(sug_rect, 4.0, egui::Stroke::new(1.0_f32, sug_stroke), egui::StrokeKind::Inside);

                            // Draw vector Sparkle icon + clean text inside the button (NEVER a square box!)
                            let spark_rect = Rect::from_min_size(Pos2::new(sug_rect.min.x + 8.0, sug_rect.center().y - 6.0), Vec2::splat(12.0));
                            paint_icon(ui.painter(), spark_rect, Icon::Sparkle, Color32::from_rgb(229, 192, 123));

                            let sug_label = if sug_count > 0 {
                                format!("Suggestions ({})", sug_count)
                            } else {
                                "Suggestions".to_string()
                            };
                            ui.painter().text(
                                Pos2::new(sug_rect.min.x + 25.0, sug_rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                sug_label,
                                egui::FontId::proportional(12.5),
                                if is_sug_open { Color32::WHITE } else { Color32::from_rgb(229, 192, 123) },
                            );

                            if sug_resp.on_hover_text("Explore smart suggestions and query templates (auto-discovered from document)").clicked() {
                                *props.show_suggestions = !*props.show_suggestions;
                            }

                            // Query Text Input well: Elastic, fills ALL remaining width up to the right-side cluster
                            let placeholder = if is_xml {
                                "e.g. //job or //job[@id] or //title..."
                            } else {
                                "e.g. $.items[*] or $..price..."
                            };

                            let input_well_width = (ui.available_width() - 4.0).max(60.0);

                            let (edit_response, input_rect) = egui::Frame::NONE
                                .fill(if props.dark_mode { Color32::from_rgb(27, 30, 36) } else { Color32::WHITE })
                                .stroke(egui::Stroke::new(1.0_f32, if props.dark_mode { Color32::from_rgb(50, 56, 66) } else { Color32::from_rgb(205, 212, 222) }))
                                .corner_radius(4.0)
                                .inner_margin(egui::Margin::symmetric(10, 3))
                                .show(ui, |ui| {
                                    let edit = ui.add(
                                        egui::TextEdit::singleline(props.query_text)
                                            .hint_text(RichText::new(placeholder).size(13.5).color(Color32::from_rgb(110, 118, 130)))
                                            .desired_width(input_well_width)
                                            .font(egui::FontId::monospace(14.0))
                                            .frame(false)
                                    );
                                    let rect = edit.rect;
                                    (edit, rect)
                                }).inner;

                            // Keyboard handlers for Query input
                            if (edit_response.has_focus() || edit_response.lost_focus()) && ui.input(|i| i.key_pressed(Key::Enter)) {
                                if ui.input(|i| i.modifiers.shift) {
                                    action = Some(BreadcrumbAction::PrevMatch);
                                } else if props.match_count > 0 {
                                    action = Some(BreadcrumbAction::NextMatch);
                                } else {
                                    action = Some(BreadcrumbAction::ExecuteQuery(props.query_text.clone()));
                                }
                                *props.show_suggestions = false;
                            } else if edit_response.changed() {
                                *props.show_suggestions = true;
                                action = Some(BreadcrumbAction::ExecuteQuery(props.query_text.clone()));
                            }

                            if ui.input(|i| i.key_pressed(Key::Escape)) {
                                *props.show_suggestions = false;
                            }

                            // Floating Suggestions Dropdown Popup
                            if *props.show_suggestions && !props.suggestions.is_empty() {
                                let screen_rect = ui.ctx().screen_rect();
                                let screen_w = screen_rect.width();
                                let max_popup_w = (screen_w - 32.0).clamp(320.0, 720.0);
                                let desired_w = (input_rect.width() + 160.0).clamp(420.0, 720.0);
                                let popup_width = desired_w.min(max_popup_w);

                                let mut popup_x = sug_rect.min.x;
                                if popup_x + popup_width > screen_w - 16.0 {
                                    popup_x = (screen_w - popup_width - 16.0).max(10.0);
                                }
                                let popup_pos = Pos2::new(popup_x, sug_rect.max.y + 6.0);

                                egui::Area::new(egui::Id::new("query_suggestions_popup_area"))
                                    .order(egui::Order::Foreground)
                                    .fixed_pos(popup_pos)
                                    .show(ui.ctx(), |ui| {
                                        egui::Frame::NONE
                                            .fill(Color32::from_rgb(33, 37, 43)) // #21252B One Dark
                                            .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(60, 68, 80)))
                                            .corner_radius(6.0)
                                            .shadow(egui::Shadow {
                                                offset: [0, 6],
                                                blur: 16,
                                                spread: 2,
                                                color: Color32::from_black_alpha(180),
                                            })
                                            .inner_margin(egui::Margin::symmetric(10, 8))
                                            .show(ui, |ui| {
                                                ui.set_max_width(popup_width);
                                                ui.set_max_height(350.0);

                                                let active_cat_id = egui::Id::new("sug_popup_selected_category");
                                                let mut active_cat = ui.data_mut(|d| d.get_temp::<SuggestionCategory>(active_cat_id).unwrap_or(SuggestionCategory::All));

                                                let count_all = props.suggestions.len();
                                                let count_bids = props.suggestions.iter().filter(|s| s.category == SuggestionCategory::BidsAndPricing).count();
                                                let count_geo = props.suggestions.iter().filter(|s| s.category == SuggestionCategory::LocationAndGeo).count();
                                                let count_links = props.suggestions.iter().filter(|s| s.category == SuggestionCategory::LinksAndQuality).count();
                                                let count_empty = props.suggestions.iter().filter(|s| s.category == SuggestionCategory::EmptyFields).count();
                                                let count_data = props.suggestions.iter().filter(|s| s.category == SuggestionCategory::HasData).count();
                                                let count_elem = props.suggestions.iter().filter(|s| s.category == SuggestionCategory::Elements).count();
                                                let count_attr = props.suggestions.iter().filter(|s| s.category == SuggestionCategory::Attributes).count();

                                                // Header row with vector sparkle icon and vector close button
                                                ui.horizontal(|ui| {
                                                    let (star_rect, _) = ui.allocate_exact_size(Vec2::splat(15.0), Sense::hover());
                                                    paint_icon(ui.painter(), star_rect, Icon::Sparkle, Color32::from_rgb(229, 192, 123));
                                                    ui.label(
                                                        RichText::new("SMART QUERY SUGGESTIONS (click any to apply)")
                                                            .size(12.5)
                                                            .strong()
                                                            .color(Color32::from_rgb(180, 190, 205)),
                                                    );
                                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                        let (c_rect, c_resp) = ui.allocate_exact_size(Vec2::new(58.0, 22.0), Sense::click());
                                                        if c_resp.hovered() {
                                                            ui.painter().rect_filled(c_rect, 3.0, Color32::from_rgb(55, 60, 70));
                                                        }
                                                        let icon_r = Rect::from_min_size(Pos2::new(c_rect.min.x + 6.0, c_rect.center().y - 5.0), Vec2::splat(10.0));
                                                        paint_icon(ui.painter(), icon_r, Icon::Close, Color32::from_rgb(180, 185, 195));
                                                        ui.painter().text(
                                                            Pos2::new(c_rect.min.x + 20.0, c_rect.center().y),
                                                            egui::Align2::LEFT_CENTER,
                                                            "Close",
                                                            egui::FontId::proportional(12.0),
                                                            Color32::from_rgb(180, 185, 195),
                                                        );
                                                        if c_resp.clicked() {
                                                            *props.show_suggestions = false;
                                                        }
                                                    });
                                                });
                                                ui.add_space(4.0);

                                                // Category filter tabs row with VECTOR ICONS (zero tofu square boxes!)
                                                ui.horizontal(|ui| {
                                                    ui.spacing_mut().item_spacing.x = 6.0;

                                                    let mut render_tab = |ui: &mut egui::Ui, cat: SuggestionCategory, icon: Option<Icon>, label: &str, count: usize, accent_color: Color32| {
                                                        let is_selected = active_cat == cat;
                                                        let text = format!("{} ({})", label, count);
                                                        let text_w = text.len() as f32 * 7.0 + if icon.is_some() { 28.0 } else { 16.0 };
                                                        let (tab_rect, tab_resp) = ui.allocate_exact_size(Vec2::new(text_w, 24.0), Sense::click());

                                                        let bg_color = if is_selected {
                                                            accent_color.gamma_multiply(0.28)
                                                        } else if tab_resp.hovered() {
                                                            Color32::from_rgb(48, 54, 64)
                                                        } else {
                                                            Color32::from_rgb(38, 42, 50)
                                                        };
                                                        let stroke_color = if is_selected {
                                                            accent_color
                                                        } else if tab_resp.hovered() {
                                                            Color32::from_rgb(70, 78, 92)
                                                        } else {
                                                            Color32::from_rgb(52, 58, 68)
                                                        };

                                                        ui.painter().rect_filled(tab_rect, 4.0, bg_color);
                                                        ui.painter().rect_stroke(tab_rect, 4.0, egui::Stroke::new(1.0_f32, stroke_color), egui::StrokeKind::Inside);

                                                        let mut text_x = tab_rect.min.x + 8.0;
                                                        if let Some(ic) = icon {
                                                            let ic_rect = Rect::from_min_size(Pos2::new(tab_rect.min.x + 7.0, tab_rect.center().y - 5.5), Vec2::splat(11.0));
                                                            paint_icon(ui.painter(), ic_rect, ic, if is_selected { accent_color } else { Color32::from_rgb(160, 170, 185) });
                                                            text_x += 16.0;
                                                        }

                                                        ui.painter().text(
                                                            Pos2::new(text_x, tab_rect.center().y),
                                                            egui::Align2::LEFT_CENTER,
                                                            text,
                                                            egui::FontId::proportional(12.0),
                                                            if is_selected { Color32::WHITE } else { Color32::from_rgb(175, 185, 200) },
                                                        );

                                                        if tab_resp.clicked() {
                                                            active_cat = cat;
                                                        }
                                                    };

                                                    render_tab(ui, SuggestionCategory::All, Some(Icon::Sparkle), "All", count_all, Color32::from_rgb(97, 175, 239));
                                                    if count_bids > 0 {
                                                        render_tab(ui, SuggestionCategory::BidsAndPricing, Some(Icon::Sparkle), "Bids & CPC", count_bids, Color32::from_rgb(229, 192, 123));
                                                    }
                                                    if count_geo > 0 {
                                                        render_tab(ui, SuggestionCategory::LocationAndGeo, Some(Icon::Cube), "Location", count_geo, Color32::from_rgb(198, 120, 221));
                                                    }
                                                    if count_links > 0 {
                                                        render_tab(ui, SuggestionCategory::LinksAndQuality, Some(Icon::Sparkle), "Quality & Links", count_links, Color32::from_rgb(86, 182, 194));
                                                    }
                                                    if count_empty > 0 {
                                                        render_tab(ui, SuggestionCategory::EmptyFields, Some(Icon::Warning), "Empty Fields", count_empty, Color32::from_rgb(224, 108, 117));
                                                    }
                                                    if count_data > 0 {
                                                        render_tab(ui, SuggestionCategory::HasData, Some(Icon::Check), "Has Data", count_data, Color32::from_rgb(152, 195, 121));
                                                    }
                                                    if count_elem > 0 {
                                                        render_tab(ui, SuggestionCategory::Elements, Some(Icon::Cube), "Elements", count_elem, Color32::from_rgb(97, 175, 239));
                                                    }
                                                    if count_attr > 0 {
                                                        render_tab(ui, SuggestionCategory::Attributes, None, "@ Attr", count_attr, Color32::from_rgb(229, 192, 123));
                                                    }
                                                });
                                                ui.data_mut(|d| d.insert_temp(active_cat_id, active_cat));

                                                ui.add_space(4.0);
                                                ui.separator();
                                                ui.add_space(4.0);

                                                let filtered_sugs: Vec<_> = props.suggestions.iter()
                                                    .filter(|s| active_cat == SuggestionCategory::All || s.category == active_cat)
                                                    .collect();

                                                egui::ScrollArea::vertical()
                                                    .id_salt("query_sug_scroll")
                                                    .auto_shrink([false, false])
                                                    .max_height(265.0)
                                                    .show(ui, |ui| {
                                                        ui.spacing_mut().item_spacing.y = 3.0;
                                                        let row_w = ui.available_width();
                                                        for sug in filtered_sugs {
                                                            let (row_rect, row_resp) = ui.allocate_exact_size(Vec2::new(row_w, 30.0), Sense::click());
                                                            let row_resp = row_resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                                                            let is_hovered = row_resp.hovered();

                                                            // 1. Paint hover background across the full row
                                                            if is_hovered {
                                                                ui.painter().rect_filled(row_rect, 4.0, Color32::from_rgb(46, 54, 68));
                                                                ui.painter().rect_stroke(
                                                                    row_rect,
                                                                    4.0,
                                                                    egui::Stroke::new(1.0_f32, Color32::from_rgb(97, 175, 239).gamma_multiply(0.4)),
                                                                    egui::StrokeKind::Inside,
                                                                );
                                                            }

                                                            // 2. Left Badge / Icon
                                                            let content_left = row_rect.min.x + 8.0;
                                                            let text_x = if sug.category == SuggestionCategory::EmptyFields {
                                                                let badge_rect = Rect::from_min_size(Pos2::new(content_left, row_rect.center().y - 9.0), Vec2::new(52.0, 18.0));
                                                                ui.painter().rect(
                                                                    badge_rect,
                                                                    3.0,
                                                                    Color32::from_rgb(224, 108, 117).gamma_multiply(0.2),
                                                                    egui::Stroke::new(1.0_f32, Color32::from_rgb(224, 108, 117).gamma_multiply(0.6)),
                                                                    egui::StrokeKind::Inside,
                                                                );
                                                                ui.painter().text(
                                                                    badge_rect.center(),
                                                                    egui::Align2::CENTER_CENTER,
                                                                    "EMPTY",
                                                                    egui::FontId::monospace(10.5),
                                                                    Color32::from_rgb(224, 108, 117),
                                                                );
                                                                content_left + 60.0
                                                            } else if sug.category == SuggestionCategory::BidsAndPricing {
                                                                let badge_rect = Rect::from_min_size(Pos2::new(content_left, row_rect.center().y - 9.0), Vec2::new(42.0, 18.0));
                                                                ui.painter().rect(
                                                                    badge_rect,
                                                                    3.0,
                                                                    Color32::from_rgb(229, 192, 123).gamma_multiply(0.2),
                                                                    egui::Stroke::new(1.0_f32, Color32::from_rgb(229, 192, 123).gamma_multiply(0.6)),
                                                                    egui::StrokeKind::Inside,
                                                                );
                                                                ui.painter().text(
                                                                    badge_rect.center(),
                                                                    egui::Align2::CENTER_CENTER,
                                                                    "BID",
                                                                    egui::FontId::monospace(10.5),
                                                                    Color32::from_rgb(229, 192, 123),
                                                                );
                                                                content_left + 50.0
                                                            } else if sug.category == SuggestionCategory::LocationAndGeo {
                                                                let badge_rect = Rect::from_min_size(Pos2::new(content_left, row_rect.center().y - 9.0), Vec2::new(42.0, 18.0));
                                                                ui.painter().rect(
                                                                    badge_rect,
                                                                    3.0,
                                                                    Color32::from_rgb(198, 120, 221).gamma_multiply(0.2),
                                                                    egui::Stroke::new(1.0_f32, Color32::from_rgb(198, 120, 221).gamma_multiply(0.6)),
                                                                    egui::StrokeKind::Inside,
                                                                );
                                                                ui.painter().text(
                                                                    badge_rect.center(),
                                                                    egui::Align2::CENTER_CENTER,
                                                                    "GEO",
                                                                    egui::FontId::monospace(10.5),
                                                                    Color32::from_rgb(198, 120, 221),
                                                                );
                                                                content_left + 50.0
                                                            } else if sug.category == SuggestionCategory::LinksAndQuality {
                                                                let badge_rect = Rect::from_min_size(Pos2::new(content_left, row_rect.center().y - 9.0), Vec2::new(42.0, 18.0));
                                                                ui.painter().rect(
                                                                    badge_rect,
                                                                    3.0,
                                                                    Color32::from_rgb(86, 182, 194).gamma_multiply(0.2),
                                                                    egui::Stroke::new(1.0_f32, Color32::from_rgb(86, 182, 194).gamma_multiply(0.6)),
                                                                    egui::StrokeKind::Inside,
                                                                );
                                                                ui.painter().text(
                                                                    badge_rect.center(),
                                                                    egui::Align2::CENTER_CENTER,
                                                                    "QA",
                                                                    egui::FontId::monospace(10.5),
                                                                    Color32::from_rgb(86, 182, 194),
                                                                );
                                                                content_left + 50.0
                                                            } else {
                                                                let icon_rect = Rect::from_min_size(Pos2::new(content_left + 2.0, row_rect.center().y - 7.0), Vec2::splat(14.0));
                                                                match sug.kind {
                                                                    SuggestionKind::Element => {
                                                                        paint_icon(ui.painter(), icon_rect, Icon::Cube, Color32::from_rgb(97, 175, 239));
                                                                    }
                                                                    SuggestionKind::Attribute => {
                                                                        ui.painter().text(
                                                                            icon_rect.center(),
                                                                            egui::Align2::CENTER_CENTER,
                                                                            "@",
                                                                            egui::FontId::monospace(14.0),
                                                                            Color32::from_rgb(152, 195, 121),
                                                                        );
                                                                    }
                                                                    SuggestionKind::Path => {
                                                                        paint_icon(ui.painter(), icon_rect, Icon::ChevronRight, Color32::from_rgb(229, 192, 123));
                                                                    }
                                                                    SuggestionKind::Template => {
                                                                        paint_icon(ui.painter(), icon_rect, Icon::Sparkle, Color32::from_rgb(152, 195, 121));
                                                                    }
                                                                }
                                                                content_left + 24.0
                                                            };

                                                            // 3. Query Label (truncated if exceptionally long)
                                                            let label_color = match sug.category {
                                                                SuggestionCategory::EmptyFields => Color32::from_rgb(224, 108, 117),
                                                                SuggestionCategory::BidsAndPricing => Color32::from_rgb(229, 192, 123),
                                                                SuggestionCategory::LocationAndGeo => Color32::from_rgb(198, 120, 221),
                                                                SuggestionCategory::LinksAndQuality => Color32::from_rgb(86, 182, 194),
                                                                _ => if is_hovered {
                                                                    Color32::WHITE
                                                                } else {
                                                                    Color32::from_rgb(97, 175, 239)
                                                                },
                                                            };
                                                            let label_font = egui::FontId::monospace(13.5);
                                                            let max_label_chars = 48;
                                                            let display_label = if sug.label.len() > max_label_chars {
                                                                format!("{}...", &sug.label[..max_label_chars])
                                                            } else {
                                                                sug.label.clone()
                                                            };
                                                            let galley_label = ui.painter().layout_no_wrap(display_label, label_font, label_color);
                                                            let label_w = galley_label.size().x;
                                                            ui.painter().galley(
                                                                Pos2::new(text_x, row_rect.center().y - galley_label.size().y * 0.5),
                                                                galley_label,
                                                                label_color,
                                                            );

                                                            // 4. Description Text (clipped to available row space)
                                                            let desc_x = text_x + label_w + 12.0;
                                                            if desc_x < row_rect.max.x - 30.0 {
                                                                let avail_w = row_rect.max.x - desc_x - 8.0;
                                                                let desc_color = Color32::from_rgb(160, 170, 185);
                                                                let desc_font = egui::FontId::proportional(12.5);

                                                                let mut desc_text = sug.description.clone();
                                                                let galley_desc = ui.painter().layout_no_wrap(desc_text.clone(), desc_font.clone(), desc_color);
                                                                if galley_desc.size().x > avail_w {
                                                                    let char_approx = ((avail_w / 7.0) as usize).saturating_sub(3);
                                                                    if char_approx > 4 && char_approx < desc_text.len() {
                                                                        desc_text = format!("{}...", &desc_text[..char_approx]);
                                                                    }
                                                                }
                                                                let galley_desc = ui.painter().layout_no_wrap(desc_text, desc_font, desc_color);
                                                                ui.painter().galley(
                                                                    Pos2::new(desc_x, row_rect.center().y - galley_desc.size().y * 0.5),
                                                                    galley_desc,
                                                                    desc_color,
                                                                );
                                                            }

                                                            // 5. Entire row responds to click 100% of the time anywhere on the row!
                                                            if row_resp.clicked() {
                                                                *props.query_text = sug.query.clone();
                                                                *props.show_suggestions = false;
                                                                action = Some(BreadcrumbAction::ExecuteQuery(sug.query.clone()));
                                                            }
                                                        }
                                                    });
                                            });
                                    });
                            }
                        });
                    });
                }
            });
        });

    action
}

fn render_breadcrumb_segments(
    ui: &mut Ui,
    file_name: Option<&str>,
    path: &str,
    is_xml: bool,
    text_color: Color32,
    sep_color: Color32,
    action: &mut Option<BreadcrumbAction>,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let mut has_preceding = false;
        let font_size = 15.0;

        // 1. Source file name with vector icon (no missing glyph boxes)
        if let Some(fname) = file_name {
            let fname_color = Color32::from_rgb(171, 178, 191);
            let icon = if is_xml { Icon::File } else { Icon::File };

            let btn_resp = ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(15.0), Sense::hover());
                paint_icon(ui.painter(), rect, icon, Color32::from_rgb(224, 108, 117));
                ui.label(RichText::new(fname).size(font_size).color(fname_color))
            }).response;

            let interact = ui.interact(btn_resp.rect, btn_resp.id.with("file_click"), Sense::click());
            if interact.hovered() {
                ui.painter().rect_filled(interact.rect.expand(2.0), 3.0, Color32::from_white_alpha(15));
            }
            interact.on_hover_text("Source document");

            has_preceding = true;
        }

        // 2. Element and attribute segments
        let segments: Vec<&str> = if is_xml {
            path.split('/').filter(|s| !s.is_empty()).collect()
        } else {
            path.split('.').filter(|s| !s.is_empty()).collect()
        };

        if segments.is_empty() {
            if !path.is_empty() {
                if has_preceding {
                    ui.label(RichText::new("›").size(14.0).strong().color(sep_color));
                }
                ui.label(RichText::new(path).size(font_size).color(text_color));
            }
        } else {
            for (i, seg) in segments.iter().enumerate() {
                if has_preceding || i > 0 {
                    ui.label(RichText::new("›").size(14.0).strong().color(sep_color));
                }
                has_preceding = true;

                let is_attr = seg.starts_with('@');
                let is_indexed = seg.contains('[');
                let seg_color = if is_attr {
                    Color32::from_rgb(152, 195, 121) // Green for attributes
                } else if is_indexed {
                    Color32::from_rgb(97, 175, 239) // Blue for indexed nodes
                } else {
                    text_color
                };

                let clean_text = if is_attr {
                    seg.trim_start_matches('@')
                } else {
                    *seg
                };

                let seg_resp = ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if is_attr {
                        ui.label(RichText::new("@").size(font_size).strong().color(Color32::from_rgb(152, 195, 121)));
                    } else {
                        // Vector 3D Isometric Cube Icon (no missing glyph boxes)
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(15.0), Sense::hover());
                        paint_icon(ui.painter(), rect, Icon::Cube, Color32::from_rgb(97, 175, 239));
                    }
                    ui.label(RichText::new(clean_text).size(font_size).color(seg_color))
                }).response;

                let interact = ui.interact(seg_resp.rect, seg_resp.id.with("seg_click"), Sense::click());
                if interact.hovered() {
                    ui.painter().rect_filled(interact.rect.expand(2.0), 3.0, Color32::from_white_alpha(15));
                }

                if interact.on_hover_text(format!("Click to query {}", seg)).clicked() {
                    let sub_path = if is_xml {
                        format!("//{}", seg)
                    } else {
                        format!("$.{}", seg)
                    };
                    *action = Some(BreadcrumbAction::ToggleQueryBar(sub_path));
                }
            }
        }
    });
}
