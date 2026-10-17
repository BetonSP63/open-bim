//! Oriented polygon BSP intersection, including coplanar face ownership.
use parry3d_f64::{math::Vector, shape::TriMesh};

const EPS: f64 = 1e-7;

#[derive(Clone)]
struct Polygon {
    vertices: Vec<Vector>,
    normal: Vector,
    offset: f64,
}
impl Polygon {
    fn new(vertices: Vec<Vector>) -> Option<Self> {
        let first = *vertices.first()?;
        let normal = (1..vertices.len().saturating_sub(1)).find_map(|i| {
            let cross = (vertices[i] - first).cross(vertices[i + 1] - first);
            (cross.length_squared() > 1e-24).then(|| cross.normalize())
        })?;
        Some(Self {
            offset: normal.dot(first),
            vertices,
            normal,
        })
    }
    fn invert(&mut self) {
        self.vertices.reverse();
        self.normal = -self.normal;
        self.offset = -self.offset;
    }
}

#[derive(Default)]
struct Node {
    plane: Option<(Vector, f64)>,
    polygons: Vec<Polygon>,
    front: Option<usize>,
    back: Option<usize>,
}

fn split(
    plane: (Vector, f64),
    polygon: Polygon,
    same: &mut Vec<Polygon>,
    opposite: &mut Vec<Polygon>,
    front: &mut Vec<Polygon>,
    back: &mut Vec<Polygon>,
) {
    let signed: Vec<f64> = polygon
        .vertices
        .iter()
        .map(|p| plane.0.dot(*p) - plane.1)
        .collect();
    let has_front = signed.iter().any(|v| *v > EPS);
    let has_back = signed.iter().any(|v| *v < -EPS);
    match (has_front, has_back) {
        (false, false) => {
            if plane.0.dot(polygon.normal) > 0. {
                same.push(polygon)
            } else {
                opposite.push(polygon)
            }
        }
        (true, false) => front.push(polygon),
        (false, true) => back.push(polygon),
        (true, true) => {
            let mut f = Vec::new();
            let mut b = Vec::new();
            for i in 0..polygon.vertices.len() {
                let j = (i + 1) % polygon.vertices.len();
                let vi = polygon.vertices[i];
                let vj = polygon.vertices[j];
                if signed[i] >= -EPS {
                    f.push(vi);
                }
                if signed[i] <= EPS {
                    b.push(vi);
                }
                if (signed[i] > EPS && signed[j] < -EPS) || (signed[i] < -EPS && signed[j] > EPS) {
                    let point = vi + (vj - vi) * (signed[i] / (signed[i] - signed[j]));
                    f.push(point);
                    b.push(point);
                }
            }
            if let Some(p) = Polygon::new(f) {
                front.push(p);
            }
            if let Some(p) = Polygon::new(b) {
                back.push(p);
            }
        }
    }
}

struct Tree {
    nodes: Vec<Node>,
}
impl Default for Tree {
    fn default() -> Self {
        Self {
            nodes: vec![Node::default()],
        }
    }
}
impl Tree {
    fn build(&mut self, polygons: Vec<Polygon>) -> Result<(), String> {
        let mut pending = vec![(0, polygons)];
        while let Some((index, polygons)) = pending.pop() {
            if polygons.is_empty() {
                continue;
            }
            if self.nodes.len() > 1_000_000 {
                return Err("Слишком сложное геометрическое пересечение".into());
            }
            let plane = *self.nodes[index]
                .plane
                .get_or_insert((polygons[0].normal, polygons[0].offset));
            let mut front = Vec::new();
            let mut back = Vec::new();
            let mut same = Vec::new();
            let mut opposite = Vec::new();
            for p in polygons {
                split(plane, p, &mut same, &mut opposite, &mut front, &mut back);
            }
            self.nodes[index].polygons.extend(same);
            self.nodes[index].polygons.extend(opposite);
            for (is_front, polygons) in [(true, front), (false, back)] {
                if polygons.is_empty() {
                    continue;
                }
                let child = if is_front {
                    self.nodes[index].front
                } else {
                    self.nodes[index].back
                };
                let child = match child {
                    Some(child) => child,
                    None => {
                        let child = self.nodes.len();
                        self.nodes.push(Node::default());
                        if is_front {
                            self.nodes[index].front = Some(child);
                        } else {
                            self.nodes[index].back = Some(child);
                        }
                        child
                    }
                };
                pending.push((child, polygons));
            }
        }
        Ok(())
    }
    fn invert(&mut self) {
        for node in &mut self.nodes {
            for p in &mut node.polygons {
                p.invert();
            }
            if let Some((n, d)) = &mut node.plane {
                *n = -*n;
                *d = -*d;
            }
            std::mem::swap(&mut node.front, &mut node.back);
        }
    }
    fn clip(&self, polygons: Vec<Polygon>) -> Vec<Polygon> {
        let mut pending = vec![(0, polygons)];
        let mut result = Vec::new();
        while let Some((index, polygons)) = pending.pop() {
            let node = &self.nodes[index];
            let Some(plane) = node.plane else {
                result.extend(polygons);
                continue;
            };
            let mut front = Vec::new();
            let mut back = Vec::new();
            let mut same = Vec::new();
            let mut opposite = Vec::new();
            for p in polygons {
                split(plane, p, &mut same, &mut opposite, &mut front, &mut back);
            }
            front.extend(same);
            back.extend(opposite);
            if let Some(child) = node.front {
                pending.push((child, front));
            } else {
                result.extend(front);
            }
            if let Some(child) = node.back {
                pending.push((child, back));
            }
        }
        result
    }
    fn clip_to(&mut self, other: &Self) {
        for node in &mut self.nodes {
            node.polygons = other.clip(std::mem::take(&mut node.polygons));
        }
    }
    fn polygons(&self) -> Vec<Polygon> {
        self.nodes
            .iter()
            .flat_map(|node| node.polygons.iter().cloned())
            .collect()
    }
}

pub fn intersection_volume(a: &TriMesh, b: &TriMesh) -> Result<f64, String> {
    let polygons = |mesh: &TriMesh| {
        mesh.triangles()
            .filter_map(|t| Polygon::new(vec![t.a, t.b, t.c]))
            .collect()
    };
    let mut a_tree = Tree::default();
    a_tree.build(polygons(a))?;
    let mut b_tree = Tree::default();
    b_tree.build(polygons(b))?;
    a_tree.invert();
    b_tree.clip_to(&a_tree);
    b_tree.invert();
    a_tree.clip_to(&b_tree);
    b_tree.clip_to(&a_tree);
    a_tree.build(b_tree.polygons())?;
    a_tree.invert();
    let polygons = a_tree.polygons();
    let Some(first) = polygons.first() else {
        return Ok(0.);
    };
    let origin = first.vertices[0];
    let volume = polygons
        .iter()
        .map(|p| {
            (1..p.vertices.len() - 1)
                .map(|i| {
                    (p.vertices[0] - origin)
                        .dot((p.vertices[i] - origin).cross(p.vertices[i + 1] - origin))
                        / 6.
                })
                .sum::<f64>()
        })
        .sum::<f64>();
    if !volume.is_finite() || volume < -1e-6 {
        return Err("Неверная ориентация объёма пересечения".into());
    }
    Ok(volume.max(0.))
}
