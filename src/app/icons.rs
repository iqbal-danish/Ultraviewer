use eframe::egui::{self, Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Logo,
    Explorer,
    Folder,
    FolderOpen,
    Search,
    File,
    XmlCode,
    JsonBraces,
    Tree,
    Chart,
    Gear,
    Clock,
    Lock,
    Pencil,
    Save,
    Wrap,
    Plus,
    Close,
    Format,
    Minify,
    Validate,
    Help,
    Check,
    ChevronRight,
    ChevronDown,
    Copy,
    Sun,
    ArrowUp,
    ArrowDown,
    SelectionLines,
    ReplaceOne,
    ReplaceAll,
    Cube,
    Sparkle,
    Warning,
}

pub fn paint_icon(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let min_x = rect.min.x.round();
    let min_y = rect.min.y.round();
    let w = rect.width().round();
    let h = rect.height().round();
    let cx = (min_x + w * 0.5).round();
    let cy = (min_y + h * 0.5).round();

    let stroke = Stroke::new(1.6_f32, color);
    let stroke_bold = Stroke::new(2.0_f32, color);

    match icon {
        Icon::Logo => {
            // Modern VS Code-style origami ribbon logo in layered blues
            let p_left = Pos2::new(min_x + (w * 0.12).round(), cy);
            let p_top_right = Pos2::new(min_x + (w * 0.88).round(), min_y + (h * 0.14).round());
            let p_bot_right = Pos2::new(min_x + (w * 0.88).round(), min_y + (h * 0.86).round());
            let p_notch = Pos2::new(min_x + (w * 0.58).round(), cy);
            let p_top_inner = Pos2::new(min_x + (w * 0.62).round(), min_y + (h * 0.26).round());
            let p_bot_inner = Pos2::new(min_x + (w * 0.62).round(), min_y + (h * 0.74).round());

            // Back wing (darker blue)
            let dark_blue = Color32::from_rgb(0, 95, 160);
            painter.add(egui::Shape::convex_polygon(
                vec![p_left, p_top_inner, p_bot_inner],
                dark_blue,
                Stroke::NONE,
            ));

            // Top fold (light vibrant blue #1F9CF0)
            let light_blue = Color32::from_rgb(31, 156, 240);
            painter.add(egui::Shape::convex_polygon(
                vec![p_left, p_top_inner, p_top_right, p_notch],
                light_blue,
                Stroke::NONE,
            ));

            // Bottom fold (signature VS Code blue #007ACC)
            let mid_blue = Color32::from_rgb(0, 122, 204);
            painter.add(egui::Shape::convex_polygon(
                vec![p_left, p_notch, p_bot_right, p_bot_inner],
                mid_blue,
                Stroke::NONE,
            ));
        }

        Icon::Explorer => {
            // VS Code dual overlapping documents (back sheet + front sheet with dog-ear)
            let b_left = min_x + 1.5;
            let b_top = min_y + 1.5;
            let b_w = (w * 0.66).round();
            let b_h = (h * 0.72).round();
            painter.rect_stroke(
                Rect::from_min_size(Pos2::new(b_left, b_top), Vec2::new(b_w, b_h)),
                1.5,
                stroke,
                egui::StrokeKind::Inside,
            );

            // Front sheet
            let f_left = min_x + (w * 0.28).round();
            let f_top = min_y + (h * 0.24).round();
            let f_right = min_x + w - 1.5;
            let f_bottom = min_y + h - 1.5;
            let fold = 3.5_f32;

            // Cutout background behind front sheet
            let bg_clear = Color32::from_rgb(35, 39, 46);
            painter.rect_filled(
                Rect::from_min_max(Pos2::new(f_left - 0.5, f_top - 0.5), Pos2::new(f_right + 0.5, f_bottom + 0.5)),
                0.0,
                bg_clear,
            );

            let pts = vec![
                Pos2::new(f_left, f_top),
                Pos2::new(f_right - fold, f_top),
                Pos2::new(f_right, f_top + fold),
                Pos2::new(f_right, f_bottom),
                Pos2::new(f_left, f_bottom),
            ];
            painter.add(egui::Shape::closed_line(pts, stroke));
            painter.line_segment([Pos2::new(f_right - fold, f_top), Pos2::new(f_right - fold, f_top + fold)], stroke);
            painter.line_segment([Pos2::new(f_right - fold, f_top + fold), Pos2::new(f_right, f_top + fold)], stroke);
        }

        Icon::Folder => {
            // Crisp folder with solid tab and rounded body
            let tab_w = (w * 0.44).round();
            let tab_h = 3.5_f32;
            painter.rect_filled(
                Rect::from_min_size(Pos2::new(min_x + 1.0, min_y + 2.0), Vec2::new(tab_w, tab_h)),
                1.5,
                color,
            );
            painter.rect_stroke(
                Rect::from_min_size(Pos2::new(min_x + 1.0, min_y + 4.5), Vec2::new(w - 2.0, h - 6.0)),
                2.0,
                stroke,
                egui::StrokeKind::Inside,
            );
        }

        Icon::FolderOpen => {
            // Open folder
            let tab_w = (w * 0.40).round();
            painter.rect_filled(
                Rect::from_min_size(Pos2::new(min_x + 1.0, min_y + 2.0), Vec2::new(tab_w, 3.5)),
                1.5,
                color,
            );
            painter.line_segment([Pos2::new(min_x + 1.0, min_y + 5.0), Pos2::new(min_x + w - 1.0, min_y + 5.0)], stroke);
            // Angled front flap
            let pts = vec![
                Pos2::new(min_x + 1.0, min_y + h - 2.0),
                Pos2::new(min_x + 3.5, min_y + 7.5),
                Pos2::new(min_x + w - 1.0, min_y + 7.5),
                Pos2::new(min_x + w - 3.5, min_y + h - 2.0),
            ];
            painter.add(egui::Shape::closed_line(pts, stroke));
        }

        Icon::Search => {
            // Bold, clean magnifying glass
            let r = (w * 0.28).round().max(4.0);
            let scx = min_x + r + 2.0;
            let scy = min_y + r + 2.0;
            painter.circle_stroke(Pos2::new(scx, scy), r, stroke_bold);
            let handle_start = Pos2::new(scx + (r * 0.707).round(), scy + (r * 0.707).round());
            let handle_end = Pos2::new(min_x + w - 2.0, min_y + h - 2.0);
            painter.line_segment([handle_start, handle_end], stroke_bold);
        }

        Icon::File => {
            // Document sheet with folded dog-ear
            let left = min_x + 2.5;
            let right = min_x + w - 2.5;
            let top = min_y + 1.5;
            let bottom = min_y + h - 1.5;
            let fold = 4.0;

            let pts = vec![
                Pos2::new(left, top),
                Pos2::new(right - fold, top),
                Pos2::new(right, top + fold),
                Pos2::new(right, bottom),
                Pos2::new(left, bottom),
            ];
            painter.add(egui::Shape::closed_line(pts, stroke));
            painter.line_segment([Pos2::new(right - fold, top), Pos2::new(right - fold, top + fold)], stroke);
            painter.line_segment([Pos2::new(right - fold, top + fold), Pos2::new(right, top + fold)], stroke);
        }

        Icon::XmlCode => {
            // Bold code brackets < >
            let dy = (h * 0.28).round();
            let dx = (w * 0.22).round();
            // <
            painter.line_segment([Pos2::new(cx - 2.0, cy - dy), Pos2::new(cx - 2.0 - dx, cy)], stroke_bold);
            painter.line_segment([Pos2::new(cx - 2.0 - dx, cy), Pos2::new(cx - 2.0, cy + dy)], stroke_bold);
            // >
            painter.line_segment([Pos2::new(cx + 2.0, cy - dy), Pos2::new(cx + 2.0 + dx, cy)], stroke_bold);
            painter.line_segment([Pos2::new(cx + 2.0 + dx, cy), Pos2::new(cx + 2.0, cy + dy)], stroke_bold);
        }

        Icon::JsonBraces => {
            // Bold curly braces { }
            let dy = (h * 0.32).round();
            // {
            painter.line_segment([Pos2::new(cx - 3.0, cy - dy), Pos2::new(cx - 5.5, cy - dy * 0.5)], stroke);
            painter.line_segment([Pos2::new(cx - 5.5, cy - dy * 0.5), Pos2::new(cx - 7.0, cy)], stroke);
            painter.line_segment([Pos2::new(cx - 7.0, cy), Pos2::new(cx - 5.5, cy + dy * 0.5)], stroke);
            painter.line_segment([Pos2::new(cx - 5.5, cy + dy * 0.5), Pos2::new(cx - 3.0, cy + dy)], stroke);
            // }
            painter.line_segment([Pos2::new(cx + 3.0, cy - dy), Pos2::new(cx + 5.5, cy - dy * 0.5)], stroke);
            painter.line_segment([Pos2::new(cx + 5.5, cy - dy * 0.5), Pos2::new(cx + 7.0, cy)], stroke);
            painter.line_segment([Pos2::new(cx + 7.0, cy), Pos2::new(cx + 5.5, cy + dy * 0.5)], stroke);
            painter.line_segment([Pos2::new(cx + 5.5, cy + dy * 0.5), Pos2::new(cx + 3.0, cy + dy)], stroke);
        }

        Icon::Tree => {
            // Branching tree with solid nodes
            let trunk_x = min_x + 4.0;
            let top_y = min_y + 3.5;
            let bottom_y = min_y + h - 3.5;

            // Root node dot
            painter.circle_filled(Pos2::new(trunk_x, top_y), 2.2, color);
            // Vertical trunk line
            painter.line_segment([Pos2::new(trunk_x, top_y), Pos2::new(trunk_x, bottom_y)], stroke);

            // Branch 1
            painter.line_segment([Pos2::new(trunk_x, cy), Pos2::new(min_x + w - 3.0, cy)], stroke);
            painter.circle_filled(Pos2::new(min_x + w - 3.0, cy), 2.0, color);

            // Branch 2
            painter.line_segment([Pos2::new(trunk_x, bottom_y), Pos2::new(min_x + w - 3.0, bottom_y)], stroke);
            painter.circle_filled(Pos2::new(min_x + w - 3.0, bottom_y), 2.0, color);
        }

        Icon::Chart => {
            // 3 Solid filled vertical bars — ZERO blur, 100% crisp!
            let baseline_y = min_y + h - 2.5;
            painter.line_segment([Pos2::new(min_x + 1.5, baseline_y), Pos2::new(min_x + w - 1.5, baseline_y)], stroke);

            let bar_w = 3.0_f32;
            // Bar 1
            let h1 = ((h - 5.0) * 0.45).round();
            painter.rect_filled(Rect::from_min_size(Pos2::new(cx - 5.5, baseline_y - h1), Vec2::new(bar_w, h1)), 1.0, color);
            // Bar 2 (tallest)
            let h2 = ((h - 5.0) * 0.90).round();
            painter.rect_filled(Rect::from_min_size(Pos2::new(cx - 1.5, baseline_y - h2), Vec2::new(bar_w, h2)), 1.0, color);
            // Bar 3
            let h3 = ((h - 5.0) * 0.65).round();
            painter.rect_filled(Rect::from_min_size(Pos2::new(cx + 2.5, baseline_y - h3), Vec2::new(bar_w, h3)), 1.0, color);
        }

        Icon::Gear => {
            let r = (w * 0.28).round().max(4.0);
            painter.circle_stroke(Pos2::new(cx, cy), r, stroke);
            for i in 0..6 {
                let angle = (i as f32) * std::f32::consts::PI / 3.0;
                let (sin, cos) = angle.sin_cos();
                let p1 = Pos2::new((cx + cos * (r - 0.5)).round(), (cy + sin * (r - 0.5)).round());
                let p2 = Pos2::new((cx + cos * (r + 2.8)).round(), (cy + sin * (r + 2.8)).round());
                painter.line_segment([p1, p2], stroke_bold);
            }
            painter.circle_filled(Pos2::new(cx, cy), 1.6, color);
        }

        Icon::Clock => {
            let r = (w * 0.38).round().max(5.0);
            painter.circle_stroke(Pos2::new(cx, cy), r, stroke);
            painter.line_segment([Pos2::new(cx, cy), Pos2::new(cx, cy - (r * 0.6).round())], stroke_bold);
            painter.line_segment([Pos2::new(cx, cy), Pos2::new(cx + (r * 0.6).round(), cy)], stroke_bold);
        }

        Icon::Lock => {
            let bw = (w * 0.56).round();
            let bh = (h * 0.40).round();
            let box_rect = Rect::from_center_size(Pos2::new(cx, cy + 2.5), Vec2::new(bw, bh));
            painter.rect_stroke(box_rect, 1.5, stroke_bold, egui::StrokeKind::Inside);
            // Arch
            let arch_r = (bw * 0.34).round();
            let arch_top = box_rect.top() - arch_r;
            painter.line_segment([Pos2::new(cx - arch_r, box_rect.top()), Pos2::new(cx - arch_r, arch_top)], stroke);
            painter.line_segment([Pos2::new(cx - arch_r, arch_top), Pos2::new(cx + arch_r, arch_top)], stroke);
            painter.line_segment([Pos2::new(cx + arch_r, arch_top), Pos2::new(cx + arch_r, box_rect.top())], stroke);
        }

        Icon::Pencil => {
            painter.line_segment([Pos2::new(min_x + 3.0, min_y + h - 3.0), Pos2::new(min_x + w - 3.0, min_y + 3.0)], stroke_bold);
            painter.line_segment([Pos2::new(min_x + 3.0, min_y + h - 3.0), Pos2::new(min_x + 6.0, min_y + h - 3.0)], stroke);
            painter.line_segment([Pos2::new(min_x + 3.0, min_y + h - 3.0), Pos2::new(min_x + 3.0, min_y + h - 6.0)], stroke);
        }

        Icon::Save => {
            let left = min_x + 2.0;
            let right = min_x + w - 2.0;
            let top = min_y + 2.0;
            let bottom = min_y + h - 2.0;
            let notch = 3.5;

            let pts = vec![
                Pos2::new(left, top),
                Pos2::new(right - notch, top),
                Pos2::new(right, top + notch),
                Pos2::new(right, bottom),
                Pos2::new(left, bottom),
            ];
            painter.add(egui::Shape::closed_line(pts, stroke));
            painter.rect_filled(Rect::from_min_max(Pos2::new(cx - 2.5, top), Pos2::new(cx + 2.5, top + 4.0)), 0.5, color);
        }

        Icon::Wrap => {
            // Return wrap arrow ↵
            let start = Pos2::new(min_x + 3.5, cy - 3.5);
            let corner = Pos2::new(min_x + w - 3.5, cy - 3.5);
            let bottom_pt = Pos2::new(min_x + w - 3.5, cy + 3.5);
            let end_pt = Pos2::new(min_x + 3.5, cy + 3.5);

            painter.line_segment([start, corner], stroke);
            painter.line_segment([corner, bottom_pt], stroke);
            painter.line_segment([bottom_pt, end_pt], stroke);

            // Left arrowhead
            painter.line_segment([end_pt, Pos2::new(end_pt.x + 3.0, end_pt.y - 3.0)], stroke);
            painter.line_segment([end_pt, Pos2::new(end_pt.x + 3.0, end_pt.y + 3.0)], stroke);
        }

        Icon::Plus => {
            let span = (w * 0.36).round();
            painter.line_segment([Pos2::new(cx - span, cy), Pos2::new(cx + span, cy)], stroke_bold);
            painter.line_segment([Pos2::new(cx, cy - span), Pos2::new(cx, cy + span)], stroke_bold);
        }

        Icon::Close => {
            let span = (w * 0.30).round();
            painter.line_segment([Pos2::new(cx - span, cy - span), Pos2::new(cx + span, cy + span)], stroke_bold);
            painter.line_segment([Pos2::new(cx + span, cy - span), Pos2::new(cx - span, cy + span)], stroke_bold);
        }

        Icon::Format => {
            // Indented formatting lines
            let left = min_x + 2.0;
            let right = min_x + w - 2.0;
            painter.line_segment([Pos2::new(left, cy - 4.5), Pos2::new(right, cy - 4.5)], stroke_bold);
            painter.line_segment([Pos2::new(left + 4.0, cy), Pos2::new(right - 1.0, cy)], stroke_bold);
            painter.line_segment([Pos2::new(left + 4.0, cy + 4.5), Pos2::new(right - 3.0, cy + 4.5)], stroke_bold);
        }

        Icon::Minify => {
            // Inward chevrons > <
            painter.line_segment([Pos2::new(cx - 5.5, cy - 4.0), Pos2::new(cx - 2.0, cy)], stroke_bold);
            painter.line_segment([Pos2::new(cx - 2.0, cy), Pos2::new(cx - 5.5, cy + 4.0)], stroke_bold);
            painter.line_segment([Pos2::new(cx + 5.5, cy - 4.0), Pos2::new(cx + 2.0, cy)], stroke_bold);
            painter.line_segment([Pos2::new(cx + 2.0, cy), Pos2::new(cx + 5.5, cy + 4.0)], stroke_bold);
        }

        Icon::Validate => {
            // Shield outline with bold checkmark
            let left = min_x + 2.5;
            let right = min_x + w - 2.5;
            let top = min_y + 1.5;
            let bottom = min_y + h - 1.5;

            let pts = vec![
                Pos2::new(left, top),
                Pos2::new(right, top),
                Pos2::new(right, cy + 1.0),
                Pos2::new(cx, bottom),
                Pos2::new(left, cy + 1.0),
            ];
            painter.add(egui::Shape::closed_line(pts, stroke));
            painter.line_segment([Pos2::new(cx - 2.5, cy), Pos2::new(cx - 0.5, cy + 2.0)], stroke_bold);
            painter.line_segment([Pos2::new(cx - 0.5, cy + 2.0), Pos2::new(cx + 3.0, cy - 1.5)], stroke_bold);
        }

        Icon::Help => {
            let r = (w * 0.38).round().max(5.0);
            painter.circle_stroke(Pos2::new(cx, cy), r, stroke);
            painter.line_segment([Pos2::new(cx - 1.5, cy - 3.0), Pos2::new(cx, cy - 4.0)], stroke);
            painter.line_segment([Pos2::new(cx, cy - 4.0), Pos2::new(cx + 1.5, cy - 3.0)], stroke);
            painter.line_segment([Pos2::new(cx + 1.5, cy - 3.0), Pos2::new(cx, cy - 1.0)], stroke);
            painter.line_segment([Pos2::new(cx, cy - 1.0), Pos2::new(cx, cy + 0.5)], stroke);
            painter.circle_filled(Pos2::new(cx, cy + 3.0), 1.2, color);
        }

        Icon::Check => {
            painter.line_segment([Pos2::new(min_x + 2.5, cy), Pos2::new(cx - 0.5, min_y + h - 3.5)], stroke_bold);
            painter.line_segment([Pos2::new(cx - 0.5, min_y + h - 3.5), Pos2::new(min_x + w - 2.0, min_y + 3.5)], stroke_bold);
        }

        Icon::ChevronRight => {
            painter.line_segment([Pos2::new(cx - 2.5, cy - 4.0), Pos2::new(cx + 2.5, cy)], stroke_bold);
            painter.line_segment([Pos2::new(cx + 2.5, cy), Pos2::new(cx - 2.5, cy + 4.0)], stroke_bold);
        }

        Icon::ChevronDown => {
            painter.line_segment([Pos2::new(cx - 4.0, cy - 2.5), Pos2::new(cx, cy + 2.5)], stroke_bold);
            painter.line_segment([Pos2::new(cx, cy + 2.5), Pos2::new(cx + 4.0, cy - 2.5)], stroke_bold);
        }

        Icon::Copy => {
            painter.rect_stroke(Rect::from_min_size(Pos2::new(min_x + 4.0, min_y + 2.0), Vec2::new(w * 0.55, h * 0.65)), 1.0, stroke, egui::StrokeKind::Inside);
            painter.rect_stroke(Rect::from_min_size(Pos2::new(min_x + 1.5, min_y + 4.5), Vec2::new(w * 0.55, h * 0.65)), 1.0, stroke, egui::StrokeKind::Inside);
        }

        Icon::Sun => {
            let r = (w * 0.22).round();
            painter.circle_stroke(Pos2::new(cx, cy), r, stroke);
            for i in 0..8 {
                let angle = (i as f32) * std::f32::consts::PI / 4.0;
                let (sin, cos) = angle.sin_cos();
                let p1 = Pos2::new((cx + cos * (r + 1.5)).round(), (cy + sin * (r + 1.5)).round());
                let p2 = Pos2::new((cx + cos * (r + 4.0)).round(), (cy + sin * (r + 4.0)).round());
                painter.line_segment([p1, p2], stroke);
            }
        }

        Icon::ArrowUp => {
            painter.line_segment([Pos2::new(cx, min_y + 3.0), Pos2::new(cx, min_y + h - 3.0)], stroke_bold);
            painter.line_segment([Pos2::new(cx - 3.5, min_y + 6.5), Pos2::new(cx, min_y + 3.0)], stroke_bold);
            painter.line_segment([Pos2::new(cx + 3.5, min_y + 6.5), Pos2::new(cx, min_y + 3.0)], stroke_bold);
        }

        Icon::ArrowDown => {
            painter.line_segment([Pos2::new(cx, min_y + 3.0), Pos2::new(cx, min_y + h - 3.0)], stroke_bold);
            painter.line_segment([Pos2::new(cx - 3.5, min_y + h - 6.5), Pos2::new(cx, min_y + h - 3.0)], stroke_bold);
            painter.line_segment([Pos2::new(cx + 3.5, min_y + h - 6.5), Pos2::new(cx, min_y + h - 3.0)], stroke_bold);
        }

        Icon::SelectionLines => {
            let left = min_x + 3.0;
            let right = min_x + w - 3.0;
            painter.line_segment([Pos2::new(left, cy - 4.0), Pos2::new(right, cy - 4.0)], stroke);
            painter.line_segment([Pos2::new(left, cy), Pos2::new(right, cy)], stroke);
            painter.line_segment([Pos2::new(left, cy + 4.0), Pos2::new(right, cy + 4.0)], stroke);
        }

        Icon::ReplaceOne => {
            let thin_stroke = Stroke::new(1.3_f32, color);
            let arr_start = Pos2::new(min_x + 1.5, min_y + 3.5);
            let arr_turn = Pos2::new(min_x + 5.5, min_y + 3.5);
            let arr_down = Pos2::new(min_x + 5.5, min_y + 11.5);
            painter.line_segment([arr_start, arr_turn], thin_stroke);
            painter.line_segment([arr_turn, arr_down], thin_stroke);
            painter.line_segment([Pos2::new(min_x + 3.2, min_y + 9.0), arr_down], thin_stroke);
            painter.line_segment([Pos2::new(min_x + 7.8, min_y + 9.0), arr_down], thin_stroke);

            painter.text(
                Pos2::new(min_x + 12.0, min_y + 4.0),
                egui::Align2::CENTER_CENTER,
                "b",
                egui::FontId::monospace(8.0),
                color,
            );
            let c_box = Rect::from_center_size(Pos2::new(min_x + 12.0, min_y + 12.0), Vec2::new(8.5, 7.5));
            painter.rect_stroke(c_box, 1.0, Stroke::new(1.0_f32, color), egui::StrokeKind::Inside);
            painter.text(
                c_box.center(),
                egui::Align2::CENTER_CENTER,
                "c",
                egui::FontId::monospace(7.0),
                color,
            );
        }

        Icon::ReplaceAll => {
            let thin_stroke = Stroke::new(1.3_f32, color);
            let arr_start = Pos2::new(min_x + 1.0, min_y + 3.5);
            let arr_turn = Pos2::new(min_x + 5.0, min_y + 3.5);
            let arr_down = Pos2::new(min_x + 5.0, min_y + 11.5);
            painter.line_segment([arr_start, arr_turn], thin_stroke);
            painter.line_segment([arr_turn, arr_down], thin_stroke);
            painter.line_segment([Pos2::new(min_x + 2.8, min_y + 9.0), arr_down], thin_stroke);
            painter.line_segment([Pos2::new(min_x + 7.2, min_y + 9.0), arr_down], thin_stroke);

            painter.text(
                Pos2::new(min_x + 12.5, min_y + 4.0),
                egui::Align2::CENTER_CENTER,
                "ab",
                egui::FontId::monospace(7.0),
                color,
            );
            let ac_box = Rect::from_center_size(Pos2::new(min_x + 12.5, min_y + 12.0), Vec2::new(11.0, 7.5));
            painter.rect_stroke(ac_box, 1.0, Stroke::new(1.0_f32, color), egui::StrokeKind::Inside);
            painter.text(
                ac_box.center(),
                egui::Align2::CENTER_CENTER,
                "ac",
                egui::FontId::monospace(6.5),
                color,
            );
        }

        Icon::Cube => {
            // Isometric 3D cube matching VS Code element symbol
            let r = (w.min(h) * 0.44).round().max(4.0);
            let p_center = Pos2::new(cx, cy);
            let p_top = Pos2::new(cx, cy - r);
            let p_top_right = Pos2::new(cx + (r * 0.866).round(), cy - (r * 0.5).round());
            let p_bot_right = Pos2::new(cx + (r * 0.866).round(), cy + (r * 0.5).round());
            let p_bot = Pos2::new(cx, cy + r);
            let p_bot_left = Pos2::new(cx - (r * 0.866).round(), cy + (r * 0.5).round());
            let p_top_left = Pos2::new(cx - (r * 0.866).round(), cy - (r * 0.5).round());

            // Shaded 3D isometric faces
            let top_face = vec![p_top, p_top_right, p_center, p_top_left];
            let left_face = vec![p_top_left, p_center, p_bot, p_bot_left];
            let right_face = vec![p_center, p_top_right, p_bot_right, p_bot];

            painter.add(egui::Shape::convex_polygon(top_face, color.gamma_multiply(0.95), Stroke::NONE));
            painter.add(egui::Shape::convex_polygon(left_face, color.gamma_multiply(0.70), Stroke::NONE));
            painter.add(egui::Shape::convex_polygon(right_face, color.gamma_multiply(0.48), Stroke::NONE));

            // Sharp edge outlines
            let edge_stroke = Stroke::new(1.0_f32, color);
            painter.add(egui::Shape::closed_line(
                vec![p_top, p_top_right, p_bot_right, p_bot, p_bot_left, p_top_left],
                edge_stroke,
            ));
            painter.line_segment([p_center, p_top], edge_stroke);
            painter.line_segment([p_center, p_bot_right], edge_stroke);
            painter.line_segment([p_center, p_bot_left], edge_stroke);
        }

        Icon::Sparkle => {
            // 4-pointed star / sparkle for smart suggestions
            let r = (w.min(h) * 0.42).round().max(4.0);
            let inner_r = (r * 0.28).round().max(1.5);
            let pts = vec![
                Pos2::new(cx, cy - r),
                Pos2::new(cx + inner_r, cy - inner_r),
                Pos2::new(cx + r, cy),
                Pos2::new(cx + inner_r, cy + inner_r),
                Pos2::new(cx, cy + r),
                Pos2::new(cx - inner_r, cy + inner_r),
                Pos2::new(cx - r, cy),
                Pos2::new(cx - inner_r, cy - inner_r),
            ];
            painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        }

        Icon::Warning => {
            // Warning triangle with exclamation mark
            let r = (w.min(h) * 0.45).round().max(5.0);
            let top = Pos2::new(cx, cy - r);
            let bot_left = Pos2::new(cx - (r * 1.05).round(), cy + (r * 0.85).round());
            let bot_right = Pos2::new(cx + (r * 1.05).round(), cy + (r * 0.85).round());

            painter.add(egui::Shape::closed_line(vec![top, bot_right, bot_left], stroke_bold));
            painter.line_segment([Pos2::new(cx, cy - (r * 0.22).round()), Pos2::new(cx, cy + (r * 0.22).round())], stroke_bold);
            painter.circle_filled(Pos2::new(cx, cy + (r * 0.55).round()), 1.1, color);
        }
    }
}

pub fn render_icon(ui: &mut Ui, icon: Icon, size: f32, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    if ui.is_rect_visible(rect) {
        paint_icon(ui.painter(), rect, icon, color);
    }
    response
}

pub fn render_icon_button(
    ui: &mut Ui,
    icon: Icon,
    size: f32,
    normal_color: Color32,
    hover_color: Color32,
    tooltip: &str,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let hovered = response.hovered();

    if hovered {
        ui.painter().rect_filled(rect, 4.0, Color32::from_rgb(24, 34, 52));
    }

    let color = if hovered { hover_color } else { normal_color };
    if ui.is_rect_visible(rect) {
        let icon_rect = rect.shrink(3.0);
        paint_icon(ui.painter(), icon_rect, icon, color);
    }

    response.on_hover_text(tooltip)
}

pub fn render_nav_item(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    shortcut: Option<&str>,
    is_active: bool,
    has_chevron: bool,
) -> Response {
    let height = 32.0;
    let available_w = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(available_w, height), Sense::click());
    let hovered = response.hovered();

    let bg_color = if is_active {
        Color32::from_rgb(19, 38, 65) // Active cyan-navy
    } else if hovered {
        Color32::from_rgb(18, 28, 44)
    } else {
        Color32::TRANSPARENT
    };

    ui.painter().rect_filled(rect, 5.0, bg_color);

    if is_active {
        ui.painter().rect_stroke(
            rect,
            5.0,
            Stroke::new(1.0_f32, Color32::from_rgb(56, 189, 248)),
            egui::StrokeKind::Inside,
        );
    }

    let icon_color = if is_active {
        Color32::from_rgb(56, 189, 248) // Electric cyan
    } else if hovered {
        Color32::from_rgb(186, 230, 253)
    } else {
        Color32::from_rgb(155, 170, 190) // Crisp, visible cool gray
    };

    let text_color = if is_active {
        Color32::WHITE
    } else if hovered {
        Color32::from_rgb(240, 246, 255)
    } else {
        Color32::from_rgb(210, 220, 235) // High contrast text
    };

    // Paint Icon (18x18, pixel aligned)
    let icon_box = Rect::from_min_size(
        Pos2::new(rect.left().round() + 10.0, (rect.center().y - 9.0).round()),
        Vec2::splat(18.0),
    );
    paint_icon(ui.painter(), icon_box, icon, icon_color);

    // Paint Label
    let text_pos = Pos2::new(rect.left().round() + 36.0, rect.center().y.round());
    ui.painter().text(
        text_pos,
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(12.5),
        text_color,
    );

    // Paint Shortcut or Chevron on right edge
    if let Some(sc) = shortcut {
        let sc_pos = Pos2::new(rect.right().round() - 10.0, rect.center().y.round());
        ui.painter().text(
            sc_pos,
            egui::Align2::RIGHT_CENTER,
            sc,
            egui::FontId::proportional(10.0),
            Color32::from_rgb(90, 105, 125),
        );
    } else if has_chevron {
        let chev_box = Rect::from_min_size(
            Pos2::new(rect.right().round() - 18.0, (rect.center().y - 6.0).round()),
            Vec2::splat(12.0),
        );
        paint_icon(ui.painter(), chev_box, Icon::ChevronRight, Color32::from_rgb(90, 105, 125));
    }

    response
}
