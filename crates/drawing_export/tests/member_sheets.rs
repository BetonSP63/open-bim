//! КЖ sheets of a beam, a column and a wall: elevation, sections, schedules
//! and callouts regenerated from the model.
use open_bim_drawing_export::{dxf, pdf};
use open_bim_drawings::{Status, generate, reinforcement_drawing, status, updated};
use open_bim_model::concrete::{CageType, REBAR_COVER, StarterPath, WallFace, WallRebarType};
use open_bim_model::pours::CONCRETE_KEY;
use open_bim_model::rebar::{self, install_cage, install_starter, install_wall};
use open_bim_model::{
    Detail, DetailKind, Element, ElementShape, Frame, LengthMm, Material, MemberKind, ObjectId,
    Point3, Profile, ProfileShape, Project, PropertyValue,
};

fn material(p: &mut Project) {
    p.materials.insert(
        ObjectId(1),
        Material {
            id: ObjectId(1),
            name: "Бетон В25".into(),
            density_kg_m3: 2500.,
        },
    );
}

fn profile(p: &mut Project, id: ObjectId, width: f64, height: f64) {
    p.profiles.insert(
        id,
        Profile {
            id,
            name: format!("{width}x{height}"),
            shape: ProfileShape::Rectangle { width, height },
            source: "Бетонное сечение".into(),
            standard: None,
            area_mm2: None,
        },
    );
}

struct Host<'a> {
    name: &'a str,
    kind: MemberKind,
    concrete: &'a str,
    length: f64,
    cover: f64,
}

fn member(p: &mut Project, id: ObjectId, profile: ObjectId, host: Host<'_>) {
    p.elements.insert(
        id,
        Element {
            id,
            name: host.name.into(),
            material: ObjectId(1),
            shape: ElementShape::Member {
                member_kind: host.kind,
                profile,
                length: LengthMm::new(host.length).unwrap(),
                frame: Frame::default(),
                position: Default::default(),
            },
            properties: [
                (REBAR_COVER.into(), PropertyValue::Number(host.cover)),
                (
                    CONCRETE_KEY.into(),
                    PropertyValue::Text(host.concrete.into()),
                ),
            ]
            .into(),
        },
    );
}

fn cage(end: bool) -> CageType {
    let mut ty = CageType::new("cage", "Каркас КЖ");
    ty.long_diameter = 16.;
    ty.long_grade = "А500С".into();
    ty.per_face = 2;
    ty.stirrup_diameter = 8.;
    ty.stirrup_grade = "А240".into();
    ty.stirrup_spacing = 200.;
    if end {
        ty.end_spacing = Some(100.);
        ty.end_length = Some(600.);
    }
    ty
}

fn beam() -> (Project, ObjectId) {
    let mut p = Project::default();
    material(&mut p);
    profile(&mut p, ObjectId(2), 300., 600.);
    let host = ObjectId(10);
    member(
        &mut p,
        host,
        ObjectId(2),
        Host {
            name: "Балка Бм1",
            kind: MemberKind::Beam,
            concrete: "beam",
            length: 4000.,
            cover: 30.,
        },
    );
    p.concrete.cage_types.push(cage(true));
    p.concrete = install_cage(&p, host, "cage").unwrap();
    p.next_id = 40;
    (p, host)
}

fn column() -> (Project, ObjectId) {
    let mut p = Project::default();
    material(&mut p);
    profile(&mut p, ObjectId(2), 400., 400.);
    let host = ObjectId(10);
    member(
        &mut p,
        host,
        ObjectId(2),
        Host {
            name: "Колонна Км1",
            kind: MemberKind::Column,
            concrete: "column",
            length: 3000.,
            cover: 35.,
        },
    );
    p.concrete.cage_types.push(cage(false));
    p.concrete = install_cage(&p, host, "cage").unwrap();
    p.concrete = install_starter(
        &p,
        host,
        StarterPath::ColumnTop,
        "gost.straight",
        16.,
        "А500С",
        200.,
    )
    .unwrap();
    p.next_id = 40;
    (p, host)
}

fn wall() -> (Project, ObjectId) {
    let mut p = Project::default();
    material(&mut p);
    profile(&mut p, ObjectId(2), 200., 1600.);
    let host = ObjectId(10);
    member(
        &mut p,
        host,
        ObjectId(2),
        Host {
            name: "Стена См1",
            kind: MemberKind::Beam,
            concrete: "panel",
            length: 2400.,
            cover: 25.,
        },
    );
    p.details.insert(
        ObjectId(11),
        Detail {
            id: ObjectId(11),
            kind: DetailKind::PrismCut {
                part: host,
                frame: Frame {
                    origin: Point3([120., -180., 900.]),
                    axes: [[0., 0., 1.], [0., 1., 0.], [-1., 0., 0.]],
                },
                contour: vec![[0., 0.], [500., 0.], [500., 400.], [0., 400.]],
                length: 240.,
            },
        },
    );
    let mut ty = WallRebarType::new("wall", "Стена");
    ty.near = Some(WallFace::new(12., 200., 8., 200.));
    ty.far = Some(WallFace::new(10., 200., 8., 200.));
    ty.cut_openings = true;
    ty.edge_diameter = 8.;
    ty.edge_spacing = 200.;
    p.concrete.wall_types.push(ty);
    p.concrete = install_wall(&p, host, "wall").unwrap();
    p.next_id = 40;
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

fn sheet_of(p: &Project, host: ObjectId) -> open_bim_drawings::Sheet {
    let drawing = reinforcement_drawing(p, ObjectId(100), host, "КЖ-2").unwrap();
    generate(p, &drawing).unwrap()
}

#[test]
fn beam_sheet_has_elevation_sections_schedules_and_exports() {
    let (p, host) = beam();
    let drawing = reinforcement_drawing(&p, ObjectId(100), host, "КЖ-2").unwrap();
    assert_eq!(status(&p, &drawing), Status::UpToDate);
    assert!(drawing.views.iter().any(|v| v.name == "Фасад"));
    assert!(drawing.views.iter().filter(|v| v.section.is_some()).count() >= 2);
    let sheet = generate(&p, &drawing).unwrap();
    let text = texts(&sheet);
    assert!(text.contains("Фасад"), "{text}");
    assert!(text.contains("1-1"), "{text}");
    assert!(text.contains("2-2"), "{text}");
    assert!(text.contains("шаг 100"), "{text}");
    assert!(text.contains("шаг 200"), "{text}");
    assert!(text.contains("Ведомость деталей"), "{text}");
    assert!(text.contains("Ведомость расхода стали"), "{text}");
    assert!(text.contains("0000-КЖ"), "{text}");
    assert!(text.contains("Армирование Балка Бм1"), "{text}");
    assert!(text.contains("ГОСТ 21.501-2018"), "{text}");
    assert!(text.contains("ГОСТ Р 21.101-2026"), "{text}");
    let pdf = pdf::render(std::slice::from_ref(&sheet), "КЖ-2").unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    let dxf = dxf::render(&sheet);
    assert!(dxf.contains("OB_VISIBLE"));
    assert!(dxf.contains("\\U+0412\\U+0435\\U+0434"), "{dxf}");
}

#[test]
fn beam_callouts_match_schedule_and_section_shows_cover() {
    let (p, host) = beam();
    let sheet = sheet_of(&p, host);
    let text = texts(&sheet);
    let rows = rebar::detail_schedule(&p);
    assert!(!rows.is_empty());
    for row in &rows {
        assert!(
            sheet.views.iter().flat_map(|v| &v.labels).any(|l| {
                l.text.contains("поз.")
                    && mentions(&l.text, row.mark)
                    && l.text.contains(&format!("Ø{}", row.diameter as i64))
                    && l.text.contains(&row.grade)
            }),
            "выноска позиции {} нет на листе: {text}",
            row.mark
        );
    }
    let section = sheet
        .views
        .iter()
        .find(|v| v.labels.iter().any(|l| l.text.starts_with("1-1")))
        .expect("разрез");
    let joined = section
        .labels
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("защитный слой 30"), "{joined}");
    assert!(
        section
            .lines
            .iter()
            .any(|l| l.style == open_bim_drawings::LineStyle::Hatch),
        "разрез без штриховки бетона"
    );
    assert!(
        section
            .lines
            .iter()
            .any(|l| l.style == open_bim_drawings::LineStyle::Visible && l.points.len() > 6),
        "хомут не показан в разрезе"
    );
}

#[test]
fn beam_sheet_follows_a_change_of_the_bars() {
    let (p, host) = beam();
    let drawing = reinforcement_drawing(&p, ObjectId(100), host, "КЖ-2").unwrap();
    let mut changed = p.clone();
    changed.revision += 1;
    let set = changed
        .concrete
        .rebars
        .iter_mut()
        .find(|s| s.kind == open_bim_model::RebarKind::Longitudinal)
        .unwrap();
    set.diameter = 20.;
    assert!(matches!(
        status(&changed, &drawing),
        Status::Outdated { .. }
    ));
    let sheet = generate(&changed, &drawing).unwrap();
    assert!(texts(&sheet).contains("Ø20"), "{}", texts(&sheet));
    let fresh = updated(&changed, &drawing);
    assert_eq!(status(&changed, &fresh), Status::UpToDate);
    changed
        .elements
        .get_mut(&host)
        .unwrap()
        .properties
        .insert(REBAR_COVER.into(), PropertyValue::Number(40.));
    changed.concrete = install_cage(&changed, host, "cage").unwrap();
    let again = generate(&changed, &fresh).unwrap();
    assert!(
        texts(&again).contains("защитный слой 40"),
        "{}",
        texts(&again)
    );
}

#[test]
fn column_sheet_shows_elevation_section_and_starters() {
    let (p, host) = column();
    let sheet = sheet_of(&p, host);
    let text = texts(&sheet);
    assert!(text.contains("Фасад"), "{text}");
    assert!(text.contains("1-1"), "{text}");
    assert!(text.contains("выпуск"), "{text}");
    assert!(text.contains("Выпуски"), "{text}");
    assert!(text.contains("защитный слой 35"), "{text}");
    assert!(text.contains("Ведомость деталей"), "{text}");
    let rows = rebar::detail_schedule(&p);
    let starter =
        rows.iter()
            .find(|r| {
                p.concrete.rebars.iter().any(|s| {
                    s.kind == open_bim_model::RebarKind::Starter && r.sets.contains(&s.number)
                })
            })
            .expect("позиция выпуска");
    assert!(
        text.contains("поз.") && mentions(&text, starter.mark),
        "выноска выпуска {} нет: {text}",
        starter.mark
    );
    let section = sheet
        .views
        .iter()
        .find(|v| v.labels.iter().any(|l| l.text.starts_with("1-1")))
        .unwrap();
    assert!(
        section
            .lines
            .iter()
            .any(|l| l.style == open_bim_drawings::LineStyle::Visible && l.points.len() > 6)
    );
    let pdf = pdf::render(std::slice::from_ref(&sheet), "КЖ-2").unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    assert!(dxf::render(&sheet).contains("OB_VISIBLE"));
}

#[test]
fn wall_sheet_shows_both_meshes_and_opening_sections() {
    let (p, host) = wall();
    let drawing = reinforcement_drawing(&p, ObjectId(100), host, "КЖ-3").unwrap();
    assert!(drawing.views.iter().any(|v| v.name == "Фасад"));
    assert!(drawing.views.iter().filter(|v| v.section.is_some()).count() >= 2);
    let sheet = generate(&p, &drawing).unwrap();
    let text = texts(&sheet);
    assert!(text.contains("Фасад"), "{text}");
    assert!(text.contains("1-1"), "{text}");
    assert!(text.contains("2-2"), "{text}");
    assert!(text.contains("ближн."), "{text}");
    assert!(text.contains("дальн."), "{text}");
    assert!(text.contains("проём 500×400"), "{text}");
    assert!(text.contains("защитный слой 25"), "{text}");
    assert!(text.contains("Ведомость расхода стали"), "{text}");
    assert!(text.contains("ГОСТ 21.501-2018"), "{text}");
    let rows = rebar::detail_schedule(&p);
    assert!(rows.iter().any(|r| r.code == "П"));
    for row in rows.iter().filter(|r| r.code == "ПР" || r.code == "П") {
        assert!(
            sheet.views.iter().flat_map(|v| &v.labels).any(|l| {
                l.text.contains("поз.")
                    && mentions(&l.text, row.mark)
                    && l.text.contains(&row.grade)
            }),
            "позиция {} не на листе: {text}",
            row.mark
        );
    }
    let section = sheet
        .views
        .iter()
        .find(|v| v.labels.iter().any(|l| l.text.starts_with("1-1")))
        .unwrap();
    assert!(
        section
            .lines
            .iter()
            .any(|l| l.style == open_bim_drawings::LineStyle::Visible),
        "разрез стены без стержней"
    );
    let pdf = pdf::render(std::slice::from_ref(&sheet), "КЖ-3").unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}
