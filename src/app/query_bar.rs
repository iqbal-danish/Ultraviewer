use eframe::egui::{self, Color32, Key, RichText, Ui, Vec2};
use super::icons::{paint_icon, Icon};
use crate::formats::FileType;

pub enum QueryBarAction {
    ExecuteQuery(String),
    NextMatch,
    PrevMatch,
    ActivateSliceView,
    ExtractToCsv,
    ExtractToNewTab,
    Close,
}

pub struct QueryBarProps<'a> {
    pub file_type: Option<FileType>,
    pub query_text: &'a mut String,
    pub match_count: usize,
    pub current_match_idx: usize,
    pub is_searching: bool,
    pub dark_mode: bool,
    pub is_slice_active: bool,
}

pub fn render_query_bar(ui: &mut Ui, props: QueryBarProps) -> Option<QueryBarAction> {
    let mut action = None;

    let is_xml = matches!(props.file_type, Some(FileType::Xml));
    let is_json = matches!(props.file_type, Some(FileType::Json));

    let bg_color = if props.dark_mode {
        Color32::from_rgb(33, 37, 43)
    } else {
        Color32::from_rgb(235, 238, 242)
    };

    let border_color = if props.dark_mode {
        Color32::from_rgb(40, 44, 52)
    } else {
        Color32::from_rgb(215, 220, 225)
    };

    let (badge_text, badge_color) = if is_xml {
        ("XPath", Color32::from_rgb(224, 108, 117))
    } else if is_json {
        ("JSONPath", Color32::from_rgb(229, 192, 123))
    } else {
        ("Query", Color32::from_rgb(97, 175, 239))
    };

    egui::Frame::NONE
        .fill(bg_color)
        .stroke(egui::Stroke::new(1.0_f32, border_color))
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;

                // Mode badge (pill styling matching breadcrumb bar)
                let badge = egui::Frame::NONE
                    .fill(badge_color.gamma_multiply(0.18))
                    .stroke(egui::Stroke::new(1.0_f32, badge_color.gamma_multiply(0.8)))
                    .corner_radius(3.0)
                    .inner_margin(egui::Margin::symmetric(6, 2));

                badge.show(ui, |ui| {
                    ui.label(RichText::new(badge_text).size(11.5).strong().color(badge_color));
                });

                // Query Text Input wrapped in sunken well
                let placeholder = if is_xml {
                    "e.g. //book[@id='bk101'] or /catalog/book/title..."
                } else {
                    "e.g. $.store.book[*].price or $.items[0]..."
                };

                let input_width = (ui.available_width() - 360.0).max(240.0);
                let edit_response = egui::Frame::NONE
                    .fill(if props.dark_mode { Color32::from_rgb(27, 30, 36) } else { Color32::WHITE })
                    .stroke(egui::Stroke::new(1.0_f32, if props.dark_mode { Color32::from_rgb(40, 44, 52) } else { Color32::from_rgb(205, 212, 222) }))
                    .corner_radius(3.0)
                    .inner_margin(egui::Margin::symmetric(6, 3))
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::singleline(props.query_text)
                                .hint_text(RichText::new(placeholder).size(12.0).color(Color32::from_rgb(110, 118, 130)))
                                .desired_width(input_width)
                                .font(egui::TextStyle::Monospace)
                                .frame(false)
                        )
                    }).inner;

                if (edit_response.has_focus() || edit_response.lost_focus()) && ui.input(|i| i.key_pressed(Key::Enter)) {
                    if ui.input(|i| i.modifiers.shift) {
                        action = Some(QueryBarAction::PrevMatch);
                    } else if props.match_count > 0 {
                        action = Some(QueryBarAction::NextMatch);
                    } else {
                        action = Some(QueryBarAction::ExecuteQuery(props.query_text.clone()));
                    }
                } else if edit_response.changed() {
                    action = Some(QueryBarAction::ExecuteQuery(props.query_text.clone()));
                }

                // Match count badge
                if props.is_searching {
                    let searching_badge = egui::Frame::NONE
                        .fill(Color32::from_rgb(97, 175, 239).gamma_multiply(0.15))
                        .corner_radius(3.0)
                        .inner_margin(egui::Margin::symmetric(6, 2));
                    searching_badge.show(ui, |ui| {
                        ui.label(RichText::new("⟳ Scanning...").size(12.0).color(Color32::from_rgb(97, 175, 239)));
                    });
                } else if props.match_count > 0 {
                    let count_badge = egui::Frame::NONE
                        .fill(Color32::from_rgb(152, 195, 121).gamma_multiply(0.15))
                        .corner_radius(3.0)
                        .inner_margin(egui::Margin::symmetric(6, 2));
                    count_badge.show(ui, |ui| {
                        let text = format!("{} of {}", props.current_match_idx.max(1), props.match_count);
                        ui.label(RichText::new(text).size(12.0).color(Color32::from_rgb(152, 195, 121)));
                    });
                } else if !props.query_text.trim().is_empty() {
                    let no_match_badge = egui::Frame::NONE
                        .fill(Color32::from_rgb(224, 108, 117).gamma_multiply(0.15))
                        .corner_radius(3.0)
                        .inner_margin(egui::Margin::symmetric(6, 2));
                    no_match_badge.show(ui, |ui| {
                        ui.label(RichText::new("No matches").size(12.0).color(Color32::from_rgb(224, 108, 117)));
                    });
                }

                // Prev / Next Navigation Buttons
                let nav_btn_up = egui::Button::new(RichText::new("▲").size(11.0))
                    .min_size(Vec2::new(22.0, 22.0))
                    .corner_radius(3.0);
                if ui.add(nav_btn_up).on_hover_text("Previous match (Shift+Enter)").clicked() {
                    action = Some(QueryBarAction::PrevMatch);
                }

                let nav_btn_down = egui::Button::new(RichText::new("▼").size(11.0))
                    .min_size(Vec2::new(22.0, 22.0))
                    .corner_radius(3.0);
                if ui.add(nav_btn_down).on_hover_text("Next match (Enter)").clicked() {
                    action = Some(QueryBarAction::NextMatch);
                }

                ui.separator();

                // Slice View Button
                let slice_btn_text = if props.is_slice_active {
                    "Exit Slice"
                } else {
                    "⚡ Slice View"
                };
                let slice_color = if props.is_slice_active {
                    Color32::from_rgb(224, 108, 117)
                } else {
                    Color32::from_rgb(97, 175, 239)
                };

                let slice_btn = egui::Button::new(RichText::new(slice_btn_text).size(12.0).strong().color(slice_color))
                    .corner_radius(3.0);
                if ui.add(slice_btn)
                    .on_hover_text("Open instant virtual sub-file containing only matches without copying disk files")
                    .clicked()
                {
                    action = Some(QueryBarAction::ActivateSliceView);
                }

                // Extract CSV Button
                let csv_btn = egui::Button::new(RichText::new("📊 Extract CSV").size(12.0).color(Color32::from_rgb(152, 195, 121)))
                    .corner_radius(3.0);
                if ui.add(csv_btn)
                    .on_hover_text("Extract matching elements/fields to CSV or CSV Grid tab")
                    .clicked()
                {
                    action = Some(QueryBarAction::ExtractToCsv);
                }

                // Close Button with vector icon
                let (close_rect, close_resp) = ui.allocate_exact_size(Vec2::new(22.0, 22.0), egui::Sense::click());
                if close_resp.hovered() {
                    ui.painter().rect_filled(close_rect, 3.0, Color32::from_rgb(55, 60, 70));
                }
                paint_icon(ui.painter(), close_rect, Icon::Close, if close_resp.hovered() { Color32::WHITE } else { Color32::from_rgb(170, 175, 185) });
                if close_resp.on_hover_text("Close Query Bar (Esc)").clicked() {
                    action = Some(QueryBarAction::Close);
                }
            });
        });

    action
}
