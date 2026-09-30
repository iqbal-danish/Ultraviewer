use eframe::egui::{self, Color32, FontId, Layout, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};
use crate::file_engine::FileEngine;
use super::icons::{paint_icon, Icon};
use super::status_bar::format_bytes;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HexColorTheme {
    ModernDark,
    CyberGreen,
    NordicFrost,
    MonokaiWarm,
}

impl Default for HexColorTheme {
    fn default() -> Self {
        HexColorTheme::ModernDark
    }
}

impl HexColorTheme {
    pub fn name(&self) -> &'static str {
        match self {
            HexColorTheme::ModernDark => "Modern Dark",
            HexColorTheme::CyberGreen => "Cyber Green",
            HexColorTheme::NordicFrost => "Nordic Frost",
            HexColorTheme::MonokaiWarm => "Monokai Warm",
        }
    }

    pub fn offset_color(&self, dark_mode: bool) -> Color32 {
        if !dark_mode {
            return Color32::from_rgb(90, 100, 115);
        }
        match self {
            HexColorTheme::ModernDark => Color32::from_rgb(97, 175, 239),
            HexColorTheme::CyberGreen => Color32::from_rgb(73, 208, 115),
            HexColorTheme::NordicFrost => Color32::from_rgb(136, 192, 208),
            HexColorTheme::MonokaiWarm => Color32::from_rgb(229, 192, 123),
        }
    }

    pub fn ascii_color(&self, dark_mode: bool) -> Color32 {
        if !dark_mode {
            return Color32::from_rgb(36, 41, 47);
        }
        match self {
            HexColorTheme::ModernDark => Color32::from_rgb(171, 178, 191),
            HexColorTheme::CyberGreen => Color32::from_rgb(152, 222, 150),
            HexColorTheme::NordicFrost => Color32::from_rgb(216, 222, 233),
            HexColorTheme::MonokaiWarm => Color32::from_rgb(248, 248, 242),
        }
    }

    pub fn null_color(&self, dark_mode: bool) -> Color32 {
        if !dark_mode {
            return Color32::from_rgb(180, 185, 195);
        }
        match self {
            HexColorTheme::ModernDark => Color32::from_rgb(75, 82, 99),
            HexColorTheme::CyberGreen => Color32::from_rgb(45, 80, 55),
            HexColorTheme::NordicFrost => Color32::from_rgb(76, 86, 106),
            HexColorTheme::MonokaiWarm => Color32::from_rgb(115, 113, 94),
        }
    }

    pub fn control_color(&self, _dark_mode: bool) -> Color32 {
        match self {
            HexColorTheme::ModernDark => Color32::from_rgb(229, 192, 123),
            HexColorTheme::CyberGreen => Color32::from_rgb(255, 215, 0),
            HexColorTheme::NordicFrost => Color32::from_rgb(235, 203, 139),
            HexColorTheme::MonokaiWarm => Color32::from_rgb(253, 151, 31),
        }
    }

    pub fn high_byte_color(&self, _dark_mode: bool) -> Color32 {
        match self {
            HexColorTheme::ModernDark => Color32::from_rgb(198, 120, 221),
            HexColorTheme::CyberGreen => Color32::from_rgb(86, 182, 194),
            HexColorTheme::NordicFrost => Color32::from_rgb(180, 142, 173),
            HexColorTheme::MonokaiWarm => Color32::from_rgb(249, 38, 114),
        }
    }

    pub fn stripe_color(&self, dark_mode: bool) -> Color32 {
        if !dark_mode {
            return Color32::from_rgba_unmultiplied(0, 0, 0, 6);
        }
        match self {
            HexColorTheme::ModernDark => Color32::from_rgba_unmultiplied(255, 255, 255, 4),
            HexColorTheme::CyberGreen => Color32::from_rgba_unmultiplied(73, 208, 115, 6),
            HexColorTheme::NordicFrost => Color32::from_rgba_unmultiplied(136, 192, 208, 6),
            HexColorTheme::MonokaiWarm => Color32::from_rgba_unmultiplied(229, 192, 123, 6),
        }
    }

    pub fn selection_bg(&self) -> Color32 {
        match self {
            HexColorTheme::ModernDark => Color32::from_rgb(38, 79, 120),
            HexColorTheme::CyberGreen => Color32::from_rgb(25, 75, 40),
            HexColorTheme::NordicFrost => Color32::from_rgb(67, 76, 94),
            HexColorTheme::MonokaiWarm => Color32::from_rgb(73, 72, 62),
        }
    }

    pub fn selection_stroke(&self) -> Color32 {
        match self {
            HexColorTheme::ModernDark => Color32::from_rgb(97, 175, 239),
            HexColorTheme::CyberGreen => Color32::from_rgb(73, 208, 115),
            HexColorTheme::NordicFrost => Color32::from_rgb(136, 192, 208),
            HexColorTheme::MonokaiWarm => Color32::from_rgb(229, 192, 123),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HexViewerState {
    pub is_enabled: bool,
    pub bytes_per_row: usize, // 8, 16 (default), or 32
    pub current_offset: u64,  // Top-row byte offset
    pub selected_offset: Option<u64>,
    pub hovered_offset: Option<u64>,
    pub jump_input: String,
    pub is_big_endian: bool,
    pub auto_focus_jump: bool,
    pub theme: HexColorTheme,
}

impl Default for HexViewerState {
    fn default() -> Self {
        Self {
            is_enabled: false,
            bytes_per_row: 16,
            current_offset: 0,
            selected_offset: Some(0),
            hovered_offset: None,
            jump_input: String::new(),
            is_big_endian: false,
            auto_focus_jump: false,
            theme: HexColorTheme::default(),
        }
    }
}

pub enum HexViewerAction {
    None,
    Close,
    JumpToOffset(u64),
    SelectOffset(u64),
    CopyText(String),
}

pub fn parse_offset(s: &str) -> Option<u64> {
    let clean = s.trim().replace(',', "").replace('_', "");
    if clean.is_empty() {
        return None;
    }
    if let Some(hex_part) = clean.strip_prefix("0x").or_else(|| clean.strip_prefix("0X")) {
        u64::from_str_radix(hex_part, 16).ok()
    } else if let Some(hex_part) = clean.strip_suffix('h').or_else(|| clean.strip_suffix('H')) {
        u64::from_str_radix(hex_part, 16).ok()
    } else if clean.chars().any(|c| matches!(c, 'a'..='f' | 'A'..='F')) {
        u64::from_str_radix(&clean, 16).ok()
    } else {
        clean.parse::<u64>().ok().or_else(|| u64::from_str_radix(&clean, 16).ok())
    }
}

pub fn render_hex_viewer(
    ui: &mut Ui,
    engine: &FileEngine,
    state: &mut HexViewerState,
    font_size: f32,
    dark_mode: bool,
) -> HexViewerAction {
    let mut action = HexViewerAction::None;
    let file_size = engine.size();
    let bytes_per_row = state.bytes_per_row.clamp(8, 32);

    let total_rows = if file_size == 0 {
        1
    } else {
        (file_size + bytes_per_row as u64 - 1) / bytes_per_row as u64
    };

    let available_rect = ui.available_rect_before_wrap();
    let total_w = available_rect.width();
    let total_h = available_rect.height();

    // 1. Top Toolbar (Height ~36.0)
    let toolbar_h = 36.0_f32;
    let toolbar_rect = Rect::from_min_size(available_rect.min, Vec2::new(total_w, toolbar_h));

    ui.allocate_rect(toolbar_rect, Sense::hover());
    let mut tb_ui = ui.new_child(egui::UiBuilder::new().max_rect(toolbar_rect));
    tb_ui.painter().rect_filled(
        toolbar_rect,
        0.0,
        if dark_mode { Color32::from_rgb(22, 27, 34) } else { Color32::from_rgb(240, 243, 246) },
    );
    tb_ui.painter().line_segment(
        [
            Pos2::new(toolbar_rect.left(), toolbar_rect.bottom()),
            Pos2::new(toolbar_rect.right(), toolbar_rect.bottom()),
        ],
        Stroke::new(1.0_f32, if dark_mode { Color32::from_rgb(48, 54, 61) } else { Color32::from_rgb(209, 217, 224) }),
    );

    tb_ui.horizontal_centered(|ui| {
        ui.add_space(10.0);

        // Icon + Title
        let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
        paint_icon(ui.painter(), icon_rect, Icon::Binary, Color32::from_rgb(97, 175, 239));
        ui.add_space(4.0);
        ui.label(RichText::new("HEX & BINARY INSPECTOR").strong().size(12.0).color(Color32::from_rgb(97, 175, 239)));

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        // File Size Badge
        let size_str = format_bytes(file_size);
        ui.label(RichText::new(format!("Size: {}", size_str)).size(11.5).color(Color32::from_rgb(140, 150, 165)));

        ui.add_space(8.0);
        // Current Cursor Offset Display
        let sel_off = state.selected_offset.unwrap_or(0).min(file_size.saturating_sub(1));
        let cursor_label = format!("Offset: 0x{:08X} ({})", sel_off, sel_off);
        ui.label(RichText::new(cursor_label).strong().size(11.5).color(Color32::from_rgb(152, 195, 121)));

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(12.0);

        // Jump to Offset input
        ui.label(RichText::new("Go to:").size(11.5).color(Color32::from_rgb(170, 180, 195)));
        let jump_edit = ui.add(
            egui::TextEdit::singleline(&mut state.jump_input)
                .desired_width(90.0)
                .hint_text("0x... / Dec")
                .font(FontId::monospace(11.5)),
        );
        if state.auto_focus_jump {
            jump_edit.request_focus();
            state.auto_focus_jump = false;
        }

        let mut do_jump = false;
        if jump_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            do_jump = true;
        }
        if ui.button(RichText::new("Jump").size(11.0)).clicked() {
            do_jump = true;
        }

        if do_jump {
            if let Some(target) = parse_offset(&state.jump_input) {
                let clamped = target.min(file_size.saturating_sub(1));
                state.selected_offset = Some(clamped);
                action = HexViewerAction::JumpToOffset(clamped);
            }
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        // Bytes Per Row Selector: 8 | 16 | 32
        ui.label(RichText::new("Cols:").size(11.5).color(Color32::from_rgb(170, 180, 195)));
        for &cols in &[8, 16, 32] {
            let is_sel = state.bytes_per_row == cols;
            let resp = ui.selectable_label(is_sel, RichText::new(format!("{}", cols)).size(11.0));
            if resp.clicked() {
                state.bytes_per_row = cols;
            }
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        // Endianness Toggle: LE / BE
        let endian_label = if state.is_big_endian { "Big Endian (BE)" } else { "Little Endian (LE)" };
        if ui.button(RichText::new(endian_label).size(11.0).color(Color32::from_rgb(229, 192, 123))).clicked() {
            state.is_big_endian = !state.is_big_endian;
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        // Theme selector
        ui.label(RichText::new("Theme:").size(11.5).color(Color32::from_rgb(170, 180, 195)));
        ui.menu_button(RichText::new(format!("{} ▾", state.theme.name())).size(11.0), |ui| {
            for t in [
                HexColorTheme::ModernDark,
                HexColorTheme::CyberGreen,
                HexColorTheme::NordicFrost,
                HexColorTheme::MonokaiWarm,
            ] {
                let is_sel = state.theme == t;
                if ui.selectable_label(is_sel, t.name()).clicked() {
                    state.theme = t;
                    ui.close_menu();
                }
            }
        });

        // Right side: Close / Exit Hex View button
        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(10.0);
            let btn_text = "Close Hex View (Ctrl+Shift+H)";
            let btn_font = FontId::proportional(11.5);
            let text_w = ui.fonts(|f| f.layout_no_wrap(btn_text.to_string(), btn_font.clone(), Color32::WHITE).size().x);
            let btn_w = text_w + 32.0;
            let (btn_rect, btn_resp) = ui.allocate_exact_size(Vec2::new(btn_w, 24.0), Sense::click());
            let is_hovered = btn_resp.hovered();
            let bg_color = if is_hovered {
                Color32::from_rgba_unmultiplied(224, 108, 117, 45)
            } else {
                Color32::from_rgba_unmultiplied(224, 108, 117, 20)
            };
            let stroke_color = if is_hovered {
                Color32::from_rgb(224, 108, 117)
            } else {
                Color32::from_rgba_unmultiplied(224, 108, 117, 120)
            };
            ui.painter().rect(btn_rect, 4.0, bg_color, Stroke::new(1.0_f32, stroke_color), egui::StrokeKind::Inside);

            // Vector Close Icon (never missing glyph)
            let icon_rect = Rect::from_center_size(Pos2::new(btn_rect.left() + 13.0, btn_rect.center().y), Vec2::splat(10.0));
            paint_icon(ui.painter(), icon_rect, Icon::Close, Color32::from_rgb(224, 108, 117));

            // Button text
            ui.painter().text(
                Pos2::new(btn_rect.left() + 24.0, btn_rect.center().y),
                egui::Align2::LEFT_CENTER,
                btn_text,
                btn_font,
                if is_hovered { Color32::WHITE } else { Color32::from_rgb(224, 108, 117) },
            );
            if btn_resp.clicked() {
                action = HexViewerAction::Close;
            }
        });
    });

    // 2. Main Content Split Area
    let content_origin = Pos2::new(available_rect.min.x, available_rect.min.y + toolbar_h);
    let content_h = (total_h - toolbar_h).max(10.0);
    let _content_rect = Rect::from_min_size(content_origin, Vec2::new(total_w, content_h));

    // Right Inspector Panel width: 280.0
    let inspector_w = 280.0_f32;
    let grid_w = (total_w - inspector_w - 1.0).max(100.0);

    let grid_rect = Rect::from_min_size(content_origin, Vec2::new(grid_w, content_h));
    let divider_rect = Rect::from_min_size(Pos2::new(content_origin.x + grid_w, content_origin.y), Vec2::new(1.0, content_h));
    let inspector_rect = Rect::from_min_size(Pos2::new(content_origin.x + grid_w + 1.0, content_origin.y), Vec2::new(inspector_w, content_h));

    // Render Divider Line
    ui.painter().rect_filled(
        divider_rect,
        0.0,
        if dark_mode { Color32::from_rgb(48, 54, 61) } else { Color32::from_rgb(209, 217, 224) },
    );

    // 3. Render Hex & ASCII Grid
    {
        let mut grid_ui = ui.new_child(egui::UiBuilder::new().max_rect(grid_rect));
        grid_ui.set_clip_rect(grid_rect);

        let row_h = (font_size * 1.5).max(20.0);
        let visible_rows = ((content_h / row_h).floor() as usize).max(1);
        let max_top_row = total_rows.saturating_sub(visible_rows as u64);

        // Handle Jump Action
        if let HexViewerAction::JumpToOffset(target) = action {
            let target_row = target / bytes_per_row as u64;
            let centered_top = target_row.saturating_sub((visible_rows as u64) / 2);
            state.current_offset = centered_top.min(max_top_row) * bytes_per_row as u64;
        }

        // Scroll Handling via Mouse Wheel
        let grid_interact = grid_ui.interact(grid_rect, grid_ui.id().with("hex_scroll_area"), Sense::click_and_drag());
        if grid_interact.hovered() {
            let scroll_delta = grid_ui.input(|i| i.raw_scroll_delta.y);
            if scroll_delta.abs() > 0.0 {
                let lines_to_scroll = (scroll_delta / 24.0).round() as i64;
                let current_row = (state.current_offset / bytes_per_row as u64) as i64;
                let new_row = (current_row - lines_to_scroll).clamp(0, max_top_row as i64) as u64;
                state.current_offset = new_row * bytes_per_row as u64;
            }

            // Keyboard navigation
            if grid_ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                let curr = state.selected_offset.unwrap_or(0);
                let next = (curr + bytes_per_row as u64).min(file_size.saturating_sub(1));
                state.selected_offset = Some(next);
                let top_off = state.current_offset;
                let bot_off = top_off + (visible_rows as u64 * bytes_per_row as u64);
                if next >= bot_off {
                    state.current_offset = (next / bytes_per_row as u64).saturating_sub(visible_rows as u64 - 1) * bytes_per_row as u64;
                }
            } else if grid_ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                let curr = state.selected_offset.unwrap_or(0);
                let prev = curr.saturating_sub(bytes_per_row as u64);
                state.selected_offset = Some(prev);
                if prev < state.current_offset {
                    state.current_offset = (prev / bytes_per_row as u64) * bytes_per_row as u64;
                }
            } else if grid_ui.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                let curr = state.selected_offset.unwrap_or(0);
                let next = (curr + 1).min(file_size.saturating_sub(1));
                state.selected_offset = Some(next);
            } else if grid_ui.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
                let curr = state.selected_offset.unwrap_or(0);
                let prev = curr.saturating_sub(1);
                state.selected_offset = Some(prev);
            } else if grid_ui.input(|i| i.key_pressed(egui::Key::PageDown)) {
                let current_row = state.current_offset / bytes_per_row as u64;
                let new_row = (current_row + visible_rows as u64).min(max_top_row);
                state.current_offset = new_row * bytes_per_row as u64;
            } else if grid_ui.input(|i| i.key_pressed(egui::Key::PageUp)) {
                let current_row = state.current_offset / bytes_per_row as u64;
                let new_row = current_row.saturating_sub(visible_rows as u64);
                state.current_offset = new_row * bytes_per_row as u64;
            } else if grid_ui.input(|i| i.key_pressed(egui::Key::Home)) {
                state.current_offset = 0;
                state.selected_offset = Some(0);
            } else if grid_ui.input(|i| i.key_pressed(egui::Key::End)) {
                state.current_offset = max_top_row * bytes_per_row as u64;
                state.selected_offset = Some(file_size.saturating_sub(1));
            }
        }

        let top_row = (state.current_offset / bytes_per_row as u64).min(max_top_row);
        let pointer_pos = grid_ui.input(|i| i.pointer.hover_pos());

        // Font metrics
        let mono_font = FontId::monospace(font_size);
        let char_w = font_size * 0.60; // Approximate monospace character width
        let offset_col_w = char_w * 12.0;

        let hex_byte_w = char_w * 2.8;
        let hex_col_w = hex_byte_w * bytes_per_row as f32 + (if bytes_per_row == 16 { char_w * 2.0 } else { 0.0 });
        let _ascii_col_w = char_w * (bytes_per_row as f32 + 3.0);

        let mut hovered_this_frame = None;

        for r in 0..visible_rows {
            let row_idx = top_row + r as u64;
            if row_idx >= total_rows {
                break;
            }
            let row_offset = row_idx * bytes_per_row as u64;
            let bytes_to_read = (bytes_per_row as u64).min(file_size.saturating_sub(row_offset)) as usize;
            let row_bytes = engine.read_range(row_offset, bytes_to_read).unwrap_or(&[]);

            let y = grid_rect.min.y + r as f32 * row_h;
            let row_rect = Rect::from_min_size(Pos2::new(grid_rect.min.x, y), Vec2::new(grid_w, row_h));

            // Alternate row background subtle striping
            if row_idx % 2 == 1 {
                let stripe_color = state.theme.stripe_color(dark_mode);
                grid_ui.painter().rect_filled(row_rect, 0.0, stripe_color);
            }

            // 1. Draw Offset Column
            let offset_text = format!("{:08X}:", row_offset);
            grid_ui.painter().text(
                Pos2::new(grid_rect.min.x + 8.0, y + row_h * 0.5),
                egui::Align2::LEFT_CENTER,
                offset_text,
                mono_font.clone(),
                state.theme.offset_color(dark_mode),
            );

            // 2. Draw Hex Bytes
            let hex_start_x = grid_rect.min.x + offset_col_w + 14.0;
            for col in 0..bytes_per_row {
                let extra_gap = if bytes_per_row == 16 && col >= 8 { char_w * 1.5 } else { 0.0 };
                let byte_x = hex_start_x + col as f32 * hex_byte_w + extra_gap;
                let cell_rect = Rect::from_min_size(Pos2::new(byte_x - 2.0, y + 1.0), Vec2::new(hex_byte_w, row_h - 2.0));

                if col < row_bytes.len() {
                    let b = row_bytes[col];
                    let b_offset = row_offset + col as u64;
                    let is_selected = state.selected_offset == Some(b_offset);
                    let is_hovered = pointer_pos.map_or(false, |pos| cell_rect.contains(pos)) || state.hovered_offset == Some(b_offset);

                    if pointer_pos.map_or(false, |pos| cell_rect.contains(pos)) {
                        hovered_this_frame = Some(b_offset);
                        if grid_ui.input(|i| i.pointer.primary_clicked()) {
                            state.selected_offset = Some(b_offset);
                            action = HexViewerAction::SelectOffset(b_offset);
                        }
                    }

                    if is_selected {
                        grid_ui.painter().rect_filled(cell_rect, 3.0, state.theme.selection_bg());
                        grid_ui.painter().rect_stroke(cell_rect, 3.0, Stroke::new(1.0_f32, state.theme.selection_stroke()), egui::StrokeKind::Inside);
                    } else if is_hovered {
                        grid_ui.painter().rect_filled(cell_rect, 3.0, if dark_mode { Color32::from_rgb(44, 49, 58) } else { Color32::from_rgb(225, 230, 238) });
                    }

                    let byte_color = if is_selected {
                        Color32::WHITE
                    } else if b == 0x00 {
                        state.theme.null_color(dark_mode)
                    } else if b.is_ascii_graphic() || b == b' ' {
                        state.theme.ascii_color(dark_mode)
                    } else if b <= 0x1F || b == 0x7F {
                        state.theme.control_color(dark_mode)
                    } else {
                        state.theme.high_byte_color(dark_mode)
                    };

                    grid_ui.painter().text(
                        Pos2::new(byte_x, y + row_h * 0.5),
                        egui::Align2::LEFT_CENTER,
                        format!("{:02X}", b),
                        mono_font.clone(),
                        byte_color,
                    );
                }
            }

            // 3. Draw ASCII Column
            let ascii_start_x = hex_start_x + hex_col_w + 14.0;
            // Divider pipe
            grid_ui.painter().text(
                Pos2::new(ascii_start_x - 8.0, y + row_h * 0.5),
                egui::Align2::LEFT_CENTER,
                "|",
                mono_font.clone(),
                Color32::from_rgb(75, 82, 99),
            );

            for col in 0..row_bytes.len() {
                let b = row_bytes[col];
                let b_offset = row_offset + col as u64;
                let ch_x = ascii_start_x + col as f32 * char_w;
                let ch_rect = Rect::from_min_size(Pos2::new(ch_x - 1.0, y + 1.0), Vec2::new(char_w, row_h - 2.0));

                let is_selected = state.selected_offset == Some(b_offset);
                let is_hovered = pointer_pos.map_or(false, |pos| ch_rect.contains(pos)) || state.hovered_offset == Some(b_offset);

                if pointer_pos.map_or(false, |pos| ch_rect.contains(pos)) {
                    hovered_this_frame = Some(b_offset);
                    if grid_ui.input(|i| i.pointer.primary_clicked()) {
                        state.selected_offset = Some(b_offset);
                        action = HexViewerAction::SelectOffset(b_offset);
                    }
                }

                if is_selected {
                    grid_ui.painter().rect_filled(ch_rect, 2.0, state.theme.selection_bg());
                } else if is_hovered {
                    grid_ui.painter().rect_filled(ch_rect, 2.0, if dark_mode { Color32::from_rgb(44, 49, 58) } else { Color32::from_rgb(225, 230, 238) });
                }

                let (ch, ch_color) = if b.is_ascii_graphic() || b == b' ' {
                    (b as char, if is_selected { Color32::WHITE } else { state.theme.ascii_color(dark_mode) })
                } else {
                    ('.', state.theme.null_color(dark_mode))
                };

                grid_ui.painter().text(
                    Pos2::new(ch_x, y + row_h * 0.5),
                    egui::Align2::LEFT_CENTER,
                    ch.to_string(),
                    mono_font.clone(),
                    ch_color,
                );
            }

            grid_ui.painter().text(
                Pos2::new(ascii_start_x + row_bytes.len() as f32 * char_w + 2.0, y + row_h * 0.5),
                egui::Align2::LEFT_CENTER,
                "|",
                mono_font.clone(),
                Color32::from_rgb(75, 82, 99),
            );
        }

        state.hovered_offset = hovered_this_frame;

        // Vertical Scrollbar on the right edge of grid_rect
        let scrollbar_w = 12.0_f32;
        let scrollbar_rect = Rect::from_min_size(Pos2::new(grid_rect.right() - scrollbar_w, grid_rect.top()), Vec2::new(scrollbar_w, content_h));
        grid_ui.painter().rect_filled(
            scrollbar_rect,
            0.0,
            if dark_mode { Color32::from_rgba_premultiplied(0, 0, 0, 40) } else { Color32::from_rgba_premultiplied(0, 0, 0, 15) },
        );

        let thumb_ratio = (visible_rows as f32 / total_rows as f32).clamp(0.02, 1.0);
        let thumb_h = (content_h * thumb_ratio).max(24.0);
        let max_thumb_top = content_h - thumb_h;
        let scroll_progress = if max_top_row > 0 { top_row as f32 / max_top_row as f32 } else { 0.0 };
        let thumb_y = grid_rect.top() + scroll_progress * max_thumb_top;
        let thumb_rect = Rect::from_min_size(Pos2::new(scrollbar_rect.min.x + 2.0, thumb_y), Vec2::new(scrollbar_w - 4.0, thumb_h));

        let sb_resp = grid_ui.interact(scrollbar_rect, grid_ui.id().with("hex_scrollbar"), Sense::click_and_drag());
        let is_sb_active = sb_resp.dragged() || sb_resp.clicked();
        let thumb_color = if is_sb_active {
            Color32::from_rgb(97, 175, 239)
        } else if sb_resp.hovered() {
            Color32::from_rgb(120, 130, 145)
        } else {
            Color32::from_rgb(75, 82, 99)
        };
        grid_ui.painter().rect_filled(thumb_rect, 4.0, thumb_color);

        if is_sb_active {
            if let Some(pos) = sb_resp.interact_pointer_pos() {
                let frac = ((pos.y - grid_rect.top() - thumb_h * 0.5) / max_thumb_top).clamp(0.0, 1.0);
                let target_row = (frac * max_top_row as f32).round() as u64;
                state.current_offset = target_row * bytes_per_row as u64;
            }
        }
    }

    // 4. Render Data Inspector Panel
    {
        let mut insp_ui = ui.new_child(egui::UiBuilder::new().max_rect(inspector_rect));
        insp_ui.set_clip_rect(inspector_rect);

        // Panel background
        insp_ui.painter().rect_filled(
            inspector_rect,
            0.0,
            if dark_mode { Color32::from_rgb(22, 27, 34) } else { Color32::from_rgb(246, 248, 250) },
        );

        let inspect_offset = state.selected_offset.unwrap_or(0).min(file_size.saturating_sub(1));
        let chunk = engine.read_range(inspect_offset, 8).unwrap_or(&[]);
        let is_be = state.is_big_endian;

        insp_ui.vertical(|ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                let (c_rect, _) = ui.allocate_exact_size(Vec2::splat(15.0), Sense::hover());
                paint_icon(ui.painter(), c_rect, Icon::Chart, Color32::from_rgb(97, 175, 239));
                ui.add_space(4.0);
                ui.label(RichText::new("DATA INSPECTOR").strong().size(12.0).color(Color32::from_rgb(97, 175, 239)));

                ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(10.0);
                    let end_str = if is_be { "BE" } else { "LE" };
                    if ui.button(RichText::new(end_str).strong().size(11.0).color(Color32::from_rgb(229, 192, 123))).on_hover_text("Toggle Endianness (Little/Big Endian)").clicked() {
                        state.is_big_endian = !state.is_big_endian;
                    }
                });
            });

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);

            // Active Offset Banner
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.label(RichText::new("Selected Byte:").size(11.5).color(Color32::from_rgb(140, 150, 165)));
                ui.label(RichText::new(format!("0x{:08X}", inspect_offset)).strong().size(12.0).color(Color32::from_rgb(152, 195, 121)));
                ui.label(RichText::new(format!("({})", inspect_offset)).size(11.0).color(Color32::from_rgb(120, 130, 145)));
            });

            ui.add_space(8.0);

            // Inspector Items Container
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(4.0);

                    // 1. Binary representation
                    if let Some(&b) = chunk.first() {
                        let bin_str = format!("{:04b} {:04b}", (b >> 4) & 0x0F, b & 0x0F);
                        render_inspector_row(ui, "Binary", &bin_str, "8-bit binary bit pattern");
                    }

                    // 2. 8-bit Integer (Unsigned / Signed)
                    if let Some(&b) = chunk.first() {
                        let u8_val = format!("{}", b);
                        let i8_val = format!("{}", b as i8);
                        render_inspector_row(ui, "UInt8", &u8_val, "8-bit Unsigned Integer");
                        render_inspector_row(ui, "Int8", &i8_val, "8-bit Signed Integer");
                    }

                    // 3. 16-bit Integer
                    if chunk.len() >= 2 {
                        let slice2: [u8; 2] = [chunk[0], chunk[1]];
                        let (u16_val, i16_val) = if is_be {
                            (u16::from_be_bytes(slice2), i16::from_be_bytes(slice2))
                        } else {
                            (u16::from_le_bytes(slice2), i16::from_le_bytes(slice2))
                        };
                        render_inspector_row(ui, "UInt16", &format!("{}", u16_val), "16-bit Unsigned Integer");
                        render_inspector_row(ui, "Int16", &format!("{}", i16_val), "16-bit Signed Integer");
                    }

                    // 4. 32-bit Integer
                    if chunk.len() >= 4 {
                        let slice4: [u8; 4] = [chunk[0], chunk[1], chunk[2], chunk[3]];
                        let (u32_val, i32_val) = if is_be {
                            (u32::from_be_bytes(slice4), i32::from_be_bytes(slice4))
                        } else {
                            (u32::from_le_bytes(slice4), i32::from_le_bytes(slice4))
                        };
                        render_inspector_row(ui, "UInt32", &format!("{}", u32_val), "32-bit Unsigned Integer");
                        render_inspector_row(ui, "Int32", &format!("{}", i32_val), "32-bit Signed Integer");

                        // Float 32
                        let f32_val = if is_be { f32::from_be_bytes(slice4) } else { f32::from_le_bytes(slice4) };
                        let f32_str = if f32_val.is_finite() { format!("{:.6}", f32_val) } else { format!("{}", f32_val) };
                        render_inspector_row(ui, "Float32", &f32_str, "32-bit Single Precision Float");
                    }

                    // 5. 64-bit Integer
                    if chunk.len() >= 8 {
                        let slice8: [u8; 8] = [chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7]];
                        let (u64_val, i64_val) = if is_be {
                            (u64::from_be_bytes(slice8), i64::from_be_bytes(slice8))
                        } else {
                            (u64::from_le_bytes(slice8), i64::from_le_bytes(slice8))
                        };
                        render_inspector_row(ui, "UInt64", &format!("{}", u64_val), "64-bit Unsigned Integer");
                        render_inspector_row(ui, "Int64", &format!("{}", i64_val), "64-bit Signed Integer");

                        // Float 64
                        let f64_val = if is_be { f64::from_be_bytes(slice8) } else { f64::from_le_bytes(slice8) };
                        let f64_str = if f64_val.is_finite() { format!("{:.8}", f64_val) } else { format!("{}", f64_val) };
                        render_inspector_row(ui, "Float64", &f64_str, "64-bit Double Precision Float");
                    }

                    // 6. Character representation
                    if let Some(&b) = chunk.first() {
                        let char_str = if b.is_ascii_graphic() || b == b' ' {
                            format!("'{}' (ASCII)", b as char)
                        } else if b == 0x00 {
                            "<NUL> (0x00)".to_string()
                        } else if b == 0x0A {
                            "<LF> (0x0A)".to_string()
                        } else if b == 0x0D {
                            "<CR> (0x0D)".to_string()
                        } else if b == 0x09 {
                            "<TAB> (0x09)".to_string()
                        } else {
                            format!("0x{:02X} (non-printable)", b)
                        };
                        render_inspector_row(ui, "Char", &char_str, "Character description");
                    }
                });
        });
    }

    action
}

fn render_inspector_row(ui: &mut Ui, label: &str, value: &str, tooltip: &str) {
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        let l_resp = ui.allocate_ui_with_layout(Vec2::new(55.0, 20.0), Layout::left_to_right(egui::Align::Center), |ui| {
            ui.label(RichText::new(label).strong().size(11.0).color(Color32::from_rgb(140, 150, 165)));
        });
        l_resp.response.on_hover_text(tooltip);

        let copy_resp = ui.button(RichText::new(value).font(FontId::monospace(11.0)).color(Color32::from_rgb(220, 225, 235)));
        if copy_resp.on_hover_text("Click to copy value").clicked() {
            ui.ctx().copy_text(value.to_string());
        }
    });
    ui.add_space(2.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_offset() {
        assert_eq!(parse_offset("0"), Some(0));
        assert_eq!(parse_offset("1024"), Some(1024));
        assert_eq!(parse_offset("1_000_000"), Some(1_000_000));
        assert_eq!(parse_offset("1,024"), Some(1024));

        // Hex prefixes
        assert_eq!(parse_offset("0x10"), Some(16));
        assert_eq!(parse_offset("0X1A40"), Some(0x1A40));
        assert_eq!(parse_offset("0x1a_40"), Some(0x1A40));

        // Hex suffixes
        assert_eq!(parse_offset("10h"), Some(16));
        assert_eq!(parse_offset("1A40H"), Some(0x1A40));

        // Hex letters
        assert_eq!(parse_offset("deadbeef"), Some(0xdeadbeef));
        assert_eq!(parse_offset("A0B1"), Some(0xA0B1));

        // Invalid
        assert_eq!(parse_offset(""), None);
        assert_eq!(parse_offset("   "), None);
        assert_eq!(parse_offset("xyz"), None);
    }

    #[test]
    fn test_hex_viewer_default_state() {
        let state = HexViewerState::default();
        assert!(!state.is_enabled);
        assert_eq!(state.bytes_per_row, 16);
        assert_eq!(state.current_offset, 0);
        assert_eq!(state.selected_offset, Some(0));
        assert!(!state.is_big_endian);
        assert_eq!(state.theme, HexColorTheme::ModernDark);
    }
}

