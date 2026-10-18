//! Viewport depth and shading: nothing is clipped when zooming in close, depth
//! keeps millimetre resolution far away, and every display mesh is oriented
//! consistently outward (no dark «rippling» triangles).
use open_bim_model::example_steel_frame;
use open_bim_renderer::{Bounds, Camera, DVec3, DisplayMesh, NamedView, Scene};
use std::collections::HashMap;

/// Edges used twice in the same direction (after welding coincident vertices)
/// = triangles wound against their neighbours.
fn inconsistent_edges(mesh: &DisplayMesh) -> usize {
    let key = |p: DVec3| (p * 1e3).round().to_array().map(|v| v as i64);
    let mut directed: HashMap<([i64; 3], [i64; 3]), usize> = HashMap::new();
    for t in &mesh.triangles {
        let p = t.map(|i| key(mesh.positions[i as usize]));
        for k in 0..3 {
            let (a, b) = (p[k], p[(k + 1) % 3]);
            if a != b {
                *directed.entry((a, b)).or_default() += 1;
            }
        }
    }
    directed.values().filter(|n| **n > 1).count()
}

/// Signed volume (positive = outward winding for a closed solid).
fn signed_volume(mesh: &DisplayMesh) -> f64 {
    mesh.triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| mesh.positions[i as usize]);
            a.dot(b.cross(c)) / 6.
        })
        .sum()
}

/// The steel release fixture: drilled parts go through the exact CSG body
/// tessellation (bolt holes, end plates, clip angles).
fn drilled() -> open_bim_model::Project {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/steel_release/source.obim"
    ))
    .unwrap();
    let file: serde_json::Value = serde_json::from_str(&text).unwrap();
    serde_json::from_value(file["project"].clone()).unwrap()
}

#[test]
fn every_display_mesh_is_wound_outward() {
    for project in [example_steel_frame().unwrap(), drilled()] {
        let scene = Scene::from_project(&project).unwrap();
        for (i, mesh) in scene.meshes.iter().enumerate() {
            assert_eq!(
                inconsistent_edges(mesh),
                0,
                "mesh {i}: triangles wound against neighbours"
            );
            assert!(signed_volume(mesh) > 0., "mesh {i}: inside-out");
        }
    }
}

#[test]
fn zooming_close_never_clips_the_model_away() {
    let project = example_steel_frame().unwrap();
    let scene = Scene::from_project(&project).unwrap();
    let corners = |b: Bounds| {
        (0..8).map(move |i| {
            DVec3::new(
                if i & 1 == 0 { b.min.x } else { b.max.x },
                if i & 2 == 0 { b.min.y } else { b.max.y },
                if i & 4 == 0 { b.min.z } else { b.max.z },
            )
        })
    };
    let target = scene.instances[0].bounds.max;
    for ortho in [false, true] {
        for distance in [50., 1000., 100_000., 10_000_000.] {
            let mut camera = Camera::default();
            camera.fit(scene.bounds);
            camera.set_view(NamedView::Isometric);
            camera.orthographic = ortho;
            camera.target = target;
            camera.distance = distance;
            camera.ortho_height = distance;
            let m = camera.matrix(scene.origin, 1.5);
            let eye = camera.eye();
            let forward = (camera.target - eye).normalize();
            for c in corners(scene.bounds) {
                if !ortho && (c - eye).dot(forward) <= 1.0 {
                    continue; // behind the eye in perspective
                }
                let p = m * (c - scene.origin).extend(1.);
                let z = p.z / p.w;
                assert!(
                    (0.0..=1.0).contains(&z),
                    "ortho={ortho} distance={distance}: corner {c:?} depth {z}"
                );
            }
        }
    }
}

#[test]
fn depth_resolves_a_millimetre_at_fifty_metres() {
    let mut camera = Camera::default();
    let bounds = Bounds {
        min: DVec3::new(-60_000., -60_000., -10_000.),
        max: DVec3::new(60_000., 60_000., 60_000.),
    };
    camera.fit(bounds);
    camera.target = DVec3::ZERO;
    camera.distance = 1000.;
    let m = camera.matrix(DVec3::ZERO, 1.5);
    let eye = camera.eye();
    let forward = (camera.target - eye).normalize();
    let depth = |d: f64| {
        let p = m * (eye + forward * d).extend(1.);
        (p.z / p.w) as f32
    };
    assert_ne!(depth(50_000.), depth(50_001.), "1 mm at 50 m");
    assert!(
        depth(50_000.) > depth(50_001.),
        "reversed-Z: nearer is larger"
    );
}

#[test]
fn rays_still_start_at_the_eye_side() {
    let camera = Camera {
        target: DVec3::new(1000., 2000., 0.),
        ..Camera::default()
    };
    let ray = camera.ray(DVec3::ZERO, [0.5, 0.5], 1.5).unwrap();
    let to_target = (camera.target - ray.origin).normalize();
    assert!(
        ray.direction.dot(to_target) > 0.999,
        "ray points from the eye to the target"
    );
}
