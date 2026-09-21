use eframe::egui::{self, Color32, Key, RichText, Sense, Stroke, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteAction {
    OpenFile,
    CloseActiveTab,
    CloseAllTabs,
    SaveFile,
    SaveFileAs,
    Find,
    GoToLine,
    ToggleEditMode,
    ToggleWrap,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    SetThemeOneDark,
    SetThemeGitHubDark,
    SetThemeMonokai,
    SetThemeTokyoNight,
    SetThemeLightModern,
    XmlValidate,
    XmlToggleTree,
    JsonValidate,
    JsonToggleTree,
    FormatBeautify2,
    FormatBeautify4,
    FormatMinify,
    AnalyzeFields,
    OpenFolder,
    RegisterContextMenu,
    UnregisterContextMenu,
    About,
}

#[derive(Debug, Clone)]
pub struct CommandItem {
    pub category: &'static str,
    pub title: &'static str,
    pub shortcut: &'static str,
    pub action: PaletteAction,
}

impl CommandItem {
    pub fn all() -> Vec<CommandItem> {
        vec![
            CommandItem { category: "File", title: "Open File...", shortcut: "Ctrl+O", action: PaletteAction::OpenFile },
            CommandItem { category: "File", title: "Open Folder / Workspace...", shortcut: "", action: PaletteAction::OpenFolder },
            CommandItem { category: "File", title: "Save", shortcut: "Ctrl+S", action: PaletteAction::SaveFile },
            CommandItem { category: "File", title: "Save As...", shortcut: "Ctrl+Shift+S", action: PaletteAction::SaveFileAs },
            CommandItem { category: "File", title: "Close Tab", shortcut: "Ctrl+W", action: PaletteAction::CloseActiveTab },
            CommandItem { category: "File", title: "Close All Tabs", shortcut: "", action: PaletteAction::CloseAllTabs },
            CommandItem { category: "Edit", title: "Find in File...", shortcut: "Ctrl+F", action: PaletteAction::Find },
            CommandItem { category: "Edit", title: "Go to Line...", shortcut: "Ctrl+G", action: PaletteAction::GoToLine },
            CommandItem { category: "Edit", title: "Toggle In-Place Edit Mode", shortcut: "Ctrl+E", action: PaletteAction::ToggleEditMode },
            CommandItem { category: "View", title: "Toggle Word Wrap", shortcut: "Alt+Z", action: PaletteAction::ToggleWrap },
            CommandItem { category: "View", title: "Zoom In", shortcut: "Ctrl +", action: PaletteAction::ZoomIn },
            CommandItem { category: "View", title: "Zoom Out", shortcut: "Ctrl -", action: PaletteAction::ZoomOut },
            CommandItem { category: "View", title: "Reset Zoom", shortcut: "Ctrl 0", action: PaletteAction::ZoomReset },
            CommandItem { category: "Preferences", title: "Color Theme: One Dark Pro Darker", shortcut: "", action: PaletteAction::SetThemeOneDark },
            CommandItem { category: "Preferences", title: "Color Theme: GitHub Dark", shortcut: "", action: PaletteAction::SetThemeGitHubDark },
            CommandItem { category: "Preferences", title: "Color Theme: Monokai Pro", shortcut: "", action: PaletteAction::SetThemeMonokai },
            CommandItem { category: "Preferences", title: "Color Theme: Tokyo Night", shortcut: "", action: PaletteAction::SetThemeTokyoNight },
            CommandItem { category: "Preferences", title: "Color Theme: VS Code Light Modern", shortcut: "", action: PaletteAction::SetThemeLightModern },
            CommandItem { category: "XML", title: "Validate XML Document", shortcut: "", action: PaletteAction::XmlValidate },
            CommandItem { category: "XML", title: "Toggle XML Structure Tree", shortcut: "Ctrl+Shift+T", action: PaletteAction::XmlToggleTree },
            CommandItem { category: "JSON", title: "Validate JSON Document", shortcut: "", action: PaletteAction::JsonValidate },
            CommandItem { category: "JSON", title: "Toggle JSON Structure Tree", shortcut: "Ctrl+Shift+T", action: PaletteAction::JsonToggleTree },
            CommandItem { category: "Format", title: "Beautify Document (2 Spaces)", shortcut: "Ctrl+Shift+B", action: PaletteAction::FormatBeautify2 },
            CommandItem { category: "Format", title: "Beautify Document (4 Spaces)", shortcut: "", action: PaletteAction::FormatBeautify4 },
            CommandItem { category: "Format", title: "Minify Document (Compact)", shortcut: "Ctrl+Shift+M", action: PaletteAction::FormatMinify },
            CommandItem { category: "Tools", title: "Field Analyzer & Schema Profiler", shortcut: "Ctrl+Shift+A", action: PaletteAction::AnalyzeFields },
            CommandItem { category: "Tools", title: "Register 'Open with UltraViewer' Windows Context Menu", shortcut: "", action: PaletteAction::RegisterContextMenu },
            CommandItem { category: "Tools", title: "Deregister / Remove 'Open with UltraViewer' Windows Context Menu", shortcut: "", action: PaletteAction::UnregisterContextMenu },
            CommandItem { category: "Help", title: "About UltraViewer", shortcut: "F1", action: PaletteAction::About },
        ]
    }
}

pub struct CommandPaletteState {
    pub is_open: bool,
    pub query: String,
    pub selected_idx: usize,
}

impl Default for CommandPaletteState {
    fn default() -> Self {
        Self {
            is_open: false,
            query: String::new(),
            selected_idx: 0,
        }
    }
}

pub fn render_command_palette(
    ctx: &egui::Context,
    state: &mut CommandPaletteState,
) -> Option<PaletteAction> {
    if !state.is_open {
        return None;
    }

    let mut executed_action = None;
    let all_commands = CommandItem::all();
    let q = state.query.to_lowercase();

    let filtered: Vec<&CommandItem> = all_commands
        .iter()
        .filter(|cmd| {
            if q.is_empty() {
                return true;
            }
            let full = format!("{}: {}", cmd.category, cmd.title).to_lowercase();
            q.split_whitespace().all(|word| full.contains(word))
        })
        .collect();

    if state.selected_idx >= filtered.len() {
        state.selected_idx = 0;
    }

    // Keyboard navigation
    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        state.is_open = false;
        return None;
    }
    if ctx.input(|i| i.key_pressed(Key::ArrowDown)) {
        if !filtered.is_empty() {
            state.selected_idx = (state.selected_idx + 1) % filtered.len();
        }
    }
    if ctx.input(|i| i.key_pressed(Key::ArrowUp)) {
        if !filtered.is_empty() {
            state.selected_idx = if state.selected_idx == 0 {
                filtered.len() - 1
            } else {
                state.selected_idx - 1
            };
        }
    }
    if ctx.input(|i| i.key_pressed(Key::Enter)) {
        if let Some(cmd) = filtered.get(state.selected_idx) {
            executed_action = Some(cmd.action);
            state.is_open = false;
            return executed_action;
        }
    }

    // Floating palette window centered at the top
    let screen_w = ctx.screen_rect().width();
    let palette_w = 560.0_f32.min(screen_w - 40.0);

    egui::Window::new("CommandPaletteModal")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .movable(false)
        .anchor(egui::Align2::CENTER_TOP, Vec2::new(0.0, 36.0))
        .fixed_size(Vec2::new(palette_w, 340.0))
        .frame(
            egui::Frame::NONE
                .fill(Color32::from_rgb(26, 30, 35))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0, 122, 204))) // Active VS Code blue border
                .inner_margin(egui::Margin::same(8)),
        )
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;

            // Search input field
            ui.horizontal(|ui| {
                ui.label(RichText::new(">").size(14.0).strong().color(Color32::from_rgb(0, 122, 204)));
                let text_edit = egui::TextEdit::singleline(&mut state.query)
                    .hint_text(RichText::new("Type a command or search...").color(Color32::from_rgb(110, 125, 145)))
                    .desired_width(f32::INFINITY)
                    .lock_focus(true);
                let resp = ui.add(text_edit);
                resp.request_focus();
            });

            ui.separator();

            // Command items list
            egui::ScrollArea::vertical()
                .max_height(280.0)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;

                    if filtered.is_empty() {
                        ui.add_space(10.0);
                        ui.label(RichText::new("No matching commands found").weak().italics().size(12.0));
                    } else {
                        for (idx, cmd) in filtered.iter().enumerate() {
                            let is_selected = idx == state.selected_idx;
                            let bg_color = if is_selected {
                                Color32::from_rgb(4, 57, 94) // VS Code selection blue
                            } else {
                                Color32::TRANSPARENT
                            };

                            let (item_rect, item_resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::click());
                            if item_resp.hovered() && !is_selected {
                                ui.painter().rect_filled(item_rect, 3.0, Color32::from_rgb(36, 42, 50));
                            } else if is_selected {
                                ui.painter().rect_filled(item_rect, 3.0, bg_color);
                            }

                            // Command Category & Title
                            let label_text = format!("{}: {}", cmd.category, cmd.title);
                            let text_color = if is_selected {
                                Color32::WHITE
                            } else {
                                Color32::from_rgb(210, 220, 235)
                            };
                            ui.painter().text(
                                egui::Pos2::new(item_rect.left() + 8.0, item_rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                label_text,
                                egui::FontId::proportional(12.0),
                                text_color,
                            );

                            // Shortcut label on right
                            if !cmd.shortcut.is_empty() {
                                ui.painter().text(
                                    egui::Pos2::new(item_rect.right() - 8.0, item_rect.center().y),
                                    egui::Align2::RIGHT_CENTER,
                                    cmd.shortcut,
                                    egui::FontId::proportional(11.0),
                                    Color32::from_rgb(130, 145, 165),
                                );
                            }

                            if item_resp.clicked() {
                                executed_action = Some(cmd.action);
                                state.is_open = false;
                            }
                        }
                    }
                });
        });

    executed_action
}
