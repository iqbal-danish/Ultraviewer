use eframe::egui::{self, Color32, Rect, Response, Sense, Stroke, Ui, Vec2};
use crate::search::SearchResultMatch;

pub struct OverviewRulerProps<'a> {
    pub current_line: usize,
    pub visible_lines_count: usize,
    pub total_lines: usize,
    pub search_matches: &'a [SearchResultMatch],
    pub active_match_line: Option<usize>,
}

pub fn render_overview_ruler(
    ui: &mut Ui,
    height: f32,
    props: &OverviewRulerProps,
) -> (Option<usize>, Response) {
    let ruler_width = 14.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ruler_width, height), Sense::click_and_drag());

    let total = props.total_lines.max(1) as f32;
    let track_bg = Color32::from_rgb(18, 22, 28);
    let border_color = Color32::from_rgb(33, 38, 45);

    // 1. Draw track
    ui.painter().rect_filled(rect, 2.0, track_bg);
    ui.painter().rect_stroke(rect, 2.0, Stroke::new(1.0_f32, border_color), egui::StrokeKind::Inside);

    // 2. Draw search match ticks (Heatmap)
    let match_color = Color32::from_rgb(245, 158, 11); // Amber/Orange
    let active_match_color = Color32::from_rgb(56, 189, 248); // Cyan

    let num_pixels = (rect.height() as usize).max(1);
    let mut painted_pixels = vec![false; num_pixels + 1];

    for m in props.search_matches {
        let ratio = ((m.line_number.saturating_sub(1)) as f32 / total).clamp(0.0, 1.0);
        let px = ((ratio * (rect.height() - 1.0)) as usize).min(num_pixels);
        let is_active = Some(m.line_number) == props.active_match_line;

        if !painted_pixels[px] || is_active {
            painted_pixels[px] = true;
            let tick_y = rect.top() + ratio * rect.height();
            let tick_rect = Rect::from_min_size(
                egui::pos2(rect.left() + 1.0, tick_y - 1.0),
                Vec2::new(ruler_width - 2.0, 2.5),
            );
            let color = if is_active {
                active_match_color
            } else {
                match_color
            };
            ui.painter().rect_filled(tick_rect, 0.5, color);
        }
    }

    // 3. Draw Viewport Thumb (Indicator)
    let current_ratio = ((props.current_line.saturating_sub(1)) as f32 / total).clamp(0.0, 1.0);
    let view_span_ratio = (props.visible_lines_count as f32 / total).clamp(0.02, 1.0);
    let thumb_height = (view_span_ratio * rect.height()).clamp(16.0, rect.height());
    let thumb_y = rect.top() + current_ratio * (rect.height() - thumb_height);

    let thumb_rect = Rect::from_min_size(
        egui::pos2(rect.left() + 1.0, thumb_y),
        Vec2::new(ruler_width - 2.0, thumb_height),
    );
    let thumb_bg = if response.dragged() {
        Color32::from_rgba_premultiplied(56, 189, 248, 140)
    } else if response.hovered() {
        Color32::from_rgba_premultiplied(56, 189, 248, 90)
    } else {
        Color32::from_rgba_premultiplied(100, 116, 139, 70)
    };
    ui.painter().rect_filled(thumb_rect, 2.0, thumb_bg);
    ui.painter().rect_stroke(
        thumb_rect,
        2.0,
        Stroke::new(1.0_f32, Color32::from_rgba_premultiplied(56, 189, 248, 120)),
        egui::StrokeKind::Inside,
    );

    // 4. Handle Click or Drag to Jump
    let mut target_line = None;
    if response.clicked() || response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let rel_y = (pos.y - rect.top()).clamp(0.0, rect.height());
            let click_ratio = rel_y / rect.height();
            let line = (click_ratio * total).round() as usize;
            target_line = Some(line.clamp(1, props.total_lines.max(1)));
        }
    }

    (target_line, response)
}
