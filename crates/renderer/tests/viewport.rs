use open_bim_model::{
    Element, ElementShape, Frame, LengthMm, ObjectId, Point3, Project, example_steel_frame,
};
use open_bim_renderer::{Camera, ClipPlane, DVec3, NamedView, Ray, Scene, ViewState};

fn overlap() -> Project {
    let mut project = Project::default();
    for (id, z) in [(8, 0.0), (9, 100.0)] {
        project.elements.insert(
            ObjectId(id),
            Element {
                id: ObjectId(id),
                name: format!("plate {id}"),
                material: ObjectId(1),
                shape: ElementShape::Plate {
                    contour: vec![
                        [-100.0, -100.0],
                        [100.0, -100.0],
                        [100.0, 100.0],
                        [-100.0, 100.0],
                    ],
                    thickness: LengthMm::new(20.0).unwrap(),
                    frame: Frame {
                        origin: Point3([0.0, 0.0, z]),
                        ..Default::default()
                    },
                },
                properties: Default::default(),
            },
        );
    }
    project.next_id = 10;
    project
}
#[test]
fn nearest_visible_stable_id_and_section() {
    let project = overlap();
    let scene = Scene::from_project(&project).unwrap();
    let ray = Ray {
        origin: DVec3::new(1.0, 1.0, 1000.0),
        direction: DVec3::NEG_Z,
    };
    let mut state = ViewState::default();
    assert_eq!(scene.pick(ray, &state), Some(ObjectId(9)));
    state.hidden.insert(ObjectId(9));
    assert_eq!(scene.pick(ray, &state), Some(ObjectId(8)));
    state.section = Some(ClipPlane {
        normal: DVec3::Z,
        offset: 50.0,
    });
    assert_eq!(scene.pick(ray, &state), None);
    state.hidden.clear();
    assert_eq!(scene.pick(ray, &state), Some(ObjectId(9)));
    state.section = Some(ClipPlane {
        normal: DVec3::NEG_Z,
        offset: -50.0,
    });
    assert_eq!(scene.pick(ray, &state), Some(ObjectId(8)));
    state.hidden.insert(ObjectId(8));
    assert_eq!(scene.pick(ray, &state), None);
}
#[test]
fn camera_ray_projection_roundtrip_all_views() {
    let scene = Scene::from_project(&example_steel_frame().unwrap()).unwrap();
    let mut camera = Camera::default();
    camera.fit(scene.bounds);
    for view in [
        NamedView::Isometric,
        NamedView::Front,
        NamedView::Top,
        NamedView::Right,
    ] {
        camera.set_view(view);
        let target = camera.target;
        let uv = camera.project(scene.origin, target, 1.6).unwrap();
        assert!((uv[0] - 0.5).abs() < 1e-10 && (uv[1] - 0.5).abs() < 1e-10);
        let ray = camera.ray(scene.origin, uv, 1.6).unwrap();
        assert!((target - ray.origin).cross(ray.direction).length() < 1e-5);
        let prior = camera.target;
        camera.pan(50.0, 30.0, 600.0);
        assert!(camera.target.is_finite() && camera.target != prior);
        let distance = camera.distance;
        camera.zoom(100.0);
        assert!(camera.distance < distance);
        camera.orbit(30.0, 15.0);
        assert!(camera.matrix(scene.origin, 1.6).is_finite());
    }
    assert!(camera.ray(scene.origin, [f64::NAN, 0.0], 1.0).is_none());
    assert!(camera.ray(scene.origin, [0.5, 0.5], 0.0).is_none());
}
#[test]
fn shared_meshes_far_origin_and_hidden_counts() {
    let mut project = example_steel_frame().unwrap();
    let original = Scene::from_project(&project).unwrap();
    assert_eq!(original.instances.len(), 7);
    assert_eq!(original.meshes.len(), 3);
    let shift = DVec3::new(900_000_000.123, -800_000_000.789, 0.0);
    for element in project.elements.values_mut() {
        let frame = element.shape.frame_mut();
        frame.origin = Point3((DVec3::from_array(frame.origin.0) + shift).to_array());
    }
    let scene = Scene::from_project(&project).unwrap();
    let mut camera = Camera::default();
    camera.fit(scene.bounds);
    camera.set_view(NamedView::Top);
    let point = shift + DVec3::new(10.0, 10.0, 20.0);
    let uv = camera.project(scene.origin, point, 1.0).unwrap();
    let ray = camera.ray(scene.origin, uv, 1.0).unwrap();
    let state = ViewState::default();
    assert!(scene.pick(ray, &state).is_some());
    let mut hidden = state.clone();
    hidden.hidden.extend(scene.instances.iter().map(|i| i.id));
    assert_eq!(scene.visible_triangles(&hidden), 0);
    assert_eq!(scene.pick(ray, &hidden), None);
}
#[test]
fn hollow_display_mesh_is_closed_and_preserves_void() {
    let mut project = overlap();
    project.elements.clear();
    for profile in [ObjectId(5), ObjectId(6)] {
        let id = project.allocate_id().unwrap();
        project.elements.insert(
            id,
            Element {
                id,
                name: "hollow".into(),
                material: ObjectId(1),
                shape: ElementShape::Member {
                    member_kind: open_bim_model::MemberKind::Column,
                    profile,
                    length: LengthMm::new(100.0).unwrap(),
                    frame: Frame::default(),
                    position: Default::default(),
                },
                properties: Default::default(),
            },
        );
    }
    let scene = Scene::from_project(&project).unwrap();
    for mesh in &scene.meshes {
        let mut edges = std::collections::BTreeMap::new();
        for t in &mesh.triangles {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let entry = edges.entry((a.min(b), a.max(b))).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += if a < b { 1 } else { -1 };
            }
        }
        assert!(edges.values().all(|e| *e == (2, 0)));
    }
    assert_eq!(
        scene.pick(
            Ray {
                origin: DVec3::new(0.0, 0.0, 200.0),
                direction: DVec3::NEG_Z
            },
            &ViewState::default()
        ),
        None
    );
}
