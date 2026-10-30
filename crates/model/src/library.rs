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

/// Hot-rolled plate: thicknesses and sheet sizes per ГОСТ 19903-2015.
pub const PLATE_STANDARD: &str = "ГОСТ 19903-2015";

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
    IBeamExtraNormal57837,
    IBeamExtraColumn57837,
    IBeamPile57837,
    IBeamNormalSto,
    IBeamWideSto,
    IBeamColumnSto,
    TeeNormal,
    TeeWide,
    TeeColumn,
    Channel,
    ChannelP,
    Angle,
    AngleUnequal,
    Pipe,
    SquareTube,
    RectTube,
    PipeWaterGas,
}

impl Family {
    pub const ALL: [Self; 24] = [
        Self::IBeamNormal57837,
        Self::IBeamWide57837,
        Self::IBeamColumn57837,
        Self::IBeamExtraNormal57837,
        Self::IBeamExtraColumn57837,
        Self::IBeamPile57837,
        Self::IBeamNormal,
        Self::IBeamWide,
        Self::IBeamColumn,
        Self::IBeamSloped,
        Self::IBeamNormalSto,
        Self::IBeamWideSto,
        Self::IBeamColumnSto,
        Self::TeeNormal,
        Self::TeeWide,
        Self::TeeColumn,
        Self::Channel,
        Self::ChannelP,
        Self::Angle,
        Self::AngleUnequal,
        Self::Pipe,
        Self::PipeWaterGas,
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
            Self::IBeamExtraNormal57837 => "Двутавры дополнительные балочные ДБ (ГОСТ Р 57837)",
            Self::IBeamExtraColumn57837 => "Двутавры дополнительные колонные ДК (ГОСТ Р 57837)",
            Self::IBeamPile57837 => "Двутавры свайные С (ГОСТ Р 57837)",
            Self::IBeamNormalSto => "Двутавры балочные Б (СТО АСЧМ 20-93)",
            Self::IBeamWideSto => "Двутавры широкополочные Ш (СТО АСЧМ 20-93)",
            Self::IBeamColumnSto => "Двутавры колонные К (СТО АСЧМ 20-93)",
            Self::TeeNormal => "Тавры из двутавров Б (ГОСТ 26020)",
            Self::TeeWide => "Тавры из двутавров Ш (ГОСТ 26020)",
            Self::TeeColumn => "Тавры из двутавров К (ГОСТ 26020)",
            Self::Channel => "Швеллеры с уклоном полок У (ГОСТ 8240)",
            Self::ChannelP => "Швеллеры с параллельными гранями полок П (ГОСТ 8240)",
            Self::Angle => "Уголки равнополочные (ГОСТ 8509)",
            Self::AngleUnequal => "Уголки неравнополочные (ГОСТ 8510)",
            Self::Pipe => "Трубы электросварные (ГОСТ 10704)",
            Self::PipeWaterGas => "Трубы водогазопроводные ВГП (ГОСТ 3262)",
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
const GOST_3262: &str = "ГОСТ 3262-75";
const STO_ASCM: &str = "СТО АСЧМ 20-93";
/// Suffix that keeps СТО АСЧМ 20-93 names apart from ГОСТ 26020 and
/// ГОСТ Р 57837.
const SUFFIX_STO: &str = " (СТО АСЧМ)";
/// Prefix of a tee cut from a ГОСТ 26020 beam along the web axis.
const TEE_PREFIX: &str = "Тавр ½";
const GOST_30245: &str = "ГОСТ 30245-2003";

/// Sourced reference tables (`data/reference`, see its `sources.md`).
const I_8239_CSV: &str = include_str!("../../../data/reference/i_beams_gost_8239_89.csv");
const I_26020_CSV: &str = include_str!("../../../data/reference/i_beams_gost_26020_83.csv");
const I_57837_CSV: &str = include_str!("../../../data/reference/i_beams_gost_r_57837_2017.csv");
const I_STO_CSV: &str = include_str!("../../../data/reference/i_beams_sto_ascm_20_93.csv");
const CHANNELS_CSV: &str = include_str!("../../../data/reference/channels_gost_8240_97.csv");
const ANGLES_CSV: &str = include_str!("../../../data/reference/angles_equal_gost_8509_93.csv");
const ANGLES_UNEQUAL_CSV: &str =
    include_str!("../../../data/reference/angles_unequal_gost_8510_86.csv");
const PIPES_CSV: &str = include_str!("../../../data/reference/pipes_round_gost_10704_91.csv");
const PIPES_3262_CSV: &str =
    include_str!("../../../data/reference/pipes_water_gas_gost_3262_75.csv");
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

/// Parallel-flange beam rows: designation, h, b, tw, tf, r, A cm².
fn i_rows(csv: &'static str) -> Vec<(&'static str, f64, f64, f64, f64, f64, f64)> {
    table(csv)
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

/// ГОСТ 26020-83, series Б, Ш and К.
fn i_26020() -> Vec<(&'static str, f64, f64, f64, f64, f64, f64)> {
    i_rows(I_26020_CSV)
}

/// ГОСТ Р 57837-2017.
fn i_57837() -> Vec<(&'static str, f64, f64, f64, f64, f64, f64)> {
    i_rows(I_57837_CSV)
}

/// СТО АСЧМ 20-93, series Б, Ш and К.
fn i_sto() -> Vec<(&'static str, f64, f64, f64, f64, f64, f64)> {
    i_rows(I_STO_CSV)
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

/// ГОСТ 3262-75: nominal bore Dy, D, t.
fn pipes_3262() -> Vec<(&'static str, f64, f64)> {
    table(PIPES_3262_CSV)
        .iter()
        .map(|r| (r.text("Dy"), r.num("D"), r.num("t")))
        .collect()
}

/// Half of an I-section cut along the web axis, flange up, centred on its
/// bounds like the other shapes.
fn tee(h: f64, b: f64, s: f64, t: f64) -> ProfileShape {
    let (y0, y1) = (-h / 4.0, h / 4.0);
    ProfileShape::Polygon {
        outer: vec![
            [-s / 2.0, y0],
            [s / 2.0, y0],
            [s / 2.0, y1 - t],
            [b / 2.0, y1 - t],
            [b / 2.0, y1],
            [-b / 2.0, y1],
            [-b / 2.0, y1 - t],
            [-s / 2.0, y1 - t],
        ],
        voids: Vec::new(),
    }
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
    let parallel = i_26020();
    for &(name, h, b, s, t, r, a) in &parallel {
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
        let family = if name.contains("ДБ") {
            Family::IBeamExtraNormal57837
        } else if name.contains("ДК") {
            Family::IBeamExtraColumn57837
        } else if name.contains('С') {
            Family::IBeamPile57837
        } else if name.contains('Б') {
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
    for (name, h, b, s, t, r, a) in i_sto() {
        let family = if name.contains('Б') {
            Family::IBeamNormalSto
        } else if name.contains('Ш') {
            Family::IBeamWideSto
        } else {
            Family::IBeamColumnSto
        };
        out.push(Entry {
            name: format!("{name}{SUFFIX_STO}"),
            family,
            shape: ProfileShape::ISection {
                height: h,
                width: b,
                web: s,
                flange: t,
            },
            standard: STO_ASCM,
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
    for &(name, h, b, s, t, r, a) in &parallel {
        let family = if name.contains('Б') {
            Family::TeeNormal
        } else if name.contains('Ш') {
            Family::TeeWide
        } else {
            Family::TeeColumn
        };
        out.push(Entry {
            name: format!("{TEE_PREFIX}{name}"),
            family,
            shape: tee(h, b, s, t),
            standard: GOST_26020,
            area_mm2: a * 50.0,
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
    for (dy, d, t) in pipes_3262() {
        out.push(Entry {
            name: format!("Труба ВГП {dy}×{}", fmt(t)),
            family: Family::PipeWaterGas,
            shape: ProfileShape::CircularHollow {
                diameter: d,
                thickness: t,
            },
            standard: GOST_3262,
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
        let rows = i_26020();
        assert_eq!(rows.len(), 65);
        for (name, h, b, s, t, r, a) in rows {
            let computed = i_area(h, b, s, t, r);
            let listed = a * 100.0;
            let err = (computed - listed).abs() / listed;
            assert!(err < 0.01, "{name}: {computed:.0} vs {listed:.0} mm²");
        }
    }

    #[test]
    fn gost_57837_beams_match_their_areas_within_one_percent() {
        let rows = i_57837();
        assert_eq!(rows.len(), 340);
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
        // The official 2019 edition with Amendment 1: all series.
        for (name, family) in [
            ("40К19 (Р 57837)", Family::IBeamColumn57837),
            ("35К1.5 (Р 57837)", Family::IBeamColumn57837),
            ("100ДБ3 (Р 57837)", Family::IBeamExtraNormal57837),
            ("10ДК0 (Р 57837)", Family::IBeamExtraColumn57837),
            ("40С3 (Р 57837)", Family::IBeamPile57837),
        ] {
            assert_eq!(section(name).unwrap().family, family, "{name}");
        }
        assert!(section("20Б0 (Р 57837)").is_none());
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
        // ГОСТ 26020, Р 57837, СТО АСЧМ and the two tees.
        assert_eq!(search("30б").len(), 2 + 4 + 2 + 2);
        assert_eq!(search("30дб").len(), 8);
        assert_eq!(search("30б 57837").len(), 4);
        assert_eq!(search("Труба 219×").len(), 14);
        assert!((section("20Б1").unwrap().mass_kg_m() - 22.4).abs() < 0.05);
        assert!((section("I20").unwrap().mass_kg_m() - 21.0).abs() < 0.05);
    }

    #[test]
    fn sto_beams_match_their_areas_and_keep_their_own_names() {
        let rows = i_sto();
        assert_eq!(rows.len(), 74);
        for (name, h, b, s, t, r, a) in rows {
            let err = (i_area(h, b, s, t, r) - a * 100.0).abs() / (a * 100.0);
            assert!(err < 0.01, "{name}: {err:.4}");
        }
        // Same name, other sections: 20Б1 is 200×100×5.5×8 here.
        let sto = section("20Б1 (СТО АСЧМ)").unwrap();
        assert_eq!(sto.standard, "СТО АСЧМ 20-93");
        assert_eq!(sto.family, Family::IBeamNormalSto);
        assert!((sto.area_mm2 - 2716.0).abs() < 1.0);
        assert_eq!(
            section("100Ш4 (СТО АСЧМ)").unwrap().family,
            Family::IBeamWideSto
        );
        assert_eq!(
            section("40К5 (СТО АСЧМ)").unwrap().family,
            Family::IBeamColumnSto
        );
    }

    #[test]
    fn deep_gost_26020_beams_are_listed() {
        for name in ["70Б1", "80Б2", "90Б1", "100Б4"] {
            assert_eq!(section(name).unwrap().standard, "ГОСТ 26020-83", "{name}");
        }
        // 100Б4: A = 400,6 cm², 314,5 kg/m.
        assert!((section("100Б4").unwrap().mass_kg_m() - 314.5).abs() < 0.1);
    }

    #[test]
    fn tees_are_halves_of_their_beams() {
        let beams: Vec<_> = sections()
            .into_iter()
            .filter(|e| {
                matches!(
                    e.family,
                    Family::IBeamNormal | Family::IBeamWide | Family::IBeamColumn
                )
            })
            .collect();
        for beam in &beams {
            let tee = section(&format!("Тавр ½{}", beam.name)).unwrap();
            assert_eq!(tee.standard, beam.standard);
            assert!((2.0 * tee.area_mm2 - beam.area_mm2).abs() < 1e-6);
            let half = beam.shape.area_mm2().unwrap() / 2.0;
            assert!(
                (tee.shape.area_mm2().unwrap() - half).abs() < 1e-6,
                "{}",
                tee.name
            );
            let ProfileShape::ISection { height, width, .. } = beam.shape else {
                panic!()
            };
            let [x0, x1, y0, y1] = tee.shape.bounds().unwrap();
            assert!((x1 - x0 - width).abs() < 1e-9 && (y1 - y0 - height / 2.0).abs() < 1e-9);
            assert!((x0 + x1).abs() < 1e-9 && (y0 + y1).abs() < 1e-9);
        }
        let t = section("Тавр ½30Ш1").unwrap();
        assert_eq!(t.family, Family::TeeWide);
        assert!((t.mass_kg_m() - 26.8).abs() < 0.05, "{}", t.mass_kg_m());
    }

    #[test]
    fn pipes_cover_gost_10704_and_water_gas_pipes() {
        let pipes = pipes();
        assert_eq!(pipes.len(), 803);
        let (dmin, dmax) = pipes
            .iter()
            .fold((f64::MAX, 0.0f64), |(a, b), (d, _)| (a.min(*d), b.max(*d)));
        assert_eq!((dmin, dmax), (10.0, 1420.0));
        // The mass of the tables is the exact formula of the standards.
        for row in table(PIPES_CSV).iter().chain(table(PIPES_3262_CSV).iter()) {
            let (d, t, m) = (row.num("D"), row.num("t"), row.num("mass_kg_m"));
            let f = 0.0246615 * t * (d - t);
            assert!((f - m).abs() <= 0.011 + 0.002 * m, "{d}×{t}: {m} vs {f:.3}");
        }
        assert!(section("Труба 1420×20").is_some());
        assert!(section("Труба 159×8").is_none());
        let vgp = section("Труба ВГП 20×2,8").unwrap();
        assert_eq!(vgp.standard, "ГОСТ 3262-75");
        assert_eq!(vgp.family, Family::PipeWaterGas);
        assert_eq!(
            vgp.shape,
            ProfileShape::CircularHollow {
                diameter: 26.8,
                thickness: 2.8
            }
        );
        assert!((vgp.mass_kg_m() - 1.66).abs() < 0.01);
        assert_eq!(table(PIPES_3262_CSV).len(), 47);
        assert_eq!(search("вгп 15×").len(), 4);
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
