use open_bim_connections::{ConnectionInstance, ConnectionTemplate, base_plate_geometry};
use open_bim_model::{ElementShape, ObjectId, Point3, example_steel_frame};

#[test]
fn base_plate_recipe_tracks_section_attachment_and_world_frame() {
    let mut p = example_steel_frame().unwrap();
    let template = ConnectionTemplate::from_json(include_bytes!(
        "../../../tests/fixtures/connections/base-plate.json"
    ))
    .unwrap();
    let mut instance = ConnectionInstance {
        id: ObjectId(16),
        primary: ObjectId(8),
        secondary: None,
        at_end: false,
        template,
        children: Default::default(),
    };
    let recipe = base_plate_geometry(&p, &instance).unwrap();
    let ElementShape::Plate {
        contour,
        thickness,
        frame,
    } = recipe.shape
    else {
        panic!()
    };
    assert_eq!(thickness.value(), 20.0);
    // Built-in I300 is 150mm wide and 300mm high; 70mm overhang each side.
    assert_eq!(contour, [[0., 0.], [290., 0.], [290., 440.], [0., 440.]]);
    assert_eq!(frame.origin, Point3([-145., -220., -20.]));
    assert_eq!(recipe.steps_x, [220.]);
    assert_eq!(recipe.steps_y, [370.]);
    assert_eq!(recipe.holes.len(), 4);
    assert_eq!(recipe.weld_path.first(), recipe.weld_path.last());
    instance.at_end = true;
    let upper = base_plate_geometry(&p, &instance).unwrap();
    assert_eq!(upper.shape.frame().origin.0[2], 3000.);
    p.elements
        .get_mut(&ObjectId(8))
        .unwrap()
        .shape
        .frame_mut()
        .origin = Point3([100., 200., 300.]);
    let moved = base_plate_geometry(&p, &instance).unwrap();
    assert_eq!(moved.shape.frame().origin, Point3([-45., -20., 3300.]));
    assert_eq!(moved.weld_path[0].0[2], 3300.);
    instance.secondary = Some(ObjectId(9));
    assert!(base_plate_geometry(&p, &instance).is_err());
}
