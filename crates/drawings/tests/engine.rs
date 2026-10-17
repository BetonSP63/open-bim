use open_bim_commands::{Command, DeletePolicy, Document};
use open_bim_drawings::{
    LineStyle, Sheet, Status, assembly_drawing, general_drawing, generate, issue, part_drawing,
    svg, updated,
};
use open_bim_model::{
    Assembly, Detail, DetailKind, Drawing, Element, ElementShape, Frame, LengthMm, MemberKind,
    ObjectId, Point3, Project, SheetSize, ViewDirection,
};
use std::collections::{BTreeMap, BTreeSet};

const H: f64 = 300.;
const B: f64 = 150.;
const TW: f64 = 8.;
const TF: f64 = 12.;
const HOLES_Z: [f64; 2] = [500., 1250.];

struct Fixture {
    project: Project,
    beam: ObjectId,
    plate: ObjectId,
    assembly: ObjectId,
}

/// I300x150x8x12 beam 3000 mm along world X with two web holes, end plate
/// 400x300x20 with two holes, assembled with the beam as main part.
fn fixture() -> Fixture {
    let mut p = Project::default();
    let beam = p.allocate_id().unwrap();
    p.elements.insert(
        beam,
        Element {
            id: beam,
            name: "Балка Б1".into(),
            material: ObjectId(1),
            shape: ElementShape::Member {
                member_kind: MemberKind::Beam,
                profile: ObjectId(2),
                length: LengthMm::new(3000.).unwrap(),
                frame: Frame {
                    origin: Point3([0., 0., 3000.]),
                    axes: [[0., 1., 0.], [0., 0., 1.], [1., 0., 0.]],
                },
            },
            properties: BTreeMap::new(),
        },
    );
    for z in HOLES_Z {
        let id = p.allocate_id().unwrap();
        p.details.insert(
            id,
            Detail {
                id,
                kind: DetailKind::Hole {
                    part: beam,
                    frame: Frame {
                        origin: Point3([-20., 40., z]),
                        axes: [[0., 1., 0.], [0., 0., 1.], [1., 0., 0.]],
                    },
                    diameter: 22.,
                    depth: 40.,
                },
            },
        );
    }
    let plate = p.allocate_id().unwrap();
    p.elements.insert(
        plate,
        Element {
            id: plate,
            name: "Пластина П1".into(),
            material: ObjectId(1),
            shape: ElementShape::Plate {
                contour: vec![[0., 0.], [400., 0.], [400., 300.], [0., 300.]],
                thickness: LengthMm::new(20.).unwrap(),
                frame: Frame {
                    origin: Point3([3000., -200., 2850.]),
                    axes: [[0., 1., 0.], [0., 0., 1.], [1., 0., 0.]],
                },
            },
            properties: BTreeMap::new(),
        },
    );
    for (x, y) in [(60., 80.), (340., 220.)] {
        let id = p.allocate_id().unwrap();
        p.details.insert(
            id,
            Detail {
                id,
                kind: DetailKind::Hole {
                    part: plate,
                    frame: Frame {
                        origin: Point3([x, y, 0.]),
                        ..Frame::default()
                    },
                    diameter: 22.,
                    depth: 20.,
                },
            },
        );
    }
    let assembly = p.allocate_id().unwrap();
    p.assemblies.insert(
        assembly,
        Assembly {
            id: assembly,
            name: "Балка в сборе Б1".into(),
            main_part: beam,
            members: BTreeSet::from([beam, plate]),
            children: BTreeSet::new(),
            properties: BTreeMap::new(),
        },
    );
    p.validate().unwrap();
    Fixture {
        project: p,
        beam,
        plate,
        assembly,
    }
}

fn view(sheet: &Sheet, id: u32) -> &open_bim_drawings::ViewGraphic {
    sheet.views.iter().find(|v| v.id == id).unwrap()
}

fn extent(sheet: &Sheet, view_id: u32, style: LineStyle) -> [f64; 2] {
    let points: Vec<[f64; 2]> = view(sheet, view_id)
        .lines
        .iter()
        .filter(|l| l.style == style)
        .flat_map(|l| l.points.clone())
        .collect();
    let span = |i: usize| {
        let (lo, hi) = points
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p[i]), hi.max(p[i]))
            });
        hi - lo
    };
    [span(0), span(1)]
}

fn shoelace(points: &[[f64; 2]]) -> f64 {
    (0..points.len())
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % points.len()]);
            a[0] * b[1] - a[1] * b[0]
        })
        .sum::<f64>()
        / 2.
}

fn bless_or_compare(name: &str, svg: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/drawings")
        .join(name);
    if std::env::var_os("OPEN_BIM_BLESS").is_some() {
        std::fs::write(&path, svg).unwrap();
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{name} missing"));
    assert_eq!(svg, expected, "{name} differs from reference SVG");
}

#[test]
fn part_views_hidden_lines_sections_and_reference_svg() {
    let f = fixture();
    let mut d = part_drawing(&f.project, ObjectId(900), f.beam, "ОБ-КМД-Б1").unwrap();
    // Add a cross-section at mid-span, viewed from the left like the end view.
    d.views.push(open_bim_model::DrawingView {
        id: 4,
        name: "Сечение 1-1".into(),
        direction: ViewDirection::Left,
        section: Some(-1500.),
        position: [330., 90.],
        scale: Some(10.),
    });
    let sheet = generate(&f.project, &d).unwrap();
    let s = d.scale;
    // Front view: length x height; end view: width x height (analytic profile).
    let front = extent(&sheet, 1, LineStyle::Visible);
    assert!((front[0] * s - 3000.).abs() < 0.01 && (front[1] * s - H).abs() < 0.01);
    let end = extent(&sheet, 3, LineStyle::Visible);
    assert!((end[0] * s - B).abs() < 0.01 && (end[1] * s - H).abs() < 0.01);
    // The web is hidden behind the flange in the top view.
    let top_hidden = view(&sheet, 2)
        .lines
        .iter()
        .filter(|l| l.style == LineStyle::Hidden)
        .count();
    assert!(top_hidden >= 2, "web edges are hidden in plan");
    // Section loop area equals the analytic I-section area (independent formula).
    let section = view(&sheet, 4);
    let area: f64 = section
        .lines
        .iter()
        .filter(|l| l.style == LineStyle::Section)
        .map(|l| shoelace(&l.points).abs())
        .sum::<f64>()
        * 10f64.powi(2);
    let expected = 2. * B * TF + (H - 2. * TF) * TW;
    assert!((area - expected).abs() < 0.01, "{area} vs {expected}");
    assert!(section.lines.iter().any(|l| l.style == LineStyle::Hatch));
    assert!(section.lines.iter().all(|l| l.style != LineStyle::Hidden));
    // Dimensions follow the model to 0.01 mm: length and hole chain.
    assert_eq!(sheet.dimensions[0].values_mm.len(), 1);
    assert!((sheet.dimensions[0].values_mm[0] - 3000.).abs() < 0.01);
    let chain = &sheet.dimensions[1].values_mm;
    let expected = [HOLES_Z[0], HOLES_Z[1] - HOLES_Z[0], 3000. - HOLES_Z[1]];
    assert_eq!(chain.len(), 3);
    for (a, b) in chain.iter().zip(expected) {
        assert!((a - b).abs() < 0.01, "{a} vs {b}");
    }
    bless_or_compare("beam-part.svg", &svg::render(&sheet));
}

#[test]
fn every_drawing_kind_has_scale_frame_title_block_and_designation() {
    let f = fixture();
    let drawings = [
        general_drawing(&f.project, ObjectId(901), vec![], "ОБ-КМ-1", SheetSize::A3).unwrap(),
        part_drawing(&f.project, ObjectId(902), f.plate, "ОБ-КМД-П1").unwrap(),
        assembly_drawing(&f.project, ObjectId(903), f.assembly, "ОБ-КМД-С1").unwrap(),
    ];
    for (d, file) in drawings
        .iter()
        .zip(["general.svg", "plate-part.svg", "assembly.svg"])
    {
        let sheet = generate(&f.project, d).unwrap();
        assert_eq!(sheet.status, Status::UpToDate);
        let texts: Vec<&str> = sheet.labels.iter().map(|l| l.text.as_str()).collect();
        assert!(texts.contains(&d.number.as_str()), "{file}: designation");
        assert!(texts.contains(&d.title.as_str()), "{file}: title");
        assert!(
            texts.iter().any(|t| *t == format!("1:{}", d.scale)),
            "{file}: scale"
        );
        assert!(sheet.frame.iter().any(|l| l.style == LineStyle::Frame));
        // All view geometry stays inside the inner frame.
        for v in &sheet.views {
            assert!(
                v.bounds[0][0] >= 20. && v.bounds[1][0] <= sheet.size[0] - 5.,
                "{file}"
            );
            assert!(
                v.bounds[0][1] >= 5. && v.bounds[1][1] <= sheet.size[1] - 5.,
                "{file}"
            );
        }
        bless_or_compare(file, &svg::render(&sheet));
    }
    // Plate drawing: contour and hole dimensions from the model.
    let plate = generate(&f.project, &drawings[1]).unwrap();
    let x = &plate.dimensions[0].values_mm;
    let expected_x = [60., 280., 60.];
    assert_eq!(x.len(), 3);
    for (a, b) in x.iter().zip(expected_x) {
        assert!((a - b).abs() < 0.01);
    }
}

#[test]
fn length_change_updates_dimension_and_outdates_until_updated() {
    let f = fixture();
    let mut doc = Document::new(f.project.clone()).unwrap();
    let d = part_drawing(doc.project(), ObjectId(900), f.beam, "ОБ-КМД-Б1").unwrap();
    doc.transact("Чертёж", &[Command::CreateDrawing { drawing: d }])
        .unwrap();
    let mut beam = doc.project().elements[&f.beam].clone();
    if let ElementShape::Member { length, .. } = &mut beam.shape {
        *length = LengthMm::new(3600.).unwrap();
    }
    doc.transact("Длина", &[Command::ReplaceElement { element: beam }])
        .unwrap();
    let d = doc.project().drawings[&ObjectId(900)].clone();
    let sheet = generate(doc.project(), &d).unwrap();
    assert_eq!(
        sheet.status,
        Status::Outdated {
            changed: vec![f.beam]
        }
    );
    assert!((sheet.dimensions[0].values_mm[0] - 3600.).abs() < 0.01);
    assert!(
        issue(doc.project(), &d).is_err(),
        "outdated drawing cannot be issued"
    );
    let fresh = updated(doc.project(), &d);
    doc.transact(
        "Обновить",
        &[Command::ReplaceDrawing {
            drawing: fresh.clone(),
        }],
    )
    .unwrap();
    assert_eq!(
        generate(doc.project(), &fresh).unwrap().status,
        Status::UpToDate
    );
    let issued = issue(doc.project(), &fresh).unwrap();
    assert_eq!(issued.issued_revision, Some(doc.project().revision));
    // Undo restores the previous drawing record; persistence keeps it.
    doc.undo().unwrap();
    assert_eq!(doc.project().drawings[&ObjectId(900)], d);
    let json = doc.project().to_json().unwrap();
    assert_eq!(
        Project::from_json(&json).unwrap().drawings,
        doc.project().drawings
    );
}

#[test]
fn deleted_reference_is_reported_as_lost_binding() {
    let f = fixture();
    let mut doc = Document::new(f.project.clone()).unwrap();
    let d = assembly_drawing(doc.project(), ObjectId(903), f.assembly, "ОБ-КМД-С1").unwrap();
    let mut d: Drawing = d;
    // Dimension from the beam end to a plate hole: depends on the plate.
    let hole = *doc
        .project()
        .details
        .iter()
        .find(|(_, v)| v.kind.part() == Some(f.plate))
        .unwrap()
        .0;
    d.dimensions.push(open_bim_model::DimensionDef {
        id: 2,
        view: 1,
        anchors: vec![
            open_bim_model::Anchor {
                object: f.beam,
                point: open_bim_model::AnchorPoint::Start,
            },
            open_bim_model::Anchor {
                object: f.plate,
                point: open_bim_model::AnchorPoint::Hole { hole },
            },
        ],
        axis: open_bim_model::DimensionAxis::Horizontal,
        offset: 30.,
    });
    let d = updated(doc.project(), &d);
    doc.transact("Чертёж", &[Command::CreateDrawing { drawing: d.clone() }])
        .unwrap();
    assert_eq!(
        generate(doc.project(), &d).unwrap().status,
        Status::UpToDate
    );
    doc.transact(
        "Удалить",
        &[Command::Delete {
            id: f.plate,
            policy: DeletePolicy::Cascade,
        }],
    )
    .unwrap();
    let d = doc.project().drawings[&ObjectId(903)].clone();
    let sheet = generate(doc.project(), &d).unwrap();
    let Status::LostReferences { missing } = &sheet.status else {
        panic!("expected lost references, got {:?}", sheet.status)
    };
    assert!(missing.contains(&f.plate) && missing.contains(&hole));
    let dim = sheet.dimensions.iter().find(|x| x.id == 2).unwrap();
    assert!(dim.lost && dim.values_mm.is_empty());
    assert!(dim.labels.iter().all(|l| l.lost));
    assert!(issue(doc.project(), &d).is_err());
    let text = svg::render(&sheet);
    assert!(text.contains("data-lost=\"true\""));
    assert!(text.contains("ПОТЕРЯНЫ ПРИВЯЗКИ"));
}
