use eframe::egui::{self, Color32, ProgressBar, RichText, Window};
use crate::analysis::ExtractionProgress;
use std::path::PathBuf;

pub enum FieldExtractModalAction {
    StartExportToFile {
        record_tag: String,
        fields: Vec<String>,
        delimiter: u8,
        include_headers: bool,
        output_path: PathBuf,
    },
    StartOpenInCsvGrid {
        record_tag: String,
        fields: Vec<String>,
        delimiter: u8,
        include_headers: bool,
    },
    Cancel,
    Dismiss,
}

pub struct FieldExtractModalState {
    pub is_open: bool,
    pub record_tag: String,
    pub fields_input: String,
    pub delimiter_idx: usize, // 0: Comma, 1: Tab, 2: Semicolon
    pub include_headers: bool,
    pub is_running: bool,
    pub progress: Option<ExtractionProgress>,
    pub status_message: Option<String>,
}

impl Default for FieldExtractModalState {
    fn default() -> Self {
        Self {
            is_open: false,
            record_tag: "item".to_string(),
            fields_input: "@id, name, value".to_string(),
            delimiter_idx: 0,
            include_headers: true,
            is_running: false,
            progress: None,
            status_message: None,
        }
    }
}

pub fn render_field_extract_modal(
    ctx: &egui::Context,
    state: &mut FieldExtractModalState,
    is_xml: bool,
) -> Option<FieldExtractModalAction> {
    if !state.is_open {
        return None;
    }

    let mut action = None;

    Window::new(RichText::new("📊 Streaming Field Extraction & CSV Export").strong())
        .collapsible(false)
        .resizable(false)
        .default_width(460.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.add_space(4.0);

            if state.is_running {
                ui.heading("Extracting Fields...");
                ui.add_space(6.0);

                if let Some(ref prog) = state.progress {
                    let pct = if prog.total_bytes > 0 {
                        (prog.bytes_processed as f32 / prog.total_bytes as f32).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    ui.add(ProgressBar::new(pct).show_percentage().animate(true));
                    ui.add_space(6.0);

                    let mb_proc = prog.bytes_processed as f64 / (1024.0 * 1024.0);
                    let mb_tot = prog.total_bytes as f64 / (1024.0 * 1024.0);
                    ui.label(format!("Records Extracted: {}", prog.records_extracted));
                    ui.label(format!("{:.1} MB / {:.1} MB processed", mb_proc, mb_tot));
                } else {
                    ui.spinner();
                }

                ui.add_space(10.0);
                if ui.button("❌ Cancel Extraction").clicked() {
                    action = Some(FieldExtractModalAction::Cancel);
                }
                return;
            }

            if let Some(ref msg) = state.status_message {
                ui.colored_label(Color32::from_rgb(152, 195, 121), msg);
                ui.add_space(8.0);
                if ui.button("Done").clicked() {
                    state.status_message = None;
                    state.is_open = false;
                    action = Some(FieldExtractModalAction::Dismiss);
                }
                return;
            }

            ui.label(RichText::new("Extract recurring records and target attributes/fields into a clean CSV without building a DOM in memory.").size(12.0));
            ui.add_space(8.0);

            // Record Tag
            ui.horizontal(|ui| {
                let tag_label = if is_xml { "Repeating XML Element:" } else { "Record Container / Key:" };
                ui.label(tag_label);
                ui.text_edit_singleline(&mut state.record_tag);
            });

            // Fields Input
            ui.add_space(6.0);
            ui.label("Fields to Extract (comma-separated):");
            let fields_hint = if is_xml {
                "e.g. @id, title, price, author@country"
            } else {
                "e.g. id, name, price, timestamp"
            };
            ui.add(
                egui::TextEdit::singleline(&mut state.fields_input)
                    .hint_text(fields_hint)
                    .desired_width(ui.available_width())
            );

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Delimiter:");
                ui.radio_value(&mut state.delimiter_idx, 0, "Comma (,)");
                ui.radio_value(&mut state.delimiter_idx, 1, "Tab (\\t)");
                ui.radio_value(&mut state.delimiter_idx, 2, "Semicolon (;)");
            });

            ui.checkbox(&mut state.include_headers, "Include Header Row");

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let delimiter = match state.delimiter_idx {
                    1 => b'\t',
                    2 => b';',
                    _ => b',',
                };
                let parsed_fields: Vec<String> = state
                    .fields_input
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();

                // Export to file button
                if ui.button(RichText::new("💾 Save as CSV File...").strong()).clicked() {
                    if !parsed_fields.is_empty() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_title("Save Extracted Records As CSV")
                            .add_filter("CSV File", &["csv"])
                            .add_filter("TSV File", &["tsv"])
                            .add_filter("All Files", &["*"])
                            .save_file()
                        {
                            action = Some(FieldExtractModalAction::StartExportToFile {
                                record_tag: state.record_tag.trim().to_string(),
                                fields: parsed_fields.clone(),
                                delimiter,
                                include_headers: state.include_headers,
                                output_path: path,
                            });
                        }
                    }
                }

                // Open in CSV Grid Tab button
                if ui.button(RichText::new("📊 Open in CSV Grid Tab").color(Color32::from_rgb(97, 175, 239))).clicked() {
                    if !parsed_fields.is_empty() {
                        action = Some(FieldExtractModalAction::StartOpenInCsvGrid {
                            record_tag: state.record_tag.trim().to_string(),
                            fields: parsed_fields,
                            delimiter,
                            include_headers: state.include_headers,
                        });
                    }
                }

                if ui.button("Cancel").clicked() {
                    state.is_open = false;
                    action = Some(FieldExtractModalAction::Dismiss);
                }
            });
            ui.add_space(4.0);
        });

    action
}
