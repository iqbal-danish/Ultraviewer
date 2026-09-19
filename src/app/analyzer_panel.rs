use eframe::egui::{self, Align, Color32, Layout, ProgressBar, RichText, ScrollArea, Stroke, Vec2, Window};
use crate::analysis::field_analyzer::{AnalysisReport, FieldStats, InferredType};
use crate::formats::formatter::FormattingProgress;
use std::sync::{Arc, RwLock};

pub enum AnalyzerPanelAction {
    Dismiss,
    Cancel,
    SearchValue(String),
    ExportJson,
}

pub struct AnalyzerPanelState {
    pub is_open: bool,
    pub is_analyzing: bool,
    pub report: Option<AnalysisReport>,
    pub progress: Option<Arc<RwLock<FormattingProgress>>>,
    pub selected_field_index: Option<usize>,
    pub field_filter: String,
    pub status_msg: Option<String>,
}

impl Default for AnalyzerPanelState {
    fn default() -> Self {
        Self {
            is_open: false,
            is_analyzing: false,
            report: None,
            progress: None,
            selected_field_index: None,
            field_filter: String::new(),
            status_msg: None,
        }
    }
}

impl AnalyzerPanelState {
    pub fn reset_for_new_analysis(&mut self) {
        self.is_open = true;
        self.is_analyzing = true;
        self.report = None;
        self.progress = None;
        self.selected_field_index = None;
        self.field_filter.clear();
        self.status_msg = None;
    }

    pub fn set_finished(&mut self, report: AnalysisReport) {
        self.is_analyzing = false;
        if !report.fields.is_empty() {
            self.selected_field_index = Some(0);
        }
        self.report = Some(report);
    }

    pub fn set_cancelled(&mut self) {
        self.is_analyzing = false;
        self.status_msg = Some("Analysis was cancelled by user.".to_string());
    }

    pub fn set_error(&mut self, err: String) {
        self.is_analyzing = false;
        self.status_msg = Some(format!("Analysis failed: {}", err));
    }
}

pub fn render_analyzer_panel(
    ctx: &egui::Context,
    state: &mut AnalyzerPanelState,
) -> Option<AnalyzerPanelAction> {
    if !state.is_open {
        return None;
    }

    let mut action = None;
    let mut is_open = state.is_open;

    let screen = ctx.screen_rect();
    let modal_w = (screen.width() * 0.82).clamp(560.0, 940.0);
    let modal_h = (screen.height() * 0.82).clamp(400.0, 660.0);
    let max_w = (screen.width() * 0.94).max(400.0);
    let max_h = (screen.height() * 0.92).max(300.0);

    Window::new(RichText::new("📊 Field Analyzer & Schema Profiler").strong().size(15.0))
        .open(&mut is_open)
        .collapsible(false)
        .resizable(true)
        .default_size(Vec2::new(modal_w, modal_h))
        .max_size(Vec2::new(max_w, max_h))
        .min_size(Vec2::new(480.0, 340.0))
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            // Header summary or progress bar
            if state.is_analyzing {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.heading("Analyzing document fields & distributions...");
                    ui.add_space(16.0);

                    if let Some(ref prog_arc) = state.progress {
                        if let Ok(prog) = prog_arc.read() {
                            let pct = (prog.progress_pct / 100.0).clamp(0.0, 1.0);
                            ui.add_sized([480.0, 24.0], ProgressBar::new(pct).show_percentage().animate(true));
                            ui.add_space(12.0);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 16.0;
                                let processed_mb = (prog.bytes_processed as f64) / (1024.0 * 1024.0);
                                let total_mb = (prog.total_bytes as f64) / (1024.0 * 1024.0);
                                ui.label(RichText::new(format!("{:.1} MB / {:.1} MB", processed_mb, total_mb)).strong());
                                ui.separator();
                                ui.label(RichText::new(format!("{:.1} MB/s", prog.speed_mb_s)).color(Color32::from_rgb(100, 200, 255)));
                                ui.separator();
                                ui.label(format!("{:.1}s elapsed", prog.elapsed_secs));
                            });
                        }
                    } else {
                        ui.spinner();
                    }

                    ui.add_space(24.0);
                    if ui.button(RichText::new("Cancel Analysis").size(14.0)).clicked() {
                        action = Some(AnalyzerPanelAction::Cancel);
                    }
                });
                return;
            }

            if let Some(ref msg) = state.status_msg {
                ui.colored_label(Color32::from_rgb(240, 120, 120), msg);
                ui.add_space(8.0);
            }

            let Some(ref report) = state.report else {
                ui.label("No active analysis report.");
                return;
            };

            // Default to selecting the first field if none currently selected
            if state.selected_field_index.is_none() && !report.fields.is_empty() {
                state.selected_field_index = Some(0);
            }

            // Top metadata bar
            ui.horizontal(|ui| {
                ui.colored_label(
                    Color32::from_rgb(100, 200, 255),
                    format!("Total Records: {}", report.total_records),
                );
                ui.separator();
                ui.colored_label(
                    Color32::from_rgb(180, 220, 180),
                    format!("Fields: {}", report.fields.len()),
                );
                ui.separator();
                ui.label(format!("Size: {:.2} MB", (report.total_bytes as f64) / (1024.0 * 1024.0)));
                ui.separator();
                ui.label(format!("Time: {:.2}s", report.elapsed_secs));
                ui.separator();
                ui.label(format!("Speed: {:.1} MB/s", report.throughput_mb_s));

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("💾 Export JSON Schema").clicked() {
                        action = Some(AnalyzerPanelAction::ExportJson);
                    }
                });
            });

            ui.separator();

            // Main 2-column layout: Left (Field List), Right (Deep Dive)
            let avail_h = ui.available_height();
            let total_w = ui.available_width();
            let left_w = (total_w * 0.36).clamp(240.0, 360.0);

            ui.horizontal(|ui| {
                // Left column: Field selector with search
                ui.vertical(|ui| {
                    ui.set_width(left_w);
                    ui.set_max_width(left_w);
                    ui.set_height(avail_h);

                    ui.horizontal(|ui| {
                        ui.label("Filter:");
                        ui.text_edit_singleline(&mut state.field_filter);
                        if !state.field_filter.is_empty() && ui.small_button("✖").clicked() {
                            state.field_filter.clear();
                        }
                    });
                    ui.add_space(4.0);

                    // Field table header
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Field Name").strong());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new("Presence").strong());
                        });
                    });
                    ui.separator();

                    let filter_lower = state.field_filter.to_lowercase();
                    ScrollArea::vertical()
                        .id_salt("analyzer_field_list")
                        .auto_shrink([true, false])
                        .show(ui, |ui| {
                            for (orig_idx, f) in report.fields.iter().enumerate() {
                                if !filter_lower.is_empty() && !f.name.to_lowercase().contains(&filter_lower) {
                                    continue;
                                }

                                let is_selected = state.selected_field_index == Some(orig_idx);
                                let mut text = RichText::new(&f.name);
                                if is_selected {
                                    text = text.strong().color(Color32::from_rgb(255, 215, 0));
                                }

                                ui.horizontal(|ui| {
                                    render_type_badge(ui, &f.inferred_type);
                                    let resp = ui.selectable_label(is_selected, text);
                                    if resp.clicked() {
                                        state.selected_field_index = Some(orig_idx);
                                    }

                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.label(format!("{:.1}%", f.presence_pct));
                                    });
                                });
                            }
                        });
                });

                ui.separator();

                // Right column: Field details and Top-K frequency distribution
                ui.vertical(|ui| {
                    ui.set_height(avail_h);

                    if let Some(idx) = state.selected_field_index {
                        if let Some(field) = report.fields.get(idx) {
                            render_field_details(ui, field, report.total_records, &mut action);
                        }
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("Select a field from the left to view frequency distribution and statistics.");
                        });
                    }
                });
            });
        });

    if !is_open {
        state.is_open = false;
        if action.is_none() {
            action = Some(AnalyzerPanelAction::Dismiss);
        }
    }

    action
}

fn render_type_badge(ui: &mut egui::Ui, inferred_type: &InferredType) {
    let (label, bg_color) = match inferred_type {
        InferredType::Integer => ("INT", Color32::from_rgb(45, 115, 190)),
        InferredType::Float => ("FLT", Color32::from_rgb(50, 150, 150)),
        InferredType::String => ("STR", Color32::from_rgb(180, 100, 40)),
        InferredType::Boolean => ("BOOL", Color32::from_rgb(140, 60, 180)),
        InferredType::Null => ("NULL", Color32::from_rgb(120, 120, 120)),
        InferredType::Mixed => ("MIX", Color32::from_rgb(190, 60, 60)),
        InferredType::Object => ("OBJ", Color32::from_rgb(40, 140, 100)),
        InferredType::Array => ("ARR", Color32::from_rgb(100, 90, 170)),
    };

    let text = RichText::new(label).size(10.0).color(Color32::WHITE).strong();
    ui.painter().rect_filled(
        ui.available_rect_before_wrap(),
        2.0,
        Color32::TRANSPARENT,
    );
    let resp = egui::Frame::new()
        .fill(bg_color)
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(3, 1))
        .show(ui, |ui| {
            ui.label(text);
        });
    let _ = resp;
}

fn render_field_details(
    ui: &mut egui::Ui,
    field: &FieldStats,
    total_records: u64,
    action: &mut Option<AnalyzerPanelAction>,
) {
    // Header
    ui.horizontal(|ui| {
        ui.heading(&field.name);
        render_type_badge(ui, &field.inferred_type);
    });

    ui.add_space(4.0);

    // Summary statistics grid
    egui::Grid::new("field_stats_grid")
        .num_columns(4)
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            ui.label(RichText::new("Presence:").strong());
            ui.label(format!("{} / {} ({:.1}%)", field.total_occurrences, total_records, field.presence_pct));

            ui.label(RichText::new("Null Count:").strong());
            ui.label(format!("{}", field.null_count));
            ui.end_row();

            ui.label(RichText::new("Min Value:").strong());
            ui.label(field.min_value.as_deref().unwrap_or("—"));

            ui.label(RichText::new("Max Value:").strong());
            ui.label(field.max_value.as_deref().unwrap_or("—"));
            ui.end_row();

            if field.min_len > 0 {
                ui.label(RichText::new("Min Length:").strong());
                ui.label(format!("{}", field.min_len));
            } else {
                ui.label("");
                ui.label("");
            }

            if field.max_len > 0 {
                ui.label(RichText::new("Max Length:").strong());
                ui.label(format!("{}", field.max_len));
            } else {
                ui.label("");
                ui.label("");
            }
            ui.end_row();
        });

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);

    // Frequency Distribution Header
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("Top-{} Frequent Values (Approx. {} Distinct)", field.top_values.len(), field.cardinality_approx)).strong());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new("Click 🔍 to search value in document").size(11.0).italics());
        });
    });

    ui.add_space(4.0);

    // Top-K Frequency Table
    ScrollArea::vertical()
        .id_salt(format!("freq_scroll_{}", field.name))
        .auto_shrink([true, false])
        .show(ui, |ui| {
            let max_freq_count = field.top_values.first().map(|f| f.count).unwrap_or(1);

            for val_freq in &field.top_values {
                ui.horizontal(|ui| {
                    if ui.small_button("🔍").on_hover_text("Search this exact value in viewer").clicked() {
                        *action = Some(AnalyzerPanelAction::SearchValue(val_freq.value.clone()));
                    }

                    // Value label (truncated with hover tooltip)
                    let display_val = if val_freq.value.len() > 32 {
                        format!("{}...", &val_freq.value[..32])
                    } else if val_freq.value.is_empty() {
                        "\"\" (empty)".to_string()
                    } else {
                        val_freq.value.clone()
                    };

                    ui.label(RichText::new(&display_val).code())
                        .on_hover_text(&val_freq.value);

                    // Count and Percentage
                    let bar_width = 120.0;
                    let ratio = (val_freq.count as f32 / max_freq_count as f32).clamp(0.0, 1.0);

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(format!("{:.1}% ({})", val_freq.percentage, val_freq.count));

                        // Visual bar
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(bar_width, 12.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, Color32::from_rgb(45, 45, 55));
                        let filled_width = bar_width * ratio;
                        let filled_rect = egui::Rect::from_min_size(rect.min, Vec2::new(filled_width, 12.0));
                        ui.painter().rect_filled(filled_rect, 2.0, Color32::from_rgb(65, 140, 210));
                        ui.painter().rect_stroke(rect, 2.0, Stroke::new(1.0_f32, Color32::from_rgb(70, 70, 80)), egui::StrokeKind::Outside);
                    });
                });
                ui.add_space(2.0);
            }
        });
}
