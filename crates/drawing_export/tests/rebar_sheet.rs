//! КЖ sheet of a slab: plans, section, schedules and callouts regenerated
//! from the model.
use open_bim_drawing_export::{dxf, pdf};
use open_bim_drawings::{Status, generate, reinforcement_drawing, status, updated};
use open_bim_model::concrete::{MeshLayer, REBAR_COVER, SlabRebarType};
use open_bim_model::rebar::{self, install_slab};
use open_bim_model::{
    Detail, DetailKind, Element, ElementShape, Frame, GridAxis, LengthMm, Material, ObjectId,
    Point3, Project, PropertyValue,
};

fn project() -> (Project, ObjectId) {
    let mut p = Project::default();
    p.materials.insert(
        ObjectId(1),
        Material {
            id: ObjectId(1),
            name: "Бетон В25".into(),
            density_kg_m3: 2500.,
        },
    );
    let host = ObjectId(10);
    p.elements.insert(
        host,
        Element {
            id: host,
            name: "Плита Пм1".into(),
            material: ObjectId(1),
            shape: ElementShape::Plate {
                contour: vec![[0., 0.], [4000., 0.], [4000., 3000.], [0., 3000.]],
                thickness: LengthMm::new(200.).unwrap(),
                frame: Frame::default(),
            },
            properties: [(REBAR_COVER.into(), PropertyValue::Number(30.))].into(),
        },
    );
    p.details.insert(
        ObjectId(11),
        Detail {
            id: ObjectId(11),
            kind: DetailKind::PrismCut {
                part: host,
                frame: Frame {
                    origin: Point3([1800., 1100., -10.]),
                    ..Frame::default()
                },
                contour: vec![[0., 0.], [500., 0.], [500., 400.], [0., 400.]],
                length: 220.,
            },
        },
    );
    p.axes.insert(
        ObjectId(2),
        GridAxis {
            id: ObjectId(2),
            name: "1".into(),
            start: Point3([0., -200., 0.]),
            end: Point3([0., 3200., 0.]),
        },
    );
    p.axes.insert(
        ObjectId(3),
        GridAxis {
            id: ObjectId(3),
            name: "А".into(),
            start: Point3([-200., 0., 0.]),
            end: Point3([4200., 0., 0.]),
        },
    );
    let mut ty = SlabRebarType::new("slab", "Перекрытие");
    ty.bottom = Some(MeshLayer::new(12., 200., 0.));
    ty.top = Some(MeshLayer::new(10., 200., 0.));
    ty.cut_openings = true;
    ty.edge_diameter = 12.;
    ty.edge_spacing = 200.;
    ty.frog_diameter = 8.;
    ty.frog_spacing = 600.;
    p.concrete.slab_types.push(ty);
    p.concrete = install_slab(&p, host, "slab").unwrap();
    p.next_id = 100;
    (p, host)
}

fn texts(sheet: &open_bim_drawings::Sheet) -> String {
    sheet
        .labels
        .iter()
        .chain(sheet.views.iter().flat_map(|v| &v.labels))
        .chain(sheet.dimensions.iter().flat_map(|d| &d.labels))
        .map(|l| l.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

fn mentions(text: &str, mark: u32) -> bool {
    text.split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .any(|s| s == mark.to_string())
}

#[test]
fn sheet_has_plans_section_schedules_title_and_exports() {
    let (p, host) = project();
    let drawing = reinforcement_drawing(&p, ObjectId(100), host, "КЖ-1").unwrap();
    assert_eq!(status(&p, &drawing), Status::UpToDate);
    let sheet = generate(&p, &drawing).unwrap();
    let text = texts(&sheet);
    assert!(text.contains("План нижней арматуры"), "{text}");
    assert!(text.contains("План верхней арматуры"), "{text}");
    assert!(text.contains("1-1"), "{text}");
    assert!(text.contains("Ведомость деталей"), "{text}");
    assert!(text.contains("Ведомость расхода стали"), "{text}");
    assert!(text.contains("0000-КЖ"), "{text}");
    assert!(text.contains("Армирование Плита Пм1"), "{text}");
    assert!(text.contains("проём 500×400"), "{text}");
    assert!(text.contains('1') && text.contains('А'));
    assert!(text.contains("ГОСТ 21.501-2018"), "{text}");
    assert!(text.contains("ГОСТ Р 21.101-2026"), "{text}");
    let pdf = pdf::render(std::slice::from_ref(&sheet), "КЖ-1").unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    let dxf = dxf::render(&sheet);
    assert!(dxf.contains("OB_VISIBLE"));
    // DXF R12 stores Cyrillic as \U+XXXX. «Вед» of «Ведомость».
    assert!(dxf.contains("\\U+0412\\U+0435\\U+0434"), "{dxf}");
}

#[test]
fn callouts_match_schedule_positions() {
    let (p, host) = project();
    let drawing = reinforcement_drawing(&p, ObjectId(100), host, "КЖ-1").unwrap();
    let sheet = generate(&p, &drawing).unwrap();
    let text = texts(&sheet);
    let rows = rebar::detail_schedule(&p);
    assert!(!rows.is_empty());
    let zone = p.concrete.zones.iter().find(|z| z.host == host).unwrap();
    assert!(
        text.contains(&format!("Зона {}: {}", zone.number, zone.mark)),
        "{text}"
    );
    for row in &rows {
        assert!(
            text.contains(&row.mark.to_string()),
            "позиция {} нет на листе",
            row.mark
        );
        let dia = if row.diameter.fract() == 0. {
            format!("Ø{}", row.diameter as i64)
        } else {
            format!("Ø{:.1}", row.diameter).replace('.', ",")
        };
        assert!(
            sheet.views.iter().flat_map(|v| &v.labels).any(|l| {
                l.text.contains("поз.")
                    && mentions(&l.text, row.mark)
                    && l.text.contains(&dia)
                    && l.text.contains(&row.grade)
            }),
            "выноска позиции {} Ø {} {}: {text}",
            row.mark,
            dia,
            row.grade
        );
    }
}

#[test]
fn section_shows_cover_both_meshes_ubars_and_frogs() {
    let (p, host) = project();
    let drawing = reinforcement_drawing(&p, ObjectId(100), host, "КЖ-1").unwrap();
    let sheet = generate(&p, &drawing).unwrap();
    let section = sheet
        .views
        .iter()
        .find(|v| v.name == "1-1" || v.labels.iter().any(|l| l.text.starts_with("1-1")))
        .expect("разрез");
    let labels: Vec<_> = section.labels.iter().map(|l| l.text.as_str()).collect();
    let joined = labels.join("\n");
    assert!(joined.contains("защитный слой 30"), "{joined}");
    assert!(joined.contains("низ "), "{joined}");
    assert!(joined.contains("верх "), "{joined}");
    assert!(joined.contains("П "), "{joined}");
    assert!(joined.contains("лягушка "), "{joined}");
    assert!(
        section
            .lines
            .iter()
            .any(|l| l.style == open_bim_drawings::LineStyle::Hatch)
    );
    assert!(
        section
            .lines
            .iter()
            .any(|l| l.style == open_bim_drawings::LineStyle::Visible && l.points.len() > 6)
    );
    let rows = rebar::detail_schedule(&p);
    let frog = rows.iter().find(|r| r.code == "Л").unwrap();
    let edge = rows.iter().find(|r| r.code == "П").unwrap();
    assert!(joined.contains(&frog.mark.to_string()), "{joined}");
    assert!(joined.contains(&edge.mark.to_string()), "{joined}");
}

#[test]
fn sheet_follows_a_change_of_the_bars() {
    let (p, host) = project();
    let drawing = reinforcement_drawing(&p, ObjectId(100), host, "КЖ-1").unwrap();
    let mut changed = p.clone();
    changed.revision += 1;
    let set = changed
        .concrete
        .rebars
        .iter_mut()
        .find(|s| s.role == Some(open_bim_model::concrete::ZoneRole::BottomMain))
        .unwrap();
    set.diameter = 16.;
    assert!(matches!(
        status(&changed, &drawing),
        Status::Outdated { .. }
    ));
    let sheet = generate(&changed, &drawing).unwrap();
    let text = texts(&sheet);
    assert!(text.contains("Ø16"), "{text}");
    let fresh = updated(&changed, &drawing);
    assert_eq!(status(&changed, &fresh), Status::UpToDate);
    changed
        .elements
        .get_mut(&host)
        .unwrap()
        .properties
        .insert(REBAR_COVER.into(), PropertyValue::Number(40.));
    changed.concrete = install_slab(&changed, host, "slab").unwrap();
    let again = generate(&changed, &fresh).unwrap();
    assert!(
        texts(&again).contains("защитный слой 40"),
        "{}",
        texts(&again)
    );
}
