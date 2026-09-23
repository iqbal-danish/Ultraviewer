use eframe::egui::{self, Color32, Rect, Response, Sense, Ui, Vec2};
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
    // Track blends subtly into editor background (VS Code dark style)
    let track_bg = Color32::from_rgb(26, 30, 35);

    // 1. Draw subtle track
    ui.painter().rect_filled(rect, 0.0, track_bg);

    // 2. Draw search match ticks (Heatmap)
    let match_color = Color32::from_rgba_unmultiplied(65, 95, 140, 190); // Muted slate-blue
    let active_match_color = Color32::from_rgba_unmultiplied(97, 175, 239, 255); // Soft sky blue

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

    // 3. Draw Viewport Thumb (VS Code translucent aesthetic slider)
    let max_h = rect.height().max(1.0);
    let view_span_ratio = (props.visible_lines_count as f32 / total).clamp(0.01, 1.0);
    let min_h = 24.0_f32.min(max_h);
    let thumb_height = (view_span_ratio * max_h).clamp(min_h, max_h);
    let track_travel = (max_h - thumb_height).max(0.0);

    let view_lines = props.visible_lines_count.max(1);
    let max_top_line = props.total_lines.saturating_sub(view_lines.saturating_sub(2)).max(1) as f32;
    let current_ratio = if track_travel <= 0.0 {
        0.0
    } else {
        ((props.current_line.saturating_sub(1)) as f32 / max_top_line).clamp(0.0, 1.0)
    };
    let thumb_y = rect.top() + current_ratio * track_travel;

    let thumb_rect = Rect::from_min_size(
        egui::pos2(rect.left() + 2.0, thumb_y),
        Vec2::new(ruler_width - 4.0, thumb_height),
    );
    let thumb_bg = if response.dragged() {
        Color32::from_rgba_unmultiplied(190, 190, 190, 160)
    } else if response.hovered() {
        Color32::from_rgba_unmultiplied(160, 160, 160, 120)
    } else {
        Color32::from_rgba_unmultiplied(120, 120, 120, 70)
    };
    ui.painter().rect_filled(thumb_rect, 3.0, thumb_bg);

    // 4. Handle Click or Drag to Jump smoothly without offset jumping
    let mut target_line = None;
    if response.clicked() || response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let rel_y = if track_travel <= 0.0 {
                0.0
            } else {
                (pos.y - rect.top() - thumb_height * 0.5).clamp(0.0, track_travel)
            };
            let scroll_ratio = if track_travel <= 0.0 {
                0.0
            } else {
                rel_y / track_travel
            };
            let line = (scroll_ratio * max_top_line).round() as usize + 1;
            target_line = Some(line.clamp(1, props.total_lines.max(1)));
        }
    }

    (target_line, response)
}
