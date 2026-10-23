//! Vector icons drawn with egui shapes (original artwork, no external files).
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, pos2, vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Column,
    Beam,
    Plate,
    Bolt,
    Weld,
    BoltDetail,
    Assembly,
    Component,
    ConcreteColumn,
    ConcreteBeam,
    Panel,
    Slab,
    Footing,
    Rebar,
    Move,
    Copy,
    Mirror,
    Rotate,
    Delete,
    WorkPlane,
    View3d,
    Grid,
    WorkArea,
    Show,
    Drawing,
    Report,
    Numbering,
    Clash,
    Ifc,
    Properties,
    Catalog,
    Search,
    Save,
    Undo,
    Redo,
    Help,
    Menu,
    Select,
    SnapEnd,
    SnapMid,
    SnapIntersection,
    SnapPerpendicular,
    SnapNearest,
    SnapReference,
    SnapCenter,
    SnapParallel,
    SnapExtension,
    SnapLine,
    Ortho,
    SelectParts,
    SelectAssemblies,
    SelectBolts,
    SelectWelds,
    SelectComponents,
    SelectGrid,
    SelectLevels,
}

/// Toolbar icon of a snap switch.
pub fn snap(kind: crate::interaction::snap::SnapKind) -> Icon {
    use crate::interaction::snap::SnapKind::*;
    match kind {
        Reference => Icon::SnapReference,
        EndPoint => Icon::SnapEnd,
        Center => Icon::SnapCenter,
        MidPoint => Icon::SnapMid,
        Intersection => Icon::SnapIntersection,
        Perpendicular => Icon::SnapPerpendicular,
        Parallel => Icon::SnapParallel,
        Extension => Icon::SnapExtension,
        Line => Icon::SnapLine,
        Nearest | Free => Icon::SnapNearest,
    }
}

/// Map a unit-square point (0..1) into the icon rect.
fn at(r: Rect, x: f32, y: f32) -> Pos2 {
    pos2(r.min.x + x * r.width(), r.min.y + y * r.height())
}

fn poly(p: &egui::Painter, r: Rect, points: &[(f32, f32)], fill: Color32, stroke: Stroke) {
    let pts: Vec<Pos2> = points.iter().map(|(x, y)| at(r, *x, *y)).collect();
    p.add(egui::Shape::convex_polygon(pts, fill, stroke));
}

fn line(p: &egui::Painter, r: Rect, a: (f32, f32), b: (f32, f32), stroke: Stroke) {
    p.line_segment([at(r, a.0, a.1), at(r, b.0, b.1)], stroke);
}

/// I-section silhouette inside a sub-rect.
fn i_section(p: &egui::Painter, r: Rect, fill: Color32, stroke: Stroke) {
    poly(
        p,
        r,
        &[(0.1, 0.0), (0.9, 0.0), (0.9, 0.18), (0.1, 0.18)],
        fill,
        stroke,
    );
    poly(
        p,
        r,
        &[(0.42, 0.18), (0.58, 0.18), (0.58, 0.82), (0.42, 0.82)],
        fill,
        stroke,
    );
    poly(
        p,
        r,
        &[(0.1, 0.82), (0.9, 0.82), (0.9, 1.0), (0.1, 1.0)],
        fill,
        stroke,
    );
}

pub fn paint(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32, enabled: bool) {
    let ink = if enabled {
        color
    } else {
        color.gamma_multiply(0.45)
    };
    let steel = if enabled {
        Color32::from_rgb(120, 140, 165)
    } else {
        Color32::from_gray(190)
    };
    let concrete = if enabled {
        Color32::from_rgb(150, 150, 150)
    } else {
        Color32::from_gray(205)
    };
    let accent = if enabled {
        Color32::from_rgb(30, 110, 200)
    } else {
        Color32::from_gray(185)
    };
    let s = Stroke::new((rect.width() / 22.).max(1.), ink);
    let thin = Stroke::new((rect.width() / 32.).max(0.8), ink);
    let r = rect.shrink(rect.width() * 0.08);
    let p = painter;
    match icon {
        Icon::Column => {
            let c = Rect::from_min_max(at(r, 0.3, 0.05), at(r, 0.7, 0.95));
            poly(
                p,
                c,
                &[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)],
                steel,
                s,
            );
            line(p, c, (0.5, 0.0), (0.5, 1.0), thin);
        }
        Icon::Beam => {
            poly(
                p,
                r,
                &[(0.05, 0.55), (0.65, 0.25), (0.95, 0.4), (0.35, 0.7)],
                steel,
                s,
            );
            i_section(
                p,
                Rect::from_min_max(at(r, 0.02, 0.5), at(r, 0.4, 0.95)),
                steel,
                thin,
            );
        }
        Icon::Plate => poly(
            p,
            r,
            &[(0.05, 0.6), (0.55, 0.3), (0.95, 0.45), (0.45, 0.8)],
            steel,
            s,
        ),
        Icon::Bolt => {
            poly(
                p,
                r,
                &[(0.3, 0.1), (0.7, 0.1), (0.7, 0.3), (0.3, 0.3)],
                steel,
                s,
            );
            poly(
                p,
                r,
                &[(0.43, 0.3), (0.57, 0.3), (0.57, 0.9), (0.43, 0.9)],
                steel,
                thin,
            );
            for y in [0.45, 0.6, 0.75] {
                line(p, r, (0.43, y), (0.57, y + 0.05), thin);
            }
        }
        Icon::Weld => {
            line(p, r, (0.1, 0.7), (0.9, 0.7), s);
            for k in 0..6 {
                let x = 0.15 + k as f32 * 0.12;
                line(p, r, (x, 0.7), (x + 0.06, 0.45), s);
                line(p, r, (x + 0.06, 0.45), (x + 0.12, 0.7), s);
            }
        }
        Icon::BoltDetail => {
            poly(
                p,
                r,
                &[(0.05, 0.3), (0.95, 0.3), (0.95, 0.5), (0.05, 0.5)],
                steel,
                thin,
            );
            poly(
                p,
                r,
                &[(0.05, 0.5), (0.95, 0.5), (0.95, 0.7), (0.05, 0.7)],
                steel,
                thin,
            );
            for x in [0.3, 0.7] {
                poly(
                    p,
                    r,
                    &[
                        (x - 0.06, 0.18),
                        (x + 0.06, 0.18),
                        (x + 0.06, 0.82),
                        (x - 0.06, 0.82),
                    ],
                    accent,
                    thin,
                );
            }
        }
        Icon::Assembly => {
            i_section(
                p,
                Rect::from_min_max(at(r, 0.05, 0.05), at(r, 0.5, 0.5)),
                steel,
                thin,
            );
            poly(
                p,
                r,
                &[(0.5, 0.55), (0.95, 0.55), (0.95, 0.95), (0.5, 0.95)],
                steel,
                thin,
            );
            line(p, r, (0.3, 0.5), (0.6, 0.6), s);
        }
        Icon::Component => {
            i_section(
                p,
                Rect::from_min_max(at(r, 0.05, 0.1), at(r, 0.45, 0.9)),
                steel,
                thin,
            );
            poly(
                p,
                r,
                &[(0.45, 0.35), (0.6, 0.35), (0.6, 0.65), (0.45, 0.65)],
                accent,
                thin,
            );
            poly(
                p,
                r,
                &[(0.6, 0.42), (0.95, 0.42), (0.95, 0.58), (0.6, 0.58)],
                steel,
                thin,
            );
        }
        Icon::ConcreteColumn => {
            let c = Rect::from_min_max(at(r, 0.32, 0.05), at(r, 0.68, 0.95));
            poly(
                p,
                c,
                &[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)],
                concrete,
                s,
            );
        }
        Icon::ConcreteBeam => poly(
            p,
            r,
            &[
                (0.05, 0.5),
                (0.65, 0.2),
                (0.95, 0.35),
                (0.95, 0.55),
                (0.35, 0.85),
                (0.05, 0.7),
            ],
            concrete,
            s,
        ),
        Icon::Panel => poly(
            p,
            r,
            &[(0.2, 0.15), (0.8, 0.05), (0.8, 0.85), (0.2, 0.95)],
            concrete,
            s,
        ),
        Icon::Slab => poly(
            p,
            r,
            &[(0.05, 0.55), (0.5, 0.3), (0.95, 0.45), (0.5, 0.75)],
            concrete,
            s,
        ),
        Icon::Footing => {
            poly(
                p,
                r,
                &[(0.1, 0.6), (0.9, 0.6), (0.9, 0.9), (0.1, 0.9)],
                concrete,
                s,
            );
            poly(
                p,
                r,
                &[(0.4, 0.15), (0.6, 0.15), (0.6, 0.6), (0.4, 0.6)],
                concrete,
                thin,
            );
        }
        Icon::Rebar => {
            for y in [0.3, 0.5, 0.7] {
                line(p, r, (0.1, y), (0.9, y), Stroke::new(s.width * 1.4, accent));
            }
            line(p, r, (0.1, 0.3), (0.1, 0.7), thin);
        }
        Icon::Move => {
            line(p, r, (0.5, 0.05), (0.5, 0.95), s);
            line(p, r, (0.05, 0.5), (0.95, 0.5), s);
            for (a, b, c) in [
                ((0.5, 0.05), (0.38, 0.2), (0.62, 0.2)),
                ((0.5, 0.95), (0.38, 0.8), (0.62, 0.8)),
                ((0.05, 0.5), (0.2, 0.38), (0.2, 0.62)),
                ((0.95, 0.5), (0.8, 0.38), (0.8, 0.62)),
            ] {
                poly(p, r, &[a, b, c], ink, thin);
            }
        }
        Icon::Copy => {
            poly(
                p,
                r,
                &[(0.1, 0.1), (0.6, 0.1), (0.6, 0.6), (0.1, 0.6)],
                Color32::WHITE,
                s,
            );
            poly(
                p,
                r,
                &[(0.4, 0.4), (0.9, 0.4), (0.9, 0.9), (0.4, 0.9)],
                steel,
                s,
            );
        }
        Icon::Mirror => {
            line(
                p,
                r,
                (0.5, 0.0),
                (0.5, 1.0),
                Stroke::new(thin.width, accent),
            );
            poly(p, r, &[(0.1, 0.2), (0.4, 0.5), (0.1, 0.8)], steel, thin);
            poly(
                p,
                r,
                &[(0.9, 0.2), (0.6, 0.5), (0.9, 0.8)],
                Color32::WHITE,
                thin,
            );
        }
        Icon::Rotate => {
            let c = at(r, 0.5, 0.5);
            let radius = r.width() * 0.36;
            let pts: Vec<Pos2> = (0..=20)
                .map(|k| {
                    let a = 0.3 + k as f32 * 0.25;
                    c + vec2(a.cos(), a.sin()) * radius
                })
                .collect();
            p.add(egui::Shape::line(pts, s));
            poly(p, r, &[(0.78, 0.35), (0.95, 0.5), (0.7, 0.55)], ink, thin);
        }
        Icon::Delete => {
            line(
                p,
                r,
                (0.2, 0.2),
                (0.8, 0.8),
                Stroke::new(s.width * 1.5, Color32::from_rgb(200, 50, 50)),
            );
            line(
                p,
                r,
                (0.8, 0.2),
                (0.2, 0.8),
                Stroke::new(s.width * 1.5, Color32::from_rgb(200, 50, 50)),
            );
        }
        Icon::WorkPlane => {
            poly(
                p,
                r,
                &[(0.05, 0.6), (0.55, 0.35), (0.95, 0.5), (0.45, 0.8)],
                Color32::from_rgba_unmultiplied(120, 180, 240, 120),
                thin,
            );
            line(
                p,
                r,
                (0.45, 0.6),
                (0.45, 0.1),
                Stroke::new(s.width, Color32::from_rgb(40, 140, 60)),
            );
        }
        Icon::View3d => {
            poly(
                p,
                r,
                &[(0.5, 0.05), (0.92, 0.28), (0.5, 0.5), (0.08, 0.28)],
                Color32::from_gray(230),
                thin,
            );
            poly(
                p,
                r,
                &[(0.08, 0.28), (0.5, 0.5), (0.5, 0.95), (0.08, 0.72)],
                steel,
                thin,
            );
            poly(
                p,
                r,
                &[(0.5, 0.5), (0.92, 0.28), (0.92, 0.72), (0.5, 0.95)],
                Color32::from_gray(160),
                thin,
            );
        }
        Icon::Grid => {
            for k in 0..4 {
                let v = 0.15 + k as f32 * 0.23;
                line(p, r, (v, 0.05), (v, 0.95), thin);
                line(p, r, (0.05, v), (0.95, v), thin);
            }
        }
        Icon::WorkArea => {
            let g = Stroke::new(thin.width, Color32::from_rgb(40, 160, 70));
            for (a, b) in [
                ((0.1, 0.3), (0.6, 0.1)),
                ((0.6, 0.1), (0.9, 0.3)),
                ((0.9, 0.3), (0.4, 0.5)),
                ((0.4, 0.5), (0.1, 0.3)),
                ((0.1, 0.3), (0.1, 0.75)),
                ((0.4, 0.5), (0.4, 0.95)),
                ((0.9, 0.3), (0.9, 0.75)),
                ((0.1, 0.75), (0.4, 0.95)),
                ((0.4, 0.95), (0.9, 0.75)),
            ] {
                line(p, r, a, b, g);
            }
        }
        Icon::Show => {
            let c = at(r, 0.5, 0.5);
            p.circle_stroke(c, r.width() * 0.4, s);
            p.circle_filled(c, r.width() * 0.15, ink);
        }
        Icon::Drawing => {
            poly(
                p,
                r,
                &[(0.15, 0.05), (0.85, 0.05), (0.85, 0.95), (0.15, 0.95)],
                Color32::WHITE,
                s,
            );
            line(p, r, (0.25, 0.3), (0.75, 0.3), thin);
            line(p, r, (0.25, 0.5), (0.75, 0.5), thin);
            poly(
                p,
                r,
                &[(0.5, 0.7), (0.85, 0.7), (0.85, 0.95), (0.5, 0.95)],
                steel,
                thin,
            );
        }
        Icon::Report => {
            poly(
                p,
                r,
                &[(0.1, 0.1), (0.9, 0.1), (0.9, 0.9), (0.1, 0.9)],
                Color32::WHITE,
                s,
            );
            for y in [0.3, 0.5, 0.7] {
                line(p, r, (0.1, y), (0.9, y), thin);
            }
            line(p, r, (0.4, 0.1), (0.4, 0.9), thin);
        }
        Icon::Numbering => {
            line(p, r, (0.35, 0.1), (0.25, 0.9), s);
            line(p, r, (0.7, 0.1), (0.6, 0.9), s);
            line(p, r, (0.1, 0.35), (0.9, 0.35), s);
            line(p, r, (0.08, 0.65), (0.88, 0.65), s);
        }
        Icon::Clash => {
            poly(
                p,
                r,
                &[(0.05, 0.3), (0.6, 0.3), (0.6, 0.6), (0.05, 0.6)],
                steel,
                thin,
            );
            poly(
                p,
                r,
                &[(0.4, 0.45), (0.95, 0.45), (0.95, 0.75), (0.4, 0.75)],
                Color32::from_rgba_unmultiplied(220, 80, 60, 200),
                thin,
            );
        }
        Icon::Ifc => {
            p.circle_stroke(at(r, 0.5, 0.5), r.width() * 0.42, s);
            line(p, r, (0.15, 0.5), (0.85, 0.5), thin);
            line(p, r, (0.5, 0.1), (0.5, 0.9), thin);
        }
        Icon::Properties => {
            for (k, y) in [0.2, 0.5, 0.8].into_iter().enumerate() {
                line(p, r, (0.1, y), (0.9, y), thin);
                let x = [0.3, 0.65, 0.45][k];
                p.circle_filled(at(r, x, y), r.width() * 0.09, accent);
            }
        }
        Icon::Catalog => {
            poly(
                p,
                r,
                &[(0.15, 0.1), (0.75, 0.1), (0.75, 0.9), (0.15, 0.9)],
                steel,
                s,
            );
            line(p, r, (0.85, 0.15), (0.85, 0.95), s);
        }
        Icon::Search => {
            p.circle_stroke(at(r, 0.42, 0.42), r.width() * 0.28, s);
            line(
                p,
                r,
                (0.62, 0.62),
                (0.92, 0.92),
                Stroke::new(s.width * 1.4, ink),
            );
        }
        Icon::Save => {
            poly(
                p,
                r,
                &[(0.1, 0.1), (0.8, 0.1), (0.9, 0.2), (0.9, 0.9), (0.1, 0.9)],
                ink,
                Stroke::NONE,
            );
            poly(
                p,
                r,
                &[(0.25, 0.1), (0.7, 0.1), (0.7, 0.35), (0.25, 0.35)],
                Color32::WHITE,
                Stroke::NONE,
            );
            poly(
                p,
                r,
                &[(0.22, 0.55), (0.78, 0.55), (0.78, 0.9), (0.22, 0.9)],
                Color32::WHITE,
                Stroke::NONE,
            );
        }
        Icon::Undo | Icon::Redo => {
            let flip = if icon == Icon::Undo { 1.0 } else { -1.0 };
            let x = |v: f32| if flip > 0. { v } else { 1.0 - v };
            let c = at(r, 0.5, 0.6);
            let radius = r.width() * 0.3;
            let pts: Vec<Pos2> = (0..=16)
                .map(|k| {
                    let a = std::f32::consts::PI + k as f32 * std::f32::consts::PI / 16.;
                    c + vec2(-flip * a.cos(), a.sin()) * radius
                })
                .collect();
            p.add(egui::Shape::line(pts, s));
            poly(
                p,
                r,
                &[(x(0.05), 0.45), (x(0.35), 0.45), (x(0.2), 0.7)],
                ink,
                Stroke::NONE,
            );
        }
        Icon::Help => {
            p.circle_filled(at(r, 0.5, 0.5), r.width() * 0.45, accent);
            p.text(
                at(r, 0.5, 0.52),
                egui::Align2::CENTER_CENTER,
                "?",
                egui::FontId::proportional(r.height() * 0.7),
                Color32::WHITE,
            );
        }
        Icon::Menu => {
            for y in [0.25, 0.5, 0.75] {
                line(p, r, (0.15, y), (0.85, y), s);
            }
        }
        Icon::Select => poly(
            p,
            r,
            &[
                (0.2, 0.05),
                (0.8, 0.6),
                (0.5, 0.62),
                (0.65, 0.95),
                (0.55, 0.98),
                (0.4, 0.65),
                (0.2, 0.85),
            ],
            ink,
            Stroke::NONE,
        ),
        Icon::SnapEnd => poly(
            p,
            r,
            &[(0.2, 0.2), (0.8, 0.2), (0.8, 0.8), (0.2, 0.8)],
            Color32::TRANSPARENT,
            s,
        ),
        Icon::SnapMid => poly(
            p,
            r,
            &[(0.5, 0.15), (0.88, 0.82), (0.12, 0.82)],
            Color32::TRANSPARENT,
            s,
        ),
        Icon::SnapIntersection => {
            line(p, r, (0.15, 0.15), (0.85, 0.85), s);
            line(p, r, (0.85, 0.15), (0.15, 0.85), s);
        }
        Icon::SnapPerpendicular => {
            line(p, r, (0.15, 0.85), (0.85, 0.85), s);
            line(p, r, (0.5, 0.15), (0.5, 0.85), s);
            poly(
                p,
                r,
                &[(0.5, 0.7), (0.65, 0.7), (0.65, 0.85), (0.5, 0.85)],
                Color32::TRANSPARENT,
                thin,
            );
        }
        Icon::SnapNearest => {
            line(p, r, (0.1, 0.8), (0.9, 0.2), s);
            p.circle_stroke(at(r, 0.5, 0.5), r.width() * 0.15, s);
        }
        Icon::SnapReference => {
            p.circle_stroke(at(r, 0.5, 0.5), r.width() * 0.32, s);
            line(p, r, (0.5, 0.05), (0.5, 0.95), thin);
            line(p, r, (0.05, 0.5), (0.95, 0.5), thin);
        }
        Icon::SnapCenter => {
            p.circle_stroke(at(r, 0.5, 0.5), r.width() * 0.35, s);
            p.circle_filled(at(r, 0.5, 0.5), r.width() * 0.08, ink);
        }
        Icon::SnapParallel => {
            line(p, r, (0.1, 0.65), (0.6, 0.15), s);
            line(p, r, (0.4, 0.85), (0.9, 0.35), s);
        }
        Icon::SnapExtension => {
            line(p, r, (0.1, 0.9), (0.45, 0.55), s);
            for k in 0..3 {
                let a = 0.55 + k as f32 * 0.13;
                line(p, r, (a, 1. - a), (a + 0.07, 0.93 - a), thin);
            }
        }
        Icon::SnapLine => {
            line(p, r, (0.05, 0.5), (0.95, 0.5), s);
            line(p, r, (0.5, 0.3), (0.5, 0.7), thin);
        }
        Icon::Ortho => {
            line(p, r, (0.15, 0.85), (0.85, 0.85), s);
            line(p, r, (0.15, 0.85), (0.15, 0.15), s);
            line(p, r, (0.15, 0.85), (0.7, 0.3), thin);
        }
        Icon::SelectParts => i_section(p, r.shrink(r.width() * 0.1), steel, thin),
        Icon::SelectAssemblies => {
            i_section(
                p,
                Rect::from_min_max(at(r, 0.0, 0.0), at(r, 0.55, 0.55)),
                steel,
                thin,
            );
            i_section(
                p,
                Rect::from_min_max(at(r, 0.45, 0.45), at(r, 1.0, 1.0)),
                steel,
                thin,
            );
        }
        Icon::SelectBolts => {
            for x in [0.3, 0.7] {
                poly(
                    p,
                    r,
                    &[
                        (x - 0.12, 0.2),
                        (x + 0.12, 0.2),
                        (x + 0.12, 0.35),
                        (x - 0.12, 0.35),
                    ],
                    steel,
                    thin,
                );
                line(p, r, (x, 0.35), (x, 0.9), s);
            }
        }
        Icon::SelectWelds => {
            for k in 0..4 {
                let x = 0.12 + k as f32 * 0.2;
                line(p, r, (x, 0.75), (x + 0.1, 0.35), s);
                line(p, r, (x + 0.1, 0.35), (x + 0.2, 0.75), s);
            }
        }
        Icon::SelectComponents => {
            // Two parts meeting at a component symbol.
            line(p, r, (0.1, 0.8), (0.9, 0.8), thin);
            line(p, r, (0.5, 0.8), (0.5, 0.1), thin);
            p.circle_stroke(at(r, 0.5, 0.55), r.width() * 0.16, s);
        }
        Icon::SelectGrid => {
            line(p, r, (0.3, 0.05), (0.3, 0.95), Stroke::new(thin.width, ink));
            line(p, r, (0.05, 0.6), (0.95, 0.6), thin);
            p.circle_stroke(at(r, 0.3, 0.12), r.width() * 0.1, thin);
        }
        Icon::SelectLevels => {
            // Two level lines with the level mark triangle.
            line(p, r, (0.05, 0.35), (0.95, 0.35), thin);
            line(p, r, (0.05, 0.8), (0.95, 0.8), thin);
            poly(
                p,
                r,
                &[(0.2, 0.15), (0.4, 0.15), (0.3, 0.35)],
                Color32::TRANSPARENT,
                thin,
            );
            poly(
                p,
                r,
                &[(0.2, 0.6), (0.4, 0.6), (0.3, 0.8)],
                Color32::TRANSPARENT,
                thin,
            );
        }
    }
}

/// Small ribbon button: 16 px icon and the caption on one line (the ribbon
/// stacks three of them in a column).
pub fn small_button(
    ui: &mut egui::Ui,
    icon: Icon,
    name: &str,
    tooltip: &str,
    enabled: bool,
    selected: bool,
) -> egui::Response {
    // Long captions wrap onto two lines so three buttons still stack under
    // one large button and the ribbon stays narrow.
    let caption = two_lines(ui, name, &egui::FontId::proportional(12.), 90.);
    let font = egui::FontId::proportional(if caption.contains('\n') { 11. } else { 12. });
    let galley = ui
        .painter()
        .layout_no_wrap(caption.clone(), font.clone(), Color32::BLACK);
    let (rect, response) = ui.allocate_exact_size(
        vec2(galley.size().x + 28., galley.size().y.max(16.) + 4.),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    let response = response.on_hover_text(tooltip);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, selected, name)
    });
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact_selectable(&response, selected);
        if selected || response.hovered() || response.has_focus() {
            ui.painter().rect(
                rect,
                3.,
                visuals.weak_bg_fill,
                visuals.bg_stroke,
                egui::StrokeKind::Inside,
            );
        }
        let ink = ui.visuals().widgets.inactive.fg_stroke.color;
        let icon_rect =
            Rect::from_min_size(pos2(rect.min.x + 4., rect.center().y - 8.), vec2(16., 16.));
        paint(ui.painter(), icon_rect, icon, ink, enabled);
        let color = if enabled {
            ui.visuals().text_color()
        } else {
            ui.visuals().weak_text_color()
        };
        ui.painter().text(
            pos2(rect.min.x + 24., rect.center().y),
            egui::Align2::LEFT_CENTER,
            caption,
            font,
            color,
        );
    }
    response
}

/// The caption, broken at the space nearest its middle when it is wider
/// than `max` points.
pub fn two_lines(ui: &egui::Ui, text: &str, font: &egui::FontId, max: f32) -> String {
    let width = |t: &str| {
        ui.painter()
            .layout_no_wrap(t.to_string(), font.clone(), Color32::BLACK)
            .size()
            .x
    };
    if width(text) <= max || !text.contains(' ') {
        return text.to_string();
    }
    let middle = text.chars().count() / 2;
    let split = text
        .char_indices()
        .filter(|(_, c)| *c == ' ')
        .min_by_key(|(i, _)| text[..*i].chars().count().abs_diff(middle))
        .map(|(i, _)| i)
        .unwrap_or(0);
    format!("{}\n{}", &text[..split], &text[split + 1..])
}

/// Icon-only (or icon + caption) button with an accessible name and tooltip.
#[allow(clippy::too_many_arguments)]
pub fn button(
    ui: &mut egui::Ui,
    icon: Icon,
    size: f32,
    caption: Option<&str>,
    name: &str,
    tooltip: &str,
    enabled: bool,
    selected: bool,
) -> egui::Response {
    let font = egui::FontId::proportional(12.5);
    let caption = caption.map(|c| two_lines(ui, c, &font, 70.));
    let caption = caption.as_deref();
    let caption_size = caption.map(|c| {
        ui.painter()
            .layout_no_wrap(c.to_string(), font.clone(), Color32::BLACK)
            .size()
    });
    let width = caption_size.map_or(size + 8., |s| (s.x + 10.).max(size + 8.));
    let height = size + 6. + caption_size.map_or(0., |s| s.y + 2.);
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), sense);
    let response = response.on_hover_text(tooltip);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, selected, name)
    });
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact_selectable(&response, selected);
        if selected || response.hovered() || response.has_focus() {
            ui.painter().rect(
                rect,
                3.,
                visuals.weak_bg_fill,
                visuals.bg_stroke,
                egui::StrokeKind::Inside,
            );
        }
        let icon_rect = Rect::from_min_size(
            pos2(rect.center().x - size / 2., rect.min.y + 3.),
            vec2(size, size),
        );
        let ink = ui.visuals().widgets.inactive.fg_stroke.color;
        paint(ui.painter(), icon_rect, icon, ink, enabled);
        if let Some(c) = caption {
            let color = if enabled {
                ui.visuals().text_color()
            } else {
                ui.visuals().weak_text_color()
            };
            ui.painter().text(
                pos2(rect.center().x, icon_rect.max.y + 2.),
                egui::Align2::CENTER_TOP,
                c,
                font,
                color,
            );
        }
    }
    response
}
