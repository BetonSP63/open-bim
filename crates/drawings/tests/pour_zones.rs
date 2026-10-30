//! Pour zones on a concrete arrangement drawing: the volume schedule in
//! place of the steel specification, joints as thick dash-dot lines and
//! zone numbers in the views.
use open_bim_drawings::{LineStyle, general_drawing, generate};
use open_bim_model::pours::CONCRETE_KEY;
use open_bim_model::{
    Element, ElementShape, Frame, LengthMm, Material, MemberKind, ObjectId, Point3, PourBreak,
    Profile, ProfileShape, Project, PropertyValue, SheetSize,
};

fn add(p: &mut Project, kind: &str, shape: ElementShape) {
    let id = ObjectId(p.next_id);
    p.next_id += 1;
    p.elements.insert(
        id,
        Element {
            id,
            name: kind.into(),
            material: ObjectId(1),
            shape,
            properties: [(CONCRETE_KEY.into(), PropertyValue::Text(kind.into()))].into(),
        },
    );
}

/// Two columns 400×400 under a slab 10×4 m, 200 thick, joint at x = 4000.
fn project() -> Project {
    let mut p = Project::default();
    p.materials.insert(
        ObjectId(1),
        Material {
            id: ObjectId(1),
            name: "Бетон В25".into(),
            density_kg_m3: 2500.,
        },
    );
    p.profiles.insert(
        ObjectId(2),
        Profile {
            id: ObjectId(2),
            name: "400x400".into(),
            shape: ProfileShape::Rectangle {
                height: 400.,
                width: 400.,
            },
            source: "Бетонное сечение".into(),
            standard: None,
            area_mm2: None,
        },
    );
    p.next_id = 10;
    for x in [200., 9800.] {
        let (frame, length) =
            Frame::along(Point3([x, 200., 0.]), Point3([x, 200., 3000.])).unwrap();
        add(
            &mut p,
            "column",
            ElementShape::Member {
                member_kind: MemberKind::Column,
                profile: ObjectId(2),
                length,
                frame,
                position: Default::default(),
            },
        );
    }
    add(
        &mut p,
        "slab",
        ElementShape::Plate {
            frame: Frame {
                origin: Point3([0., 0., 3000.]),
                ..Frame::default()
            },
            contour: vec![[0., 0.], [10000., 0.], [10000., 4000.], [0., 4000.]],
            thickness: LengthMm::new(200.).unwrap(),
        },
    );
    p.concrete.breaks.push(PourBreak {
        start: Point3([4000., -1000., 0.]),
        end: Point3([4000., 5000., 0.]),
        horizontal: false,
    });
    p
}

#[test]
fn concrete_arrangement_shows_zones_joints_and_volumes() {
    let p = project();
    let zones = p.pour_zones();
    assert_eq!(zones.len(), 2);
    // Left: column + 4 m of slab; right: column + 6 m of slab.
    let v: Vec<f64> = zones.iter().map(|z| z.volume_m3).collect();
    assert!((v[0] - (0.48 + 3.2)).abs() < 1e-3, "{v:?}");
    assert!((v[1] - (0.48 + 4.8)).abs() < 1e-3, "{v:?}");

    let d = general_drawing(&p, ObjectId(900), vec![], "КЖ-1", SheetSize::A3).unwrap();
    let sheet = generate(&p, &d).unwrap();
    let texts: Vec<&str> = sheet.labels.iter().map(|l| l.text.as_str()).collect();
    assert!(
        texts.contains(&"Ведомость объёмов бетона по захваткам"),
        "{texts:?}"
    );
    assert!(!texts.iter().any(|t| t.contains("металлопроката")));
    for t in [
        "Колонна 400x400",
        "Плита δ=200",
        "Итого по захватке",
        "3,68",
        "5,28",
        "8,96",
    ] {
        assert!(texts.contains(&t), "{t}: {texts:?}");
    }
    for view in &sheet.views {
        let joints: Vec<_> = view
            .lines
            .iter()
            .filter(|l| l.style == LineStyle::ThickDashDot)
            .collect();
        assert_eq!(joints.len(), 1, "{}", view.name);
        // Vertical on the elevation and on the plan: x is constant.
        let [a, b] = [joints[0].points[0], joints[0].points[1]];
        assert!((a[0] - b[0]).abs() < 1e-6, "{a:?} {b:?}");
        let labels: Vec<&str> = view.labels.iter().map(|l| l.text.as_str()).collect();
        assert!(labels.contains(&"Шов бетонирования"), "{labels:?}");
        let zones_shown = labels.contains(&"Захватка 1") && labels.contains(&"Захватка 2");
        assert_eq!(zones_shown, view.name == "План", "{}", view.name);
    }
    // Moving the joint outdates the drawing.
    let mut d = d;
    d = open_bim_drawings::updated(&p, &d);
    assert_eq!(
        open_bim_drawings::status(&p, &d),
        open_bim_drawings::Status::UpToDate
    );
    let mut moved = p.clone();
    moved.concrete.breaks[0].start.0[0] = 5000.;
    moved.concrete.breaks[0].end.0[0] = 5000.;
    assert!(matches!(
        open_bim_drawings::status(&moved, &d),
        open_bim_drawings::Status::Outdated { .. }
    ));
    if let Ok(path) = std::env::var("POUR_SVG") {
        std::fs::write(path, open_bim_drawings::svg::render(&sheet)).unwrap();
    }
}

#[test]
fn horizontal_joint_shows_on_the_elevation_only() {
    let mut p = project();
    p.concrete.breaks[0] = PourBreak {
        start: Point3([-500., -500., 3000.]),
        end: Point3([10500., 4500., 3000.]),
        horizontal: true,
    };
    assert_eq!(p.pour_zones().len(), 3);
    let d = general_drawing(&p, ObjectId(900), vec![], "КЖ-1", SheetSize::A3).unwrap();
    let sheet = generate(&p, &d).unwrap();
    for view in &sheet.views {
        let n = view
            .lines
            .iter()
            .filter(|l| l.style == LineStyle::ThickDashDot)
            .count();
        assert_eq!(n, usize::from(view.name != "План"), "{}", view.name);
    }
}
