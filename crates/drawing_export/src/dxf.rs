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

/// Summary of an ASCII DXF file read back (sheet underlays, round-trip checks).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DxfSummary {
    pub version: Option<String>,
    pub insunits: Option<i64>,
    pub layers: Vec<String>,
    /// Entity type → count in the ENTITIES section.
    pub entities: std::collections::BTreeMap<String, usize>,
    pub texts: Vec<String>,
}

pub const MAX_DXF_BYTES: usize = 256 << 20;
pub const MAX_DXF_PAIRS: usize = 20_000_000;
pub const MAX_DXF_VALUE: usize = 4096;

/// Decode \U+XXXX escapes used for non-ASCII text.
fn unescape(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find("\\U+") {
        out.push_str(&rest[..i]);
        let hex = rest.get(i + 3..i + 7).unwrap_or("");
        match u32::from_str_radix(hex, 16).ok().and_then(char::from_u32) {
            Some(c) if hex.len() == 4 => {
                out.push(c);
                rest = &rest[i + 7..];
            }
            _ => {
                out.push_str("\\U+");
                rest = &rest[i + 3..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Bounded reader of group-code/value pairs; malformed input is an error.
pub fn read(text: &str) -> Result<DxfSummary, String> {
    if text.len() > MAX_DXF_BYTES {
        return Err("DXF: файл больше 256 МиБ".into());
    }
    let mut lines = text.lines();
    let mut pairs = Vec::new();
    while let Some(code) = lines.next() {
        let code = code.trim();
        if code.is_empty() && pairs.is_empty() {
            continue;
        }
        let code: i32 = code.parse().map_err(|_| {
            format!(
                "DXF: неверный групповой код «{}»",
                code.chars().take(20).collect::<String>()
            )
        })?;
        if !(0..=1071).contains(&code) {
            return Err(format!("DXF: групповой код {code} вне диапазона"));
        }
        let value = lines.next().ok_or("DXF: нет значения после кода")?;
        if value.len() > MAX_DXF_VALUE {
            return Err("DXF: слишком длинное значение".into());
        }
        pairs.push((code, value.trim_end_matches('\r')));
        if pairs.len() > MAX_DXF_PAIRS {
            return Err("DXF: слишком много пар".into());
        }
    }
    let mut summary = DxfSummary::default();
    let mut section = "";
    let mut i = 0;
    let mut current: Option<&str> = None;
    let mut saw_eof = false;
    while i < pairs.len() {
        let (code, value) = pairs[i];
        match (code, value) {
            (0, "SECTION") => {
                let (c, name) = *pairs.get(i + 1).ok_or("DXF: секция без имени")?;
                if c != 2 {
                    return Err("DXF: после SECTION ожидается код 2".into());
                }
                section = name;
                i += 2;
                continue;
            }
            (0, "ENDSEC") => section = "",
            (0, "EOF") => {
                saw_eof = true;
                break;
            }
            (9, "$ACADVER") => {
                summary.version = pairs.get(i + 1).map(|p| p.1.to_string());
            }
            (9, "$INSUNITS") => {
                let v = pairs.get(i + 1).ok_or("DXF: нет значения $INSUNITS")?;
                summary.insunits = Some(v.1.trim().parse().map_err(|_| "DXF: неверный $INSUNITS")?);
            }
            (0, kind) => {
                current = Some(kind);
                if section == "ENTITIES" {
                    *summary.entities.entry(kind.to_string()).or_default() += 1;
                }
            }
            (2, name) if section == "TABLES" && current == Some("LAYER") => {
                summary.layers.push(name.to_string());
            }
            (1, text) if section == "ENTITIES" && current == Some("TEXT") => {
                summary.texts.push(unescape(text));
            }
            (10..=39, number) => {
                let v: f64 = number
                    .trim()
                    .parse()
                    .map_err(|_| format!("DXF: неверная координата «{number}»"))?;
                if !v.is_finite() {
                    return Err("DXF: бесконечная координата".into());
                }
            }
            _ => {}
        }
        i += 1;
    }
    if !saw_eof {
        return Err("DXF: нет EOF".into());
    }
    Ok(summary)
}
