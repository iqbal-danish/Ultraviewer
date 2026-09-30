use eframe::egui::{self, Color32, Key, RichText, Vec2, Window};
use crate::file_engine::ZipEntryInfo;
use crate::formats::FileType;
use crate::app::icons::{paint_icon, Icon};
use std::path::PathBuf;

pub enum ZipModalAction {
    ExtractAndOpen(usize),
    ExtractAndOpenAllFeeds,
    Dismiss,
}

pub struct ZipModalState {
    pub is_open: bool,
    pub archive_path: Option<PathBuf>,
    pub entries: Vec<ZipEntryInfo>,
    pub selected_index: Option<usize>,
    pub filter_query: String,
}

impl Default for ZipModalState {
    fn default() -> Self {
        Self {
            is_open: false,
            archive_path: None,
            entries: Vec::new(),
            selected_index: None,
            filter_query: String::new(),
        }
    }
}

impl ZipModalState {
    pub fn open(&mut self, path: PathBuf, entries: Vec<ZipEntryInfo>) {
        self.is_open = true;
        self.archive_path = Some(path);
        self.entries = entries;
        self.filter_query.clear();
        self.selected_index = self.entries.iter().position(|e| !e.is_dir);
    }

    pub fn close(&mut self) {
        self.is_open = false;
        self.archive_path = None;
        self.entries.clear();
        self.selected_index = None;
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

pub fn render_zip_modal(
    ctx: &egui::Context,
    state: &mut ZipModalState,
) -> Option<ZipModalAction> {
    if !state.is_open {
        return None;
    }

    let mut action = None;
    let mut is_open = state.is_open;

    let archive_name = state
        .archive_path
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|f| f.to_str())
        .unwrap_or("Archive.zip")
        .to_string();

    Window::new(format!("ZIP Archive Explorer — {}", archive_name))
        .open(&mut is_open)
        .resizable(true)
        .collapsible(false)
        .default_width(680.0)
        .default_height(480.0)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.add_space(4.0);

            // Header summary info
            let total_files = state.entries.iter().filter(|e| !e.is_dir).count();
            let total_uncompressed: u64 = state.entries.iter().map(|e| e.uncompressed_size).sum();

            ui.horizontal(|ui| {
                let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
                paint_icon(ui.painter(), icon_rect, Icon::Archive, Color32::from_rgb(229, 192, 123));
                ui.label(
                    RichText::new(format!(
                        "Contains {} file{} ({})",
                        total_files,
                        if total_files == 1 { "" } else { "s" },
                        format_bytes(total_uncompressed)
                    ))
                    .size(13.5)
                    .color(Color32::from_rgb(209, 218, 230)),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new("Select a file to inspect or extract all feeds")
                            .small()
                            .color(Color32::from_rgb(130, 140, 155)),
                    );
                });
            });

            ui.add_space(6.0);

            // Filter input box
            ui.horizontal(|ui| {
                ui.label(RichText::new("Filter:").color(Color32::from_rgb(150, 160, 175)));
                ui.add(
                    egui::TextEdit::singleline(&mut state.filter_query)
                        .hint_text("Search files inside zip...")
                        .desired_width(ui.available_width() - 8.0),
                );
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            // Filtered entries
            let query = state.filter_query.to_lowercase();
            let filtered_entries: Vec<&ZipEntryInfo> = state
                .entries
                .iter()
                .filter(|e| !e.is_dir && (query.is_empty() || e.name.to_lowercase().contains(&query)))
                .collect();

            // Table Header
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(RichText::new("Name").strong().color(Color32::from_rgb(170, 180, 195)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(16.0);
                    ui.label(RichText::new("Ratio").strong().color(Color32::from_rgb(170, 180, 195)));
                    ui.add_space(30.0);
                    ui.label(RichText::new("Compressed").strong().color(Color32::from_rgb(170, 180, 195)));
                    ui.add_space(30.0);
                    ui.label(RichText::new("Size").strong().color(Color32::from_rgb(170, 180, 195)));
                });
            });

            ui.separator();

            // Scrollable List of Files
            let row_height = 28.0;
            let list_height = (ui.available_height() - 50.0).max(180.0);

            let mut double_clicked_index = None;

            egui::ScrollArea::vertical()
                .max_height(list_height)
                .auto_shrink([false, false])
                .show_rows(ui, row_height, filtered_entries.len(), |ui, row_range| {
                    for row_idx in row_range {
                        let entry = filtered_entries[row_idx];
                        let is_selected = state.selected_index == Some(entry.index);

                        let (rect, resp) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), row_height),
                            egui::Sense::click(),
                        );

                        let bg = if is_selected {
                            Color32::from_rgb(38, 79, 120) // Active selection blue
                        } else if resp.hovered() {
                            Color32::from_rgb(35, 40, 48)
                        } else if row_idx % 2 == 1 {
                            Color32::from_rgb(24, 27, 32)
                        } else {
                            Color32::from_rgb(20, 23, 27)
                        };

                        ui.painter().rect_filled(rect, 2.0, bg);

                        // File type icon
                        let (type_icon, icon_color) = match entry.file_type {
                            Some(FileType::Xml) => (Icon::XmlCode, Color32::from_rgb(56, 189, 248)),
                            Some(FileType::Json) => (Icon::JsonBraces, Color32::from_rgb(250, 204, 21)),
                            Some(FileType::Csv) => (Icon::File, Color32::from_rgb(52, 211, 153)),
                            _ => (Icon::File, Color32::from_rgb(156, 163, 175)),
                        };

                        let icon_box = egui::Rect::from_min_size(
                            egui::pos2(rect.left() + 8.0, rect.center().y - 8.0),
                            Vec2::splat(16.0),
                        );
                        paint_icon(ui.painter(), icon_box, type_icon, icon_color);

                        // File name
                        let name_pos = egui::pos2(rect.left() + 32.0, rect.center().y);
                        ui.painter().text(
                            name_pos,
                            egui::Align2::LEFT_CENTER,
                            &entry.name,
                            egui::FontId::proportional(13.0),
                            if is_selected { Color32::WHITE } else { Color32::from_rgb(220, 230, 242) },
                        );

                        // Right aligned: Ratio, Compressed size, Uncompressed size
                        let ratio = if entry.uncompressed_size > 0 {
                            let r = 100.0 - (entry.compressed_size as f64 / entry.uncompressed_size as f64 * 100.0);
                            format!("{:.0}%", r.max(0.0))
                        } else {
                            "-".to_string()
                        };

                        let ratio_pos = egui::pos2(rect.right() - 20.0, rect.center().y);
                        ui.painter().text(
                            ratio_pos,
                            egui::Align2::RIGHT_CENTER,
                            ratio,
                            egui::FontId::monospace(11.0),
                            Color32::from_rgb(140, 180, 140),
                        );

                        let comp_pos = egui::pos2(rect.right() - 85.0, rect.center().y);
                        ui.painter().text(
                            comp_pos,
                            egui::Align2::RIGHT_CENTER,
                            format_bytes(entry.compressed_size),
                            egui::FontId::monospace(11.0),
                            Color32::from_rgb(130, 140, 155),
                        );

                        let uncomp_pos = egui::pos2(rect.right() - 170.0, rect.center().y);
                        ui.painter().text(
                            uncomp_pos,
                            egui::Align2::RIGHT_CENTER,
                            format_bytes(entry.uncompressed_size),
                            egui::FontId::monospace(11.5),
                            Color32::from_rgb(200, 210, 225),
                        );

                        if resp.clicked() {
                            state.selected_index = Some(entry.index);
                        }
                        if resp.double_clicked() {
                            double_clicked_index = Some(entry.index);
                        }
                    }
                });

            if let Some(idx) = double_clicked_index {
                action = Some(ZipModalAction::ExtractAndOpen(idx));
            }

            // Keyboard navigation: Enter opens selected, Escape closes
            if ctx.input(|i| i.key_pressed(Key::Enter)) {
                if let Some(idx) = state.selected_index {
                    action = Some(ZipModalAction::ExtractAndOpen(idx));
                }
            }
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                action = Some(ZipModalAction::Dismiss);
            }

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            // Bottom Buttons
            ui.horizontal(|ui| {
                let has_selection = state.selected_index.is_some();
                let has_feeds = state.entries.iter().any(|e| {
                    !e.is_dir && matches!(e.file_type, Some(FileType::Xml | FileType::Json | FileType::Csv))
                });

                if ui
                    .add_enabled(has_selection, egui::Button::new(RichText::new("Open Selected").strong()))
                    .on_hover_text("Extract and open the highlighted file in a new tab")
                    .clicked()
                {
                    if let Some(idx) = state.selected_index {
                        action = Some(ZipModalAction::ExtractAndOpen(idx));
                    }
                }

                if has_feeds && total_files > 1 {
                    ui.add_space(4.0);
                    if ui
                        .button(RichText::new("Open All Data Feeds").color(Color32::from_rgb(97, 175, 239)))
                        .on_hover_text("Extract and open all XML, JSON, and CSV feeds in tabs")
                        .clicked()
                    {
                        action = Some(ZipModalAction::ExtractAndOpenAllFeeds);
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Cancel").clicked() {
                        action = Some(ZipModalAction::Dismiss);
                    }
                });
            });
        });

    if !is_open {
        state.close();
        if action.is_none() {
            action = Some(ZipModalAction::Dismiss);
        }
    }

    action
}
