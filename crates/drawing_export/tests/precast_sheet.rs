use open_bim_model::{
    Assembly, Element, ElementShape, Frame, LengthMm, Material, ObjectId, Point3, Project,
    PropertyValue,
    pours::{CAST_UNIT_KEY, CONCRETE_KEY, PRECAST},
};
use std::collections::{BTreeMap, BTreeSet};

use open_bim_drawing_export::{dxf, pdf};
use open_bim_drawings::{Status, generate, precast_drawing, status, updated};

fn slab(project: &mut Project, x: f64, length: f64, kind: &str) -> ObjectId {
    let part = ObjectId(project.next_id);
    let assembly = ObjectId(project.next_id + 1);
    project.next_id += 2;
    project.elements.insert(
        part,
        Element {
            id: part,
            name: "Плита".into(),
            material: ObjectId(1),
            shape: ElementShape::Plate {
                frame: Frame {
                    origin: Point3([x, 0., 0.]),
                    ..Frame::default()
                },
                contour: vec![[0., 0.], [length, 0.], [length, 1000.], [0., 1000.]],
                thickness: LengthMm::new(200.).unwrap(),
            },
            properties: BTreeMap::from([(CONCRETE_KEY.into(), PropertyValue::Text("slab".into()))]),
        },
    );
    project.assemblies.insert(
        assembly,
        Assembly {
            id: assembly,
            name: "Литая единица".into(),
            main_part: part,
            members: BTreeSet::from([part]),
            children: BTreeSet::new(),
            properties: BTreeMap::from([
                (CAST_UNIT_KEY.into(), PropertyValue::Text(kind.into())),
                ("designation".into(), PropertyValue::Text("Серия 1".into())),
            ]),
        },
    );
    assembly
}

#[test]
fn schedule_and_leader_export_and_follow_model_changes() {
    let mut project = Project::default();
    project.materials.insert(
        ObjectId(1),
        Material {
            id: ObjectId(1),
            name: "Бетон В25".into(),
            density_kg_m3: 2500.,
        },
    );
    let first = slab(&mut project, 0., 2000., PRECAST);
    slab(&mut project, 2500., 2000., PRECAST);
    slab(&mut project, 5000., 2000., "монолитный");
    for (x, name) in [(500., "А"), (3500., "Б")] {
        let id = ObjectId(project.next_id);
        project.next_id += 1;
        project.axes.insert(
            id,
            open_bim_model::GridAxis {
                id,
                name: name.into(),
                start: Point3([x, -1000., 0.]),
                end: Point3([x, 2000., 0.]),
            },
        );
    }
    let drawing = precast_drawing(&project, ObjectId(project.next_id), vec![], "КЖ-1").unwrap();
    assert_eq!(status(&project, &drawing), Status::UpToDate);
    let sheet = generate(&project, &drawing).unwrap();
    let marks: Vec<_> = sheet.views[0]
        .labels
        .iter()
        .filter(|label| label.text == "П1")
        .collect();
    assert_eq!(marks.len(), 2);
    assert!(
        sheet
            .labels
            .iter()
            .any(|label| label.text == "Спецификация сборных элементов")
    );
    assert!(sheet.labels.iter().any(|label| label.text == "1000,000"));
    assert!(
        sheet
            .labels
            .iter()
            .any(|label| label.text == "Схема по ГОСТ 21.501-2018")
    );
    for text in ["А", "Б", "3000"] {
        assert!(
            sheet.views[0].labels.iter().any(|label| label.text == text),
            "{text}"
        );
    }
    let dxf = dxf::render(&sheet);
    assert!(dxf.contains("1000,000"));
    assert!(dxf.contains("21.501-2018"));
    let pdf = pdf::render(&[sheet], "КЖ-1").unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    use std::io::Read;
    let start = pdf
        .windows(8)
        .rposition(|bytes| bytes == b"\nstream\n")
        .unwrap()
        + 8;
    let mut content = String::new();
    flate2::read::ZlibDecoder::new(&pdf[start..])
        .read_to_string(&mut content)
        .unwrap();
    let font = open_bim_drawing_export::font::Font::load().unwrap();
    for text in [
        "Спецификация сборных элементов",
        "1000,000",
        "Схема по ГОСТ 21.501-2018",
    ] {
        let encoded = text
            .chars()
            .map(|ch| format!("{:04X}", font.glyph(ch)))
            .collect::<String>();
        assert!(content.contains(&format!("<{encoded}> Tj")), "{text}");
    }
    let part = project.assemblies[&first].main_part;
    let ElementShape::Plate { frame, .. } = &mut project.elements.get_mut(&part).unwrap().shape
    else {
        panic!("expected plate")
    };
    frame.origin.0[0] += 400.;
    assert!(matches!(
        status(&project, &drawing),
        Status::Outdated { .. }
    ));
    assert_eq!(
        status(&project, &updated(&project, &drawing)),
        Status::UpToDate
    );
}
