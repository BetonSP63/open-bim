use open_bim_commands::Command;
use open_bim_model::{
    Detail, DetailKind, Element, ElementShape, Frame, LengthMm, MemberKind, ObjectId, Point3,
    Project,
};
use open_bim_nc::{Face, export, part_file};
use std::collections::BTreeMap;

const I_SECTION: ObjectId = ObjectId(2);
const ANGLE: ObjectId = ObjectId(3);
const ROUND_TUBE: ObjectId = ObjectId(5);

fn member(project: &mut Project, profile: ObjectId, length: f64, y: f64) -> ObjectId {
    let id = project.allocate_id().unwrap();
    project.elements.insert(
        id,
        Element {
            id,
            name: "Балка".into(),
            material: ObjectId(1),
            shape: ElementShape::Member {
                member_kind: MemberKind::Beam,
                profile,
                length: LengthMm::new(length).unwrap(),
                frame: Frame {
                    origin: Point3([0.0, y, 0.0]),
                    ..Default::default()
                },
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

/// Hole along local `axis` (0 = x, 1 = y, 2 = z) with its centre at `at`.
fn hole(project: &mut Project, part: ObjectId, at: [f64; 3], axis: usize, diameter: f64) {
    let axes = match axis {
        0 => [[0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]],
        1 => [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        _ => [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };
    let mut origin = at;
    origin[axis] -= 200.0;
    detail(
        project,
        DetailKind::Hole {
            part,
            frame: Frame {
                origin: Point3(origin),
                axes,
            },
            diameter,
            depth: 400.0,
            slot: 0.0,
        },
    );
}

fn lines(text: &str) -> Vec<&str> {
    text.lines().collect()
}

#[test]
fn header_lists_the_section_in_dstv_order() {
    let mut project = Project::default();
    let id = member(&mut project, I_SECTION, 6000.0, 0.0);
    let file = part_file(&project, id, "12", 3).unwrap();
    let l = lines(&file.text);
    assert_eq!(l[0], "ST");
    assert!(l[1].starts_with("**"), "comment line: {}", l[1]);
    let header: Vec<&str> = l[2..26].iter().map(|s| s.trim()).collect();
    assert_eq!(header[3], "12", "piece mark");
    assert_eq!(header[4], "S235", "steel grade without the comment");
    assert_eq!(header[5], "3", "quantity");
    assert_eq!(header[6], "I 300x150x8x12");
    assert_eq!(header[7], "I");
    assert_eq!(header[8], "6000.00");
    assert_eq!(header[9], "300.00");
    assert_eq!(header[10], "150.00");
    assert_eq!(header[11], "12.00");
    assert_eq!(header[12], "8.00");
    // (2·150·12 + 276·8) mm² · 7850 kg/m³ = 45.59 kg/m.
    assert_eq!(header[14], "45.59");
    assert!(l[2..26].iter().all(|s| s.starts_with("  ")));
    assert_eq!(*l.last().unwrap(), "EN");
    assert!(file.text.is_ascii());
    assert_eq!(file.name, "12.nc1");
    assert!(file.warnings.is_empty());
}

#[test]
fn web_and_flange_holes_go_to_their_faces() {
    let mut project = Project::default();
    let id = member(&mut project, I_SECTION, 3000.0, 0.0);
    // Web hole 80 mm above the centre line, 100 mm from the start.
    hole(&mut project, id, [0.0, 80.0, 100.0], 0, 22.0);
    // Top and bottom flange holes 40 mm off the centre line.
    hole(&mut project, id, [40.0, 150.0, 500.0], 1, 18.0);
    hole(&mut project, id, [-40.0, -150.0, 2900.0], 1, 18.0);
    let file = part_file(&project, id, "1", 1).unwrap();
    assert_eq!(
        file.holes,
        vec![
            (Face::Web, 100.0, 230.0, 22.0),
            (Face::Top, 500.0, 115.0, 18.0),
            (Face::Bottom, 2900.0, 35.0, 18.0),
        ]
    );
    let bo = file.text.find("\nBO\n").expect("BO block");
    let block: Vec<&str> = file.text[bo + 4..].lines().take(3).collect();
    assert_eq!(
        block[0].split_whitespace().collect::<Vec<_>>(),
        ["v", "100.00", "230.00", "22.00"]
    );
    assert_eq!(block[1].split_whitespace().next(), Some("o"));
    assert_eq!(block[2].split_whitespace().next(), Some("u"));
}

#[test]
fn square_end_trims_shorten_the_piece() {
    let mut project = Project::default();
    let id = member(&mut project, I_SECTION, 3000.0, 0.0);
    // Keeps z ≥ 50 and z ≤ 2900.
    detail(
        &mut project,
        DetailKind::PlaneCut {
            part: id,
            normal: [0.0, 0.0, -1.0],
            offset: -50.0,
        },
    );
    detail(
        &mut project,
        DetailKind::PlaneCut {
            part: id,
            normal: [0.0, 0.0, 1.0],
            offset: 2900.0,
        },
    );
    hole(&mut project, id, [0.0, 0.0, 150.0], 0, 22.0);
    let file = part_file(&project, id, "1", 1).unwrap();
    assert!((file.length - 2850.0).abs() < 1e-9);
    assert_eq!(file.holes, vec![(Face::Web, 100.0, 150.0, 22.0)]);
    assert!(file.warnings.is_empty());
}

#[test]
fn cuts_the_file_cannot_describe_are_reported_not_dropped_silently() {
    let mut project = Project::default();
    let id = member(&mut project, I_SECTION, 3000.0, 0.0);
    // Prism at 45° to the web and the flanges.
    let s = std::f64::consts::FRAC_1_SQRT_2;
    detail(
        &mut project,
        DetailKind::PrismCut {
            part: id,
            frame: Frame {
                origin: Point3([0.0, 0.0, 500.0]),
                axes: [[s, s, 0.0], [-s, s, 0.0], [0.0, 0.0, 1.0]],
            },
            contour: vec![[-20.0, -20.0], [20.0, -20.0], [20.0, 20.0]],
            length: 50.0,
        },
    );
    let file = part_file(&project, id, "1", 1).unwrap();
    assert!(!file.warnings.is_empty(), "{:?}", file.warnings);
    assert!(file.text.contains("\n** "), "warning repeated in the file");
}

#[test]
fn angle_legs_are_front_and_bottom() {
    let mut project = Project::default();
    let id = member(&mut project, ANGLE, 1000.0, 0.0);
    // L 100×75×8: vertical leg at x = −37.5, horizontal leg at y = −50.
    hole(&mut project, id, [-37.5, 10.0, 60.0], 0, 14.0);
    hole(&mut project, id, [5.0, -50.0, 60.0], 1, 14.0);
    let file = part_file(&project, id, "2", 1).unwrap();
    assert_eq!(
        file.holes,
        vec![
            (Face::Web, 60.0, 60.0, 14.0),
            (Face::Bottom, 60.0, 42.5, 14.0)
        ]
    );
    assert!(file.text.contains("\n  L\n"));
}

#[test]
fn round_tube_code_and_unplaceable_holes_warn() {
    let mut project = Project::default();
    let id = member(&mut project, ROUND_TUBE, 1000.0, 0.0);
    hole(&mut project, id, [0.0, 57.0, 100.0], 1, 14.0);
    let file = part_file(&project, id, "3", 1).unwrap();
    assert!(file.text.contains("\n  RU\n"));
    assert!(file.holes.is_empty());
    assert_eq!(file.warnings.len(), 1);
}

#[test]
fn plate_has_contour_and_holes_on_the_front_face() {
    let mut project = Project::default();
    // 200 wide, 400 long: the long side becomes x.
    let id = plate(
        &mut project,
        vec![
            [-100.0, -200.0],
            [100.0, -200.0],
            [100.0, 200.0],
            [-100.0, 200.0],
        ],
        20.0,
    );
    hole(&mut project, id, [-60.0, -150.0, 10.0], 2, 26.0);
    let file = part_file(&project, id, "5", 2).unwrap();
    let text = &file.text;
    assert!(text.contains("\n  PL20x200\n"), "{text}");
    assert!(text.contains("\n  B\n"));
    assert!((file.length - 400.0).abs() < 1e-9);
    // Rotated a quarter turn: x = y + 200, y = 100 − x.
    assert_eq!(file.holes, vec![(Face::Web, 50.0, 160.0, 26.0)]);
    let ak = text.find("\nAK\n").expect("AK block");
    let contour: Vec<Vec<&str>> = text[ak + 4..]
        .lines()
        .take_while(|l| l.starts_with("  "))
        .map(|l| l.split_whitespace().collect())
        .collect();
    assert_eq!(contour.len(), 5, "closed: first point repeated");
    assert_eq!(contour.first(), contour.last());
    assert_eq!(contour[0][0], "v");
    let xs: Vec<f64> = contour.iter().map(|p| p[1].parse().unwrap()).collect();
    let ys: Vec<f64> = contour.iter().map(|p| p[2].parse().unwrap()).collect();
    assert_eq!(xs.iter().cloned().fold(f64::MAX, f64::min), 0.0);
    assert_eq!(xs.iter().cloned().fold(f64::MIN, f64::max), 400.0);
    assert_eq!(ys.iter().cloned().fold(f64::MIN, f64::max), 200.0);
    // Counter-clockwise outer contour.
    let area: f64 = (0..4)
        .map(|i| xs[i] * ys[i + 1] - xs[i + 1] * ys[i])
        .sum::<f64>()
        / 2.0;
    assert!(area > 0.0);
}

#[test]
fn export_writes_one_file_per_mark_with_quantity() {
    let mut project = Project::default();
    member(&mut project, I_SECTION, 6000.0, 0.0);
    member(&mut project, I_SECTION, 6000.0, 3000.0);
    member(&mut project, I_SECTION, 4000.0, 6000.0);
    let files = export(&project).unwrap();
    assert_eq!(files.len(), 2);
    let quantities: Vec<&str> = files
        .iter()
        .map(|f| f.text.lines().nth(7).unwrap().trim())
        .collect();
    let mut quantities = quantities;
    quantities.sort();
    assert_eq!(quantities, ["1", "2"]);
    let mut names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
    names.dedup();
    assert_eq!(names.len(), 2);
}

#[test]
fn bolt_groups_from_the_detailing_tools_export_as_holes() {
    let mut project = Project::default();
    let id = member(&mut project, I_SECTION, 3000.0, 0.0);
    let pattern = open_bim_steel::BoltPattern {
        parts: [id].into(),
        frame: Frame {
            origin: Point3([-50.0, -40.0, 100.0]),
            axes: [[0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]],
        },
        diameter: 20.0,
        clearance: 2.0,
        hole_depth: 100.0,
        steps_x: vec![80.0],
        points: Vec::new(),
        steps_y: vec![70.0],
        standard: "ГОСТ 7798-70".into(),
        grade: "8.8".into(),
        slot: 0.0,
    };
    for command in open_bim_steel::bolt_commands(&project, &pattern).unwrap() {
        if let Command::CreateDetail { detail } = command {
            project.details.insert(detail.id, detail);
        }
    }
    let file = part_file(&project, id, "1", 1).unwrap();
    assert_eq!(file.holes.len(), 4);
    assert!(file.holes.iter().all(|h| h.0 == Face::Web && h.3 == 22.0));
    let mut xs: Vec<f64> = file.holes.iter().map(|h| h.1).collect();
    xs.dedup();
    assert_eq!(xs, [100.0, 170.0]);
}

#[test]
fn cyrillic_text_is_transliterated() {
    assert_eq!(
        open_bim_nc::ascii("С255 Двутавр 30Б1 «Р 57837»"),
        "S255 Dvutavr 30B1 \"R 57837\""
    );
}

#[test]
fn welded_section_exports_its_plates() {
    use open_bim_model::sections::templates;
    let mut project = open_bim_model::example_steel_frame().unwrap();
    let pid = project.allocate_id().unwrap();
    let tee = templates()
        .into_iter()
        .find(|t| t.name == "Тавр сварной")
        .unwrap();
    project
        .profiles
        .insert(pid, tee.profile(pid, &Default::default()).unwrap());
    for e in project.elements.values_mut() {
        if let open_bim_model::ElementShape::Member { profile, .. } = &mut e.shape {
            *profile = pid;
        }
    }
    let files = open_bim_nc::export(&project).unwrap();
    let names: Vec<_> = files.iter().map(|f| f.name.as_str()).collect();
    // Each member mark becomes «<mark>.1», «<mark>.2»: flange and web strips.
    assert_eq!(
        names.iter().filter(|n| n.ends_with(".1.nc1")).count(),
        2,
        "{names:?}"
    );
    assert_eq!(
        names.iter().filter(|n| n.ends_with(".2.nc1")).count(),
        2,
        "{names:?}"
    );
    assert!(files.iter().any(|f| (f.length - 3000.).abs() < 1e-6));
}
