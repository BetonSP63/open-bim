use open_bim_model::{
    Element, ElementShape, Frame, LengthMm, MemberKind, ObjectId, Project, SweptPart,
};
use open_bim_nc::{export, part_file};

fn part(swept: SweptPart) -> (Project, ObjectId) {
    let mut p = Project::default();
    let id = p.allocate_id().unwrap();
    let shape = if matches!(swept, SweptPart::BentPlate { .. }) {
        ElementShape::Plate {
            contour: vec![[0., 0.], [1., 0.], [1., 100.], [0., 100.]],
            thickness: LengthMm::new(10.).unwrap(),
            frame: Frame::default(),
        }
    } else {
        ElementShape::Member {
            member_kind: MemberKind::Beam,
            profile: ObjectId(7),
            length: LengthMm::new(1.).unwrap(),
            frame: Frame::default(),
            position: Default::default(),
        }
    };
    p.elements.insert(
        id,
        Element {
            id,
            name: "Гнутая деталь".into(),
            material: ObjectId(1),
            shape,
            properties: Default::default(),
        },
    );
    p.swept_parts.insert(id, swept);
    (p, id)
}
#[test]
fn nc_is_straight_stock_with_a_separate_bending_card_and_preserved_model() {
    for (swept, expected) in [
        (
            SweptPart::CurvedBeam {
                center: [0., 0.],
                radius: 1000.,
                start_angle: 0.,
                sweep_angle: std::f64::consts::FRAC_PI_2,
            },
            1000. * std::f64::consts::FRAC_PI_2,
        ),
        (
            SweptPart::PolyBeam {
                points: vec![[0., 0.], [1000., 0.], [1000., 1000.]],
                bend_radius: 200.,
            },
            1600. + 200. * std::f64::consts::FRAC_PI_2,
        ),
        (
            SweptPart::BentPlate {
                width: 100.,
                first_length: 200.,
                second_length: 150.,
                radius: 30.,
                angle: std::f64::consts::FRAC_PI_2,
            },
            350. + 35. * std::f64::consts::FRAC_PI_2,
        ),
    ] {
        let (p, id) = part(swept);
        let before = p.clone();
        let file = part_file(&p, id, "12", 3).unwrap();
        assert!(
            (file.length - expected).abs() < 1e-9,
            "{} {expected}",
            file.length
        );
        assert!(file.text.is_ascii());
        assert!(file.text.contains("DEVELOPED STOCK BEFORE BENDING"));
        let card = file.bending_card.unwrap();
        assert!(card.contains("Марка: 12"));
        assert!(card.contains("Количество: 3"));
        assert!(card.contains("не является программой гибочного станка"));
        assert_eq!(export(&p).unwrap().len(), 1);
        assert_eq!(p, before);
    }
}
#[test]
fn flat_plate_contour_and_neutral_bend_line_have_independent_dimensions() {
    let (p, id) = part(SweptPart::BentPlate {
        width: 100.,
        first_length: 200.,
        second_length: 150.,
        radius: 30.,
        angle: -std::f64::consts::FRAC_PI_2,
    });
    let file = part_file(&p, id, "1", 1).unwrap();
    let block = file
        .text
        .split("\nAK\n")
        .nth(1)
        .unwrap()
        .split("\nEN")
        .next()
        .unwrap();
    let coordinates: Vec<_> = block
        .lines()
        .take_while(|l| l.starts_with("  "))
        .filter_map(|l| {
            let f: Vec<_> = l.split_whitespace().collect();
            (f.first() == Some(&"v"))
                .then(|| [f[1].parse::<f64>().unwrap(), f[2].parse::<f64>().unwrap()])
        })
        .collect();
    assert_eq!(coordinates.len(), 5);
    assert!(
        coordinates
            .iter()
            .any(|p| (p[0] - 404.98).abs() < 0.001 && p[1] == 100.)
    );
    let card = file.bending_card.unwrap();
    assert!(card.contains("-90.000000°"));
    assert!(card.contains("227.489 мм"));
}

#[test]
fn wide_stock_card_uses_the_rotated_nc_coordinate_system() {
    let (p, id) = part(SweptPart::BentPlate {
        width: 1000.,
        first_length: 200.,
        second_length: 150.,
        radius: 30.,
        angle: std::f64::consts::FRAC_PI_2,
    });
    let file = part_file(&p, id, "1", 1).unwrap();
    assert_eq!(file.length, 1000.);
    assert!(file.bending_card.unwrap().contains("Y = 177.489 мм"));
    let contour = file.text.split("\nAK\n").nth(1).unwrap();
    assert!(
        contour
            .lines()
            .filter_map(|l| {
                let fields: Vec<_> = l.split_whitespace().collect();
                (fields.first() == Some(&"v")).then(|| {
                    [
                        fields[1].parse::<f64>().unwrap(),
                        fields[2].parse::<f64>().unwrap(),
                    ]
                })
            })
            .any(|p| p[0] == 1000. && (p[1] - 404.98).abs() < 0.001)
    );
}
