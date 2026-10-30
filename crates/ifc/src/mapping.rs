//! Matching sections and materials of foreign IFC files to the ГОСТ catalogue.
//!
//! Programs name sections their own way («20Б1», «I20B1_26020_83»,
//! «Двутавр ГОСТ 26020-83:20Б1», «L100X8», «Гн.150x100x6») and some write
//! only an outline. A section is recognised by its dimensions first, by its
//! designation second; an outline by its bounding box and area. Steel grades
//! are found inside material names («Сталь С255», «C345-3», «Металл - С245»).
use open_bim_model::library::{self, Entry, Family};
use open_bim_model::{Material, ObjectId, ProfileShape};

/// How a catalogue section was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Match {
    /// Same dimensions (and the designation agrees or is absent).
    Dimensions,
    /// Designation recognised; dimensions agree loosely.
    Designation,
    /// Outline: bounding box and area agree.
    Outline,
}

/// Uppercase with Cyrillic letters that look or sound Latin spelled in Latin,
/// multiplication signs as X and decimal commas as points.
fn latin(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_uppercase) {
        let s = match c {
            'А' => "A",
            'Б' | 'В' => "B",
            'Г' => "G",
            'Е' | 'Ё' => "E",
            'К' => "K",
            'Л' => "L",
            'М' => "M",
            'Н' => "H",
            'О' => "O",
            'П' => "P",
            'Р' => "P",
            'С' => "C",
            'Т' => "T",
            'У' => "U",
            'Х' | '×' | '*' => "X",
            'Ш' => "SH",
            ',' => ".",
            _ => {
                out.push(c);
                continue;
            }
        };
        out.push_str(s);
    }
    out
}

/// Section standards a designation may name, by number.
const STANDARDS: [&str; 8] = [
    "57837", "26020", "8239", "8240", "8509", "8510", "10704", "30245",
];

/// A designation without standard references, descriptive words, spaces
/// and separators, in [`latin`] spelling: «Труба 114×4,5» → «114X4.5».
fn clean(text: &str) -> String {
    let mut s = text.to_uppercase();
    // Standard references: «ГОСТ Р 57837-2017», «_26020_83», «(Р 57837)».
    for std in STANDARDS {
        while let Some(at) = s.find(std) {
            let head = s[..at].trim_end_matches([' ', '_', '-', '(', 'Р', 'P']);
            let head = head
                .strip_suffix("ГОСТ")
                .or_else(|| head.strip_suffix("GOST"))
                .unwrap_or(head);
            let start = head.len();
            let mut end = at + std.len();
            let rest = s[end..].trim_start_matches(['-', '_', ' ']);
            let year = rest.chars().take_while(char::is_ascii_digit).count();
            if year > 0 {
                end = s.len() - rest.len() + year;
            }
            if s[end..].starts_with(')') {
                end += 1;
            }
            s.replace_range(start..end, " ");
        }
    }
    for word in [
        "ДВУТАВР",
        "ШВЕЛЛЕР",
        "УГОЛОК",
        "ТРУБА",
        "ПРОФИЛЬ",
        "ГНУТЫЙ",
        "ЗАМКНУТЫЙ",
        "КВАДРАТНАЯ",
        "ПРЯМОУГОЛЬНАЯ",
        "ЭЛЕКТРОСВАРНАЯ",
        "СТАЛЬНОЙ",
        "СТАЛЬНАЯ",
        "БАЛОЧНЫЙ",
        "КОЛОННЫЙ",
        "ШИРОКОПОЛОЧНЫЙ",
        "РАВНОПОЛОЧНЫЙ",
        "НЕРАВНОПОЛОЧНЫЙ",
    ] {
        s = s.replace(word, " ");
    }
    let chars: Vec<char> = latin(&s)
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '_' | '-' | '(' | ')'))
        .collect();
    // A point stays only as a decimal point («4.5»), not as a separator («Гн.150»).
    chars
        .iter()
        .enumerate()
        .filter(|(i, c)| {
            **c != '.'
                || (*i > 0
                    && chars[i - 1].is_ascii_digit()
                    && chars.get(i + 1).is_some_and(char::is_ascii_digit))
        })
        .map(|(_, c)| *c)
        .collect()
}

/// Designation keys of a foreign section name, most specific first, and the
/// standard it names (if any).
fn keys(name: &str) -> (Vec<String>, Option<&'static str>) {
    let hint = STANDARDS.into_iter().find(|s| name.contains(s));
    let mut out: Vec<String> = Vec::new();
    // «Семейство:Тип:id» — every part may be the designation.
    let mut parts: Vec<&str> = name.split(':').collect();
    parts.reverse();
    parts.push(name);
    for part in parts {
        let k = clean(part);
        if k.is_empty() {
            continue;
        }
        let mut push = |k: String| {
            if !out.contains(&k) {
                out.push(k);
            }
        };
        let series = |r: &str| r.contains('B') || r.contains("SH") || r.contains('K');
        match k.as_bytes() {
            // «I20B1» — a ГОСТ 26020 / Р 57837 beam written with an I.
            [b'I', d, ..] if d.is_ascii_digit() && series(&k[1..]) => push(k[1..].into()),
            // «U20P», «C20U» — channels.
            [b'U' | b'C', d, ..] if d.is_ascii_digit() => push(format!("[{}", &k[1..])),
            [d, ..] if d.is_ascii_digit() && hint == Some("8239") => push(format!("I{k}")),
            [d, ..] if d.is_ascii_digit() && hint == Some("8240") => push(format!("[{k}")),
            _ => {}
        }
        push(k);
    }
    (out, hint)
}

/// Key of a catalogue name, comparable with [`keys`].
fn catalogue_key(entry: &Entry) -> String {
    clean(entry.name.trim_end_matches(" (Р 57837)"))
}

fn entry_hint(entry: &Entry) -> &'static str {
    STANDARDS
        .into_iter()
        .find(|s| entry.standard.contains(s))
        .unwrap_or("")
}

/// Largest relative difference of two shapes of the same kind, in mm; `None`
/// for different kinds.
fn difference(a: &ProfileShape, b: &ProfileShape) -> Option<f64> {
    use ProfileShape::*;
    let pairs: Vec<(f64, f64)> = match (a, b) {
        (
            ISection {
                height: h1,
                width: w1,
                web: s1,
                flange: t1,
            },
            ISection {
                height: h2,
                width: w2,
                web: s2,
                flange: t2,
            },
        )
        | (
            Channel {
                height: h1,
                width: w1,
                web: s1,
                flange: t1,
            },
            Channel {
                height: h2,
                width: w2,
                web: s2,
                flange: t2,
            },
        ) => vec![(*h1, *h2), (*w1, *w2), (*s1, *s2), (*t1, *t2)],
        (
            Angle {
                height: h1,
                width: w1,
                thickness: t1,
            },
            Angle {
                height: h2,
                width: w2,
                thickness: t2,
            },
        ) => vec![(*h1, *h2), (*w1, *w2), (*t1, *t2)],
        (
            CircularHollow {
                diameter: d1,
                thickness: t1,
            },
            CircularHollow {
                diameter: d2,
                thickness: t2,
            },
        ) => vec![(*d1, *d2), (*t1, *t2)],
        (
            RectangularHollow {
                height: h1,
                width: w1,
                thickness: t1,
            },
            RectangularHollow {
                height: h2,
                width: w2,
                thickness: t2,
            },
        ) => vec![(*h1, *h2), (*w1, *w2), (*t1, *t2)],
        _ => return None,
    };
    Some(pairs.iter().map(|(x, y)| (x - y).abs()).fold(0., f64::max))
}

/// Dimensions agree: within 0.6 mm (rounded tables, mean flange thickness).
const TIGHT_MM: f64 = 0.6;

/// Catalogue sections whose designation matches `name`.
fn by_name(name: &str) -> Vec<Entry> {
    let (keys, hint) = keys(name);
    let all = library::sections();
    for key in &keys {
        let mut found: Vec<Entry> = all
            .iter()
            .filter(|e| catalogue_key(e) == *key)
            .cloned()
            .collect();
        if found.is_empty() {
            continue;
        }
        if let Some(h) = hint
            && found.iter().any(|e| entry_hint(e) == h)
        {
            found.retain(|e| entry_hint(e) == h);
        }
        return found;
    }
    Vec::new()
}

/// Sections named by the first of `names` that names any.
fn by_names(names: &[&str]) -> Vec<Entry> {
    names
        .iter()
        .map(|n| by_name(n))
        .find(|found| !found.is_empty())
        .unwrap_or_default()
}

/// The catalogue section for a parametric IFC profile; `names` are the
/// profile, type and element names, most specific first.
pub fn section(names: &[&str], shape: &ProfileShape) -> Option<(Entry, Match)> {
    let named = by_names(names);
    // A named section with matching dimensions.
    if let Some(e) = named
        .iter()
        .filter(|e| difference(&e.shape, shape).is_some_and(|d| d <= TIGHT_MM))
        .min_by(|a, b| {
            let da = difference(&a.shape, shape).unwrap_or(f64::MAX);
            let db = difference(&b.shape, shape).unwrap_or(f64::MAX);
            da.total_cmp(&db)
        })
    {
        return Some((e.clone(), Match::Dimensions));
    }
    // Any section with the same dimensions.
    let best = library::sections()
        .into_iter()
        .filter_map(|e| difference(&e.shape, shape).map(|d| (d, e)))
        .filter(|(d, _)| *d <= TIGHT_MM)
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, e)) = best {
        return Some((e, Match::Dimensions));
    }
    // A designation whose dimensions agree loosely (exporters round, some
    // write nominal sizes): every dimension within 8 %.
    named
        .into_iter()
        .filter(|e| {
            difference(&e.shape, shape).is_some_and(|d| {
                let size = match shape {
                    ProfileShape::ISection { flange, web, .. }
                    | ProfileShape::Channel { flange, web, .. } => flange.min(*web),
                    ProfileShape::Angle { thickness, .. }
                    | ProfileShape::CircularHollow { thickness, .. }
                    | ProfileShape::RectangularHollow { thickness, .. } => *thickness,
                    ProfileShape::Rectangle { .. }
                    | ProfileShape::Polygon { .. }
                    | ProfileShape::Welded { .. } => 0.,
                };
                d <= (0.08 * size).max(TIGHT_MM)
            })
        })
        .map(|e| (e, Match::Designation))
        .next()
}

/// One of eight in-plane orientations: rotation by `quarter` × 90° after an
/// optional mirror of x.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Orientation {
    pub mirror: bool,
    pub quarter: u8,
}

impl Orientation {
    pub fn apply(self, p: [f64; 2]) -> [f64; 2] {
        let [x, y] = if self.mirror { [-p[0], p[1]] } else { p };
        match self.quarter % 4 {
            0 => [x, y],
            1 => [-y, x],
            2 => [-x, -y],
            _ => [y, -x],
        }
    }
}

fn shoelace(points: &[[f64; 2]]) -> (f64, [f64; 2]) {
    let mut a = 0.;
    let mut c = [0., 0.];
    for i in 0..points.len() {
        let p = points[i];
        let q = points[(i + 1) % points.len()];
        let k = p[0] * q[1] - q[0] * p[1];
        a += k;
        c[0] += (p[0] + q[0]) * k;
        c[1] += (p[1] + q[1]) * k;
    }
    if a.abs() < 1e-12 {
        return (0., [0., 0.]);
    }
    (a.abs() / 2., [c[0] / (3. * a), c[1] / (3. * a)])
}

fn bounds(points: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
    points.iter().fold(
        ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]),
        |(lo, hi), p| {
            (
                [lo[0].min(p[0]), lo[1].min(p[1])],
                [hi[0].max(p[0]), hi[1].max(p[1])],
            )
        },
    )
}

/// What an outline turned out to be.
#[derive(Clone, Debug, PartialEq)]
pub struct Outline {
    pub entry: Option<Entry>,
    /// Sharp rectangle (no catalogue section): width × height.
    pub rectangle: Option<(f64, f64)>,
    /// Centre of the bounding box in the outline's coordinates.
    pub centre: [f64; 2],
    /// Catalogue section coordinates → outline coordinates.
    pub orientation: Orientation,
}

/// Recognise an extruded outline (with voids for tubes): a rectangle, or a
/// catalogue section with the same bounding box (either way round), area
/// within 6 % (fillets are in both) and its centroid on the same side.
pub fn outline(names: &[&str], contour: &[[f64; 2]], voids: &[Vec<[f64; 2]>]) -> Option<Outline> {
    if contour.len() < 3 {
        return None;
    }
    let (lo, hi) = bounds(contour);
    let centre = [(lo[0] + hi[0]) / 2., (lo[1] + hi[1]) / 2.];
    let size = [hi[0] - lo[0], hi[1] - lo[1]];
    let (outer, outer_c) = shoelace(contour);
    let (mut area, mut moment) = (outer, [outer * outer_c[0], outer * outer_c[1]]);
    for void in voids {
        let (a, c) = shoelace(void);
        area -= a;
        moment = [moment[0] - a * c[0], moment[1] - a * c[1]];
    }
    if area <= 0. {
        return None;
    }
    let centroid = [moment[0] / area, moment[1] / area];
    let offset = [centroid[0] - centre[0], centroid[1] - centre[1]];
    if voids.is_empty()
        && contour.len() == 4
        && (area - size[0] * size[1]).abs() <= 1e-6 * area.max(1.)
    {
        return Some(Outline {
            entry: None,
            rectangle: Some((size[0], size[1])),
            centre,
            orientation: Orientation {
                mirror: false,
                quarter: 0,
            },
        });
    }
    let named = by_names(names);
    let tolerance = |v: f64| (0.015 * v).max(1.);
    let mut best: Option<(f64, Entry, Orientation)> = None;
    for entry in library::sections() {
        // Rectangles are told above; anything else is not a strip.
        if entry.family == Family::Flat {
            continue;
        }
        let Ok(sharp) = entry.shape.solid_contour() else {
            continue;
        };
        let (_, c) = shoelace(&sharp);
        if (entry.area_mm2 - area).abs() > 0.06 * entry.area_mm2 {
            continue;
        }
        for mirror in [false, true] {
            for quarter in 0..4 {
                let o = Orientation { mirror, quarter };
                let turned: Vec<[f64; 2]> = sharp.iter().map(|p| o.apply(*p)).collect();
                let (l, h) = bounds(&turned);
                let s = [h[0] - l[0], h[1] - l[1]];
                if (s[0] - size[0]).abs() > tolerance(size[0])
                    || (s[1] - size[1]).abs() > tolerance(size[1])
                {
                    continue;
                }
                let tc = o.apply(c);
                let off = (tc[0] - offset[0]).hypot(tc[1] - offset[1]);
                // Prefer the named section, then the closest area, then the
                // centroid side.
                let score = if named.iter().any(|n| n.name == entry.name) {
                    0.
                } else {
                    1.
                } * 1e9
                    + (entry.area_mm2 - area).abs()
                    + off * 1e-3;
                if best.as_ref().is_none_or(|(b, ..)| score < *b) {
                    best = Some((score, entry.clone(), o));
                }
            }
        }
    }
    best.map(|(_, entry, orientation)| Outline {
        entry: Some(entry),
        rectangle: None,
        centre,
        orientation,
    })
}

/// A ГОСТ steel grade named inside a material name: «С255», «Сталь C345-3»,
/// «09Г2С». Latin C is accepted for Cyrillic С.
pub fn steel(id: ObjectId, name: &str) -> Option<Material> {
    let text = latin(name);
    let mut grades: Vec<&str> = library::steels().into_iter().map(|(g, _)| g).collect();
    grades.sort_by_key(|g| std::cmp::Reverse(g.chars().count()));
    for grade in grades {
        let key = latin(grade);
        let mut from = 0;
        while let Some(at) = text[from..].find(&key) {
            let start = from + at;
            let end = start + key.len();
            let before = text[..start].chars().next_back();
            let after = text[end..].chars().next();
            if !before.is_some_and(char::is_alphanumeric)
                && !after.is_some_and(|c| c.is_alphanumeric())
            {
                return library::steel(id, grade);
            }
            from = end;
        }
    }
    None
}

/// Density for a material without one, by its name: concrete and timber are
/// told apart from steel.
pub fn density(name: &str) -> f64 {
    let n = name.to_lowercase();
    if ["бетон", "concrete", "железобетон"]
        .iter()
        .any(|w| n.contains(w))
    {
        2500.
    } else if ["дерев", "wood", "timber", "брус", "клеен"]
        .iter()
        .any(|w| n.contains(w))
    {
        500.
    } else {
        7850.
    }
}

/// Families shown in the report («двутавр», «уголок»…).
pub fn family_word(family: Family) -> &'static str {
    match family {
        Family::IBeamSloped
        | Family::IBeamNormal
        | Family::IBeamWide
        | Family::IBeamColumn
        | Family::IBeamNormal57837
        | Family::IBeamWide57837
        | Family::IBeamColumn57837
        | Family::IBeamExtraNormal57837
        | Family::IBeamExtraColumn57837
        | Family::IBeamPile57837
        | Family::IBeamNormalSto
        | Family::IBeamWideSto
        | Family::IBeamColumnSto => "двутавр",
        Family::TeeNormal | Family::TeeWide | Family::TeeColumn => "тавр",
        Family::Channel | Family::ChannelP | Family::ChannelBent => "швеллер",
        Family::Angle | Family::AngleUnequal | Family::AngleBent => "уголок",
        Family::Flat => "полоса",
        Family::Pipe | Family::PipeWaterGas | Family::SquareTube | Family::RectTube => "труба",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str) -> Vec<String> {
        by_name(name).into_iter().map(|e| e.name).collect()
    }

    #[test]
    fn designations_of_other_programs() {
        assert_eq!(named("20Б1"), ["20Б1", "20Б1 (Р 57837)"]);
        assert_eq!(named("I20B1_26020_83"), ["20Б1"]);
        assert_eq!(named("Двутавр ГОСТ 26020-83:20Б1:345678"), ["20Б1"]);
        assert_eq!(named("20Б1_ГОСТ_Р_57837-2017"), ["20Б1 (Р 57837)"]);
        assert_eq!(named("I20"), ["I20"]);
        assert_eq!(named("20 ГОСТ 8239-89"), ["I20"]);
        assert_eq!(named("[20П"), ["[20П"]);
        assert_eq!(named("U20P"), ["[20П"]);
        assert_eq!(named("L100X8"), ["L100×8"]);
        assert_eq!(named("Уголок 100x8"), Vec::<String>::new());
        assert_eq!(named("L100*8_8509_93"), ["L100×8"]);
        assert_eq!(named("Гн.150x100x6"), ["Гн 150×100×6"]);
        assert_eq!(named("Труба 114x4.5"), ["Труба 114×4,5"]);
        assert_eq!(named("40K1"), ["40К1", "40К1 (Р 57837)"]);
        assert!(named("HEA200").is_empty());
    }

    #[test]
    fn dimensions_win_over_names() {
        let entry = library::section("30Б1 (Р 57837)").unwrap();
        // Nameless: found by dimensions.
        let (e, how) = section(&[], &entry.shape).unwrap();
        assert_eq!(
            (e.name.as_str(), how),
            ("30Б1 (Р 57837)", Match::Dimensions)
        );
        // Named «30Б1» (also a ГОСТ 26020 name), dimensions pick Р 57837.
        let (e, _) = section(&["30Б1"], &entry.shape).unwrap();
        assert_eq!(e.name, "30Б1 (Р 57837)");
        // A foreign section stays foreign.
        let hea = ProfileShape::ISection {
            height: 190.,
            width: 200.,
            web: 6.5,
            flange: 10.,
        };
        assert!(section(&["HEA200"], &hea).is_none());
    }

    #[test]
    fn outlines_with_fillets() {
        let entry = library::section("L100×8").unwrap();
        // An outline of the same angle, turned by 90°, area as in the table.
        let sharp = entry.shape.solid_contour().unwrap();
        let o = Orientation {
            mirror: false,
            quarter: 1,
        };
        let turned: Vec<[f64; 2]> = sharp
            .iter()
            .map(|p| {
                let q = o.apply(*p);
                [q[0] + 500., q[1] - 20.]
            })
            .collect();
        let found = outline(&[], &turned, &[]).unwrap();
        assert_eq!(found.entry.unwrap().name, "L100×8");
        assert_eq!(found.orientation, o);
        assert!((found.centre[0] - 500.).abs() < 1e-9 && (found.centre[1] + 20.).abs() < 1e-9);
        let rect = outline(&[], &[[0., 0.], [200., 0.], [200., 400.], [0., 400.]], &[]).unwrap();
        assert_eq!(rect.rectangle, Some((200., 400.)));
    }

    #[test]
    fn steel_grades_in_material_names() {
        let name = |s: &str| steel(ObjectId(1), s).map(|m| m.name);
        assert_eq!(name("С255").as_deref(), Some("С255 · ГОСТ 27772-2021"));
        assert_eq!(
            name("Сталь C345").as_deref(),
            Some("С345 · ГОСТ 27772-2021")
        );
        assert_eq!(
            name("Металл - Сталь - С345Б-1").as_deref(),
            Some("С345Б-1 · ГОСТ 27772-2021")
        );
        assert_eq!(name("09Г2С-15").as_deref(), Some("09Г2С · ГОСТ 19281-2014"));
        assert_eq!(name("S235JR"), None);
        assert_eq!(name("C2550"), None);
        assert_eq!(density("Бетон B25"), 2500.);
        assert_eq!(density("S355"), 7850.);
    }
}
