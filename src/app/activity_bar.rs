use eframe::egui::{self, Color32, Pos2, Rect, Response, Sense, Ui, Vec2};
use super::icons::{paint_icon, Icon};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityPanel {
    None,
    Explorer,
    Search,
    Structure,
    Analyzer,
}

pub struct ActivityBarProps {
    pub active_panel: ActivityPanel,
    pub has_file: bool,
    pub is_xml_or_json: bool,
    pub is_xml: bool,
    pub is_json: bool,
    pub search_match_count: usize,
    pub is_expanded: bool,
    pub is_search_open: bool,
}

pub enum ActivityBarAction {
    TogglePanel(ActivityPanel),
    OpenFile,
    FormatBeautify,
    FormatMinify,
    Validate,
    ToggleSettings,
    ToggleHelp,
    ToggleExpanded,
}

pub fn render_activity_bar(
    ui: &mut Ui,
    props: &ActivityBarProps,
) -> Option<ActivityBarAction> {
    let mut action = None;
    let bar_width = 48.0;

    ui.allocate_ui_with_layout(
        Vec2::new(bar_width, ui.available_height()),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            ui.set_width(bar_width);
            ui.add_space(8.0);

            // 1. Explorer (Ctrl+Shift+E / Ctrl+B)
            let is_explorer = props.active_panel == ActivityPanel::Explorer;
            if render_rail_icon_button(ui, Icon::Explorer, "Explorer", "Ctrl+Shift+E", is_explorer, None).clicked() {
                action = Some(ActivityBarAction::TogglePanel(if is_explorer {
                    ActivityPanel::None
                } else {
                    ActivityPanel::Explorer
                }));
            }
            ui.add_space(4.0);

            // 2. Search (Ctrl+Shift+F / Ctrl+F)
            let is_search = props.active_panel == ActivityPanel::Search || props.is_search_open;
            let badge = if props.search_match_count > 0 {
                Some(props.search_match_count)
            } else {
                None
            };
            if render_rail_icon_button(ui, Icon::Search, "Search", "Ctrl+Shift+F", is_search, badge).clicked() {
                action = Some(ActivityBarAction::TogglePanel(if is_search {
                    ActivityPanel::None
                } else {
                    ActivityPanel::Search
                }));
            }
            ui.add_space(4.0);

            // 3. Document Hierarchy Tree (Ctrl+Shift+T)
            if props.has_file && props.is_xml_or_json {
                let is_structure = props.active_panel == ActivityPanel::Structure;
                let tree_icon = if props.is_json { Icon::JsonBraces } else { Icon::Tree };
                let tree_title = if props.is_json { "JSON Tree" } else { "XML Tree" };
                if render_rail_icon_button(ui, tree_icon, tree_title, "Ctrl+Shift+T", is_structure, None).clicked() {
                    action = Some(ActivityBarAction::TogglePanel(if is_structure {
                        ActivityPanel::None
                    } else {
                        ActivityPanel::Structure
                    }));
                }
                ui.add_space(4.0);
            }

            // 4. Schema Profiler & Field Analyzer (Ctrl+Shift+A)
            if props.has_file {
                let is_analyzer = props.active_panel == ActivityPanel::Analyzer;
                if render_rail_icon_button(ui, Icon::Chart, "Field Analyzer", "Ctrl+Shift+A", is_analyzer, None).clicked() {
                    action = Some(ActivityBarAction::TogglePanel(if is_analyzer {
                        ActivityPanel::None
                    } else {
                        ActivityPanel::Analyzer
                    }));
                }
                ui.add_space(4.0);
            }

            // 5. Format & Beautify Document (Ctrl+Shift+B)
            if props.has_file && props.is_xml_or_json {
                if render_rail_icon_button(ui, Icon::Format, "Format Document", "Ctrl+Shift+B", false, None).clicked() {
                    action = Some(ActivityBarAction::FormatBeautify);
                }
                ui.add_space(4.0);
            }

            // 6. Validate Document (Ctrl+Shift+V)
            if props.has_file && props.is_xml_or_json {
                let val_title = if props.is_json { "Validate JSON" } else { "Validate XML" };
                if render_rail_icon_button(ui, Icon::Validate, val_title, "Ctrl+Shift+V", false, None).clicked() {
                    action = Some(ActivityBarAction::Validate);
                }
                ui.add_space(4.0);
            }

            // 7. Bottom Pinned Section (Settings, Help)
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                if render_rail_icon_button(ui, Icon::Gear, "Settings", "Ctrl+,", false, None).clicked() {
                    action = Some(ActivityBarAction::ToggleSettings);
                }
                ui.add_space(4.0);
                if render_rail_icon_button(ui, Icon::Help, "Help & Shortcuts", "F1", false, None).clicked() {
                    action = Some(ActivityBarAction::ToggleHelp);
                }
            });
        },
    );

    action
}

fn render_rail_icon_button(
    ui: &mut Ui,
    icon: Icon,
    name: &str,
    shortcut: &str,
    is_active: bool,
    badge_count: Option<usize>,
) -> Response {
    let size = Vec2::new(40.0, 40.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    let hovered = response.hovered();
    let bg_color = if hovered {
        Color32::from_rgb(44, 49, 58) // #2C313A subtle hover
    } else {
        Color32::TRANSPARENT
    };

    ui.painter().rect_filled(rect, 4.0, bg_color);

    // Active vertical indicator bar flush on the very left edge (VS Code style!)
    if is_active {
        let bar_left = ui.max_rect().left();
        let indicator_rect = Rect::from_min_size(
            Pos2::new(bar_left, rect.top() + 6.0),
            Vec2::new(2.5, rect.height() - 12.0),
        );
        ui.painter().rect_filled(indicator_rect, 1.0, Color32::WHITE);
    }

    let icon_color = if is_active {
        Color32::WHITE // Pure white active icon
    } else if hovered {
        Color32::from_rgb(215, 218, 224) // Bright silver
    } else {
        Color32::from_rgb(133, 133, 133) // #858585 VS Code muted icon gray
    };

    let icon_box = Rect::from_center_size(rect.center(), Vec2::splat(20.0));
    paint_icon(ui.painter(), icon_box, icon, icon_color);

    // Badge if any (e.g. search matches count)
    if let Some(count) = badge_count {
        let badge_text = if count > 999 { "999+".to_string() } else { count.to_string() };
        let badge_pos = Pos2::new(rect.right() - 4.0, rect.top() + 4.0);
        let badge_rect = Rect::from_center_size(badge_pos, Vec2::new(16.0, 11.0));
        ui.painter().rect_filled(badge_rect, 5.5, Color32::from_rgb(77, 120, 204));
        ui.painter().text(
            badge_rect.center(),
            egui::Align2::CENTER_CENTER,
            badge_text,
            egui::FontId::proportional(8.5),
            Color32::WHITE,
        );
    }

    // Hover tooltip with action name and keyboard shortcut
    response.on_hover_ui(|ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(name)
                    .strong()
                    .size(12.0)
                    .color(Color32::from_rgb(235, 240, 248)),
            );
            if !shortcut.is_empty() {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(shortcut)
                        .size(11.0)
                        .color(Color32::from_rgb(140, 150, 165)),
                );
            }
        });
    })
}
