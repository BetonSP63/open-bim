//! ASCII DXF R12 (AC1009) in paper millimetres. One layer per line style,
//! DASHED line type for hidden lines, non-ASCII text as \U+XXXX escapes.
use open_bim_drawings::{Align, Label, LineStyle, Polyline, Sheet};
use std::fmt::Write;

pub const TEXT_CAP_RATIO: f64 = 0.7;

pub fn layer(style: LineStyle) -> (&'static str, i32, &'static str) {
    match style {
        LineStyle::Visible => ("OB_VISIBLE", 7, "CONTINUOUS"),
        LineStyle::Hidden => ("OB_HIDDEN", 8, "DASHED"),
        LineStyle::Section => ("OB_SECTION", 7, "CONTINUOUS"),
        LineStyle::Hatch => ("OB_HATCH", 9, "CONTINUOUS"),
        LineStyle::Thin => ("OB_THIN", 3, "CONTINUOUS"),
        LineStyle::Frame => ("OB_FRAME", 7, "CONTINUOUS"),
        LineStyle::Lost => ("OB_LOST", 1, "CONTINUOUS"),
    }
}
const STYLES: [LineStyle; 7] = [
    LineStyle::Visible,
    LineStyle::Hidden,
    LineStyle::Section,
    LineStyle::Hatch,
    LineStyle::Thin,
    LineStyle::Frame,
    LineStyle::Lost,
];
pub const TEXT_LAYER: &str = "OB_TEXT";

fn escape(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii() && !c.is_ascii_control() {
                c.to_string()
            } else {
                format!("\\U+{:04X}", c as u32)
            }
        })
        .collect()
}

fn n(v: f64) -> String {
    let s = format!("{:.6}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn pair(out: &mut String, code: i32, value: impl std::fmt::Display) {
    let _ = writeln!(out, "{code:>3}\n{value}");
}

fn polyline(out: &mut String, line: &Polyline) {
    let (name, _, _) = layer(line.style);
    if line.points.len() == 2 {
        pair(out, 0, "LINE");
        pair(out, 8, name);
        for (i, p) in line.points.iter().enumerate() {
            pair(out, 10 + i as i32, n(p[0]));
            pair(out, 20 + i as i32, n(p[1]));
            pair(out, 30 + i as i32, 0);
        }
        return;
    }
    if line.points.len() < 2 {
        return;
    }
    pair(out, 0, "POLYLINE");
    pair(out, 8, name);
    pair(out, 66, 1);
    pair(out, 10, 0);
    pair(out, 20, 0);
    pair(out, 30, 0);
    for p in &line.points {
        pair(out, 0, "VERTEX");
        pair(out, 8, name);
        pair(out, 10, n(p[0]));
        pair(out, 20, n(p[1]));
        pair(out, 30, 0);
    }
    pair(out, 0, "SEQEND");
    pair(out, 8, name);
}

fn text(out: &mut String, label: &Label) {
    let layer = if label.lost { "OB_LOST" } else { TEXT_LAYER };
    let justify = match label.align {
        Align::Start => 0,
        Align::Middle => 1,
        Align::End => 2,
    };
    pair(out, 0, "TEXT");
    pair(out, 8, layer);
    pair(out, 10, n(label.position[0]));
    pair(out, 20, n(label.position[1]));
    pair(out, 30, 0);
    pair(out, 40, n(label.height * TEXT_CAP_RATIO));
    pair(out, 1, escape(&label.text));
    if label.rotation.abs() > 1e-9 {
        pair(out, 50, n(label.rotation));
    }
    if justify != 0 {
        pair(out, 72, justify);
        pair(out, 11, n(label.position[0]));
        pair(out, 21, n(label.position[1]));
        pair(out, 31, 0);
    }
}

pub fn render(sheet: &Sheet) -> String {
    let mut out = String::new();
    pair(&mut out, 0, "SECTION");
    pair(&mut out, 2, "HEADER");
    pair(&mut out, 9, "$ACADVER");
    pair(&mut out, 1, "AC1009");
    // Millimetres (4) and metric measurement (1).
    pair(&mut out, 9, "$INSUNITS");
    pair(&mut out, 70, 4);
    pair(&mut out, 9, "$MEASUREMENT");
    pair(&mut out, 70, 1);
    pair(&mut out, 9, "$EXTMIN");
    pair(&mut out, 10, 0);
    pair(&mut out, 20, 0);
    pair(&mut out, 30, 0);
    pair(&mut out, 9, "$EXTMAX");
    pair(&mut out, 10, n(sheet.size[0]));
    pair(&mut out, 20, n(sheet.size[1]));
    pair(&mut out, 30, 0);
    pair(&mut out, 0, "ENDSEC");
    pair(&mut out, 0, "SECTION");
    pair(&mut out, 2, "TABLES");
    pair(&mut out, 0, "TABLE");
    pair(&mut out, 2, "LTYPE");
    pair(&mut out, 70, 2);
    for (name, description, pattern) in [
        ("CONTINUOUS", "Solid line", vec![]),
        ("DASHED", "Dashed __ __ __", vec![3.0, -1.5]),
    ] {
        pair(&mut out, 0, "LTYPE");
        pair(&mut out, 2, name);
        pair(&mut out, 70, 0);
        pair(&mut out, 3, description);
        pair(&mut out, 72, 65);
        pair(&mut out, 73, pattern.len());
        pair(&mut out, 40, n(pattern.iter().map(|v: &f64| v.abs()).sum()));
        for v in pattern {
            pair(&mut out, 49, n(v));
        }
    }
    pair(&mut out, 0, "ENDTAB");
    pair(&mut out, 0, "TABLE");
    pair(&mut out, 2, "LAYER");
    pair(&mut out, 70, STYLES.len() + 1);
    let mut layers: Vec<(&str, i32, &str)> = STYLES.iter().map(|s| layer(*s)).collect();
    layers.push((TEXT_LAYER, 7, "CONTINUOUS"));
    for (name, color, ltype) in layers {
        pair(&mut out, 0, "LAYER");
        pair(&mut out, 2, name);
        pair(&mut out, 70, 0);
        pair(&mut out, 62, color);
        pair(&mut out, 6, ltype);
    }
    pair(&mut out, 0, "ENDTAB");
    pair(&mut out, 0, "ENDSEC");
    pair(&mut out, 0, "SECTION");
    pair(&mut out, 2, "ENTITIES");
    for line in &sheet.frame {
        polyline(&mut out, line);
    }
    for view in &sheet.views {
        for line in &view.lines {
            polyline(&mut out, line);
        }
        for label in &view.labels {
            text(&mut out, label);
        }
    }
    for dim in &sheet.dimensions {
        for line in &dim.lines {
            polyline(&mut out, line);
        }
        for label in &dim.labels {
            text(&mut out, label);
        }
    }
    for label in &sheet.labels {
        text(&mut out, label);
    }
    pair(&mut out, 0, "ENDSEC");
    pair(&mut out, 0, "EOF");
    out
}
