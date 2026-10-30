//! Skewed end cuts, copes and openings as face contours, slotted holes and
//! the part mark.
use open_bim_model::{
    Detail, DetailKind, Element, ElementShape, Frame, LengthMm, MemberKind, ObjectId, Point3,
    Project,
};
use open_bim_nc::{Face, NcOptions, part_file, part_file_with};
use std::collections::BTreeMap;

const I_SECTION: ObjectId = ObjectId(2);

fn member(project: &mut Project, length: f64) -> ObjectId {
    let id = project.allocate_id().unwrap();
    project.elements.insert(
        id,
        Element {
            id,
            name: "Балка".into(),
            material: ObjectId(1),
            shape: ElementShape::Member {
                member_kind: MemberKind::Beam,
                profile: I_SECTION,
                length: LengthMm::new(length).unwrap(),
                frame: Frame::default(),
                position: Default::default(),
            },
            properties: BTreeMap::new(),
        },
    );
    id
}

fn plate(project: &mut Project, contour: Vec<[f64; 2]>, thickness: f64) -> ObjectId {
    let id = project.allocate_id().unwrap();
    project.elements.insert(
        id,
        Element {
            id,
            name: "Пластина".into(),
            material: ObjectId(1),
            shape: ElementShape::Plate {
                contour,
                thickness: LengthMm::new(thickness).unwrap(),
                frame: Frame::default(),
            },
            properties: BTreeMap::new(),
        },
    );
    id
}

fn detail(project: &mut Project, kind: DetailKind) {
    let id = project.allocate_id().unwrap();
    project.details.insert(id, Detail { id, kind });
}

fn header(text: &str) -> Vec<f64> {
    text.lines()
        .skip(10)
        .take(12)
        .map(|l| l.trim().parse().unwrap())
        .collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.01
}

/// Blocks of a kind («AK», «IK», «BO», «SI») as lists of split lines.
fn blocks<'a>(text: &'a str, name: &str) -> Vec<Vec<Vec<&'a str>>> {
    let mut out = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if line == name {
            let mut block = Vec::new();
            while let Some(next) = lines.peek() {
                if !next.starts_with("  ") {
                    break;
                }
                block.push(lines.next().unwrap().split_whitespace().collect());
            }
            out.push(block);
        }
    }
    out
}

#[test]
fn skewed_end_cut_gives_header_angle_length_and_web_contour() {
    let mut project = Project::default();
    let id = member(&mut project, 3000.0);
    // Start cut 15° in the web plane through the axis at z = 100, the top longer.
    let a = 15f64.to_radians();
    let normal = [0.0, -a.sin(), -a.cos()];
    detail(
        &mut project,
        DetailKind::PlaneCut {
            part: id,
            normal,
            offset: -a.cos() * 100.0,
        },
    );
    let file = part_file(&project, id, "1", 1).unwrap();
    assert!(file.warnings.is_empty(), "{:?}", file.warnings);
    let start = 100.0 - 150.0 * a.tan();
    assert!(close(file.length, 3000.0 - start), "{}", file.length);
    let h = header(&file.text);
    assert!(close(h[8], 15.0), "web start angle {h:?}");
    assert_eq!([h[9], h[10], h[11]], [0.0, 0.0, 0.0]);
    // The web shows the trapezoid; the flanges stay square to the axis.
    assert_eq!(file.contours.len(), 1, "{:?}", file.contours);
    let (face, inner, points) = &file.contours[0];
    assert_eq!((*face, *inner), (Face::Web, false));
    assert_eq!(points.len(), 4);
    assert!(
        points
            .iter()
            .any(|p| close(p[0], 0.0) && close(p[1], 300.0))
    );
    assert!(
        points
            .iter()
            .any(|p| close(p[0], 300.0 * a.tan()) && close(p[1], 0.0))
    );
}

#[test]
fn flange_skew_goes_to_the_flange_angle() {
    let mut project = Project::default();
    let id = member(&mut project, 2000.0);
    let a = 10f64.to_radians();
    // End cut, the +x flange edge longer.
    detail(
        &mut project,
        DetailKind::PlaneCut {
            part: id,
            normal: [-a.sin(), 0.0, a.cos()],
            offset: a.cos() * 1900.0,
        },
    );
    let file = part_file(&project, id, "1", 1).unwrap();
    let h = header(&file.text);
    assert!(close(h[11], 10.0), "flange end angle {h:?}");
    assert!(close(h[8], 0.0) && close(h[9], 0.0) && close(h[10], 0.0));
    assert!(close(file.length, 1900.0 + 75.0 * a.tan()));
    // On the flanges the cut is a slanted edge of the contour.
    let faces: Vec<Face> = file.contours.iter().map(|c| c.0).collect();
    assert_eq!(faces, [Face::Top, Face::Bottom]);
}

#[test]
fn cope_at_the_start_shapes_web_and_flange_contours() {
    let mut project = Project::default();
    let id = member(&mut project, 3000.0);
    // Takes the top flange and the upper 50 mm of the web over 100 mm.
    detail(
        &mut project,
        DetailKind::BoxCut {
            part: id,
            origin: Point3([-100.0, 100.0, -1.0]),
            size: [200.0, 100.0, 101.0],
        },
    );
    let file = part_file(&project, id, "1", 1).unwrap();
    assert!(file.warnings.is_empty(), "{:?}", file.warnings);
    assert!(close(file.length, 3000.0));
    let ak = blocks(&file.text, "AK");
    assert_eq!(ak.len(), 2, "{}", file.text);
    // Web: L-shape, 6 corners, closed.
    let web = &ak[0];
    assert_eq!(web.len(), 7);
    assert_eq!(web.first(), web.last());
    assert!(web.iter().all(|p| p[0] == "v"));
    let has = |block: &Vec<Vec<&str>>, x: f64, y: f64| {
        block
            .iter()
            .any(|p| close(p[1].parse().unwrap(), x) && close(p[2].parse().unwrap(), y))
    };
    assert!(has(web, 0.0, 250.0) && has(web, 100.0, 250.0) && has(web, 100.0, 300.0));
    // Top flange starts 100 mm later.
    let flange = &ak[1];
    assert!(flange.iter().all(|p| p[0] == "o"));
    assert!(has(flange, 100.0, 0.0) && has(flange, 3000.0, 150.0));
    assert!(!has(flange, 0.0, 0.0));
}

#[test]
fn web_opening_is_an_inner_contour() {
    let mut project = Project::default();
    let id = member(&mut project, 3000.0);
    detail(
        &mut project,
        DetailKind::BoxCut {
            part: id,
            origin: Point3([-10.0, -50.0, 1000.0]),
            size: [20.0, 100.0, 200.0],
        },
    );
    let file = part_file(&project, id, "1", 1).unwrap();
    let ik: Vec<_> = file.contours.iter().filter(|c| c.1).collect();
    assert_eq!(ik.len(), 1);
    assert_eq!(ik[0].0, Face::Web);
    let pts = &ik[0].2;
    assert_eq!(pts.len(), 4);
    // Inner contours run clockwise.
    let area: f64 = (0..4)
        .map(|i| pts[i][0] * pts[(i + 1) % 4][1] - pts[(i + 1) % 4][0] * pts[i][1])
        .sum();
    assert!(area < 0.0);
    assert!(file.text.contains("\nIK\n"));
    let ik_block = &blocks(&file.text, "IK")[0];
    assert_eq!(ik_block.len(), 5);
}

#[test]
fn cut_through_part_of_the_wall_is_reported() {
    let mut project = Project::default();
    let id = member(&mut project, 3000.0);
    // 3 mm deep into the 12 mm top flange.
    detail(
        &mut project,
        DetailKind::BoxCut {
            part: id,
            origin: Point3([-100.0, 147.0, 500.0]),
            size: [200.0, 10.0, 100.0],
        },
    );
    let file = part_file(&project, id, "1", 1).unwrap();
    assert_eq!(file.warnings.len(), 1, "{:?}", file.warnings);
    assert!(file.text.contains("\n** "));
}

#[test]
fn plate_corner_chamfer_and_opening() {
    let mut project = Project::default();
    let id = plate(
        &mut project,
        vec![[0.0, 0.0], [400.0, 0.0], [400.0, 200.0], [0.0, 200.0]],
        12.0,
    );
    // Chamfer 50×50 at the (400, 200) corner: keep x + y ≤ 550.
    detail(
        &mut project,
        DetailKind::PlaneCut {
            part: id,
            normal: [1.0, 1.0, 0.0],
            offset: 550.0,
        },
    );
    // Prism opening through the thickness.
    detail(
        &mut project,
        DetailKind::PrismCut {
            part: id,
            frame: Frame {
                origin: Point3([100.0, 50.0, -1.0]),
                ..Default::default()
            },
            contour: vec![[0.0, 0.0], [60.0, 0.0], [30.0, 80.0]],
            length: 20.0,
        },
    );
    let file = part_file(&project, id, "1", 1).unwrap();
    assert!(file.warnings.is_empty(), "{:?}", file.warnings);
    let outer: Vec<_> = file.contours.iter().filter(|c| !c.1).collect();
    let inner: Vec<_> = file.contours.iter().filter(|c| c.1).collect();
    assert_eq!(outer.len(), 1);
    assert_eq!(outer[0].2.len(), 5, "{:?}", outer[0].2);
    assert!(
        outer[0]
            .2
            .iter()
            .any(|p| close(p[0], 350.0) && close(p[1], 200.0))
    );
    assert!(
        outer[0]
            .2
            .iter()
            .any(|p| close(p[0], 400.0) && close(p[1], 150.0))
    );
    assert_eq!(inner.len(), 1);
    assert_eq!(inner[0].2.len(), 3);
    assert!(file.text.contains("\n  PL12x200\n"));
}

fn slotted_hole(project: &mut Project, part: ObjectId, at: [f64; 3], x: [f64; 3]) {
    let z = [0.0, 0.0, 1.0];
    let y = [
        z[1] * x[2] - z[2] * x[1],
        z[2] * x[0] - z[0] * x[2],
        z[0] * x[1] - z[1] * x[0],
    ];
    detail(
        project,
        DetailKind::Hole {
            part,
            frame: Frame {
                origin: Point3(at),
                axes: [x, y, z],
            },
            diameter: 22.0,
            depth: 40.0,
            slot: 30.0,
        },
    );
}

#[test]
fn slotted_holes_go_to_bo_with_length_and_angle() {
    let mut project = Project::default();
    let id = plate(
        &mut project,
        vec![[0.0, 0.0], [400.0, 0.0], [400.0, 200.0], [0.0, 200.0]],
        12.0,
    );
    slotted_hole(&mut project, id, [100.0, 100.0, -10.0], [1.0, 0.0, 0.0]);
    slotted_hole(&mut project, id, [300.0, 100.0, -10.0], [0.0, 1.0, 0.0]);
    let file = part_file(&project, id, "1", 1).unwrap();
    assert!(file.holes.is_empty());
    assert_eq!(
        file.slots,
        vec![
            (Face::Web, 100.0, 100.0, 22.0, 30.0, 0.0),
            (Face::Web, 300.0, 100.0, 22.0, 30.0, 90.0)
        ]
    );
    let bo = &blocks(&file.text, "BO")[0];
    assert_eq!(
        bo[0],
        [
            "v", "100.00", "100.00", "22.00", "0.00l", "30.00", "0.00", "0.00"
        ]
    );
    assert_eq!(bo[1][7], "90.00");
    // The solid has the slot: a 22 mm round hole plus a 22 × 30 strip less.
    let solid = project.element_body(id).unwrap().volume_mm3().unwrap();
    let full = 400.0 * 200.0 * 12.0;
    let slot = (std::f64::consts::PI * 11.0 * 11.0 + 22.0 * 30.0) * 12.0;
    assert!(
        ((full - solid) - 2.0 * slot).abs() < 0.01 * slot,
        "removed {}, slots {}",
        full - solid,
        2.0 * slot
    );
}

#[test]
fn slot_changes_the_part_mark() {
    let mut project = Project::default();
    let contour = vec![[0.0, 0.0], [400.0, 0.0], [400.0, 200.0], [0.0, 200.0]];
    let round = plate(&mut project, contour.clone(), 12.0);
    let slotted = plate(&mut project, contour, 12.0);
    for (part, slot) in [(round, 0.0), (slotted, 30.0)] {
        detail(
            &mut project,
            DetailKind::Hole {
                part,
                frame: Frame {
                    origin: Point3([100.0, 100.0, -10.0]),
                    ..Default::default()
                },
                diameter: 22.0,
                depth: 40.0,
                slot,
            },
        );
    }
    let files = open_bim_nc::export(&project).unwrap();
    assert_eq!(files.len(), 2);
}

#[test]
fn part_mark_is_stamped_on_the_front_face_clear_of_holes() {
    let mut project = Project::default();
    let id = member(&mut project, 3000.0);
    // A web hole right where the mark would go first.
    detail(
        &mut project,
        DetailKind::Hole {
            part: id,
            frame: Frame {
                origin: Point3([-100.0, 0.0, 40.0]),
                axes: [[0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]],
            },
            diameter: 22.0,
            depth: 200.0,
            slot: 0.0,
        },
    );
    let file = part_file(&project, id, "Б-12", 1).unwrap();
    let (face, x, y, text) = file.marking.clone().expect("mark");
    assert_eq!(face, Face::Web);
    assert_eq!(text, "B-12");
    assert!(close(y, 145.0), "centred on the web: {y}");
    // Clear of the hole at x = 40 (r = 11).
    assert!(x > 51.0, "{x}");
    let si = &blocks(&file.text, "SI")[0];
    assert_eq!(si[0][0], "v");
    assert_eq!(si[0][4], "10");
    assert_eq!(si[0][5], "B-12");
    let off = part_file_with(
        &project,
        id,
        "Б-12",
        1,
        &NcOptions {
            marking: false,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(off.marking.is_none() && !off.text.contains("\nSI\n"));
}
