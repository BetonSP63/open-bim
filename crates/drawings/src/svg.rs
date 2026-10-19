//! Deterministic SVG in paper millimetres (y up in the sheet, flipped here).
use crate::{Align, Label, LineStyle, Polyline, Sheet};
use std::fmt::Write;

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Fixed 3-decimal coordinates keep reference files stable.
fn n(value: f64) -> String {
    let v = (value * 1000.).round() / 1000.;
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

pub fn stroke(style: LineStyle) -> (&'static str, f64, &'static str) {
    match style {
        LineStyle::Visible => ("#000", 0.5, ""),
        LineStyle::Hidden => ("#000", 0.25, " stroke-dasharray=\"3 1.5\""),
        LineStyle::Section => ("#000", 0.7, ""),
        LineStyle::Hatch => ("#000", 0.18, ""),
        LineStyle::Thin => ("#000", 0.25, ""),
        LineStyle::Frame => ("#000", 0.7, ""),
        LineStyle::Lost => ("#d00", 0.35, " stroke-dasharray=\"1 1\""),
    }
}

fn polyline(out: &mut String, line: &Polyline, height: f64) {
    let (color, width, dash) = stroke(line.style);
    let points: Vec<String> = line
        .points
        .iter()
        .map(|p| format!("{},{}", n(p[0]), n(height - p[1])))
        .collect();
    let object = line
        .object
        .map(|id| format!(" data-object=\"{}\"", id.0))
        .unwrap_or_default();
    let _ = writeln!(
        out,
        "<polyline class=\"{:?}\"{object} points=\"{}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{}\"{dash}/>",
        line.style,
        points.join(" "),
        n(width)
    );
}

fn label(out: &mut String, l: &Label, height: f64) {
    let anchor = match l.align {
        Align::Start => "start",
        Align::Middle => "middle",
        Align::End => "end",
    };
    let (x, y) = (n(l.position[0]), n(height - l.position[1]));
    let rotate = if l.rotation.abs() > 1e-9 {
        format!(" transform=\"rotate({} {x} {y})\"", n(-l.rotation))
    } else {
        String::new()
    };
    let object = l
        .object
        .map(|id| format!(" data-object=\"{}\"", id.0))
        .unwrap_or_default();
    let fill = if l.lost { "#d00" } else { "#000" };
    let _ = writeln!(
        out,
        "<text x=\"{x}\" y=\"{y}\" font-size=\"{}\" text-anchor=\"{anchor}\" fill=\"{fill}\"{rotate}{object}>{}</text>",
        n(l.height),
        escape(&l.text)
    );
}

pub fn render(sheet: &Sheet) -> String {
    let [w, h] = sheet.size;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}mm\" height=\"{}mm\" viewBox=\"0 0 {} {}\" font-family=\"DejaVu Sans, Arial, sans-serif\" data-drawing=\"{}\">",
        n(w),
        n(h),
        n(w),
        n(h),
        sheet.drawing.0
    );
    let _ = writeln!(
        out,
        "<rect width=\"{}\" height=\"{}\" fill=\"#fff\"/>",
        n(w),
        n(h)
    );
    for line in &sheet.frame {
        polyline(&mut out, line, h);
    }
    for f in &sheet.fills {
        let _ = writeln!(
            out,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#000\"/>",
            n(f[0]),
            n(h - f[3]),
            n(f[2] - f[0]),
            n(f[3] - f[1])
        );
    }
    for view in &sheet.views {
        let _ = writeln!(out, "<g class=\"view\" data-view=\"{}\">", view.id);
        // Hidden lines first so visible outlines stay on top.
        for style in [
            LineStyle::Hidden,
            LineStyle::Hatch,
            LineStyle::Visible,
            LineStyle::Section,
            LineStyle::Thin,
        ] {
            for line in view.lines.iter().filter(|l| l.style == style) {
                polyline(&mut out, line, h);
            }
        }
        for l in &view.labels {
            label(&mut out, l, h);
        }
        let _ = writeln!(out, "</g>");
    }
    for dim in &sheet.dimensions {
        let lost = if dim.lost { " data-lost=\"true\"" } else { "" };
        let _ = writeln!(
            out,
            "<g class=\"dimension\" data-dimension=\"{}\" data-view=\"{}\"{lost}>",
            dim.id, dim.view
        );
        for line in &dim.lines {
            polyline(&mut out, line, h);
        }
        for l in &dim.labels {
            label(&mut out, l, h);
        }
        let _ = writeln!(out, "</g>");
    }
    for l in &sheet.labels {
        label(&mut out, l, h);
    }
    out.push_str("</svg>\n");
    out
}
