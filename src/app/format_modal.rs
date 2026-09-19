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
                        "✓ Successfully formatted {:.2} MB in {:.2}s ({:.1} MB/s)",
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
