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
            .with_drag_and_drop(true),
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
            let mut app = UltraViewerApp::new(cc);
            if let Some(file_path) = initial_file {
                app.open_file(file_path);
            }
            Ok(Box::new(app))
        }),
    )
}
