//! Built-in catalogue of Russian rolled steel sections and steel grades.
//!
//! The library lives outside the project: a
//! project stores only the profiles it uses, copied from here when the user
//! picks one. Sections are modelled with sharp corners (the geometry kernel
//! has no fillets); the mass uses the catalogue area `A`, so specifications
//! give the standard's mass per metre.
//!
//! Dimensions and areas are taken from the standards named in `standard`.
//! The unit tests recompute each area from the dimensions and fillet radii
//! and fail on any row that does not match, which catches transcription
//! errors. Check critical values against the current edition of the
//! standard before issuing documents.
use crate::{Material, ObjectId, Profile, ProfileShape};

/// Group shown in the profile picker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Family {
    IBeamSloped,
    IBeamNormal,
    IBeamWide,
    IBeamColumn,
    IBeamNormal57837,
    IBeamWide57837,
    IBeamColumn57837,
    Channel,
    ChannelP,
    Angle,
    AngleUnequal,
    Pipe,
    SquareTube,
    RectTube,
}

impl Family {
    pub const ALL: [Self; 14] = [
        Self::IBeamNormal57837,
        Self::IBeamWide57837,
        Self::IBeamColumn57837,
        Self::IBeamNormal,
        Self::IBeamWide,
        Self::IBeamColumn,
        Self::IBeamSloped,
        Self::Channel,
        Self::ChannelP,
        Self::Angle,
        Self::AngleUnequal,
        Self::Pipe,
        Self::SquareTube,
        Self::RectTube,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::IBeamSloped => "Двутавры с уклоном полок (ГОСТ 8239)",
            Self::IBeamNormal => "Двутавры балочные Б (ГОСТ 26020)",
            Self::IBeamWide => "Двутавры широкополочные Ш (ГОСТ 26020)",
            Self::IBeamColumn => "Двутавры колонные К (ГОСТ 26020)",
            Self::IBeamNormal57837 => "Двутавры балочные Б (ГОСТ Р 57837)",
            Self::IBeamWide57837 => "Двутавры широкополочные Ш (ГОСТ Р 57837)",
            Self::IBeamColumn57837 => "Двутавры колонные К (ГОСТ Р 57837)",
            Self::Channel => "Швеллеры с уклоном полок У (ГОСТ 8240)",
            Self::ChannelP => "Швеллеры с параллельными гранями полок П (ГОСТ 8240)",
            Self::Angle => "Уголки равнополочные (ГОСТ 8509)",
            Self::AngleUnequal => "Уголки неравнополочные (ГОСТ 8510)",
            Self::Pipe => "Трубы круглые (ГОСТ 10704)",
            Self::SquareTube => "Трубы квадратные (ГОСТ 30245)",
            Self::RectTube => "Трубы прямоугольные (ГОСТ 30245)",
        }
    }
}

/// One catalogue section.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub family: Family,
    pub shape: ProfileShape,
    pub standard: &'static str,
    /// Catalogue area, mm² (includes fillets and corner radii).
    pub area_mm2: f64,
    /// Fillet radius used by the area check, mm.
    pub radius: f64,
}

impl Entry {
    /// The profile record to store in a project under `id`.
    pub fn profile(&self, id: ObjectId) -> Profile {
        Profile {
            id,
            name: self.name.clone(),
            shape: self.shape.clone(),
            source: format!("Каталог Open BIM: {}", self.standard),
            standard: Some(self.standard.into()),
            area_mm2: Some(self.area_mm2),
        }
    }

    /// Mass per metre, kg (ρ = 7850 kg/m³).
    pub fn mass_kg_m(&self) -> f64 {
        self.area_mm2 * 7850.0 / 1e6
    }
}

const GOST_8239: &str = "ГОСТ 8239-89";
const GOST_26020: &str = "ГОСТ 26020-83";
const GOST_57837: &str = "ГОСТ Р 57837-2017";
/// Suffix that keeps ГОСТ Р 57837 names apart from the same names in
/// ГОСТ 26020, whose sections differ.
const SUFFIX_57837: &str = " (Р 57837)";
const GOST_8240: &str = "ГОСТ 8240-97";
const GOST_8509: &str = "ГОСТ 8509-93";
const GOST_8510: &str = "ГОСТ 8510-86";
const GOST_10704: &str = "ГОСТ 10704-91";
const GOST_30245: &str = "ГОСТ 30245-2003";

/// ГОСТ 26020-83: name, h, b, s, t, R, A cm².
const I_PARALLEL: [(&str, f64, f64, f64, f64, f64, f64); 55] = [
    ("10Б1", 100., 55., 4.1, 5.7, 7., 10.32),
    ("12Б1", 117.6, 64., 3.8, 5.1, 7., 11.03),
    ("12Б2", 120., 64., 4.4, 6.3, 7., 13.21),
    ("14Б1", 137.4, 73., 3.8, 5.6, 7., 13.39),
    ("14Б2", 140., 73., 4.7, 6.9, 7., 16.43),
    ("16Б1", 157., 82., 4.0, 5.9, 9., 16.18),
    ("16Б2", 160., 82., 5.0, 7.4, 9., 20.09),
    ("18Б1", 177., 91., 4.3, 6.5, 9., 19.58),
    ("18Б2", 180., 91., 5.3, 8.0, 9., 23.95),
    ("20Б1", 200., 100., 5.6, 8.5, 12., 28.49),
    ("23Б1", 230., 110., 5.6, 9.0, 12., 32.91),
    ("26Б1", 258., 120., 5.8, 8.5, 12., 35.62),
    ("26Б2", 261., 120., 6.0, 10.0, 12., 39.70),
    ("30Б1", 296., 140., 5.8, 8.5, 15., 41.92),
    ("30Б2", 299., 140., 6.0, 10.0, 15., 46.67),
    ("35Б1", 346., 155., 6.2, 8.5, 18., 49.53),
    ("35Б2", 349., 155., 6.5, 10.0, 18., 55.17),
    ("40Б1", 392., 165., 7.0, 9.5, 21., 61.25),
    ("40Б2", 396., 165., 7.5, 11.5, 21., 69.72),
    ("45Б1", 443., 180., 7.8, 11.0, 21., 76.23),
    ("45Б2", 447., 180., 8.4, 13.0, 21., 85.96),
    ("50Б1", 492., 200., 8.8, 12.0, 21., 92.98),
    ("50Б2", 496., 200., 9.2, 14.0, 21., 102.8),
    ("55Б1", 543., 220., 9.5, 13.5, 24., 113.37),
    ("55Б2", 547., 220., 10.0, 15.5, 24., 124.75),
    ("60Б1", 593., 230., 10.5, 15.5, 24., 135.26),
    ("60Б2", 597., 230., 11.0, 17.5, 24., 147.3),
    ("20Ш1", 193., 150., 6.0, 9.0, 13., 38.95),
    ("23Ш1", 226., 155., 6.5, 10.0, 14., 46.08),
    ("26Ш1", 251., 180., 7.0, 10.0, 16., 54.37),
    ("26Ш2", 255., 180., 7.5, 12.0, 16., 62.73),
    ("30Ш1", 291., 200., 8.0, 11.0, 18., 68.31),
    ("30Ш2", 295., 200., 8.5, 13.0, 18., 77.65),
    ("35Ш1", 338., 250., 9.5, 12.5, 20., 95.67),
    ("35Ш2", 341., 250., 10.0, 14.0, 20., 104.74),
    ("40Ш1", 388., 300., 9.5, 14.0, 22., 122.4),
    ("40Ш2", 392., 300., 11.5, 16.0, 22., 141.6),
    ("50Ш1", 484., 300., 11.0, 15.0, 26., 145.7),
    ("50Ш2", 489., 300., 14.5, 17.5, 26., 176.6),
    ("60Ш1", 580., 320., 12.0, 17.0, 28., 181.1),
    ("20К1", 195., 200., 6.5, 10.0, 13., 52.82),
    ("20К2", 198., 200., 7.0, 11.5, 13., 59.7),
    ("23К1", 227., 240., 7.0, 10.5, 14., 66.51),
    ("23К2", 230., 240., 8.0, 12.0, 14., 75.77),
    ("26К1", 255., 260., 8.0, 12.0, 16., 83.08),
    ("26К2", 258., 260., 9.0, 13.5, 16., 93.19),
    ("26К3", 262., 260., 10.0, 15.5, 16., 105.9),
    ("30К1", 296., 300., 9.0, 13.5, 18., 108.0),
    ("30К2", 300., 300., 10.0, 15.5, 18., 122.7),
    ("30К3", 304., 300., 11.5, 17.5, 18., 138.72),
    ("35К1", 343., 350., 10.0, 15.0, 20., 139.7),
    ("35К2", 348., 350., 11.0, 17.5, 20., 160.4),
    ("35К3", 353., 350., 13.0, 20.0, 20., 184.1),
    ("40К1", 393., 400., 11.0, 16.5, 22., 175.8),
    ("40К2", 400., 400., 13.0, 20.0, 22., 210.96),
];

/// Sourced reference tables (`data/reference`, see its `sources.md`).
const I_8239_CSV: &str = include_str!("../../../data/reference/i_beams_gost_8239_89.csv");
const I_57837_CSV: &str = include_str!("../../../data/reference/i_beams_gost_r_57837_2017.csv");
const CHANNELS_CSV: &str = include_str!("../../../data/reference/channels_gost_8240_97.csv");
const ANGLES_CSV: &str = include_str!("../../../data/reference/angles_equal_gost_8509_93.csv");
const ANGLES_UNEQUAL_CSV: &str =
    include_str!("../../../data/reference/angles_unequal_gost_8510_86.csv");
const PIPES_CSV: &str = include_str!("../../../data/reference/pipes_round_gost_10704_91.csv");
const TUBES_CSV: &str = include_str!("../../../data/reference/hollow_sections_gost_30245_2003.csv");
const STEELS_CSV: &str = include_str!("../../../data/reference/steel_grades.csv");

/// Rows of a reference table as `column → value`; quoted cells (notes) may
/// hold commas, the columns read here never do.
fn table(csv: &'static str) -> Vec<Row> {
    let mut lines = csv.lines().filter(|l| !l.trim().is_empty());
    let header: Vec<&'static str> = lines.next().expect("table header").split(',').collect();
    lines
        .map(|line| Row {
            header: header.clone(),
            cells: split_csv(line),
        })
        .collect()
}

fn split_csv(line: &'static str) -> Vec<&'static str> {
    let mut cells = Vec::new();
    let (mut start, mut quoted) = (0, false);
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => {
                cells.push(&line[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    cells.push(&line[start..]);
    cells
}

struct Row {
    header: Vec<&'static str>,
    cells: Vec<&'static str>,
}

impl Row {
    fn text(&self, column: &str) -> &'static str {
        let i = self
            .header
            .iter()
            .position(|h| *h == column)
            .unwrap_or_else(|| panic!("column {column}"));
        self.cells.get(i).copied().unwrap_or("")
    }
    fn num(&self, column: &str) -> f64 {
        self.text(column)
            .parse()
            .unwrap_or_else(|_| panic!("{column} of {}", self.cells[0]))
    }
}

/// ГОСТ 8239-89: №, h, b, s, t (mean flange), R, A cm².
fn i_sloped() -> Vec<(&'static str, f64, f64, f64, f64, f64, f64)> {
    table(I_8239_CSV)
        .iter()
        .map(|r| {
            let n = |c| r.num(c);
            (
                r.text("designation"),
                n("h"),
                n("b"),
                n("tw"),
                n("tf"),
                n("r"),
                n("area_cm2"),
            )
        })
        .collect()
}

/// ГОСТ Р 57837-2017 rows: designation, h, b, tw, tf, r, A cm².
fn i_57837() -> Vec<(&'static str, f64, f64, f64, f64, f64, f64)> {
    table(I_57837_CSV)
        .iter()
        .map(|r| {
            let n = |c| r.num(c);
            (
                r.text("designation"),
                n("h"),
                n("b"),
                n("tw"),
                n("tf"),
                n("r"),
                n("area_cm2"),
            )
        })
        .collect()
}

/// ГОСТ 8240-97, series У and П: №, h, b, s, t (mean flange), R, A cm².
fn channels() -> Vec<(&'static str, f64, f64, f64, f64, f64, f64)> {
    table(CHANNELS_CSV)
        .iter()
        .map(|r| {
            let n = |c| r.num(c);
            (
                r.text("designation"),
                n("h"),
                n("b"),
                n("tw"),
                n("tf"),
                n("r"),
                n("area_cm2"),
            )
        })
        .collect()
}

/// ГОСТ 8509-93: b, t, R, r (toe), A cm².
fn angles() -> Vec<(f64, f64, f64, f64, f64)> {
    table(ANGLES_CSV)
        .iter()
        .map(|r| {
            let n = |c| r.num(c);
            (n("b"), n("t"), n("r"), n("r2"), n("area_cm2"))
        })
        .collect()
}

/// ГОСТ 8510-86: B, b, t, R, r (toe), A cm².
fn angles_unequal() -> Vec<(f64, f64, f64, f64, f64, f64)> {
    table(ANGLES_UNEQUAL_CSV)
        .iter()
        .map(|r| {
            let n = |c| r.num(c);
            (n("B"), n("b"), n("t"), n("r"), n("r2"), n("area_cm2"))
        })
        .collect()
}

/// ГОСТ 10704-91: D, t (the area is exact).
fn pipes() -> Vec<(f64, f64)> {
    table(PIPES_CSV)
        .iter()
        .map(|r| (r.num("D"), r.num("t")))
        .collect()
}

/// ГОСТ 30245-2003: H, B, t, outer corner radius R, A cm².
fn tubes() -> Vec<(f64, f64, f64, f64, f64)> {
    table(TUBES_CSV)
        .iter()
        .map(|r| {
            let n = |c| r.num(c);
            (n("h"), n("b"), n("t"), n("r_outer"), n("area_cm2"))
        })
        .collect()
}

fn fmt(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        format!("{v}").replace('.', ",")
    }
}

/// Every catalogue section, in picker order.
pub fn sections() -> Vec<Entry> {
    all().to_vec()
}

/// The catalogue, read from the tables once.
fn all() -> &'static [Entry] {
    static ALL: std::sync::OnceLock<Vec<Entry>> = std::sync::OnceLock::new();
    ALL.get_or_init(read_sections)
}

fn read_sections() -> Vec<Entry> {
    let mut out = Vec::new();
    for (name, h, b, s, t, r, a) in I_PARALLEL {
        let family = if name.contains('Б') {
            Family::IBeamNormal
        } else if name.contains('Ш') {
            Family::IBeamWide
        } else {
            Family::IBeamColumn
        };
        out.push(Entry {
            name: name.into(),
            family,
            shape: ProfileShape::ISection {
                height: h,
                width: b,
                web: s,
                flange: t,
            },
            standard: GOST_26020,
            area_mm2: a * 100.0,
            radius: r,
        });
    }
    for (name, h, b, s, t, r, a) in i_57837() {
        let family = if name.contains('Б') {
            Family::IBeamNormal57837
        } else if name.contains('Ш') {
            Family::IBeamWide57837
        } else {
            Family::IBeamColumn57837
        };
        out.push(Entry {
            name: format!("{name}{SUFFIX_57837}"),
            family,
            shape: ProfileShape::ISection {
                height: h,
                width: b,
                web: s,
                flange: t,
            },
            standard: GOST_57837,
            area_mm2: a * 100.0,
            radius: r,
        });
    }
    for (n, h, b, s, t, r, a) in i_sloped() {
        out.push(Entry {
            name: format!("I{n}"),
            family: Family::IBeamSloped,
            shape: ProfileShape::ISection {
                height: h,
                width: b,
                web: s,
                flange: t,
            },
            standard: GOST_8239,
            area_mm2: a * 100.0,
            radius: r,
        });
    }
    for (n, h, b, s, t, r, a) in channels() {
        out.push(Entry {
            name: format!("[{}", n.replace('.', ",")),
            family: if n.ends_with('П') {
                Family::ChannelP
            } else {
                Family::Channel
            },
            shape: ProfileShape::Channel {
                height: h,
                width: b,
                web: s,
                flange: t,
            },
            standard: GOST_8240,
            area_mm2: a * 100.0,
            radius: r,
        });
    }
    for (b, t, r, _, a) in angles() {
        out.push(Entry {
            name: format!("L{}×{}", fmt(b), fmt(t)),
            family: Family::Angle,
            shape: ProfileShape::Angle {
                height: b,
                width: b,
                thickness: t,
            },
            standard: GOST_8509,
            area_mm2: a * 100.0,
            radius: r,
        });
    }
    for (big, b, t, r, _, a) in angles_unequal() {
        out.push(Entry {
            name: format!("L{}×{}×{}", fmt(big), fmt(b), fmt(t)),
            family: Family::AngleUnequal,
            shape: ProfileShape::Angle {
                height: big,
                width: b,
                thickness: t,
            },
            standard: GOST_8510,
            area_mm2: a * 100.0,
            radius: r,
        });
    }
    for (d, t) in pipes() {
        out.push(Entry {
            name: format!("Труба {}×{}", fmt(d), fmt(t)),
            family: Family::Pipe,
            shape: ProfileShape::CircularHollow {
                diameter: d,
                thickness: t,
            },
            standard: GOST_10704,
            area_mm2: std::f64::consts::PI * t * (d - t),
            radius: 0.0,
        });
    }
    for (h, b, t, r, a) in tubes() {
        let family = if h == b {
            Family::SquareTube
        } else {
            Family::RectTube
        };
        out.push(Entry {
            name: format!("Гн {}×{}×{}", fmt(h), fmt(b), fmt(t)),
            family,
            shape: ProfileShape::RectangularHollow {
                height: h,
                width: b,
                thickness: t,
            },
            standard: GOST_30245,
            area_mm2: a * 100.0,
            radius: r,
        });
    }
    out
}

/// A catalogue section by its exact name («20Б1», «I20», «[20У»…).
pub fn section(name: &str) -> Option<Entry> {
    all().iter().find(|e| e.name == name).cloned()
}

/// Sections whose name contains every word of `query`, case-insensitive
/// («30б», «ш1», «труба 219»).
pub fn search(query: &str) -> Vec<Entry> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    all()
        .iter()
        .filter(|e| {
            let name = e.name.to_lowercase();
            words.iter().all(|w| name.contains(w.as_str()))
        })
        .cloned()
        .collect()
}

/// Structural steel grades, ρ = 7850 kg/m³: every grade of ГОСТ 27772-2021
/// in СП 16.13330 tables В.3–В.5 (`steel_grades.csv`), then 09Г2С of
/// ГОСТ 19281-2014.
pub fn steels() -> Vec<(&'static str, &'static str)> {
    let mut out: Vec<(&'static str, &'static str)> = Vec::new();
    for row in table(STEELS_CSV) {
        let grade = row.text("grade");
        if !out.iter().any(|(g, _)| *g == grade) {
            out.push((grade, "ГОСТ 27772-2021"));
        }
    }
    out.sort_by_key(|(g, _)| steel_order(g));
    out.push(("09Г2С", "ГОСТ 19281-2014"));
    out
}

/// С235 < С245 < … < С440, and within a strength С345 < С345Б < С345Б-1 < С345К.
fn steel_order(grade: &str) -> (u32, String) {
    let digits: String = grade.chars().filter(char::is_ascii_digit).take(3).collect();
    (digits.parse().unwrap_or(0), grade.into())
}

/// A steel grade as a project material.
pub fn steel(id: ObjectId, name: &str) -> Option<Material> {
    steels()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(n, standard)| Material {
            id,
            // The specification takes the grade before « · ».
            name: format!("{n} · {standard}"),
            density_kg_m3: 7850.0,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    /// Area from the dimensions: flanges, web and the four root fillets.
    fn i_area(h: f64, b: f64, s: f64, t: f64, r: f64) -> f64 {
        2.0 * b * t + (h - 2.0 * t) * s + (4.0 - PI) * r * r
    }

    #[test]
    fn parallel_flange_beams_match_their_areas_within_one_percent() {
        for (name, h, b, s, t, r, a) in I_PARALLEL {
            let computed = i_area(h, b, s, t, r);
            let listed = a * 100.0;
            let err = (computed - listed).abs() / listed;
            assert!(err < 0.01, "{name}: {computed:.0} vs {listed:.0} mm²");
        }
    }

    #[test]
    fn gost_57837_beams_match_their_areas_within_one_percent() {
        let rows = i_57837();
        assert_eq!(rows.len(), 171);
        for (name, h, b, s, t, r, a) in rows {
            let computed = i_area(h, b, s, t, r);
            let listed = a * 100.0;
            let err = (computed - listed).abs() / listed;
            assert!(err < 0.01, "{name}: {computed:.0} vs {listed:.0} mm²");
        }
        // Same name, different section: 20Б1 is 100×5.5×8 here and 100×5.6×8.5
        // in ГОСТ 26020.
        let new = section("20Б1 (Р 57837)").unwrap();
        assert_eq!(new.standard, "ГОСТ Р 57837-2017");
        assert!((new.area_mm2 - 2716.0).abs() < 1.0);
        assert_eq!(section("20Б1").unwrap().standard, "ГОСТ 26020-83");
    }

    #[test]
    fn sloped_flange_beams_and_channels_match_their_areas() {
        // Sloped flanges and toe radii: the mean-thickness formula agrees
        // within 3 %.
        for (name, h, b, s, t, r, a) in i_sloped() {
            let err = (i_area(h, b, s, t, r) - a * 100.0).abs() / (a * 100.0);
            assert!(err < 0.03, "I{name}: {err:.3}");
        }
        let channels = channels();
        assert_eq!(channels.len(), 36);
        for (name, h, b, s, t, r, a) in channels {
            let computed = 2.0 * b * t + (h - 2.0 * t) * s + (2.0 - PI / 2.0) * r * r;
            let err = (computed - a * 100.0).abs() / (a * 100.0);
            assert!(err < 0.03, "[{name}: {err:.3}");
        }
    }

    #[test]
    fn angles_match_their_areas_within_one_percent() {
        let angles = angles();
        assert_eq!(angles.len(), 89);
        for (b, t, r, toe, a) in angles {
            let computed = t * (2.0 * b - t) + (1.0 - PI / 4.0) * (r * r - 2.0 * toe * toe);
            let err = (computed - a * 100.0).abs() / (a * 100.0);
            assert!(err < 0.01, "L{b}×{t}: {err:.4}");
        }
        let unequal = angles_unequal();
        assert_eq!(unequal.len(), 62);
        for (big, b, t, r, toe, a) in unequal {
            let computed = t * (big + b - t) + (1.0 - PI / 4.0) * (r * r - 2.0 * toe * toe);
            let err = (computed - a * 100.0).abs() / (a * 100.0);
            assert!(err < 0.01, "L{big}×{b}×{t}: {err:.4}");
        }
    }

    #[test]
    fn tubes_match_their_areas_with_the_standard_corner_radii() {
        let tubes = tubes();
        assert_eq!(tubes.len(), 577);
        for (h, b, t, r, a) in tubes {
            // Inner radius R − t.
            let computed = 2.0 * t * (h + b - 2.0 * t) - (4.0 - PI) * (r * r - (r - t) * (r - t));
            let err = (computed - a * 100.0).abs() / (a * 100.0);
            assert!(err < 0.005, "{h}×{b}×{t}: {err:.4}");
        }
    }

    #[test]
    fn every_section_is_valid_named_once_and_findable() {
        let all = sections();
        let mut names = std::collections::BTreeSet::new();
        for e in &all {
            assert!(names.insert(e.name.clone()), "duplicate {}", e.name);
            let sharp = e.shape.area_mm2().unwrap();
            // The sharp model and the catalogue area differ only by fillets
            // (thick tubes with R = 2,5t: up to 11 %).
            assert!(
                (sharp - e.area_mm2).abs() / e.area_mm2 < 0.12,
                "{}: {sharp:.0} vs {:.0}",
                e.name,
                e.area_mm2
            );
            assert_eq!(section(&e.name).as_ref(), Some(e));
        }
        assert!(all.len() > 1200);
        assert_eq!(search("30б").len(), 2 + 4);
        assert_eq!(search("30б 57837").len(), 4);
        assert_eq!(search("Труба 219×").len(), 14);
        assert!((section("20Б1").unwrap().mass_kg_m() - 22.4).abs() < 0.05);
        assert!((section("I20").unwrap().mass_kg_m() - 21.0).abs() < 0.05);
    }

    #[test]
    fn names_of_earlier_versions_still_resolve() {
        // Projects and property files store these names.
        for name in [
            "I20",
            "[6,5У",
            "[20У",
            "[40У",
            "L50×4",
            "L200×14",
            "Труба 57×3,5",
            "Труба 426×10",
            "Гн 60×60×4",
            "Гн 200×160×8",
            "20Б1",
            "30К2 (Р 57837)",
        ] {
            assert!(section(name).is_some(), "{name}");
        }
        assert_eq!(section("[16аП").unwrap().family, Family::ChannelP);
        let l = section("L100×63×8").unwrap();
        assert_eq!(l.standard, "ГОСТ 8510-86");
        assert!((l.mass_kg_m() - 9.87).abs() < 0.1, "{}", l.mass_kg_m());
    }

    #[test]
    fn steels_cover_gost_27772_in_strength_order() {
        let grades: Vec<&str> = steels().into_iter().map(|(g, _)| g).collect();
        for g in [
            "С235", "С245", "С255", "С345", "С345К", "С355", "С390", "С440", "09Г2С",
        ] {
            assert!(grades.contains(&g), "{g}");
        }
        let pos = |g| grades.iter().position(|x| *x == g).unwrap();
        assert!(
            pos("С235") < pos("С255") && pos("С255") < pos("С345") && pos("С390") < pos("С440")
        );
        assert_eq!(
            steel(ObjectId(1), "С345").unwrap().name,
            "С345 · ГОСТ 27772-2021"
        );
    }
}
