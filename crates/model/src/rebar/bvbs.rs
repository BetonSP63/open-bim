//! BVBS BF2D export of reinforcing bars, one line per schedule position.
//!
//! The record follows the BVBS guideline «Data exchange of reinforcement
//! data», version 3.1 (14 June 2021), main group BF2D. Header fields are in
//! the fixed order `j r i p l n e d g s v`. The geometry block is legs `l`
//! and the bend after each leg `w`; a bar ends with `w0`. The mandrel
//! diameter is header field `s` (one value for the whole shape).
//!
//! The checksum is the guideline's plotter checksum: the sum of the bytes
//! from the start of the record through the `C` of the checksum block,
//! then `96 - (sum mod 32)`. For ASCII this is the sum of character codes.
//! The file is UTF-8, so a Cyrillic project name is summed as its UTF-8
//! bytes — the same sum a reader obtains from the file bytes.
//!
//! `l` in the header is the sum of the leg lengths. `e` is the nominal mass
//! of one bar from the detail schedule (the cut length, with the arc
//! correction of the model). Those two differ by the bend deduction; a
//! bending machine takes the shape from the geometry block.
use crate::{DrawingKind, Project};

use super::schedule::{self, BendLeg, DetailRow};

/// Guideline cited by the export and the documentation.
pub const SPEC: &str = "BVBS-Richtlinie 3.1 (2021-06-14), BF2D";

/// Checksum of the record prefix that already ends with the `C` of the
/// checksum block. Result is an integer from 65 to 96.
pub fn checksum(prefix_through_c: &str) -> u32 {
    let sum: u64 = prefix_through_c.bytes().map(u64::from).sum();
    96 - (sum % 32) as u32
}

/// Whether `line` (without CR/LF) ends with the checksum the guideline
/// computes for the bytes before it.
pub fn checksum_matches(line: &str) -> bool {
    let line = line.trim_end_matches(['\r', '\n']);
    let Some(at) = line.rfind("@C") else {
        return false;
    };
    let prefix = &line[..=at + 1];
    let Some(digits) = line[at + 2..].strip_suffix('@') else {
        return false;
    };
    digits.parse::<u32>().ok() == Some(checksum(prefix))
}

/// Absolute BF2D file: one record per position, CRLF, nothing else.
pub fn file(project: &Project) -> String {
    let header = header_of(project);
    let mut out = String::new();
    for row in schedule::detail_schedule(project) {
        out.push_str(&position_line(&header, project, &row));
        out.push_str("\r\n");
    }
    out
}

struct Header {
    project: String,
    plan: String,
    index: String,
}

fn header_of(project: &Project) -> Header {
    let plan = project
        .drawings
        .values()
        .filter(|d| matches!(d.kind, DrawingKind::Reinforcement { .. }))
        .min_by_key(|d| d.id.0)
        .map(|d| {
            let number = d.number.trim();
            if number.is_empty() {
                d.title.trim().to_string()
            } else {
                number.to_string()
            }
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "КЖ".into());
    Header {
        project: field(&project.name),
        plan: field(&plan),
        index: field(&project.revision.to_string()),
    }
}

fn field(value: &str) -> String {
    let text: String = value
        .chars()
        .filter(|c| *c != '@' && !c.is_control())
        .collect();
    let text = text.trim();
    if text.is_empty() {
        "-".to_string()
    } else {
        text.to_string()
    }
}

fn position_line(header: &Header, project: &Project, row: &DetailRow) -> String {
    let legs = schedule::bend_legs(row);
    let length: i64 = legs.iter().map(|l| l.length_mm).sum();
    let mandrel = project
        .concrete
        .rebars
        .iter()
        .find(|s| row.sets.contains(&s.number))
        .map(|s| schedule::mandrel_diameter(project, s).round() as i64)
        .unwrap_or(1)
        .max(1);
    let prefix = format!(
        "BF2D@Hj{}@r{}@i{}@p{}@l{}@n{}@e{:.3}@d{}@g{}@s{}@v@{}C",
        header.project,
        header.plan,
        header.index,
        row.mark,
        length,
        row.count,
        row.unit_mass_kg,
        plain_number(row.diameter),
        field(&row.grade),
        mandrel,
        geometry(&legs),
    );
    format!("{prefix}{}@", checksum(&prefix))
}

fn geometry(legs: &[BendLeg]) -> String {
    let mut out = String::from("G");
    for leg in legs {
        out.push_str(&format!("l{}@w{}@", leg.length_mm, leg.angle_deg));
    }
    out
}

fn plain_number(value: f64) -> String {
    if (value - value.round()).abs() < 1e-6 && value.abs() < 1e12 {
        format!("{}", value.round() as i64)
    } else {
        let text = format!("{value:.3}");
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Element, ElementShape, Frame, LengthMm, Material, ObjectId, Point3, RebarKind, RebarSet,
    };
    use std::collections::BTreeMap;

    /// The worked example in the guideline: "abcde@C" → 78.
    #[test]
    fn guideline_checksum_examples() {
        assert_eq!(checksum("abcde@C"), 78);
        let samples = [
            (
                "BF2D@HjTestPDF@r417@ia@p1@l1000@n10@e0.888@d12@gB500A@s48@v@Gl400@w90@l600@w0@C",
                72u32,
            ),
            (
                "BF2D@HjTestPDF@r417@ia@p1@l800@n10@e0.710@d12@gB500A@s48@v@Gl100@w180@l600@w180@l100@w0@C",
                71,
            ),
            (
                "BF2D@HjTestPDF@r417@ia@p1@l1224@n10@e1.087@d12@gB500A@s48@v@Gl100@w90@l300@w45@l424@w-45@l300@w-90@l100@w0@C",
                82,
            ),
            (
                "BF2D@HjTestPDF@r417@ia@p1@l1428@n10@e1.268@d12@gB500A@s48@v@Gl400@w0@r400@w90@w0@l400@w0@C",
                79,
            ),
        ];
        for (prefix, ip) in samples {
            assert_eq!(checksum(prefix), ip, "{prefix}");
            let line = format!("{prefix}{ip}@");
            assert!(checksum_matches(&line), "{line}");
        }
    }

    #[test]
    fn a_straight_bar_matches_a_hand_built_record() {
        let mut project = Project {
            name: "P".into(),
            ..Project::default()
        };
        let mut set = RebarSet::basic(RebarKind::Single, 10., "A500C");
        set.number = 1;
        set.shape_id = Some("gost.straight".into());
        set.bars = vec![vec![Point3([0.; 3]), Point3([1000., 0., 0.])]];
        project.concrete.rebars.push(set);
        let text = file(&project);
        let line = text.trim_end_matches(['\r', '\n']);
        // Ø10, ribbed: mandrel 5·ds = 50. Mass of 1 m is 0.617 kg.
        // The class is stored as «А500С». No КЖ sheet: plan is «КЖ»,
        // index is the project revision. The checksum sums UTF-8 bytes.
        let prefix = "BF2D@HjP@rКЖ@i0@p1@l1000@n1@e0.617@d10@gА500С@s50@v@Gl1000@w0@C";
        assert_eq!(checksum(prefix), 89);
        assert_eq!(line, format!("{prefix}{}@", checksum(prefix)));
        assert!(checksum_matches(line));
        assert!(text.ends_with("\r\n"));
        assert_eq!(text.lines().count(), 1);
    }

    #[test]
    fn hooks_and_a_bend_become_legs_with_signed_angles() {
        let mut project = Project::default();
        let shape = super::super::shape::builtin_shapes()
            .into_iter()
            .find(|s| s.id == "gost.hooks")
            .unwrap();
        let bar = super::super::shape::instantiate(
            &shape,
            &BTreeMap::new(),
            12.,
            "А500С",
            &crate::concrete::ShapePlace {
                origin: Point3([0.; 3]),
                x: [1., 0., 0.],
                y: [0., 1., 0.],
                local_to_host: false,
            },
        )
        .unwrap()
        .bar;
        let mut set = RebarSet::basic(RebarKind::Shape, 12., "А500С");
        set.number = 1;
        set.shape_id = Some(shape.id);
        set.hooks = shape.hooks;
        set.bars = vec![bar];
        project.concrete.rebars.push(set);
        let line = file(&project);
        // 180° hooks, extension 3·ds = 36, shaft 1000. Two bends, three legs.
        assert!(line.contains("@Gl36@w180@l1000@w180@l36@w0@C"), "{line}");
        assert!(line.contains("@l1072@"));
        assert!(checksum_matches(line.lines().next().unwrap()));

        let mut bent = Project::default();
        let shape = super::super::shape::builtin_shapes()
            .into_iter()
            .find(|s| s.id == "gost.l")
            .unwrap();
        let bar = super::super::shape::instantiate(
            &shape,
            &BTreeMap::new(),
            12.,
            "А500С",
            &crate::concrete::ShapePlace {
                origin: Point3([0.; 3]),
                x: [1., 0., 0.],
                y: [0., 1., 0.],
                local_to_host: false,
            },
        )
        .unwrap()
        .bar;
        let mut set = RebarSet::basic(RebarKind::Shape, 12., "А500С");
        set.number = 1;
        set.shape_id = Some("gost.l".into());
        set.bars = vec![bar];
        bent.concrete.rebars.push(set);
        let line = file(&bent);
        assert!(line.contains("@Gl1000@w90@l500@w0@C"), "{line}");
        let row = schedule::detail_schedule(&bent)[0].clone();
        let leg_sum: i64 = schedule::bend_legs(&row).iter().map(|l| l.length_mm).sum();
        assert!(row.length_mm < leg_sum as f64 - 1.);
        assert!(line.contains(&format!("@e{:.3}@", row.unit_mass_kg)));
    }

    #[test]
    fn a_slab_writes_one_checked_line_per_position() {
        let (mut project, host) = slab();
        let mut ty = crate::concrete::SlabRebarType::new("s", "Плита");
        let layer = crate::concrete::MeshLayer::new(10., 200., 0.);
        ty.bottom = Some(layer.clone());
        ty.top = Some(layer);
        ty.edge_diameter = 12.;
        ty.frog_diameter = 8.;
        ty.frog_spacing = 600.;
        project.concrete.slab_types.push(ty);
        project.concrete = super::super::install_slab(&project, host, "s").unwrap();
        let rows = schedule::detail_schedule(&project);
        let text = file(&project);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), rows.len());
        assert!(lines.len() > 3, "{}", lines.len());
        let codes = ["ПР", "П", "Л"];
        for code in codes {
            assert!(
                rows.iter().any(|r| r.code == code),
                "{code} {:?}",
                rows.iter().map(|r| &r.code).collect::<Vec<_>>()
            );
        }
        for (line, row) in lines.iter().zip(&rows) {
            assert!(checksum_matches(line), "{line}");
            assert!(line.starts_with("BF2D@H"));
            assert!(line.contains(&format!("@p{}@", row.mark)));
            assert!(line.contains(&format!("@n{}@", row.count)));
            assert!(line.contains(&format!("@g{}@", row.grade)));
            let legs = schedule::bend_legs(row);
            let sum: i64 = legs.iter().map(|l| l.length_mm).sum();
            assert!(line.contains(&format!("@l{sum}@")), "{line}");
            if row.code == "П" {
                assert!(line.contains("@w90@"), "{line}");
            }
            if row.code == "Л" {
                assert!(line.contains("@w-90@"), "{line}");
            }
        }
        // Meshes, U-bars and frogs are separate positions, not one mesh record.
        assert!(!text.contains("BFMA"));
    }

    fn slab() -> (Project, ObjectId) {
        let mut project = Project {
            name: "Корпус".into(),
            next_id: 30,
            ..Project::default()
        };
        project.materials.insert(
            ObjectId(1),
            Material {
                id: ObjectId(1),
                name: "Бетон В25".into(),
                density_kg_m3: 2500.,
            },
        );
        let id = ObjectId(10);
        project.elements.insert(
            id,
            Element {
                id,
                name: "Перекрытие".into(),
                material: ObjectId(1),
                shape: ElementShape::Plate {
                    contour: vec![[0., 0.], [6000., 0.], [6000., 4000.], [0., 4000.]],
                    thickness: LengthMm::new(200.).unwrap(),
                    frame: Frame {
                        origin: Point3([0.; 3]),
                        ..Frame::default()
                    },
                },
                properties: BTreeMap::new(),
            },
        );
        (project, id)
    }
}
