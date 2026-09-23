use eframe::egui::{self, Color32, ProgressBar, RichText, Window};
use crate::formats::formatter::FormattingProgress;
use std::path::{Path, PathBuf};

pub enum FormatModalAction {
    Cancel,
    Dismiss,
    OpenTarget(PathBuf),
}

pub fn render_format_modal(
    ctx: &egui::Context,
    title: &str,
    progress: &FormattingProgress,
    dst_path: &Path,
    is_in_place: bool,
) -> Option<FormatModalAction> {
    let mut result = None;

    Window::new(RichText::new("⚡ Streaming Document Formatter").strong())
        .collapsible(false)
        .resizable(false)
        .default_width(440.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.add_space(6.0);
            ui.heading(title);
            ui.add_space(4.0);

            if let Some(ref err) = progress.error {
                ui.colored_label(Color32::from_rgb(230, 70, 70), format!("Error: {}", err));
                ui.add_space(10.0);
                if ui.button("Close").clicked() {
                    result = Some(FormatModalAction::Dismiss);
                }
                return;
            }

            if progress.is_finished {
                ui.colored_label(
                    Color32::from_rgb(80, 220, 100),
                    format!(
                        "Successfully formatted {:.2} MB in {:.2}s ({:.1} MB/s)",
                        (progress.bytes_processed as f64) / (1024.0 * 1024.0),
                        progress.elapsed_secs,
                        progress.speed_mb_s,
                    ),
                );
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    if !is_in_place {
                        if ui.button("📂 Open Formatted File").clicked() {
                            result = Some(FormatModalAction::OpenTarget(dst_path.to_path_buf()));
                        }
                    }
                    if ui.button("Dismiss").clicked() {
                        result = Some(FormatModalAction::Dismiss);
                    }
                });
            } else {
                // Formatting in progress
                let pct = (progress.progress_pct / 100.0).clamp(0.0, 1.0);
                ui.add(ProgressBar::new(pct).show_percentage().animate(true));
                ui.add_space(6.0);

                let processed_mb = (progress.bytes_processed as f64) / (1024.0 * 1024.0);
                let total_mb = (progress.total_bytes as f64) / (1024.0 * 1024.0);

                ui.horizontal(|ui| {
                    ui.label(format!("{:.1} MB / {:.1} MB", processed_mb, total_mb));
                    ui.separator();
                    ui.label(format!("{:.1} MB/s", progress.speed_mb_s));
                    if let Some(eta) = progress.eta_secs {
                        ui.separator();
                        ui.label(format!("ETA: {:.0}s", eta));
                    }
                });

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("❌ Cancel Formatting").clicked() {
                        result = Some(FormatModalAction::Cancel);
                    }
                });
            }
            ui.add_space(6.0);
        });

    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatTarget {
    NewTab,
    InPlace,
    SaveAs,
}

pub enum FormatOptionsModalAction {
    Execute {
        indent_size: usize,
        use_tabs: bool,
        is_minify: bool,
        target: FormatTarget,
    },
    Dismiss,
}

pub struct FormatOptionsModalState {
    pub is_open: bool,
    pub is_minify: bool,
    pub indent_idx: usize, // 0: 2 spaces, 1: 4 spaces, 2: tabs
    pub target: FormatTarget,
}

impl Default for FormatOptionsModalState {
    fn default() -> Self {
        Self {
            is_open: false,
            is_minify: false,
            indent_idx: 0,
            target: FormatTarget::NewTab,
        }
    }
}

pub fn render_format_options_modal(
    ctx: &egui::Context,
    state: &mut FormatOptionsModalState,
    file_type_name: &str,
) -> Option<FormatOptionsModalAction> {
    if !state.is_open {
        return None;
    }

    let mut action = None;
    let title = if state.is_minify {
        format!("Minify {} Document", file_type_name)
    } else {
        format!("Format & Beautify {} Document", file_type_name)
    };

    Window::new(RichText::new(format!("⚡ {}", title)).strong())
        .collapsible(false)
        .resizable(false)
        .default_width(380.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.add_space(4.0);

            if !state.is_minify {
                ui.label(RichText::new("Indentation:").strong());
                ui.horizontal(|ui| {
                    ui.radio_value(&mut state.indent_idx, 0, "2 Spaces");
                    ui.radio_value(&mut state.indent_idx, 1, "4 Spaces");
                    ui.radio_value(&mut state.indent_idx, 2, "Tabs");
                });
                ui.add_space(8.0);
            }

            ui.label(RichText::new("Destination:").strong());
            ui.radio_value(&mut state.target, FormatTarget::NewTab, "Open Formatted in New Tab (Recommended / Non-destructive)");
            ui.radio_value(&mut state.target, FormatTarget::InPlace, "Replace Current File In-Place");
            ui.radio_value(&mut state.target, FormatTarget::SaveAs, "Save Formatted As New File...");

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let action_btn_text = if state.is_minify { "⚡ Minify" } else { "⚡ Format" };
                if ui.button(RichText::new(action_btn_text).strong().color(Color32::from_rgb(97, 175, 239))).clicked() {
                    let (indent_size, use_tabs) = match state.indent_idx {
                        1 => (4, false),
                        2 => (1, true),
                        _ => (2, false),
                    };
                    action = Some(FormatOptionsModalAction::Execute {
                        indent_size,
                        use_tabs,
                        is_minify: state.is_minify,
                        target: state.target,
                    });
                    state.is_open = false;
                }

                if ui.button("Cancel").clicked() {
                    state.is_open = false;
                    action = Some(FormatOptionsModalAction::Dismiss);
                }
            });
            ui.add_space(4.0);
        });

    action
}

