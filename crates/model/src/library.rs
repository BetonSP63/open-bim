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
    Angle,
    Pipe,
    SquareTube,
    RectTube,
}

impl Family {
    pub const ALL: [Self; 12] = [
        Self::IBeamNormal57837,
        Self::IBeamWide57837,
        Self::IBeamColumn57837,
        Self::IBeamNormal,
        Self::IBeamWide,
        Self::IBeamColumn,
        Self::IBeamSloped,
        Self::Channel,
        Self::Angle,
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
            Self::Channel => "Швеллеры У (ГОСТ 8240)",
            Self::Angle => "Уголки равнополочные (ГОСТ 8509)",
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
const GOST_10704: &str = "ГОСТ 10704-91";
const GOST_30245: &str = "ГОСТ 30245-2003";

/// ГОСТ 8239-89: №, h, b, s, t (mean flange), R, A cm².
const I_SLOPED: [(&str, f64, f64, f64, f64, f64, f64); 17] = [
    ("10", 100., 55., 4.5, 7.2, 7.0, 12.0),
    ("12", 120., 64., 4.8, 7.3, 7.5, 14.7),
    ("14", 140., 73., 4.9, 7.5, 8.0, 17.4),
    ("16", 160., 81., 5.0, 7.8, 8.5, 20.2),
    ("18", 180., 90., 5.1, 8.1, 9.0, 23.4),
    ("20", 200., 100., 5.2, 8.4, 9.5, 26.8),
    ("22", 220., 110., 5.4, 8.7, 10.0, 30.6),
    ("24", 240., 115., 5.6, 9.5, 10.5, 34.8),
    ("27", 270., 125., 6.0, 9.8, 11.0, 40.2),
    ("30", 300., 135., 6.5, 10.2, 12.0, 46.5),
    ("33", 330., 140., 7.0, 11.2, 13.0, 53.8),
    ("36", 360., 145., 7.5, 12.3, 14.0, 61.9),
    ("40", 400., 155., 8.3, 13.0, 15.0, 72.6),
    ("45", 450., 160., 9.0, 14.2, 16.0, 84.7),
    ("50", 500., 170., 10.0, 15.2, 17.0, 100.0),
    ("55", 550., 180., 11.0, 16.5, 18.0, 118.0),
    ("60", 600., 190., 12.0, 17.8, 20.0, 138.0),
];

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

/// ГОСТ 8240-97, series У: №, h, b, s, t (mean flange), R, A cm².
const CHANNELS: [(&str, f64, f64, f64, f64, f64, f64); 16] = [
    ("5У", 50., 32., 4.4, 7.0, 6.0, 6.16),
    ("6,5У", 65., 36., 4.4, 7.2, 6.0, 7.51),
    ("8У", 80., 40., 4.5, 7.4, 6.5, 8.98),
    ("10У", 100., 46., 4.5, 7.6, 7.0, 10.9),
    ("12У", 120., 52., 4.8, 7.8, 7.5, 13.3),
    ("14У", 140., 58., 4.9, 8.1, 8.0, 15.6),
    ("16У", 160., 64., 5.0, 8.4, 8.5, 18.1),
    ("18У", 180., 70., 5.1, 8.7, 9.0, 20.7),
    ("20У", 200., 76., 5.2, 9.0, 9.5, 23.4),
    ("22У", 220., 82., 5.4, 9.5, 10.0, 26.7),
    ("24У", 240., 90., 5.6, 10.0, 10.5, 30.6),
    ("27У", 270., 95., 6.0, 10.5, 11.0, 35.2),
    ("30У", 300., 100., 6.5, 11.0, 12.0, 40.5),
    ("33У", 330., 105., 7.0, 11.7, 13.0, 46.5),
    ("36У", 360., 110., 7.5, 12.6, 14.0, 53.4),
    ("40У", 400., 115., 8.0, 13.5, 15.0, 61.5),
];

/// ГОСТ Р 57837-2017 with Amendment 1, from the sourced reference table.
const I_57837_CSV: &str = include_str!("../../../data/reference/i_beams_gost_r_57837_2017.csv");

/// ГОСТ Р 57837-2017 rows: designation, h, b, tw, tf, r, A cm².
fn i_57837() -> Vec<(&'static str, f64, f64, f64, f64, f64, f64)> {
    I_57837_CSV
        .lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let f: Vec<&str> = line.splitn(9, ',').collect();
            let n = |i: usize| {
                f[i].parse::<f64>()
                    .expect("number in the ГОСТ Р 57837 table")
            };
            (f[0], n(2), n(3), n(4), n(5), n(6), n(7))
        })
        .collect()
}

/// ГОСТ 8509-93: b, t, R, A cm².
const ANGLES: [(f64, f64, f64, f64); 23] = [
    (50., 4., 5.5, 3.89),
    (50., 5., 5.5, 4.80),
    (63., 5., 7.0, 6.13),
    (63., 6., 7.0, 7.28),
    (70., 6., 8.0, 8.15),
    (70., 7., 8.0, 9.42),
    (75., 6., 9.0, 8.78),
    (75., 8., 9.0, 11.5),
    (80., 6., 9.0, 9.38),
    (80., 8., 9.0, 12.3),
    (90., 7., 10.0, 12.28),
    (90., 8., 10.0, 13.93),
    (100., 8., 12.0, 15.6),
    (100., 10., 12.0, 19.24),
    (110., 8., 12.0, 17.2),
    (125., 8., 14.0, 19.69),
    (125., 10., 14.0, 24.33),
    (140., 10., 14.0, 27.33),
    (160., 10., 16.0, 31.43),
    (160., 12., 16.0, 37.39),
    (180., 12., 16.0, 42.19),
    (200., 12., 18.0, 47.1),
    (200., 14., 18.0, 54.6),
];

/// ГОСТ 10704-91: D, t (common sizes; the area is exact).
const PIPES: [(f64, f64); 14] = [
    (57., 3.5),
    (76., 3.5),
    (89., 4.0),
    (108., 4.0),
    (114., 4.5),
    (127., 5.0),
    (133., 5.0),
    (159., 6.0),
    (168., 6.0),
    (219., 6.0),
    (219., 8.0),
    (273., 8.0),
    (325., 8.0),
    (426., 10.0),
];

/// ГОСТ 30245-2003 (common sizes): H, B, t, outer corner radius R, A cm².
const TUBES: [(f64, f64, f64, f64, f64); 18] = [
    (60., 60., 4., 8., 8.55),
    (80., 80., 4., 8., 11.75),
    (80., 80., 5., 10., 14.36),
    (100., 100., 5., 10., 18.36),
    (100., 100., 6., 12., 21.63),
    (120., 120., 6., 12., 26.43),
    (140., 140., 6., 12., 31.23),
    (160., 160., 6., 12., 36.03),
    (160., 160., 8., 20., 46.44),
    (180., 180., 8., 20., 52.84),
    (200., 200., 8., 20., 59.24),
    (80., 60., 4., 8., 10.15),
    (100., 60., 4., 8., 11.75),
    (120., 80., 5., 10., 18.36),
    (140., 100., 6., 12., 26.43),
    (160., 120., 6., 12., 31.23),
    (180., 140., 6., 12., 36.03),
    (200., 160., 8., 20., 52.84),
];

fn fmt(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        format!("{v}").replace('.', ",")
    }
}

/// Every catalogue section, in picker order.
pub fn sections() -> Vec<Entry> {
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
    for (n, h, b, s, t, r, a) in I_SLOPED {
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
    for (n, h, b, s, t, r, a) in CHANNELS {
        out.push(Entry {
            name: format!("[{n}"),
            family: Family::Channel,
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
    for (b, t, r, a) in ANGLES {
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
    for (d, t) in PIPES {
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
    for (h, b, t, r, a) in TUBES {
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
    sections().into_iter().find(|e| e.name == name)
}

/// Sections whose name contains every word of `query`, case-insensitive
/// («30б», «ш1», «труба 219»).
pub fn search(query: &str) -> Vec<Entry> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    sections()
        .into_iter()
        .filter(|e| {
            let name = e.name.to_lowercase();
            words.iter().all(|w| name.contains(w.as_str()))
        })
        .collect()
}

/// Structural steel grades (ГОСТ 27772-2021, ГОСТ 19281-2014), ρ = 7850 kg/m³.
pub fn steels() -> Vec<(&'static str, &'static str)> {
    vec![
        ("С245", "ГОСТ 27772-2021"),
        ("С255", "ГОСТ 27772-2021"),
        ("С345", "ГОСТ 27772-2021"),
        ("С355", "ГОСТ 27772-2021"),
        ("С390", "ГОСТ 27772-2021"),
        ("09Г2С", "ГОСТ 19281-2014"),
    ]
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
        for (name, h, b, s, t, r, a) in I_SLOPED {
            let err = (i_area(h, b, s, t, r) - a * 100.0).abs() / (a * 100.0);
            assert!(err < 0.03, "I{name}: {err:.3}");
        }
        for (name, h, b, s, t, r, a) in CHANNELS {
            let computed = 2.0 * b * t + (h - 2.0 * t) * s + (2.0 - PI / 2.0) * r * r;
            let err = (computed - a * 100.0).abs() / (a * 100.0);
            assert!(err < 0.03, "[{name}: {err:.3}");
        }
    }

    #[test]
    fn angles_match_their_areas_within_one_percent() {
        for (b, t, r, a) in ANGLES {
            let toe = r / 3.0;
            let computed = t * (2.0 * b - t) + (1.0 - PI / 4.0) * (r * r - 2.0 * toe * toe);
            let err = (computed - a * 100.0).abs() / (a * 100.0);
            assert!(err < 0.01, "L{b}×{t}: {err:.4}");
        }
    }

    #[test]
    fn tubes_match_their_areas_with_the_standard_corner_radii() {
        for (h, b, t, r, a) in TUBES {
            // ГОСТ 30245: R = 2t up to t = 6 and 2.5t above; inner radius R − t.
            assert_eq!(r, if t <= 6.0 { 2.0 * t } else { 2.5 * t }, "{h}×{b}×{t}");
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
            // The sharp model and the catalogue area differ only by fillets.
            assert!(
                (sharp - e.area_mm2).abs() / e.area_mm2 < 0.10,
                "{}: {sharp:.0} vs {:.0}",
                e.name,
                e.area_mm2
            );
            assert_eq!(section(&e.name).as_ref(), Some(e));
        }
        assert!(all.len() > 130);
        assert_eq!(search("30б").len(), 2 + 4);
        assert_eq!(search("30б 57837").len(), 4);
        assert_eq!(search("Труба 219").len(), 2);
        assert!((section("20Б1").unwrap().mass_kg_m() - 22.4).abs() < 0.05);
        assert!((section("I20").unwrap().mass_kg_m() - 21.0).abs() < 0.05);
    }
}
