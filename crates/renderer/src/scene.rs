use crate::{DVec3, Ray, RenderError};
use open_bim_geometry::Body;
use open_bim_model::{ElementShape, Frame, ObjectId, Point3, ProfileShape, Project};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub min: DVec3,
    pub max: DVec3,
}
impl Bounds {
    fn empty() -> Self {
        Self {
            min: DVec3::splat(f64::INFINITY),
            max: DVec3::splat(f64::NEG_INFINITY),
        }
    }
    fn add(&mut self, p: DVec3) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }
    fn union(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }
    fn hit(&self, ray: Ray) -> Option<f64> {
        let mut near = 0.0_f64;
        let mut far = f64::INFINITY;
        for i in 0..3 {
            if ray.direction[i].abs() < 1e-12 {
                if ray.origin[i] < self.min[i] || ray.origin[i] > self.max[i] {
                    return None;
                }
                continue;
            }
            let a = (self.min[i] - ray.origin[i]) / ray.direction[i];
            let b = (self.max[i] - ray.origin[i]) / ray.direction[i];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return None;
            }
        }
        Some(near)
    }
}

#[derive(Clone, Debug)]
pub struct DisplayMesh {
    pub positions: Vec<DVec3>,
    pub triangles: Vec<[u32; 3]>,
}
#[derive(Clone, Debug)]
pub struct Instance {
    pub id: ObjectId,
    pub mesh: usize,
    pub frame: Frame,
    pub color: [f32; 4],
    pub bounds: Bounds,
}
#[derive(Clone, Debug)]
enum Bvh {
    Leaf {
        bounds: Bounds,
        indices: Vec<usize>,
    },
    Branch {
        bounds: Bounds,
        left: Box<Bvh>,
        right: Box<Bvh>,
    },
}
impl Bvh {
    fn bounds(&self) -> Bounds {
        match self {
            Self::Leaf { bounds, .. } | Self::Branch { bounds, .. } => *bounds,
        }
    }
    fn build(instances: &[Instance], mut indices: Vec<usize>) -> Option<Self> {
        let first = *indices.first()?;
        let bounds = indices
            .iter()
            .skip(1)
            .fold(instances[first].bounds, |bounds, index| {
                bounds.union(instances[*index].bounds)
            });
        if indices.len() <= 8 {
            return Some(Self::Leaf { bounds, indices });
        }
        let extent = bounds.max - bounds.min;
        let axis = if extent.x >= extent.y && extent.x >= extent.z {
            0
        } else if extent.y >= extent.z {
            1
        } else {
            2
        };
        indices.sort_unstable_by(|a, b| {
            (instances[*a].bounds.min[axis] + instances[*a].bounds.max[axis])
                .total_cmp(&(instances[*b].bounds.min[axis] + instances[*b].bounds.max[axis]))
        });
        let right_indices = indices.split_off(indices.len() / 2);
        Some(Self::Branch {
            bounds,
            left: Box::new(Self::build(instances, indices)?),
            right: Box::new(Self::build(instances, right_indices)?),
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ClipPlane {
    pub normal: DVec3,
    pub offset: f64,
}
impl ClipPlane {
    pub fn keeps(&self, point: DVec3) -> bool {
        self.normal.dot(point) >= self.offset
    }
}

#[derive(Clone, Debug, Default)]
pub struct ViewState {
    pub hidden: BTreeSet<ObjectId>,
    pub selected: Option<ObjectId>,
    pub selected_ids: BTreeSet<ObjectId>,
    pub section: Option<ClipPlane>,
}

#[derive(Clone, Debug)]
pub struct Scene {
    pub meshes: Vec<Arc<DisplayMesh>>,
    /// Coarser display meshes for curved sections: full mesh index →
    /// (feature diameter mm, [32-segment, 12-segment] mesh indices).
    pub lods: BTreeMap<usize, (f64, [usize; 2])>,
    pub instances: Vec<Instance>,
    pub bounds: Bounds,
    pub origin: DVec3,
    bvh: Option<Bvh>,
}
impl Scene {
    pub fn from_project(project: &Project) -> Result<Self, RenderError> {
        project.validate().map_err(|e| RenderError(e.to_string()))?;
        let mut meshes = Vec::new();
        let mut instances = Vec::new();
        let mut cache = BTreeMap::new();
        let mut lods = BTreeMap::new();
        let mut total = Bounds::empty();
        let featured: std::collections::BTreeSet<ObjectId> = project
            .details
            .values()
            .filter_map(|d| d.kind.part())
            .collect();
        for element in project.elements.values() {
            let (mut mesh_index, color) = match &element.shape {
                ElementShape::Member {
                    profile,
                    length,
                    member_kind,
                    ..
                } => {
                    let key = (*profile, length.value().to_bits());
                    let index = if let Some(index) = cache.get(&key) {
                        *index
                    } else {
                        let shape = &project.profiles[profile].shape;
                        let mesh = section_mesh(shape, length.value())?;
                        let index = meshes.len();
                        meshes.push(Arc::new(mesh));
                        cache.insert(key, index);
                        if let ProfileShape::CircularHollow {
                            diameter,
                            thickness,
                        } = *shape
                        {
                            let coarse = [32, 12].map(|n| {
                                meshes.push(Arc::new(tube_mesh(
                                    diameter,
                                    thickness,
                                    length.value(),
                                    n,
                                )));
                                meshes.len() - 1
                            });
                            lods.insert(index, (diameter, coarse));
                        }
                        index
                    };
                    let color = match member_kind {
                        open_bim_model::MemberKind::Beam => [0.28, 0.53, 0.80, 1.0],
                        open_bim_model::MemberKind::Column => [0.58, 0.68, 0.75, 1.0],
                    };
                    (index, color)
                }
                ElementShape::Plate {
                    contour, thickness, ..
                } => {
                    let index = meshes.len();
                    meshes.push(Arc::new(prism_mesh(contour, thickness.value())?));
                    (index, [0.86, 0.61, 0.26, 1.0])
                }
            };
            if featured.contains(&element.id) {
                let body = project
                    .element_body(element.id)
                    .map_err(|e| RenderError(e.to_string()))?;
                let (positions, triangles) = body
                    .tessellation_f64()
                    .map_err(|e| RenderError(e.to_string()))?;
                mesh_index = meshes.len();
                meshes.push(Arc::new(DisplayMesh {
                    positions: positions.into_iter().map(DVec3::from_array).collect(),
                    triangles,
                }));
            }
            let frame = *element.shape.frame();
            let mesh: &DisplayMesh = &meshes[mesh_index];
            let mut bounds = Bounds::empty();
            for position in &mesh.positions {
                bounds.add(DVec3::from_array(
                    frame.world(Point3(position.to_array())).0,
                ));
            }
            total = total.union(bounds);
            instances.push(Instance {
                id: element.id,
                mesh: mesh_index,
                frame,
                color,
                bounds,
            });
        }
        for axis in project.axes.values() {
            total.add(DVec3::from_array(axis.start.0));
            total.add(DVec3::from_array(axis.end.0));
        }
        if instances.is_empty() && project.axes.is_empty() {
            total = Bounds {
                min: DVec3::splat(-100.0),
                max: DVec3::splat(100.0),
            };
        }
        let origin = (total.min + total.max) * 0.5;
        let bvh = Bvh::build(&instances, (0..instances.len()).collect());
        Ok(Self {
            meshes,
            lods,
            instances,
            bounds: total,
            origin,
            bvh,
        })
    }
    /// Instances whose bounds come within `radius(distance)` of the ray,
    /// nearest first (screen-space snapping candidates). `radius` maps the
    /// distance along the ray to a world tolerance (pixels × mm per pixel).
    pub fn near_ray(&self, ray: Ray, radius: impl Fn(f64) -> f64, limit: usize) -> Vec<usize> {
        let mut out: Vec<(f64, usize)> = Vec::new();
        let mut stack = Vec::new();
        if let Some(root) = &self.bvh {
            stack.push(root);
        }
        let expanded = |b: Bounds| {
            let centre = (b.min + b.max) * 0.5;
            let far = (centre - ray.origin).length() + (b.max - b.min).length() * 0.5;
            let r = radius(far.max(0.0));
            Bounds {
                min: b.min - DVec3::splat(r),
                max: b.max + DVec3::splat(r),
            }
        };
        while let Some(node) = stack.pop() {
            if expanded(node.bounds()).hit(ray).is_none() {
                continue;
            }
            match node {
                Bvh::Branch { left, right, .. } => {
                    stack.push(left);
                    stack.push(right);
                }
                Bvh::Leaf { indices, .. } => {
                    for index in indices {
                        if let Some(t) = expanded(self.instances[*index].bounds).hit(ray) {
                            out.push((t, *index));
                        }
                    }
                }
            }
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out.truncate(limit);
        out.into_iter().map(|(_, i)| i).collect()
    }
    pub fn pick(&self, ray: Ray, state: &ViewState) -> Option<ObjectId> {
        let mut nearest = f64::INFINITY;
        let mut selected = None;
        let mut stack = Vec::new();
        if let Some(root) = &self.bvh {
            stack.push(root);
        }
        while let Some(node) = stack.pop() {
            if node.bounds().hit(ray).is_none_or(|near| near > nearest) {
                continue;
            }
            match node {
                Bvh::Branch { left, right, .. } => {
                    stack.push(left);
                    stack.push(right);
                }
                Bvh::Leaf { indices, .. } => {
                    for index in indices {
                        let instance = &self.instances[*index];
                        if state.hidden.contains(&instance.id)
                            || instance.bounds.hit(ray).is_none_or(|near| near > nearest)
                        {
                            continue;
                        }
                        let frame = instance.frame;
                        let relative = ray.origin - DVec3::from_array(frame.origin.0);
                        let local_ray = Ray {
                            origin: DVec3::from_array(std::array::from_fn(|i| {
                                DVec3::from_array(frame.axes[i]).dot(relative)
                            })),
                            direction: DVec3::from_array(std::array::from_fn(|i| {
                                DVec3::from_array(frame.axes[i]).dot(ray.direction)
                            })),
                        };
                        let mesh = &self.meshes[instance.mesh];
                        for triangle in &mesh.triangles {
                            let [a, b, c] = triangle.map(|index| mesh.positions[index as usize]);
                            if let Some(distance) = triangle_hit(local_ray, a, b, c) {
                                if distance >= nearest {
                                    continue;
                                }
                                let world = ray.origin + distance * ray.direction;
                                if state.section.is_some_and(|plane| !plane.keeps(world)) {
                                    continue;
                                }
                                nearest = distance;
                                selected = Some(instance.id);
                            }
                        }
                    }
                }
            }
        }
        selected
    }
    pub fn visible_triangles(&self, state: &ViewState) -> usize {
        self.instances
            .iter()
            .filter(|i| !state.hidden.contains(&i.id))
            .map(|i| self.meshes[i.mesh].triangles.len())
            .sum()
    }
}

fn triangle_hit(ray: Ray, a: DVec3, b: DVec3, c: DVec3) -> Option<f64> {
    let e1 = b - a;
    let e2 = c - a;
    let p = ray.direction.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inverse = 1.0 / det;
    let t = ray.origin - a;
    let u = t.dot(p) * inverse;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = t.cross(e1);
    let v = ray.direction.dot(q) * inverse;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let distance = e2.dot(q) * inverse;
    (distance >= 0.0).then_some(distance)
}

fn prism_mesh(contour: &[[f64; 2]], length: f64) -> Result<DisplayMesh, RenderError> {
    let body =
        Body::prism(contour, length, Point3([0.0; 3])).map_err(|e| RenderError(e.to_string()))?;
    let (positions, triangles) = body
        .tessellation_f64()
        .map_err(|e| RenderError(e.to_string()))?;
    Ok(DisplayMesh {
        positions: positions.into_iter().map(DVec3::from_array).collect(),
        triangles,
    })
}

fn section_mesh(shape: &ProfileShape, length: f64) -> Result<DisplayMesh, RenderError> {
    match *shape {
        ProfileShape::ISection {
            height: h,
            width: w,
            web: tw,
            flange: tf,
        } => prism_mesh(
            &[
                [-w / 2.0, -h / 2.0],
                [w / 2.0, -h / 2.0],
                [w / 2.0, -h / 2.0 + tf],
                [tw / 2.0, -h / 2.0 + tf],
                [tw / 2.0, h / 2.0 - tf],
                [w / 2.0, h / 2.0 - tf],
                [w / 2.0, h / 2.0],
                [-w / 2.0, h / 2.0],
                [-w / 2.0, h / 2.0 - tf],
                [-tw / 2.0, h / 2.0 - tf],
                [-tw / 2.0, -h / 2.0 + tf],
                [-w / 2.0, -h / 2.0 + tf],
            ],
            length,
        ),
        ProfileShape::Angle {
            height: h,
            width: w,
            thickness: t,
        } => prism_mesh(
            &[
                [-w / 2.0, -h / 2.0],
                [w / 2.0, -h / 2.0],
                [w / 2.0, -h / 2.0 + t],
                [-w / 2.0 + t, -h / 2.0 + t],
                [-w / 2.0 + t, h / 2.0],
                [-w / 2.0, h / 2.0],
            ],
            length,
        ),
        ProfileShape::Channel {
            height: h,
            width: w,
            web: tw,
            flange: tf,
        } => prism_mesh(
            &[
                [-w / 2.0, -h / 2.0],
                [w / 2.0, -h / 2.0],
                [w / 2.0, -h / 2.0 + tf],
                [-w / 2.0 + tw, -h / 2.0 + tf],
                [-w / 2.0 + tw, h / 2.0 - tf],
                [w / 2.0, h / 2.0 - tf],
                [w / 2.0, h / 2.0],
                [-w / 2.0, h / 2.0],
            ],
            length,
        ),
        ProfileShape::Rectangle {
            height: h,
            width: w,
        } => prism_mesh(
            &[
                [-w / 2.0, -h / 2.0],
                [w / 2.0, -h / 2.0],
                [w / 2.0, h / 2.0],
                [-w / 2.0, h / 2.0],
            ],
            length,
        ),
        ProfileShape::CircularHollow {
            diameter,
            thickness,
        } => Ok(tube_mesh(diameter, thickness, length, 128)),
        ProfileShape::RectangularHollow {
            height: h,
            width: w,
            thickness: t,
        } => Ok(ring_mesh(
            &[
                [-w / 2.0, -h / 2.0],
                [w / 2.0, -h / 2.0],
                [w / 2.0, h / 2.0],
                [-w / 2.0, h / 2.0],
            ],
            &[
                [-w / 2.0 + t, -h / 2.0 + t],
                [w / 2.0 - t, -h / 2.0 + t],
                [w / 2.0 - t, h / 2.0 - t],
                [-w / 2.0 + t, h / 2.0 - t],
            ],
            length,
        )),
    }
}

fn tube_mesh(diameter: f64, thickness: f64, length: f64, segments: usize) -> DisplayMesh {
    let circle = |r: f64| -> Vec<[f64; 2]> {
        (0..segments)
            .map(|i| {
                let angle = i as f64 * std::f64::consts::TAU / segments as f64;
                [angle.cos() * r, angle.sin() * r]
            })
            .collect()
    };
    ring_mesh(
        &circle(diameter / 2.0),
        &circle(diameter / 2.0 - thickness),
        length,
    )
}

impl Scene {
    /// Display mesh for an instance: curved sections switch to 32 or 12
    /// segments when their diameter covers fewer than 48 or 10 pixels.
    pub fn lod_mesh(&self, instance: &Instance, pixels_per_mm_at: impl Fn(DVec3) -> f64) -> usize {
        let Some((diameter, coarse)) = self.lods.get(&instance.mesh) else {
            return instance.mesh;
        };
        let centre = (instance.bounds.min + instance.bounds.max) * 0.5;
        let pixels = diameter * pixels_per_mm_at(centre);
        if pixels < 10.0 {
            coarse[1]
        } else if pixels < 48.0 {
            coarse[0]
        } else {
            instance.mesh
        }
    }
}

fn ring_mesh(outer: &[[f64; 2]], inner: &[[f64; 2]], length: f64) -> DisplayMesh {
    let n = outer.len();
    let mut positions = Vec::with_capacity(n * 4);
    let mut triangles = Vec::with_capacity(n * 8);
    for z in [0.0, length] {
        for contour in [outer, inner] {
            for p in contour {
                positions.push(DVec3::new(p[0], p[1], z));
            }
        }
    }
    let mut quad = |v: [usize; 4]| {
        triangles.push([v[0] as u32, v[1] as u32, v[2] as u32]);
        triangles.push([v[0] as u32, v[2] as u32, v[3] as u32]);
    };
    for i in 0..n {
        let j = (i + 1) % n;
        quad([i, j, j + 2 * n, i + 2 * n]);
        quad([i + n, i + 3 * n, j + 3 * n, j + n]);
        quad([i, i + n, j + n, j]);
        quad([i + 2 * n, j + 2 * n, j + 3 * n, i + 3 * n]);
    }
    DisplayMesh {
        positions,
        triangles,
    }
}
