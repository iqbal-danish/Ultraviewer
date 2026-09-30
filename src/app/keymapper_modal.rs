use eframe::egui::{self, Color32, Key, RichText, Sense, Stroke, Vec2, Window};
use super::keymapper::{ActionCategory, KeyChord, KeybindingsConfig, ShortcutAction};

pub struct KeymapperModalState {
    pub is_open: bool,
    pub search_query: String,
    pub selected_category: Option<ActionCategory>,
    pub recording_action: Option<ShortcutAction>,
    pub status_message: Option<(String, f32)>, // message, display timestamp
}

impl Default for KeymapperModalState {
    fn default() -> Self {
        Self {
            is_open: false,
            search_query: String::new(),
            selected_category: None,
            recording_action: None,
            status_message: None,
        }
    }
}

impl KeymapperModalState {
    pub fn open(&mut self) {
        self.is_open = true;
        self.recording_action = None;
        self.status_message = None;
    }

    pub fn close(&mut self) {
        self.is_open = false;
        self.recording_action = None;
    }
}

pub fn render_keymapper_modal(
    ctx: &egui::Context,
    state: &mut KeymapperModalState,
    config: &mut KeybindingsConfig,
    dark_mode: bool,
) {
    if !state.is_open {
        return;
    }

    // Handle key recording if an action is currently waiting for input
    if let Some(target_action) = state.recording_action {
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            state.recording_action = None;
            state.status_message = Some(("Recording cancelled.".to_string(), ctx.input(|i| i.time as f32)));
        } else {
            // Check if user pressed a non-modifier key
            let captured = ctx.input(|i| KeyChord::capture_from_input(i));
            if let Some(chord) = captured {
                // Check for collision with another action
                let mut collided_with = None;
                for a in ShortcutAction::all() {
                    if *a != target_action {
                        if let Some(existing) = config.get(*a) {
                            if existing == chord {
                                collided_with = Some(*a);
                                break;
                            }
                        }
                    }
                }

                config.set(target_action, Some(chord));
                state.recording_action = None;

                if let Some(other) = collided_with {
                    state.status_message = Some((
                        format!("Mapped '{}' to {} (Note: previously used by '{}')", chord.to_chord_string(), target_action.title(), other.title()),
                        ctx.input(|i| i.time as f32),
                    ));
                } else {
                    state.status_message = Some((
                        format!("Mapped '{}' to {}", chord.to_chord_string(), target_action.title()),
                        ctx.input(|i| i.time as f32),
                    ));
                }
            }
        }
    }

    let bg_color = if dark_mode {
        Color32::from_rgb(28, 32, 40)
    } else {
        Color32::from_rgb(248, 250, 252)
    };

    let border_color = if dark_mode {
        Color32::from_rgb(55, 62, 74)
    } else {
        Color32::from_rgb(205, 212, 222)
    };

    let card_bg = if dark_mode {
        Color32::from_rgb(33, 38, 48)
    } else {
        Color32::WHITE
    };

    let text_primary = if dark_mode { Color32::WHITE } else { Color32::from_rgb(20, 24, 30) };
    let text_muted = if dark_mode { Color32::from_rgb(155, 165, 180) } else { Color32::from_rgb(100, 110, 125) };
    let accent_blue = Color32::from_rgb(97, 175, 239);
    let accent_gold = Color32::from_rgb(229, 192, 123);
    let accent_red = Color32::from_rgb(224, 108, 117);

    let screen_rect = ctx.screen_rect();
    let max_w = 800.0_f32.min(screen_rect.width() - 40.0);
    let max_h = 680.0_f32.min(screen_rect.height() - 60.0);

    Window::new("Keyboard Shortcuts Mapper")
        .collapsible(false)
        .resizable(true)
        .default_size(Vec2::new(max_w, max_h))
        .min_size(Vec2::new(520.0, 420.0))
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(screen_rect.center())
        .frame(
            egui::Frame::window(&ctx.style())
                .fill(bg_color)
                .stroke(Stroke::new(1.0_f32, border_color))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::same(16)),
        )
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;

            // ── Top Header ──
            ui.horizontal(|ui| {
                ui.heading(RichText::new("Keyboard Shortcuts Mapper").color(text_primary).strong().size(18.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let close_btn = egui::Button::new(RichText::new("Close (Esc)").color(text_muted).size(13.0))
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::new(1.0_f32, border_color))
                        .corner_radius(4.0);
                    if ui.add(close_btn).clicked() || (!state.recording_action.is_some() && ui.input(|i| i.key_pressed(Key::Escape))) {
                        state.close();
                    }

                    let reset_btn = egui::Button::new(RichText::new("Reset All to Defaults").color(accent_red).size(12.5))
                        .fill(accent_red.gamma_multiply(0.12))
                        .stroke(Stroke::new(1.0_f32, accent_red.gamma_multiply(0.4)))
                        .corner_radius(4.0);
                    if ui.add(reset_btn).on_hover_text("Restore all default keyboard shortcuts").clicked() {
                        config.reset_all();
                        state.status_message = Some(("All shortcuts reset to defaults.".to_string(), ctx.input(|i| i.time as f32)));
                    }
                });
            });

            // ── Status Notification Banner ──
            let current_time = ctx.input(|i| i.time as f32);
            if let Some((ref msg, timestamp)) = state.status_message {
                if current_time - timestamp < 4.0 {
                    egui::Frame::NONE
                        .fill(accent_blue.gamma_multiply(0.15))
                        .stroke(Stroke::new(1.0_f32, accent_blue.gamma_multiply(0.6)))
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("ℹ ").color(accent_blue).strong());
                                ui.label(RichText::new(msg).color(text_primary).size(12.5));
                            });
                        });
                }
            }

            // ── Search & Filter Bar ──
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;

                // Search field
                let search_box = egui::Frame::NONE
                    .fill(card_bg)
                    .stroke(Stroke::new(1.0_f32, border_color))
                    .corner_radius(5.0)
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("🔍").size(12.0).color(text_muted));
                            ui.add(
                                egui::TextEdit::singleline(&mut state.search_query)
                                    .hint_text(RichText::new("Search actions or shortcuts...").color(text_muted).size(13.0))
                                    .desired_width(260.0)
                                    .frame(false),
                            );
                            if !state.search_query.is_empty() {
                                if ui.add(egui::Button::new("✖").frame(false)).clicked() {
                                    state.search_query.clear();
                                }
                            }
                        });
                    });
                let _ = search_box;

                // Category Filter Pills
                let mut render_cat_pill = |ui: &mut egui::Ui, cat: Option<ActionCategory>, label: &str| {
                    let is_active = state.selected_category == cat;
                    let (pill_rect, pill_resp) = ui.allocate_exact_size(Vec2::new(label.len() as f32 * 7.5 + 16.0, 26.0), Sense::click());
                    let fill = if is_active {
                        accent_blue.gamma_multiply(0.25)
                    } else if pill_resp.hovered() {
                        Color32::from_rgb(46, 52, 64)
                    } else {
                        Color32::TRANSPARENT
                    };
                    let stroke = if is_active {
                        accent_blue
                    } else {
                        border_color
                    };

                    ui.painter().rect_filled(pill_rect, 4.0, fill);
                    ui.painter().rect_stroke(pill_rect, 4.0, Stroke::new(1.0_f32, stroke), egui::StrokeKind::Inside);
                    ui.painter().text(
                        pill_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        label,
                        egui::FontId::proportional(12.0),
                        if is_active { Color32::WHITE } else { text_muted },
                    );

                    if pill_resp.clicked() {
                        state.selected_category = cat;
                    }
                };

                render_cat_pill(ui, None, "All Actions");
                render_cat_pill(ui, Some(ActionCategory::File), "File & General");
                render_cat_pill(ui, Some(ActionCategory::Navigation), "Navigation & View");
                render_cat_pill(ui, Some(ActionCategory::Editing), "Editing & Formatting");
            });

            ui.separator();

            // ── Actions Table ──
            let search_lower = state.search_query.trim().to_lowercase();
            let actions: Vec<_> = ShortcutAction::all()
                .iter()
                .filter(|a| {
                    if let Some(cat) = state.selected_category {
                        if a.category() != cat {
                            return false;
                        }
                    }
                    if search_lower.is_empty() {
                        return true;
                    }
                    a.title().to_lowercase().contains(&search_lower)
                        || a.description().to_lowercase().contains(&search_lower)
                        || config.get(**a).map(|c| c.to_chord_string().to_lowercase().contains(&search_lower)).unwrap_or(false)
                })
                .copied()
                .collect();

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;

                    egui::Frame::NONE
                        .fill(card_bg)
                        .stroke(Stroke::new(1.0_f32, border_color))
                        .corner_radius(6.0)
                        .inner_margin(egui::Margin::same(12))
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                // Table Header
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("Action & Description").strong().color(text_muted).size(12.0));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.add_space(20.0);
                                        ui.label(RichText::new("Keybinding").strong().color(text_muted).size(12.0));
                                        ui.add_space(40.0);
                                        ui.label(RichText::new("Category").strong().color(text_muted).size(12.0));
                                    });
                                });
                                ui.separator();

                                for action in actions {
                                    let current_chord = config.get(action);
                                    let is_recording = state.recording_action == Some(action);
                                    let is_modified = current_chord != action.default_chord();

                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 8.0;

                                        // Left: Title and Description
                                        ui.vertical(|ui| {
                                            ui.label(RichText::new(action.title()).strong().color(text_primary).size(13.0));
                                            ui.label(RichText::new(action.description()).color(text_muted).size(11.5));
                                        });

                                        // Right side elements
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            // Reset button
                                            if is_modified {
                                                if ui.add(egui::Button::new("↺").frame(false)).on_hover_text("Reset to default").clicked() {
                                                    config.reset_action(action);
                                                }
                                            } else {
                                                ui.add_space(16.0);
                                            }

                                            // Clear binding button
                                            if current_chord.is_some() && !is_recording {
                                                if ui.add(egui::Button::new("✖").frame(false)).on_hover_text("Unbind / remove shortcut").clicked() {
                                                    config.set(action, None);
                                                }
                                            } else {
                                                ui.add_space(16.0);
                                            }

                                            // Key Chord Pill / Recorder
                                            let chord_label = if is_recording {
                                                "Press keys... (Esc to cancel)".to_string()
                                            } else {
                                                current_chord.map(|c| c.to_chord_string()).unwrap_or_else(|| "None".to_string())
                                            };

                                            let pill_w = if is_recording { 180.0 } else { 120.0 };
                                            let (chord_rect, chord_resp) = ui.allocate_exact_size(Vec2::new(pill_w, 26.0), Sense::click());
                                            let chord_resp = chord_resp.on_hover_cursor(egui::CursorIcon::PointingHand);

                                            let pill_bg = if is_recording {
                                                accent_gold.gamma_multiply(0.25)
                                            } else if chord_resp.hovered() {
                                                Color32::from_rgb(52, 60, 74)
                                            } else if current_chord.is_some() {
                                                Color32::from_rgb(40, 46, 58)
                                            } else {
                                                Color32::from_rgb(30, 34, 42)
                                            };

                                            let pill_border = if is_recording {
                                                accent_gold
                                            } else if is_modified {
                                                accent_blue
                                            } else {
                                                Color32::from_rgb(55, 62, 76)
                                            };

                                            ui.painter().rect_filled(chord_rect, 4.0, pill_bg);
                                            ui.painter().rect_stroke(chord_rect, 4.0, Stroke::new(1.0_f32, pill_border), egui::StrokeKind::Inside);

                                            ui.painter().text(
                                                chord_rect.center(),
                                                egui::Align2::CENTER_CENTER,
                                                &chord_label,
                                                egui::FontId::monospace(12.0),
                                                if is_recording {
                                                    accent_gold
                                                } else if current_chord.is_some() {
                                                    if is_modified { accent_blue } else { text_primary }
                                                } else {
                                                    text_muted
                                                },
                                            );

                                            if chord_resp.on_hover_text("Click to rebind shortcut").clicked() {
                                                state.recording_action = if is_recording { None } else { Some(action) };
                                            }

                                            // Category Badge
                                            ui.add_space(8.0);
                                            let cat_label = action.category().name();
                                            ui.label(RichText::new(cat_label).color(text_muted).size(11.5));
                                        });
                                    });

                                    ui.add_space(2.0);
                                }
                            });
                        });
                });
        });
}
