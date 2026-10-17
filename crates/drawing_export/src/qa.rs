//! Sheet layout check: every label inside the drawing frame, no label
//! overlapping another label, and only title-block labels in the title block.
use open_bim_drawings::font::Font;
use open_bim_drawings::{Align, Label, MARGIN, MARGIN_LEFT, Sheet, TITLE_BLOCK};
pub use rect::Rect;

mod rect {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Rect {
        pub min: [f64; 2],
        pub max: [f64; 2],
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Issue {
    Clipped { text: String },
    Overlap { first: String, second: String },
    InTitleBlock { text: String },
    MissingGlyphs { text: String, missing: Vec<char> },
}

/// Axis-aligned bounds of a (possibly rotated) label; descenders included.
pub fn bounds(font: &Font, label: &Label) -> Rect {
    let width = font.width(&label.text, label.height);
    let shift = match label.align {
        Align::Start => 0.,
        Align::Middle => -width / 2.,
        Align::End => -width,
    };
    let top = font.ascent / font.units_per_em * label.height;
    let bottom = font.descent / font.units_per_em * label.height;
    let (s, c) = label.rotation.to_radians().sin_cos();
    let corners = [
        [shift, bottom],
        [shift + width, bottom],
        [shift + width, top],
        [shift, top],
    ];
    let mut r = Rect {
        min: [f64::INFINITY; 2],
        max: [f64::NEG_INFINITY; 2],
    };
    for [x, y] in corners {
        let p = [
            label.position[0] + x * c - y * s,
            label.position[1] + x * s + y * c,
        ];
        r.min = [r.min[0].min(p[0]), r.min[1].min(p[1])];
        r.max = [r.max[0].max(p[0]), r.max[1].max(p[1])];
    }
    r
}

fn overlap(a: Rect, b: Rect) -> bool {
    // 0.1 mm grace keeps touching baselines of stacked rows legal.
    (0..2).all(|i| a.min[i] + 0.1 < b.max[i] && b.min[i] + 0.1 < a.max[i])
}

pub fn check(sheet: &Sheet) -> Result<Vec<Issue>, String> {
    let font = Font::load()?;
    let [w, h] = sheet.size;
    let inner = Rect {
        min: [MARGIN_LEFT, MARGIN],
        max: [w - MARGIN, h - MARGIN],
    };
    let title = Rect {
        min: [w - MARGIN - TITLE_BLOCK[0], MARGIN],
        max: [w - MARGIN, MARGIN + TITLE_BLOCK[1]],
    };
    let drawing_labels: Vec<&Label> = sheet
        .views
        .iter()
        .flat_map(|v| &v.labels)
        .chain(sheet.dimensions.iter().flat_map(|d| &d.labels))
        .collect();
    let mut issues = Vec::new();
    let all: Vec<(&Label, bool)> = drawing_labels
        .iter()
        .map(|l| (*l, false))
        .chain(sheet.labels.iter().map(|l| (l, true)))
        .collect();
    let rects: Vec<Rect> = all.iter().map(|(l, _)| bounds(&font, l)).collect();
    for (i, (label, own)) in all.iter().enumerate() {
        let r = rects[i];
        let missing = font.missing(&label.text);
        if !missing.is_empty() {
            issues.push(Issue::MissingGlyphs {
                text: label.text.clone(),
                missing,
            });
        }
        let limit = if *own { title } else { inner };
        let inside = |outer: Rect| {
            r.min[0] >= outer.min[0] - 1e-9
                && r.min[1] >= outer.min[1] - 1e-9
                && r.max[0] <= outer.max[0] + 1e-9
                && r.max[1] <= outer.max[1] + 1e-9
        };
        // Title block labels must stay in the block; others inside the frame.
        if !inside(limit) && !(*own && inside(inner)) {
            issues.push(Issue::Clipped {
                text: label.text.clone(),
            });
        }
        if !own && overlap(r, title) {
            issues.push(Issue::InTitleBlock {
                text: label.text.clone(),
            });
        }
        for (j, (other, _)) in all.iter().enumerate().skip(i + 1) {
            if overlap(r, rects[j]) {
                issues.push(Issue::Overlap {
                    first: label.text.clone(),
                    second: other.text.clone(),
                });
            }
        }
    }
    Ok(issues)
}
