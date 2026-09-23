use eframe::egui::{self, Color32, Key, ProgressBar, RichText, Vec2, Window};
use crate::file_engine::DownloadStatus;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub enum UrlModalAction {
    StartDownload {
        url: String,
        headers: Vec<(String, String)>,
    },
    CancelDownload,
    Dismiss,
    OpenDownloadedFile(PathBuf),
}

pub struct UrlModalState {
    pub is_open: bool,
    pub url_input: String,
    pub headers_input: String,
    pub show_headers: bool,
    pub is_downloading: bool,
    pub progress: Option<DownloadStatus>,
    pub error_message: Option<String>,
    pub cancel_token: Option<Arc<AtomicBool>>,
    pub rx: Option<crossbeam_channel::Receiver<DownloadStatus>>,
}

impl Default for UrlModalState {
    fn default() -> Self {
        Self {
            is_open: false,
            url_input: String::new(),
            headers_input: String::new(),
            show_headers: false,
            is_downloading: false,
            progress: None,
            error_message: None,
            cancel_token: None,
            rx: None,
        }
    }
}

impl UrlModalState {
    pub fn open(&mut self) {
        self.is_open = true;
        self.error_message = None;
    }

    pub fn close(&mut self) {
        if self.is_downloading {
            if let Some(ref cancel) = self.cancel_token {
                cancel.store(true, Ordering::Relaxed);
            }
        }
        self.is_open = false;
        self.is_downloading = false;
        self.progress = None;
        self.rx = None;
        self.cancel_token = None;
    }

    pub fn parse_headers(&self) -> Vec<(String, String)> {
        let mut headers = Vec::new();
        for line in self.headers_input.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some(idx) = trimmed.find(':') {
                let key = trimmed[..idx].trim().to_string();
                let val = trimmed[idx + 1..].trim().to_string();
                if !key.is_empty() && !val.is_empty() {
                    headers.push((key, val));
                }
            }
        }
        headers
    }
}

pub fn render_url_modal(
    ctx: &egui::Context,
    state: &mut UrlModalState,
    dark_mode: bool,
) -> Option<UrlModalAction> {
    if !state.is_open {
        return None;
    }

    let mut action = None;

    // Check for incoming progress updates from background downloader thread
    let mut completed_path = None;
    let mut failed_error = None;
    let mut cancelled = false;

    if let Some(ref rx) = state.rx {
        while let Ok(status) = rx.try_recv() {
            match status {
                DownloadStatus::Completed { file_path, .. } => {
                    completed_path = Some(file_path);
                }
                DownloadStatus::Failed { error } => {
                    failed_error = Some(error);
                }
                DownloadStatus::Cancelled => {
                    cancelled = true;
                }
                _ => {
                    state.progress = Some(status);
                    ctx.request_repaint();
                }
            }
        }
    }

    if let Some(path) = completed_path {
        state.is_downloading = false;
        state.rx = None;
        state.cancel_token = None;
        state.is_open = false;
        action = Some(UrlModalAction::OpenDownloadedFile(path));
    } else if let Some(err) = failed_error {
        state.error_message = Some(err);
        state.progress = None;
        state.is_downloading = false;
        state.rx = None;
        state.cancel_token = None;
    } else if cancelled {
        state.error_message = Some("Download was cancelled.".to_string());
        state.progress = None;
        state.is_downloading = false;
        state.rx = None;
        state.cancel_token = None;
    }

    let bg_color = if dark_mode { Color32::from_rgb(33, 37, 43) } else { Color32::from_rgb(250, 252, 255) };
    let border_color = if dark_mode { Color32::from_rgb(60, 68, 80) } else { Color32::from_rgb(210, 218, 230) };

    Window::new(RichText::new("🌐 Open Feed from URL").size(15.0).strong())
        .collapsible(false)
        .resizable(false)
        .fixed_size(Vec2::new(540.0, 0.0))
        .anchor(egui::Align2::CENTER_CENTER, Vec2::new(0.0, -40.0))
        .frame(
            egui::Frame::window(&ctx.style())
                .fill(bg_color)
                .stroke(egui::Stroke::new(1.0_f32, border_color))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::symmetric(18, 16))
                .shadow(egui::Shadow {
                    offset: [0, 8],
                    blur: 24,
                    spread: 4,
                    color: Color32::from_black_alpha(180),
                }),
        )
        .show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(
                RichText::new("Enter an HTTP or HTTPS link to stream and inspect online XML/JSON feeds directly.")
                    .size(13.0)
                    .color(if dark_mode { Color32::from_rgb(175, 185, 200) } else { Color32::from_rgb(80, 90, 105) }),
            );
            ui.label(
                RichText::new("Supports direct streaming for multi-GB files and automatic .gz decompression.")
                    .size(11.5)
                    .color(if dark_mode { Color32::from_rgb(130, 140, 155) } else { Color32::from_rgb(110, 120, 135) }),
            );

            ui.add_space(6.0);

            // Storage Location & Open Folder
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("📁 Storage:")
                        .size(11.5)
                        .strong()
                        .color(if dark_mode { Color32::from_rgb(160, 172, 190) } else { Color32::from_rgb(80, 90, 105) }),
                );
                ui.label(
                    RichText::new("%TEMP%\\ultraviewer_downloads\\")
                        .size(11.5)
                        .monospace()
                        .color(if dark_mode { Color32::from_rgb(97, 175, 239) } else { Color32::from_rgb(33, 100, 200) }),
                );
                if ui.small_button("📂 Open Folder").on_hover_text("Open temporary downloads folder in Windows Explorer to view or delete files").clicked() {
                    let dir = std::env::temp_dir().join("ultraviewer_downloads");
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                }
            });

            ui.add_space(10.0);

            // URL Input Field
            ui.label(RichText::new("Feed URL:").strong().size(13.0));
            let url_edit = ui.add_enabled(
                !state.is_downloading,
                egui::TextEdit::singleline(&mut state.url_input)
                    .hint_text("https://example.com/feed.xml or feed.xml.gz...")
                    .desired_width(ui.available_width())
                    .font(egui::FontId::monospace(13.5))
                    .margin(egui::Margin::symmetric(8, 8)),
            );

            // Auto-focus URL input on open
            if state.is_open && !state.is_downloading && state.url_input.is_empty() {
                url_edit.request_focus();
            }

            ui.add_space(8.0);

            // Collapsible Advanced Headers
            ui.horizontal(|ui| {
                let arrow = if state.show_headers { "▼" } else { "▶" };
                if ui.button(format!("{} Advanced: HTTP Headers (Auth / Tokens)", arrow)).clicked() {
                    state.show_headers = !state.show_headers;
                }
            });

            if state.show_headers {
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Optional headers (one per line, e.g. 'Authorization: Bearer <token>'):")
                        .size(11.5)
                        .color(Color32::from_rgb(140, 150, 165)),
                );
                ui.add_enabled(
                    !state.is_downloading,
                    egui::TextEdit::multiline(&mut state.headers_input)
                        .hint_text("Authorization: Bearer secret_token_123\nAccept: application/xml")
                        .desired_rows(3)
                        .desired_width(ui.available_width())
                        .font(egui::FontId::monospace(12.5)),
                );
            }

            // Error display
            if let Some(ref err) = state.error_message {
                ui.add_space(10.0);
                egui::Frame::NONE
                    .fill(Color32::from_rgb(50, 24, 28))
                    .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(224, 108, 117)))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("⚠️").size(14.0));
                            ui.label(RichText::new(err).color(Color32::from_rgb(224, 108, 117)).size(12.5));
                        });
                    });
            }

            // Progress / Status display
            if state.is_downloading {
                ui.add_space(12.0);
                egui::Frame::NONE
                    .fill(if dark_mode { Color32::from_rgb(26, 30, 36) } else { Color32::from_rgb(240, 244, 250) })
                    .stroke(egui::Stroke::new(1.0_f32, if dark_mode { Color32::from_rgb(50, 56, 66) } else { Color32::from_rgb(205, 212, 222) }))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        match &state.progress {
                            Some(DownloadStatus::Connecting) | None => {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(RichText::new("Connecting to host...").size(13.0).color(Color32::from_rgb(97, 175, 239)));
                                });
                            }
                            Some(DownloadStatus::Downloading { bytes_downloaded, total_bytes, speed_mb_s, elapsed_secs }) => {
                                let mb_down = *bytes_downloaded as f64 / (1024.0 * 1024.0);
                                let (pct, progress_text) = if let Some(tot) = total_bytes {
                                    let mb_total = *tot as f64 / (1024.0 * 1024.0);
                                    let p = (*bytes_downloaded as f32 / *tot as f32).clamp(0.0, 1.0);
                                    (p, format!("{:.1} MB / {:.1} MB ({:.0}%)", mb_down, mb_total, p * 100.0))
                                } else {
                                    (0.5, format!("{:.1} MB streamed", mb_down))
                                };

                                ui.label(RichText::new("Streaming download:").strong().size(12.5));
                                ui.add(ProgressBar::new(pct).show_percentage().animate(true));

                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(progress_text).size(12.0).color(Color32::from_rgb(170, 180, 195)));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(RichText::new(format!("{:.1} MB/s | {:.0}s", speed_mb_s, elapsed_secs)).size(12.0).color(Color32::from_rgb(152, 195, 121)));
                                    });
                                });
                            }
                            Some(DownloadStatus::Decompressing { bytes_decompressed }) => {
                                let mb = *bytes_decompressed as f64 / (1024.0 * 1024.0);
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(RichText::new(format!("Decompressing gzip archive... ({:.1} MB unpacked)", mb)).size(13.0).color(Color32::from_rgb(229, 192, 123)));
                                });
                            }
                            _ => {}
                        }
                    });
            }

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(8.0);

            // Bottom Buttons
            ui.horizontal(|ui| {
                if state.is_downloading {
                    let cancel_btn = egui::Button::new(RichText::new("Cancel Download").color(Color32::WHITE))
                        .fill(Color32::from_rgb(190, 60, 70))
                        .min_size(Vec2::new(120.0, 30.0))
                        .corner_radius(4.0);
                    if ui.add(cancel_btn).clicked() {
                        action = Some(UrlModalAction::CancelDownload);
                    }
                } else {
                    let close_btn = egui::Button::new("Cancel")
                        .min_size(Vec2::new(80.0, 30.0))
                        .corner_radius(4.0);
                    if ui.add(close_btn).clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                        action = Some(UrlModalAction::Dismiss);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let has_valid_url = state.url_input.trim().starts_with("http://")
                            || state.url_input.trim().starts_with("https://");

                        let dl_btn = egui::Button::new(
                            RichText::new("⬇ Download & Open")
                                .strong()
                                .color(if has_valid_url { Color32::WHITE } else { Color32::from_rgb(140, 150, 160) }),
                        )
                        .fill(if has_valid_url { Color32::from_rgb(45, 110, 200) } else { Color32::from_rgb(50, 56, 66) })
                        .min_size(Vec2::new(150.0, 30.0))
                        .corner_radius(4.0);

                        let enter_pressed = url_edit.has_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                        if ui.add_enabled(has_valid_url, dl_btn).clicked() || (has_valid_url && enter_pressed) {
                            let headers = state.parse_headers();
                            action = Some(UrlModalAction::StartDownload {
                                url: state.url_input.trim().to_string(),
                                headers,
                            });
                        }
                    });
                }
            });
        });

    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_headers() {
        let mut state = UrlModalState::default();
        state.headers_input = "Authorization: Bearer secret_123\nAccept: application/xml\n# Comment line\n\nContent-Type: text/xml".to_string();

        let headers = state.parse_headers();
        assert_eq!(headers.len(), 3);
        assert_eq!(headers[0], ("Authorization".to_string(), "Bearer secret_123".to_string()));
        assert_eq!(headers[1], ("Accept".to_string(), "application/xml".to_string()));
        assert_eq!(headers[2], ("Content-Type".to_string(), "text/xml".to_string()));
    }

    #[test]
    fn test_modal_lifecycle() {
        let mut state = UrlModalState::default();
        assert!(!state.is_open);

        state.open();
        assert!(state.is_open);

        state.close();
        assert!(!state.is_open);
    }
}

