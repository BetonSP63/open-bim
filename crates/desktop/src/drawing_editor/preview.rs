//! Sheet preview painted with egui using the same font as PDF export.
use eframe::egui;
use open_bim_drawings::{Align, Label, LineStyle, Polyline, Sheet};

#[derive(Clone, Debug, Default)]
pub struct PreviewState {
    pub zoom: f32,
    pub pan: egui::Vec2,
    /// View being dragged and its accumulated paper offset.
    pub drag: Option<(u32, [f64; 2])>,
}

pub enum PreviewEvent {
    SelectView(u32),
    MoveView(u32, [f64; 2]),
    /// Click on empty paper at sheet coordinates.
    Paper([f64; 2]),
}

struct Mapping {
    origin: egui::Pos2,
    scale: f32,
    height: f64,
}
impl Mapping {
    fn screen(&self, p: [f64; 2]) -> egui::Pos2 {
        egui::pos2(
            self.origin.x + p[0] as f32 * self.scale,
            self.origin.y + (self.height - p[1]) as f32 * self.scale,
        )
    }
    fn paper(&self, s: egui::Pos2) -> [f64; 2] {
        [
            ((s.x - self.origin.x) / self.scale) as f64,
            self.height - ((s.y - self.origin.y) / self.scale) as f64,
        ]
    }
}

fn stroke(style: LineStyle, scale: f32) -> egui::Stroke {
    let (_, width, _) = open_bim_drawings::svg::stroke(style);
    let color = if style == LineStyle::Lost {
        egui::Color32::from_rgb(210, 0, 0)
    } else {
        egui::Color32::BLACK
    };
    egui::Stroke::new((width as f32 * scale).max(0.6), color)
}

fn polyline(painter: &egui::Painter, map: &Mapping, line: &Polyline, offset: [f64; 2]) {
    let points: Vec<egui::Pos2> = line
        .points
        .iter()
        .map(|p| map.screen([p[0] + offset[0], p[1] + offset[1]]))
        .collect();
    let stroke = stroke(line.style, map.scale);
    if line.style == LineStyle::Hidden {
        painter.extend(egui::Shape::dashed_line(
            &points,
            stroke,
            3. * map.scale,
            1.5 * map.scale,
        ));
    } else {
        painter.add(egui::Shape::line(points, stroke));
    }
}

fn label(painter: &egui::Painter, map: &Mapping, l: &Label, offset: [f64; 2]) {
    let size = (l.height as f32 * map.scale).max(1.);
    let color = if l.lost {
        egui::Color32::from_rgb(210, 0, 0)
    } else {
        egui::Color32::BLACK
    };
    let galley = painter.layout_no_wrap(l.text.clone(), egui::FontId::proportional(size), color);
    let width = (galley.size().x / map.scale) as f64;
    let shift = match l.align {
        Align::Start => 0.,
        Align::Middle => -width / 2.,
        Align::End => -width,
    };
    let (s, c) = l.rotation.to_radians().sin_cos();
    let ascent = l.height * 0.93;
    let top_left = [
        l.position[0] + offset[0] + shift * c - ascent * s,
        l.position[1] + offset[1] + shift * s + ascent * c,
    ];
    painter.add(
        egui::epaint::TextShape::new(map.screen(top_left), galley, color)
            .with_angle(-(l.rotation.to_radians() as f32)),
    );
}

/// Paint the sheet; returns the user's selection or drag result.
pub fn show(
    ui: &mut egui::Ui,
    sheet: &Sheet,
    state: &mut PreviewState,
    selected_view: Option<u32>,
) -> Option<PreviewEvent> {
    let (rect, response) =
        ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
    if state.zoom <= 0. {
        state.zoom = 1.;
    }
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0. {
            state.zoom = (state.zoom * (1. + scroll * 0.002)).clamp(0.2, 20.);
        }
    }
    if response.dragged_by(egui::PointerButton::Secondary)
        || response.dragged_by(egui::PointerButton::Middle)
    {
        state.pan += response.drag_delta();
    }
    let [w, h] = sheet.size;
    let fit = (rect.width() / w as f32).min(rect.height() / h as f32) * 0.96;
    let scale = fit * state.zoom;
    let size = egui::vec2(w as f32 * scale, h as f32 * scale);
    let map = Mapping {
        origin: rect.center() - size / 2. + state.pan,
        scale,
        height: h,
    };
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0., egui::Color32::from_gray(60));
    painter.rect_filled(
        egui::Rect::from_min_size(map.origin, size),
        0.,
        egui::Color32::WHITE,
    );
    let mut event = None;
    let pointer = response.interact_pointer_pos();
    // Primary drag moves the view under the pointer.
    if response.drag_started_by(egui::PointerButton::Primary)
        && let Some(p) = ui.input(|i| i.pointer.press_origin()).or(pointer)
    {
        let at = map.paper(p);
        if let Some(view) = sheet.views.iter().find(|v| {
            at[0] >= v.bounds[0][0] - 5.
                && at[0] <= v.bounds[1][0] + 5.
                && at[1] >= v.bounds[0][1] - 5.
                && at[1] <= v.bounds[1][1] + 5.
        }) {
            state.drag = Some((view.id, [0., 0.]));
        }
    }
    if let Some((_, offset)) = &mut state.drag
        && response.dragged_by(egui::PointerButton::Primary)
    {
        // The first drag frame already contains the movement past the threshold.
        let d = if response.drag_started() {
            response.interact_pointer_pos().unwrap_or_default()
                - ui.input(|i| i.pointer.press_origin()).unwrap_or_default()
        } else {
            response.drag_delta()
        };
        offset[0] += (d.x / scale) as f64;
        offset[1] -= (d.y / scale) as f64;
    }
    if response.drag_stopped()
        && let Some((view, offset)) = state.drag.take()
        && offset[0].hypot(offset[1]) > 0.5
    {
        event = Some(PreviewEvent::MoveView(view, offset));
    }
    if response.clicked()
        && let Some(p) = pointer
    {
        let at = map.paper(p);
        event = Some(
            sheet
                .views
                .iter()
                .find(|v| {
                    at[0] >= v.bounds[0][0]
                        && at[0] <= v.bounds[1][0]
                        && at[1] >= v.bounds[0][1]
                        && at[1] <= v.bounds[1][1]
                })
                .map_or(PreviewEvent::Paper(at), |v| PreviewEvent::SelectView(v.id)),
        );
    }
    let dragging = state.drag;
    let shift = |view: u32| match dragging {
        Some((id, offset)) if id == view => offset,
        _ => [0., 0.],
    };
    for line in &sheet.frame {
        polyline(&painter, &map, line, [0., 0.]);
    }
    for f in &sheet.fills {
        let (a, b) = (map.screen([f[0], f[1]]), map.screen([f[2], f[3]]));
        painter.rect_filled(egui::Rect::from_two_pos(a, b), 0., egui::Color32::BLACK);
    }
    for view in &sheet.views {
        let offset = shift(view.id);
        for line in &view.lines {
            polyline(&painter, &map, line, offset);
        }
        for l in &view.labels {
            label(&painter, &map, l, offset);
        }
        if selected_view == Some(view.id) {
            let a = map.screen([
                view.bounds[0][0] - 3. + offset[0],
                view.bounds[0][1] - 3. + offset[1],
            ]);
            let b = map.screen([
                view.bounds[1][0] + 3. + offset[0],
                view.bounds[1][1] + 3. + offset[1],
            ]);
            painter.rect_stroke(
                egui::Rect::from_two_pos(a, b),
                0.,
                egui::Stroke::new(1.5, egui::Color32::from_rgb(30, 110, 230)),
                egui::StrokeKind::Outside,
            );
        }
    }
    for dim in &sheet.dimensions {
        let offset = shift(dim.view);
        for line in &dim.lines {
            polyline(&painter, &map, line, offset);
        }
        for l in &dim.labels {
            label(&painter, &map, l, offset);
        }
    }
    for l in &sheet.labels {
        label(&painter, &map, l, [0., 0.]);
    }
    event
}
