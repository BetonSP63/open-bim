//! Welded sections: one member in the model, separate plates and seams in
//! the lists and the rolled-steel specification.
use open_bim_model::sections::templates;
use open_bim_model::{Element, ElementShape, Frame, MemberKind, ObjectId, Point3, Project};
use std::collections::BTreeMap;

fn welded_beam() -> (Project, ObjectId) {
    let mut project = Project::default();
    let profile_id = project.allocate_id().unwrap();
    let i = templates().into_iter().next().unwrap();
    let profile = i.profile(profile_id, &BTreeMap::new()).unwrap();
    project.profiles.insert(profile_id, profile);
    let (frame, length) =
        Frame::along(Point3([0., 0., 3000.]), Point3([6000., 0., 3000.])).unwrap();
    let id = project.allocate_id().unwrap();
    project.elements.insert(
        id,
        Element {
            id,
            name: "Балка".into(),
            material: ObjectId(1),
            shape: ElementShape::Member {
                member_kind: MemberKind::Beam,
                profile: profile_id,
                length,
                frame,
                position: Default::default(),
            },
            properties: BTreeMap::new(),
        },
    );
    project.validate().unwrap();
    (project, id)
}

#[test]
fn lists_show_plates_and_seams() {
    let (project, id) = welded_beam();
    let mass = project.element_mass_kg(id).unwrap();
    // 2 × 250×16 + 10×568 mm² over 6 m of steel.
    let area = 2. * 250. * 16. + 10. * 568.;
    assert!((mass - area * 6000. * 7850. / 1e9).abs() < 1e-6);
    let r = open_bim_reports::generate(&project).unwrap();
    let rows: Vec<_> = r
        .parts
        .iter()
        .map(|p| (p.section.as_str(), p.length_mm))
        .collect();
    assert_eq!(
        rows,
        [("—16×250", 6000.), ("—10×568", 6000.), ("—16×250", 6000.)]
    );
    assert!(r.parts[0].position.ends_with(".1") && r.parts[2].position.ends_with(".3"));
    let listed: f64 = r.parts.iter().map(|p| p.total_mass_kg).sum();
    assert!((listed - mass).abs() < 0.01);
    // Two belt seams, both sides: four runs of 6 m.
    let seams: Vec<_> = r.fasteners.iter().filter(|f| f.kind == "Сварка").collect();
    assert_eq!(seams.len(), 1, "{seams:?}");
    assert_eq!((seams[0].quantity, seams[0].length_mm), (4, 24000.));
    assert!(seams[0].description.contains("ГОСТ 8713") && seams[0].description.contains("k=7"));

    let steel = open_bim_drawings::tables::rolled_steel(&project, &[id]).unwrap();
    let sizes: Vec<_> = steel.lines.iter().map(|l| l.size.as_str()).collect();
    assert_eq!(sizes, ["—10", "—16"]);
    assert!(
        steel
            .lines
            .iter()
            .all(|l| l.assortment.starts_with("Прокат листовой"))
    );
    assert!((steel.total.total_t * 1000. - mass).abs() < 0.01);
}

#[test]
fn body_render_and_ifc_take_user_sections() {
    let (mut project, id) = welded_beam();
    let volume = project.element_body(id).unwrap().volume_mm3().unwrap();
    assert!((volume - project.element_volume_mm3(id).unwrap()).abs() < 1.);
    // A box section keeps its void in the exact body.
    let boxed = templates()
        .into_iter()
        .find(|t| t.name == "Коробка сварная")
        .unwrap();
    let pid = project.allocate_id().unwrap();
    project
        .profiles
        .insert(pid, boxed.profile(pid, &BTreeMap::new()).unwrap());
    if let ElementShape::Member { profile, .. } = &mut project.elements.get_mut(&id).unwrap().shape
    {
        *profile = pid;
    }
    let body = project.element_body(id).unwrap().volume_mm3().unwrap();
    assert!((body - project.element_volume_mm3(id).unwrap()).abs() < 1.);
    let (step, _) = open_bim_ifc::export(&project).unwrap();
    assert!(
        step.contains("IFCARBITRARYPROFILEDEFWITHVOIDS"),
        "box exported with its void"
    );
}
