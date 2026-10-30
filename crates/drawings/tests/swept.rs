use open_bim_drawings::{LineStyle, generate, part_drawing};
use open_bim_model::{Element, ElementShape, Frame, LengthMm, ObjectId, Project, SweptPart};
fn fixture() -> (Project, ObjectId) {
    let mut p = Project::default();
    let id = p.allocate_id().unwrap();
    p.elements.insert(
        id,
        Element {
            id,
            name: "Гнутый лист".into(),
            material: ObjectId(1),
            properties: Default::default(),
            shape: ElementShape::Plate {
                contour: vec![[0., 0.], [1., 0.], [1., 1.], [0., 1.]],
                thickness: LengthMm::new(10.).unwrap(),
                frame: Frame::default(),
            },
        },
    );
    p.swept_parts.insert(
        id,
        SweptPart::BentPlate {
            width: 100.,
            first_length: 200.,
            second_length: 150.,
            radius: 30.,
            angle: std::f64::consts::FRAC_PI_2,
        },
    );
    (p, id)
}
#[test]
fn formed_views_and_development_share_one_part_and_regenerate_after_a_bend_change() {
    let (mut p, id) = fixture();
    let d = part_drawing(&p, ObjectId(99), id, "КМД-1").unwrap();
    assert_eq!(d.views.len(), 4);
    assert_eq!(d.views.iter().filter(|v| v.unfolded).count(), 1);
    let sheet = generate(&p, &d).unwrap();
    let flat = &sheet.views[3];
    let length = 350. + 35. * std::f64::consts::FRAC_PI_2;
    assert!(((flat.bounds[1][0] - flat.bounds[0][0]) * flat.scale - length).abs() < 1e-8);
    assert!(((flat.bounds[1][1] - flat.bounds[0][1]) * flat.scale - 100.).abs() < 1e-8);
    let fold = flat
        .lines
        .iter()
        .find(|l| l.style == LineStyle::DashDot)
        .unwrap();
    assert!(
        ((fold.points[0][0] - flat.bounds[0][0]) * flat.scale
            - (200. + 35. * std::f64::consts::FRAC_PI_2 / 2.))
            .abs()
            < 1e-8
    );
    assert!(
        sheet.views[0]
            .labels
            .iter()
            .any(|l| l.text.contains("R внутр. 30"))
    );
    if let Some(SweptPart::BentPlate { radius, .. }) = p.swept_parts.get_mut(&id) {
        *radius = 50.;
    }
    let updated = generate(&p, &d).unwrap();
    let flat2 = &updated.views[3];
    assert!(
        ((flat2.bounds[1][0] - flat2.bounds[0][0]) * flat2.scale
            - (350. + 55. * std::f64::consts::FRAC_PI_2))
            .abs()
            < 1e-8
    );
    assert_eq!(p.elements.len(), 1);
    let svg = open_bim_drawings::svg::render(&sheet);
    assert!(svg.contains("Развёртка до гибки"));
    assert!(svg.contains("Зона сгиба"));
}

#[test]
fn flat_and_formed_dimensions_use_their_own_real_anchors() {
    let (p, id) = fixture();
    let mut d = part_drawing(&p, ObjectId(99), id, "КМД-1").unwrap();
    use open_bim_model::{Anchor, AnchorPoint, DimensionAxis, DimensionDef};
    d.dimensions.push(DimensionDef {
        id: 1,
        view: 4,
        anchors: vec![
            Anchor {
                object: id,
                point: AnchorPoint::Vertex { index: 0 },
            },
            Anchor {
                object: id,
                point: AnchorPoint::Vertex { index: 1 },
            },
        ],
        axis: DimensionAxis::Horizontal,
        offset: 5.,
    });
    let sheet = generate(&p, &d).unwrap();
    assert!(
        (sheet.dimensions[0].values_mm[0] - (350. + 35. * std::f64::consts::FRAC_PI_2)).abs()
            < 1e-8
    );
    let anchors = open_bim_drawings::anchor_candidates(&p, &d).unwrap();
    let a = anchors
        .iter()
        .find(|(v, a, _)| *v == 4 && a.point == AnchorPoint::Vertex { index: 0 })
        .unwrap()
        .2;
    let b = anchors
        .iter()
        .find(|(v, a, _)| *v == 4 && a.point == AnchorPoint::Vertex { index: 1 })
        .unwrap()
        .2;
    assert!(
        ((b[0] - a[0]) * sheet.views[3].scale - (350. + 35. * std::f64::consts::FRAC_PI_2)).abs()
            < 1e-8
    );
    assert!(open_bim_drawings::svg::render(&sheet).contains("class=\"DashDot\""));
}
