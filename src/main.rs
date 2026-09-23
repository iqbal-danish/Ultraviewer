#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use eframe::egui;
use ultraviewer::UltraViewerApp;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("UltraViewer")
            .with_inner_size([1200.0, 800.0])
            .with_decorations(false)
            .with_resizable(true)
            .with_drag_and_drop(true)
            .with_maximized(true),
        ..Default::default()
    };

    let args: Vec<String> = std::env::args().collect();
    let initial_file = if args.len() > 1 {
        let path = PathBuf::from(&args[1]);
        if path.exists() {
            Some(path)
        } else {
            None
        }
    } else {
        None
    };

    eframe::run_native(
        "UltraViewer",
        native_options,
        Box::new(move |cc| {
            // Configure native Windows fonts for VS Code look & feel
            let mut fonts = egui::FontDefinitions::default();
            if let Ok(consola_bytes) = std::fs::read(r"C:\Windows\Fonts\consola.ttf") {
                fonts.font_data.insert(
                    "Consolas".to_owned(),
                    std::sync::Arc::new(egui::FontData::from_owned(consola_bytes)),
                );
                if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                    family.insert(0, "Consolas".to_owned());
                }
            }
            if let Ok(segoe_bytes) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
                fonts.font_data.insert(
                    "SegoeUI".to_owned(),
                    std::sync::Arc::new(egui::FontData::from_owned(segoe_bytes)),
                );
                if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                    family.insert(0, "SegoeUI".to_owned());
                }
            }
            cc.egui_ctx.set_fonts(fonts);

            // Configure comfortable text styles matching modern editors
            let mut style = (*cc.egui_ctx.style()).clone();
            style.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
            style.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(15.0));
            style.text_styles.insert(egui::TextStyle::Heading, egui::FontId::proportional(20.0));
            style.text_styles.insert(egui::TextStyle::Monospace, egui::FontId::monospace(16.0));
            style.text_styles.insert(egui::TextStyle::Small, egui::FontId::proportional(13.0));
            cc.egui_ctx.set_style(style);

            let mut app = UltraViewerApp::new(cc);
            if let Some(file_path) = initial_file {
                app.open_file(file_path);
            }
            Ok(Box::new(app))
        }),
    )
}
