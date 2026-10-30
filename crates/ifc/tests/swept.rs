use open_bim_ifc::export;
use open_bim_model::{ElementShape, MemberKind, SweptPart, example_steel_frame};

#[test]
fn curved_beam_is_exported_as_tessellation_not_as_a_straight_extrusion() {
    let mut project = example_steel_frame().unwrap();
    let id = project
        .elements
        .values()
        .find(|e| {
            matches!(
                e.shape,
                ElementShape::Member {
                    member_kind: MemberKind::Beam,
                    ..
                }
            )
        })
        .unwrap()
        .id;
    project.swept_parts.insert(
        id,
        SweptPart::CurvedBeam {
            center: [0., 0.],
            radius: 1000.,
            start_angle: 0.,
            sweep_angle: 1.,
        },
    );
    let (text, report) = export(&project).unwrap();
    if let Some(path) = std::env::var_os("OPEN_BIM_SWEEP_IFC") {
        std::fs::write(path, &text).unwrap();
    }
    assert_eq!(report.tessellated, vec![id]);
    assert!(text.contains("IFCCARTESIANPOINTLIST3D("));
    assert!(text.contains("IFCTRIANGULATEDFACESET("));
    assert!(text.contains("'Body','Tessellation'"));
}
