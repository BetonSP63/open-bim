//! Members with welded and contour sections get display meshes.
use open_bim_model::sections::templates;
use open_bim_model::{ElementShape, example_steel_frame};
use open_bim_renderer::Scene;

#[test]
fn welded_and_contour_sections_render() {
    for name in ["Коробка сварная", "Z-профиль"] {
        let mut project = example_steel_frame().unwrap();
        let id = project.allocate_id().unwrap();
        let t = templates().into_iter().find(|t| t.name == name).unwrap();
        project
            .profiles
            .insert(id, t.profile(id, &Default::default()).unwrap());
        for e in project.elements.values_mut() {
            if let ElementShape::Member { profile, .. } = &mut e.shape {
                *profile = id;
            }
        }
        let scene = Scene::from_project(&project).unwrap();
        assert!(
            scene.meshes.iter().all(|m| !m.triangles.is_empty()),
            "{name}"
        );
    }
}
