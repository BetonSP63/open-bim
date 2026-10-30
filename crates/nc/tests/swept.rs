use open_bim_model::{ElementShape, MemberKind, SweptPart, example_steel_frame};
use open_bim_nc::{export, part_file};

#[test]
fn nc_rejects_every_swept_part_without_producing_a_file() {
    for swept in [
        SweptPart::CurvedBeam {
            center: [0., 0.],
            radius: 1000.,
            start_angle: 0.,
            sweep_angle: 1.,
        },
        SweptPart::PolyBeam {
            points: vec![[0., 0.], [1000., 0.], [1000., 1000.]],
            bend_radius: 100.,
        },
        SweptPart::BentPlate {
            width: 100.,
            first_length: 200.,
            second_length: 150.,
            radius: 30.,
            angle: std::f64::consts::FRAC_PI_2,
        },
    ] {
        let mut project = example_steel_frame().unwrap();
        let id = project
            .elements
            .values()
            .find(|e| match swept {
                SweptPart::BentPlate { .. } => matches!(e.shape, ElementShape::Plate { .. }),
                _ => matches!(
                    e.shape,
                    ElementShape::Member {
                        member_kind: MemberKind::Beam,
                        ..
                    }
                ),
            })
            .unwrap()
            .id;
        project.swept_parts.insert(id, swept);
        for error in [
            part_file(&project, id, "1", 1).unwrap_err(),
            export(&project).unwrap_err(),
        ] {
            assert!(
                error.contains("экспорт гнутых деталей в NC пока не поддерживается"),
                "{error}"
            );
            assert!(error.contains("могут быть устаревшими"), "{error}");
        }
    }
}
