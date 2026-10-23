//! «Визуализатор»: the ray-traced still shows the model lit, with shadows.
use open_bim_model::example_steel_frame;
use open_bim_renderer::visualize::{Settings, render};
use open_bim_renderer::{Camera, NamedView, Scene, ViewState};

fn setup() -> (Scene, Camera) {
    let scene = Scene::from_project(&example_steel_frame().unwrap()).unwrap();
    let mut camera = Camera::default();
    camera.set_view(NamedView::Isometric);
    camera.fit(scene.bounds);
    (scene, camera)
}

fn small(shadows: bool) -> Settings {
    Settings {
        width: 160,
        height: 100,
        samples: 1,
        shadows,
        ..Default::default()
    }
}

fn luminance(px: &[u8]) -> u32 {
    px[0] as u32 + px[1] as u32 + px[2] as u32
}

#[test]
fn render_shows_parts_and_their_shadows() {
    let (scene, camera) = setup();
    let state = ViewState::default();
    let lit = render(&scene, &camera, &state, &small(true));
    assert_eq!(lit.rgba.len(), 160 * 100 * 4);
    // Deterministic.
    assert!(lit.rgba == render(&scene, &camera, &state, &small(true)).rgba);
    // Parts cover the middle of the frame: not all pixels are sky or ground.
    let distinct: std::collections::BTreeSet<[u8; 3]> =
        lit.rgba.chunks(4).map(|p| [p[0], p[1], p[2]]).collect();
    assert!(distinct.len() > 50, "flat image: {}", distinct.len());
    // Shadows darken the picture.
    let flat = render(&scene, &camera, &state, &small(false));
    let sum = |rgba: &[u8]| rgba.chunks(4).map(luminance).sum::<u32>();
    assert!(sum(&lit.rgba) < sum(&flat.rgba));
    // Hiding every part leaves only sky and ground.
    let hidden = ViewState {
        hidden: scene.instances.iter().map(|i| i.id).collect(),
        ..Default::default()
    };
    let empty = render(&scene, &camera, &hidden, &small(true));
    assert!(sum(&empty.rgba) > sum(&lit.rgba));
}
