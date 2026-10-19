//! Minimal PDF 1.7 writer. User space is scaled to millimetres with the
//! sheet origin at the lower-left corner, so sheet coordinates map 1:1.
use flate2::{Compression, write::ZlibEncoder};
use open_bim_drawings::font::{FONT, FONT_NAME, Font};
use open_bim_drawings::{Align, Label, LineStyle, Polyline, Sheet};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write as _;

pub const POINTS_PER_MM: f64 = 72. / 25.4;

fn n(v: f64) -> String {
    let s = format!("{:.4}", (v * 10000.).round() / 10000.);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn deflate(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).map_err(|e| e.to_string())?;
    encoder.finish().map_err(|e| e.to_string())
}

fn style(style: LineStyle) -> (f64, &'static str, &'static str) {
    let (_, width, _) = open_bim_drawings::svg::stroke(style);
    match style {
        LineStyle::Hidden => (width, "[3 1.5] 0 d", "0 0 0 RG"),
        LineStyle::Lost => (width, "[1 1] 0 d", "0.85 0 0 RG"),
        LineStyle::DashDot | LineStyle::ThickDashDot => (width, "[12 3 1 3] 0 d", "0 0 0 RG"),
        LineStyle::ThickDashed => (width, "[3 1.5] 0 d", "0 0 0 RG"),
        _ => (width, "[] 0 d", "0 0 0 RG"),
    }
}

struct Page<'a> {
    font: &'a Font,
    content: String,
    used: &'a mut BTreeMap<u16, char>,
}

impl Page<'_> {
    fn line(&mut self, line: &Polyline) {
        if line.points.len() < 2 {
            return;
        }
        let (width, dash, color) = style(line.style);
        let _ = writeln!(self.content, "{} w {dash} {color}", n(width));
        for (i, p) in line.points.iter().enumerate() {
            let op = if i == 0 { "m" } else { "l" };
            let _ = writeln!(self.content, "{} {} {op}", n(p[0]), n(p[1]));
        }
        self.content.push_str("S\n");
    }
    fn text(&mut self, label: &Label) {
        let width = self.font.width(&label.text, label.height);
        let shift = match label.align {
            Align::Start => 0.,
            Align::Middle => -width / 2.,
            Align::End => -width,
        };
        let (s, c) = label.rotation.to_radians().sin_cos();
        let x = label.position[0] + shift * c;
        let y = label.position[1] + shift * s;
        let mut hex = String::new();
        for ch in label.text.chars() {
            let glyph = self.font.glyph(ch);
            self.used.insert(glyph, ch);
            let _ = write!(hex, "{glyph:04X}");
        }
        let color = if label.lost {
            "0.85 0 0 rg"
        } else {
            "0 0 0 rg"
        };
        let _ = writeln!(
            self.content,
            "BT {color} /F1 {} Tf {} {} {} {} {} {} Tm <{hex}> Tj ET",
            n(label.height),
            n(c),
            n(s),
            n(-s),
            n(c),
            n(x),
            n(y)
        );
    }
}

fn utf16_hex(c: char) -> String {
    let mut buffer = [0u16; 2];
    c.encode_utf16(&mut buffer)
        .iter()
        .map(|u| format!("{u:04X}"))
        .collect()
}

/// One page per sheet, sizes taken from each sheet.
pub fn render(sheets: &[Sheet], title: &str) -> Result<Vec<u8>, String> {
    if sheets.is_empty() {
        return Err("Нет листов для PDF".into());
    }
    let font = Font::load()?;
    let mut used = BTreeMap::new();
    let mut contents = Vec::new();
    for sheet in sheets {
        let mut page = Page {
            font: &font,
            content: format!(
                "{} 0 0 {} 0 0 cm 1 J 1 j\n",
                n(POINTS_PER_MM),
                n(POINTS_PER_MM)
            ),
            used: &mut used,
        };
        for line in &sheet.frame {
            page.line(line);
        }
        if !sheet.fills.is_empty() {
            page.content.push_str("0 0 0 rg\n");
            for f in &sheet.fills {
                let _ = writeln!(
                    page.content,
                    "{} {} {} {} re",
                    n(f[0]),
                    n(f[1]),
                    n(f[2] - f[0]),
                    n(f[3] - f[1])
                );
            }
            page.content.push_str("f\n");
        }
        for view in &sheet.views {
            for line in &view.lines {
                page.line(line);
            }
            for label in &view.labels {
                page.text(label);
            }
        }
        for dim in &sheet.dimensions {
            for line in &dim.lines {
                page.line(line);
            }
            for label in &dim.labels {
                page.text(label);
            }
        }
        for o in &sheet.objects {
            if let Some(mask) = &o.mask {
                page.content.push_str("1 1 1 rg\n");
                for (i, p) in mask.iter().enumerate() {
                    let op = if i == 0 { "m" } else { "l" };
                    let _ = writeln!(page.content, "{} {} {op}", n(p[0]), n(p[1]));
                }
                page.content.push_str("h f\n0 0 0 rg\n");
            }
            for line in &o.lines {
                page.line(line);
            }
        }
        for label in &sheet.labels {
            page.text(label);
        }
        contents.push(page.content);
    }
    // Object numbers: 1 catalog, 2 pages, 3 type0, 4 cid font, 5 descriptor,
    // 6 font file, 7 ToUnicode, 8 info, then page/content pairs.
    let mut objects: Vec<Vec<u8>> = Vec::new();
    let page_ids: Vec<usize> = (0..sheets.len()).map(|i| 9 + 2 * i).collect();
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    objects.push(
        format!(
            "<< /Type /Pages /Count {} /Kids [{}] >>",
            sheets.len(),
            page_ids
                .iter()
                .map(|id| format!("{id} 0 R"))
                .collect::<Vec<_>>()
                .join(" ")
        )
        .into_bytes(),
    );
    objects.push(
        format!(
            "<< /Type /Font /Subtype /Type0 /BaseFont /{FONT_NAME} /Encoding /Identity-H /DescendantFonts [4 0 R] /ToUnicode 7 0 R >>"
        )
        .into_bytes(),
    );
    let scale = 1000. / font.units_per_em;
    let widths: String = used
        .keys()
        .map(|g| format!("{g} [{}]", (font.advance(*g) * scale).round() as i64))
        .collect::<Vec<_>>()
        .join(" ");
    objects.push(
        format!(
            "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{FONT_NAME} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor 5 0 R /CIDToGIDMap /Identity /DW 500 /W [{widths}] >>"
        )
        .into_bytes(),
    );
    objects.push(
        format!(
            "<< /Type /FontDescriptor /FontName /{FONT_NAME} /Flags 32 /FontBBox [-200 -250 1200 1000] /ItalicAngle 0 /Ascent {} /Descent {} /CapHeight 700 /StemV 70 /FontFile2 6 0 R >>",
            (font.ascent * scale).round() as i64,
            (font.descent * scale).round() as i64
        )
        .into_bytes(),
    );
    let compressed = deflate(FONT)?;
    let mut file = format!(
        "<< /Length {} /Length1 {} /Filter /FlateDecode >>\nstream\n",
        compressed.len(),
        FONT.len()
    )
    .into_bytes();
    file.extend(&compressed);
    file.extend(b"\nendstream");
    objects.push(file);
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def /CMapType 2 def\n1 begincodespacerange <0000> <FFFF> endcodespacerange\n",
    );
    let pairs: Vec<_> = used.iter().collect();
    for chunk in pairs.chunks(100) {
        let _ = writeln!(cmap, "{} beginbfchar", chunk.len());
        for (glyph, c) in chunk {
            let _ = writeln!(cmap, "<{glyph:04X}> <{}>", utf16_hex(**c));
        }
        cmap.push_str("endbfchar\n");
    }
    cmap.push_str("endcmap CMapName currentdict /CMap defineresource pop end end\n");
    let mut stream = format!("<< /Length {} >>\nstream\n", cmap.len()).into_bytes();
    stream.extend(cmap.as_bytes());
    stream.extend(b"endstream");
    objects.push(stream);
    let escaped: String = title.chars().map(utf16_hex).collect();
    objects.push(
        format!("<< /Title <FEFF{escaped}> /Producer (Open BIM drawing export) >>").into_bytes(),
    );
    for (sheet, content) in sheets.iter().zip(&contents) {
        let content_id = objects.len() + 2;
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 3 0 R >> >> /Contents {content_id} 0 R >>",
                n(sheet.size[0] * POINTS_PER_MM),
                n(sheet.size[1] * POINTS_PER_MM)
            )
            .into_bytes(),
        );
        let data = deflate(content.as_bytes())?;
        let mut stream = format!(
            "<< /Length {} /Filter /FlateDecode >>\nstream\n",
            data.len()
        )
        .into_bytes();
        stream.extend(&data);
        stream.extend(b"\nendstream");
        objects.push(stream);
    }
    let mut out = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend(object);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        out.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info 8 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    Ok(out)
}
