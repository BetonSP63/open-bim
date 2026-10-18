//! Model views like Tekla «Создать виды по линиям сетки»: a plan per level
//! and an elevation per grid line, each with its camera direction, view depth
//! (two clip planes) and its own work plane.
use crate::interaction::Workplane;
use open_bim_model::Project;
use open_bim_renderer::{ClipPlane, DVec3};

#[derive(Clone, Debug, PartialEq)]
pub enum ViewKind {
    Model3d,
    Plan { elevation: f64 },
    Elevation { from: DVec3, to: DVec3 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelView {
    pub name: String,
    pub kind: ViewKind,
    /// Camera yaw and pitch (radians, see `Camera`), orthographic for plane views.
    pub yaw: f64,
    pub pitch: f64,
    pub orthographic: bool,
    /// View depth: the region kept is between the two planes.
    pub clip: [Option<ClipPlane>; 2],
    /// Work plane used while this view is active (kept per view).
    pub workplane: Workplane,
}

impl ModelView {
    pub fn model_3d() -> Self {
        Self {
            name: "3D".into(),
            kind: ViewKind::Model3d,
            yaw: -2.35,
            pitch: 0.52,
            orthographic: false,
            clip: [None, None],
            workplane: Workplane::default(),
        }
    }

    /// Unit vector from the target towards the eye (the viewer side).
    pub fn towards_viewer(&self) -> DVec3 {
        DVec3::new(
            self.pitch.cos() * self.yaw.cos(),
            self.pitch.cos() * self.yaw.sin(),
            self.pitch.sin(),
        )
    }

    /// Whether a world point lies inside the view depth.
    pub fn keeps(&self, p: DVec3) -> bool {
        self.clip.iter().flatten().all(|c| c.keeps(p))
    }
}

/// Slab between `offset - below` and `offset + above` along `normal`.
fn slab(normal: DVec3, offset: f64, below: f64, above: f64) -> [Option<ClipPlane>; 2] {
    [
        Some(ClipPlane {
            normal,
            offset: offset - below,
        }),
        Some(ClipPlane {
            normal: -normal,
            offset: -(offset + above),
        }),
    ]
}

/// View depth defaults (mm): plans keep 1000 below and 1000 above the level,
/// elevations 500 in front of and behind the grid line.
pub const PLAN_DEPTH: [f64; 2] = [1000., 1000.];
pub const ELEVATION_DEPTH: f64 = 500.;

/// 3D view, a plan per level and an elevation per grid line.
pub fn grid_views(project: &Project) -> Vec<ModelView> {
    let mut views = vec![ModelView::model_3d()];
    let mut levels: Vec<_> = project.levels.values().collect();
    levels.sort_by(|a, b| a.elevation_mm.total_cmp(&b.elevation_mm));
    for level in levels {
        let z = level.elevation_mm;
        views.push(ModelView {
            name: format!("План {}", level.name),
            kind: ViewKind::Plan { elevation: z },
            yaw: 0.,
            pitch: std::f64::consts::FRAC_PI_2,
            orthographic: true,
            clip: slab(DVec3::Z, z, PLAN_DEPTH[0], PLAN_DEPTH[1]),
            workplane: Workplane::at_elevation(z),
        });
    }
    for axis in project.axes.values() {
        let (a, b) = (
            DVec3::from_array(axis.start.0),
            DVec3::from_array(axis.end.0),
        );
        let Some(d) = (b - a).with_z(0.).try_normalize() else {
            continue;
        };
        // Look at the line from the side of its normal (d × Z), with Z up.
        let n = d.cross(DVec3::Z);
        let workplane = Workplane {
            origin: a,
            axes: [d, DVec3::Z, n],
        };
        views.push(ModelView {
            name: format!("Фасад по оси {}", axis.name),
            kind: ViewKind::Elevation { from: a, to: b },
            yaw: n.y.atan2(n.x),
            pitch: 0.,
            orthographic: true,
            clip: slab(n, n.dot(a), ELEVATION_DEPTH, ELEVATION_DEPTH),
            workplane,
        });
    }
    views
}
