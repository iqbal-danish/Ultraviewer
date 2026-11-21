use eframe::egui::{self, Color32, FontId, Layout, RichText, Sense, Ui, Vec2};
use crate::scripting::{OutputChunkKind, PythonRunStatus, PythonRunnerState};
use super::icons::{paint_icon, Icon};

pub enum PythonDrawerAction {
    Run,
    Stop,
    Clear,
    Close,
    ToggleWrap,
    OpenInNewTab,
    SaveToFile,
}

pub fn render_python_drawer(
    ui: &mut Ui,
    state: &mut PythonRunnerState,
    base_font_size: f32,
) -> Option<PythonDrawerAction> {
    let mut action = None;

    ui.vertical(|ui| {
        // --- 1. Top Header Bar ---
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;

            // Python Logo / Icon
            let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
            paint_icon(ui.painter(), icon_rect, Icon::Play, Color32::from_rgb(56, 189, 248));

            // Title
            ui.label(
                RichText::new("Python Console")
                    .strong()
                    .size(13.0)
                    .color(Color32::from_rgb(220, 225, 235)),
            );

            // Status Badge
            match &state.status {
                PythonRunStatus::Idle => {
                    ui.label(
                        RichText::new("Idle")
                            .size(11.5)
                            .color(Color32::from_rgb(130, 140, 155)),
                    );
                }
                PythonRunStatus::Running { started_at, pid } => {
                    let elapsed = started_at.elapsed().as_secs_f64();
                    let badge_text = format!("● Running [PID: {}] ({:.1}s)", pid, elapsed);
                    ui.colored_label(
                        Color32::from_rgb(97, 175, 239),
                        RichText::new(badge_text).strong().size(11.5),
                    );
                }
                PythonRunStatus::Finished { exit_code, duration, success } => {
                    let (color, icon_str, code_str) = if *success {
                        (Color32::from_rgb(152, 195, 121), "✔", "exit 0".to_string())
                    } else {
                        (
                            Color32::from_rgb(224, 108, 117),
                            "✖",
                            format!("exit {}", exit_code.unwrap_or(1)),
                        )
                    };
                    let text = format!("{} Finished in {:.2}s ({})", icon_str, duration.as_secs_f64(), code_str);
                    ui.colored_label(color, RichText::new(text).strong().size(11.5));
                }
                PythonRunStatus::Terminated { duration } => {
                    let text = format!("⏹ Terminated after {:.2}s", duration.as_secs_f64());
                    ui.colored_label(
                        Color32::from_rgb(229, 192, 123),
                        RichText::new(text).strong().size(11.5),
                    );
                }
                PythonRunStatus::FailedToStart(_) => {
                    ui.colored_label(
                        Color32::from_rgb(224, 108, 117),
                        RichText::new("✖ Failed to start").strong().size(11.5),
                    );
                }
            }

            // Interpreter version tag
            if let Some(ref ver) = state.cached_interpreter_info {
                ui.separator();
                ui.label(
                    RichText::new(ver)
                        .size(11.0)
                        .color(Color32::from_rgb(110, 120, 138)),
                );
            }

            // Right-aligned actions: Run/Stop, Copy, Clear, Wrap, Close
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(4.0);

                // Close drawer button
                if ui
                    .small_button("✖")
                    .on_hover_text("Close Console Drawer (Ctrl+`)")
                    .clicked()
                {
                    action = Some(PythonDrawerAction::Close);
                }

                ui.separator();

                // Clear output button
                if ui
                    .small_button("🗑 Clear")
                    .on_hover_text("Clear Output")
                    .clicked()
                {
                    action = Some(PythonDrawerAction::Clear);
                }

                // Save full output to file
                let has_output = state.total_stdout_lines > 0 || state.output_lines.iter().any(|l| l.kind == OutputChunkKind::Stdout);
                if has_output {
                    if ui
                        .small_button("💾 Save As...")
                        .on_hover_text("Save complete 100% output to a file on disk")
                        .clicked()
                    {
                        action = Some(PythonDrawerAction::SaveToFile);
                    }
                }

                // Open output in new tab
                if has_output {
                    if ui
                        .small_button("📄 New Tab")
                        .on_hover_text("Open complete stdout results in a new UltraViewer tab for viewing, searching, or formatting")
                        .clicked()
                    {
                        action = Some(PythonDrawerAction::OpenInNewTab);
                    }
                }

                // Copy all output button (copies full spool if within clipboard limit)
                if ui
                    .small_button("📋 Copy All")
                    .on_hover_text("Copy complete output to clipboard")
                    .clicked()
                {
                    let mut copied = false;
                    if let Some(ref spool) = state.spool_path {
                        if spool.exists() {
                            if let Ok(metadata) = std::fs::metadata(spool) {
                                if metadata.len() <= 33_554_432 { // Up to 32 MB
                                    if let Ok(full_spool_text) = std::fs::read_to_string(spool) {
                                        ui.ctx().copy_text(full_spool_text);
                                        copied = true;
                                    }
                                }
                            }
                        }
                    }
                    if !copied {
                        let full_text = state
                            .output_lines
                            .iter()
                            .map(|l| l.text.as_str())
                            .collect::<Vec<_>>()
                            .join("\n");
                        ui.ctx().copy_text(full_text);
                    }
                }

                // Word wrap toggle button
                let wrap_label = if state.word_wrap { "Wrap: ON" } else { "Wrap: OFF" };
                if ui
                    .small_button(wrap_label)
                    .on_hover_text("Toggle text wrapping for long lines")
                    .clicked()
                {
                    state.word_wrap = !state.word_wrap;
                    action = Some(PythonDrawerAction::ToggleWrap);
                }

                // Stop or Run primary button
                if state.is_running() {
                    let stop_btn = egui::Button::new(
                        RichText::new("⏹ Stop (Shift+F5)")
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(Color32::from_rgb(224, 108, 117))
                    .corner_radius(4.0);
                    if ui.add(stop_btn).on_hover_text("Kill running Python process").clicked() {
                        action = Some(PythonDrawerAction::Stop);
                    }
                } else {
                    let run_btn = egui::Button::new(
                        RichText::new("▶ Run (F5)")
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(Color32::from_rgb(46, 160, 67)) // GitHub/VSCode green
                    .corner_radius(4.0);
                    if ui.add(run_btn).on_hover_text("Execute Python buffer without saving").clicked() {
                        action = Some(PythonDrawerAction::Run);
                    }
                }
            });
        });

        ui.add_space(2.0);
        ui.separator();

        if state.is_truncated_in_preview {
            let mb = (state.total_stdout_bytes as f64) / (1024.0 * 1024.0);
            let size_str = if mb >= 0.1 {
                format!("{:.1} MB", mb)
            } else {
                format!("{:.1} KB", (state.total_stdout_bytes as f64) / 1024.0)
            };
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.colored_label(
                    Color32::from_rgb(229, 192, 123),
                    RichText::new(format!(
                        "ℹ️ Preview limited to first {} lines (Total: {} lines • {}) — Use '💾 Save As...' or '📄 New Tab' for full output",
                        state.preview_line_limit, state.total_stdout_lines, size_str
                    )).size(11.5).strong(),
                );
            });
            ui.add_space(2.0);
        }

        // --- 2. Console Output Area ---
        let console_bg = Color32::from_rgb(22, 25, 30); // Deep obsidian console
        let console_font = FontId::monospace((base_font_size * 0.92).max(11.0));

        egui::Frame::NONE
            .fill(console_bg)
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                if state.output_lines.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("Ready. Press F5 or click 'Run' to execute Python code directly from memory.")
                                .italics()
                                .size(12.0)
                                .color(Color32::from_rgb(100, 110, 125)),
                        );
                    });
                } else {
                    let scroll_area = egui::ScrollArea::vertical()
                        .id_salt("python_console_scroll")
                        .auto_shrink([false, false])
                        .stick_to_bottom(state.auto_scroll_to_bottom);

                    scroll_area.show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 1.5;

                        for line in &state.output_lines {
                            let text_color = match line.kind {
                                OutputChunkKind::System => Color32::from_rgb(97, 175, 239), // Cyan / soft blue
                                OutputChunkKind::Stdout => Color32::from_rgb(171, 178, 191), // One dark foreground
                                OutputChunkKind::Stderr => Color32::from_rgb(224, 108, 117), // Soft red (Tracebacks)
                            };

                            let mut rich = RichText::new(&line.text)
                                .font(console_font.clone())
                                .color(text_color);

                            if line.kind == OutputChunkKind::System {
                                rich = rich.italics();
                            }

                            if state.word_wrap {
                                ui.label(rich);
                            } else {
                                ui.add(egui::Label::new(rich).wrap_mode(egui::TextWrapMode::Extend));
                            }
                        }
                    });
                }
            });
    });

    action
}
