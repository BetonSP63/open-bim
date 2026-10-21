//! Paints the renderer's screen-space overlay marks and the viewport
//! background gradient with egui.
use crate::interaction::snap::SnapKind;
use eframe::egui;
use open_bim_renderer::overlay::{Glyph, Mark, Rgba};

fn color(c: Rgba) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

fn pos(p: [f64; 2]) -> egui::Pos2 {
    egui::pos2(p[0] as f32, p[1] as f32)
}

pub fn glyph(kind: SnapKind) -> Glyph {
    match kind {
        SnapKind::Reference => Glyph::Reference,
        SnapKind::EndPoint => Glyph::End,
        SnapKind::MidPoint => Glyph::Mid,
        SnapKind::Intersection => Glyph::Intersection,
        SnapKind::Perpendicular => Glyph::Perpendicular,
        SnapKind::Nearest => Glyph::Nearest,
        SnapKind::Center => Glyph::Center,
        SnapKind::Parallel => Glyph::Parallel,
        SnapKind::Extension => Glyph::Extension,
        SnapKind::Line => Glyph::Line,
        SnapKind::Free => Glyph::Free,
    }
}

/// Vertical two-colour gradient filling `rect` (drawn under the 3D scene).
pub fn gradient(painter: &egui::Painter, rect: egui::Rect, colors: [Rgba; 2]) {
    let mut mesh = egui::Mesh::default();
    let (top, bottom) = (color(colors[0]), color(colors[1]));
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

pub fn paint(painter: &egui::Painter, marks: &[Mark]) {
    for m in marks {
        match m {
            Mark::Line {
                a,
                b,
                color: c,
                width,
                dash,
            } => {
                let stroke = egui::Stroke::new(*width, color(*c));
                match dash {
                    Some([d, g]) => painter.extend(egui::Shape::dashed_line(
                        &[pos(*a), pos(*b)],
                        stroke,
                        *d,
                        *g,
                    )),
                    None => {
                        painter.line_segment([pos(*a), pos(*b)], stroke);
                    }
                }
            }
            Mark::Circle {
                center,
                radius,
                color: c,
                fill,
            } => {
                if let Some(f) = fill {
                    painter.circle_filled(pos(*center), *radius, color(*f));
                }
                painter.circle_stroke(pos(*center), *radius, egui::Stroke::new(1.2, color(*c)));
            }
            Mark::Square {
                center,
                half,
                color: c,
                fill,
            } => {
                let r =
                    egui::Rect::from_center_size(pos(*center), egui::vec2(half * 2., half * 2.));
                if *fill {
                    painter.rect_filled(r, 0., color(*c));
                    painter.rect_stroke(
                        r,
                        0.,
                        egui::Stroke::new(1., egui::Color32::BLACK),
                        egui::StrokeKind::Outside,
                    );
                } else {
                    painter.rect_stroke(
                        r,
                        0.,
                        egui::Stroke::new(2., color(*c)),
                        egui::StrokeKind::Middle,
                    );
                }
            }
            Mark::Fill { points, color: c } => {
                let mut mesh = egui::Mesh::default();
                for q in points {
                    mesh.colored_vertex(pos(*q), color(*c));
                }
                for t in crate::interaction::polygon::triangulate(points) {
                    mesh.add_triangle(t[0] as u32, t[1] as u32, t[2] as u32);
                }
                painter.add(egui::Shape::mesh(mesh));
            }
            Mark::Text {
                at,
                text,
                color: c,
                align,
                size,
            } => {
                let a = |v: i8| match v {
                    -1 => egui::Align::Min,
                    0 => egui::Align::Center,
                    _ => egui::Align::Max,
                };
                painter.text(
                    pos(*at),
                    egui::Align2([a(align[0]), a(align[1])]),
                    text,
                    egui::FontId::proportional(*size),
                    color(*c),
                );
            }
        }
    }
}

/// Component symbols: a filled circle in the status colour (green created,
/// yellow with remarks, red not created), a ring when selected, and the
/// connection code or name beside it.
pub fn paint_symbols(
    painter: &egui::Painter,
    symbols: &[crate::interaction::component_symbols::Symbol],
    light: bool,
) {
    use crate::interaction::component_symbols::Status;
    let ink = if light {
        egui::Color32::from_gray(30)
    } else {
        egui::Color32::from_gray(230)
    };
    for s in symbols {
        let at = egui::pos2(s.at[0] as f32, s.at[1] as f32);
        let fill = match s.status {
            Status::Created => egui::Color32::from_rgb(40, 170, 70),
            Status::Remarks(_) => egui::Color32::from_rgb(235, 190, 30),
            Status::Failed => egui::Color32::from_rgb(220, 50, 40),
        };
        painter.circle_filled(at, 5., fill);
        painter.circle_stroke(at, 5., egui::Stroke::new(1., ink));
        if s.selected {
            painter.circle_stroke(
                at,
                8.5,
                egui::Stroke::new(2., egui::Color32::from_rgb(255, 0, 200)),
            );
        }
        painter.text(
            at + egui::vec2(9., -9.),
            egui::Align2::LEFT_BOTTOM,
            &s.label,
            egui::FontId::proportional(11.),
            ink,
        );
    }
}
