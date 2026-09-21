use std::path::{Path, PathBuf};
use std::collections::HashSet;
use std::fs;
use eframe::egui::{self, Color32, RichText, Sense, Ui, Vec2};
use super::icons::{paint_icon, Icon};

pub struct FolderExplorerState {
    pub root_path: Option<PathBuf>,
    pub expanded_folders: HashSet<PathBuf>,
    pub search_filter: String,
}

impl Default for FolderExplorerState {
    fn default() -> Self {
        Self {
            root_path: None,
            expanded_folders: HashSet::new(),
            search_filter: String::new(),
        }
    }
}

pub enum FolderAction {
    OpenFile(PathBuf),
    OpenFolderDialog,
}

pub fn render_folder_explorer(
    ui: &mut Ui,
    state: &mut FolderExplorerState,
) -> Option<FolderAction> {
    let mut action = None;

    ui.horizontal(|ui| {
        if ui.button(RichText::new("Open Folder...").size(11.5)).clicked() {
            action = Some(FolderAction::OpenFolderDialog);
        }
    });

    ui.add_space(6.0);

    if let Some(root) = state.root_path.clone() {
        let root_name = root.file_name().and_then(|n| n.to_str()).unwrap_or("Workspace");
        ui.horizontal(|ui| {
            let (f_rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
            paint_icon(ui.painter(), f_rect, Icon::FolderOpen, Color32::from_rgb(229, 192, 123));
            ui.label(RichText::new(root_name).strong().size(12.0).color(Color32::from_rgb(240, 246, 252)));
        });

        ui.add_space(4.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                render_dir_tree(ui, &root, state, &mut action, 0);
            });
    } else {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.label(RichText::new("No folder opened").weak().size(11.5));
            ui.add_space(8.0);
            if ui.button("Choose Folder").clicked() {
                action = Some(FolderAction::OpenFolderDialog);
            }
        });
    }

    action
}

fn render_dir_tree(
    ui: &mut Ui,
    dir: &Path,
    state: &mut FolderExplorerState,
    action: &mut Option<FolderAction>,
    depth: usize,
) {
    if depth > 10 {
        return; // Prevent excessive recursion
    }

    let read_res = fs::read_dir(dir);
    let Ok(entries) = read_res else {
        return;
    };

    let mut dirs = Vec::new();
    let mut files = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue; // Skip hidden
        }
        if path.is_dir() {
            dirs.push(path);
        } else {
            files.push(path);
        }
    }

    dirs.sort();
    files.sort();

    // Render Subdirectories first
    for d in dirs {
        let name = d.file_name().and_then(|n| n.to_str()).unwrap_or("dir");
        let is_expanded = state.expanded_folders.contains(&d);
        let indent = depth as f32 * 12.0;

        ui.horizontal(|ui| {
            ui.add_space(indent);
            let (c_rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
            let chev_icon = if is_expanded { Icon::ChevronDown } else { Icon::ChevronRight };
            paint_icon(ui.painter(), c_rect, chev_icon, Color32::from_rgb(130, 140, 155));

            let (f_rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
            let f_icon = if is_expanded { Icon::FolderOpen } else { Icon::Folder };
            paint_icon(ui.painter(), f_rect, f_icon, Color32::from_rgb(229, 192, 123));

            let label = ui.selectable_label(false, RichText::new(name).size(11.5));
            if label.clicked() {
                if is_expanded {
                    state.expanded_folders.remove(&d);
                } else {
                    state.expanded_folders.insert(d.clone());
                }
            }
        });

        if is_expanded {
            render_dir_tree(ui, &d, state, action, depth + 1);
        }
    }

    // Render Files
    for f in files {
        let name = f.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let indent = (depth + 1) as f32 * 12.0;

        let ext = f.extension().and_then(|s| s.to_str()).map(|s| s.to_lowercase());
        let (icon, color) = match ext.as_deref() {
            Some("xml") => (Icon::XmlCode, Color32::from_rgb(224, 108, 117)),
            Some("json") => (Icon::JsonBraces, Color32::from_rgb(229, 192, 123)),
            Some("csv" | "tsv") => (Icon::File, Color32::from_rgb(152, 195, 121)),
            _ => (Icon::File, Color32::from_rgb(148, 163, 184)),
        };

        ui.horizontal(|ui| {
            ui.add_space(indent);
            let (f_rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
            paint_icon(ui.painter(), f_rect, icon, color);

            let label = ui.selectable_label(false, RichText::new(name).size(11.5));
            if label.double_clicked() || label.clicked() {
                *action = Some(FolderAction::OpenFile(f.clone()));
            }
        });
    }
}
