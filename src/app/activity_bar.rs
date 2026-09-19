use eframe::egui::{self, Color32, Pos2, Rect, Response, Sense, Ui, Vec2};
use super::icons::{paint_icon, render_nav_item, Icon};

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

    if props.is_expanded {
        // Full Rich Navigation Sidebar
        render_expanded_sidebar(ui, props, &mut action);
    } else {
        // Slim 44px Rail with Vector Icons
        render_slim_rail(ui, props, &mut action);
    }

    action
}

fn render_expanded_sidebar(
    ui: &mut Ui,
    props: &ActivityBarProps,
    action: &mut Option<ActivityBarAction>,
) {
    let sidebar_width = 210.0;
    ui.allocate_ui_with_layout(
        Vec2::new(sidebar_width, ui.available_height()),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_width(sidebar_width);
            ui.add_space(10.0);

            // 1. Brand Header (Top Left)
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                // Vector lightning bolt logo
                let (logo_rect, _) = ui.allocate_exact_size(Vec2::new(18.0, 22.0), Sense::hover());
                // Radiant electric blue color
                paint_icon(ui.painter(), logo_rect, Icon::Logo, Color32::from_rgb(0, 144, 255));

                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("UltraViewer")
                            .strong()
                            .size(15.5)
                            .color(Color32::from_rgb(240, 245, 255)),
                    );
                    ui.label(
                        egui::RichText::new("Open Bigger. Explore Faster.")
                            .size(9.5)
                            .color(Color32::from_rgb(110, 120, 138)),
                    );
                });
            });

            ui.add_space(14.0);

            // 2. "+ New File" Button (Bright saturated blue pill matching mockup)
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                let btn_w = sidebar_width - 20.0;
                let btn_h = 32.0;
                let (btn_rect, btn_resp) = ui.allocate_exact_size(Vec2::new(btn_w, btn_h), Sense::click());
                let hovered = btn_resp.hovered();

                let bg_color = if hovered {
                    Color32::from_rgb(16, 130, 228)
                } else {
                    Color32::from_rgb(0, 120, 212) // #0078D4
                };

                ui.painter().rect_filled(btn_rect, 5.0, bg_color);

                // Paint "+" icon and text inside
                let plus_box = Rect::from_min_size(
                    Pos2::new(btn_rect.left() + 28.0, btn_rect.top() + (btn_h - 14.0) / 2.0),
                    Vec2::splat(14.0),
                );
                paint_icon(ui.painter(), plus_box, Icon::Plus, Color32::WHITE);

                ui.painter().text(
                    Pos2::new(btn_rect.left() + 48.0, btn_rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    "New File",
                    egui::FontId::proportional(13.0),
                    Color32::WHITE,
                );

                if btn_resp.on_hover_text("Open or create a new file (Ctrl+O)").clicked() {
                    *action = Some(ActivityBarAction::OpenFile);
                }
            });

            ui.add_space(12.0);

            // 3. Navigation Actions (strictly tools built in codebase)
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.set_width(sidebar_width - 16.0);

                    if render_nav_item(ui, Icon::FolderOpen, "Open File", Some("Ctrl+O"), false, false)
                        .on_hover_text("Open file from disk")
                        .clicked()
                    {
                        *action = Some(ActivityBarAction::OpenFile);
                    }

                    let is_explorer = props.active_panel == ActivityPanel::Explorer;
                    if render_nav_item(ui, Icon::Clock, "Recent Files", None, is_explorer, false)
                        .on_hover_text("View recently opened files")
                        .clicked()
                    {
                        *action = Some(ActivityBarAction::TogglePanel(if is_explorer {
                            ActivityPanel::None
                        } else {
                            ActivityPanel::Explorer
                        }));
                    }

                    ui.add_space(6.0);

                    let is_search = props.active_panel == ActivityPanel::Search;
                    if render_nav_item(ui, Icon::Search, "Search", Some("Ctrl+F"), is_search, false)
                        .on_hover_text("Search across document")
                        .clicked()
                    {
                        *action = Some(ActivityBarAction::TogglePanel(if is_search {
                            ActivityPanel::None
                        } else {
                            ActivityPanel::Search
                        }));
                    }

                    if props.has_file {
                        let is_analyzer = props.active_panel == ActivityPanel::Analyzer;
                        if render_nav_item(ui, Icon::Chart, "Field Analyzer", None, is_analyzer, false)
                            .on_hover_text("Schema profiling and distribution analyzer")
                            .clicked()
                        {
                            *action = Some(ActivityBarAction::TogglePanel(if is_analyzer {
                                ActivityPanel::None
                            } else {
                                ActivityPanel::Analyzer
                            }));
                        }

                        if props.is_xml {
                            let is_structure = props.active_panel == ActivityPanel::Structure;
                            if render_nav_item(ui, Icon::XmlCode, "XML Structure", None, is_structure, false)
                                .on_hover_text("XML Hierarchy Tree")
                                .clicked()
                            {
                                *action = Some(ActivityBarAction::TogglePanel(if is_structure {
                                    ActivityPanel::None
                                } else {
                                    ActivityPanel::Structure
                                }));
                            }
                        }

                        if props.is_json {
                            let is_structure = props.active_panel == ActivityPanel::Structure;
                            if render_nav_item(ui, Icon::JsonBraces, "JSON Structure", None, is_structure, false)
                                .on_hover_text("JSON Hierarchy Tree")
                                .clicked()
                            {
                                *action = Some(ActivityBarAction::TogglePanel(if is_structure {
                                    ActivityPanel::None
                                } else {
                                    ActivityPanel::Structure
                                }));
                            }
                        }
                    }

                    // 4. Tools Group Header (strictly built formatters and validator)
                    if props.has_file && props.is_xml_or_json {
                        ui.add_space(10.0);
                        ui.label(
                            egui::RichText::new("TOOLS")
                                .size(10.0)
                                .strong()
                                .color(Color32::from_rgb(90, 105, 125)),
                        );
                        ui.add_space(4.0);

                        if render_nav_item(ui, Icon::Format, "Format", None, false, false)
                            .on_hover_text("Format and beautify document")
                            .clicked()
                        {
                            *action = Some(ActivityBarAction::FormatBeautify);
                        }

                        if render_nav_item(ui, Icon::Minify, "Minify", None, false, false)
                            .on_hover_text("Minify and compact document")
                            .clicked()
                        {
                            *action = Some(ActivityBarAction::FormatMinify);
                        }

                        if render_nav_item(ui, Icon::Validate, "Validate", None, false, false)
                            .on_hover_text("Validate XML/JSON syntax")
                            .clicked()
                        {
                            *action = Some(ActivityBarAction::Validate);
                        }
                    }
                });
            });

            // 6. Bottom Pinned Section (Settings, Help, Collapse Toggle)
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.set_width(sidebar_width - 16.0);

                        if render_nav_item(ui, Icon::Help, "Help", None, false, false)
                            .on_hover_text("Help, documentation & shortcuts")
                            .clicked()
                        {
                            *action = Some(ActivityBarAction::ToggleHelp);
                        }

                        if render_nav_item(ui, Icon::Gear, "Settings", None, false, false)
                            .on_hover_text("Application preferences & about")
                            .clicked()
                        {
                            *action = Some(ActivityBarAction::ToggleSettings);
                        }
                    });
                });
            });
        },
    );
}

fn render_slim_rail(
    ui: &mut Ui,
    props: &ActivityBarProps,
    action: &mut Option<ActivityBarAction>,
) {
    let bar_width = 44.0;

    ui.allocate_ui_with_layout(
        Vec2::new(bar_width, ui.available_height()),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            ui.set_width(bar_width);
            ui.add_space(10.0);

            // Vector Logo
            let (logo_rect, _) = ui.allocate_exact_size(Vec2::new(20.0, 24.0), Sense::hover());
            paint_icon(ui.painter(), logo_rect, Icon::Logo, Color32::from_rgb(0, 144, 255));
            ui.add_space(14.0);

            // Explorer
            let is_explorer = props.active_panel == ActivityPanel::Explorer;
            if render_rail_icon_button(ui, Icon::Folder, "Explorer & Files (Ctrl+B)", is_explorer, None).clicked() {
                *action = Some(ActivityBarAction::TogglePanel(if is_explorer {
                    ActivityPanel::None
                } else {
                    ActivityPanel::Explorer
                }));
            }
            ui.add_space(4.0);

            // Search
            let is_search = props.active_panel == ActivityPanel::Search;
            let badge = if props.search_match_count > 0 {
                Some(props.search_match_count)
            } else {
                None
            };
            if render_rail_icon_button(ui, Icon::Search, "Search in Document (Ctrl+F)", is_search, badge).clicked() {
                *action = Some(ActivityBarAction::TogglePanel(if is_search {
                    ActivityPanel::None
                } else {
                    ActivityPanel::Search
                }));
            }
            ui.add_space(4.0);

            // Structure Tree
            if props.has_file && props.is_xml_or_json {
                let is_structure = props.active_panel == ActivityPanel::Structure;
                let tree_icon = if props.is_json { Icon::JsonBraces } else { Icon::Tree };
                if render_rail_icon_button(ui, tree_icon, "Document Hierarchy Tree", is_structure, None).clicked() {
                    *action = Some(ActivityBarAction::TogglePanel(if is_structure {
                        ActivityPanel::None
                    } else {
                        ActivityPanel::Structure
                    }));
                }
                ui.add_space(4.0);
            }

            // Field Analyzer
            if props.has_file {
                let is_analyzer = props.active_panel == ActivityPanel::Analyzer;
                if render_rail_icon_button(ui, Icon::Chart, "Schema Profiler & Field Analyzer", is_analyzer, None).clicked() {
                    *action = Some(ActivityBarAction::TogglePanel(if is_analyzer {
                        ActivityPanel::None
                    } else {
                        ActivityPanel::Analyzer
                    }));
                }
                ui.add_space(4.0);
            }

            // Bottom pinned icons
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.add_space(10.0);
                if render_rail_icon_button(ui, Icon::Gear, "Settings & About", false, None).clicked() {
                    *action = Some(ActivityBarAction::ToggleSettings);
                }
                ui.add_space(4.0);
                if render_rail_icon_button(ui, Icon::Help, "Help & Documentation", false, None).clicked() {
                    *action = Some(ActivityBarAction::ToggleHelp);
                }
            });
        },
    );
}

fn render_rail_icon_button(
    ui: &mut Ui,
    icon: Icon,
    tooltip: &str,
    is_active: bool,
    badge_count: Option<usize>,
) -> Response {
    let size = Vec2::new(36.0, 36.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    let hovered = response.hovered();
    let bg_color = if is_active {
        Color32::from_rgb(19, 35, 58) // active navy
    } else if hovered {
        Color32::from_rgb(16, 24, 38)
    } else {
        Color32::TRANSPARENT
    };

    ui.painter().rect_filled(rect, 6.0, bg_color);

    // Active vertical indicator bar on left edge
    if is_active {
        let indicator_rect = Rect::from_min_size(
            Pos2::new(rect.left() - 4.0, rect.top() + 6.0),
            Vec2::new(3.0, rect.height() - 12.0),
        );
        ui.painter().rect_filled(indicator_rect, 1.5, Color32::from_rgb(56, 189, 248));
    }

    let icon_color = if is_active {
        Color32::from_rgb(56, 189, 248)
    } else if hovered {
        Color32::from_rgb(220, 225, 235)
    } else {
        Color32::from_rgb(139, 148, 158)
    };

    let icon_box = Rect::from_center_size(rect.center(), Vec2::splat(18.0));
    paint_icon(ui.painter(), icon_box, icon, icon_color);

    // Badge if any
    if let Some(count) = badge_count {
        let badge_text = if count > 999 { "999+".to_string() } else { count.to_string() };
        let badge_pos = Pos2::new(rect.right() - 8.0, rect.top() + 2.0);
        let badge_rect = Rect::from_center_size(badge_pos, Vec2::new(14.0, 10.0));
        ui.painter().rect_filled(badge_rect, 4.0, Color32::from_rgb(56, 189, 248));
        ui.painter().text(
            badge_rect.center(),
            egui::Align2::CENTER_CENTER,
            badge_text,
            egui::FontId::proportional(8.0),
            Color32::BLACK,
        );
    }

    response.on_hover_text(tooltip)
}
