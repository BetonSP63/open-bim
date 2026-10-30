//! Pour zones («захватки бетонирования»): connected cast-in-place concrete
//! of one class between construction joints. A joint that cuts a part
//! splits it, so a slab poured in two zones counts in both with the volume
//! on each side of the joint. Zones are recalculated from the model on every
//! request; the stored pour units only keep the zone numbers stable.
use crate::{ObjectId, PourBreak, PourUnit, Project, PropertyValue};
use std::collections::{BTreeMap, BTreeSet};

/// Element property holding the concrete part type («column», «slab»...).
pub const CONCRETE_KEY: &str = "concrete";
/// Assembly property of a cast unit: «монолитный» or «сборный».
pub const CAST_UNIT_KEY: &str = "cast_unit";
pub const PRECAST: &str = "сборный";

/// Parts closer than this touch, mm.
const TOUCH: f64 = 1.0;
/// Pieces smaller than this are clipping noise, mm³.
const SLIVER_MM3: f64 = 1e3;

/// Part of one concrete part that lies in one zone.
#[derive(Clone, Debug, PartialEq)]
pub struct PourPiece {
    pub part: ObjectId,
    pub volume_m3: f64,
    /// World box [min, max] of the piece, mm.
    pub bounds: [[f64; 3]; 2],
    /// Whether a joint cut the part.
    pub split: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PourZone {
    pub number: u32,
    /// Concrete class, «В25».
    pub grade: String,
    pub pieces: Vec<PourPiece>,
    pub volume_m3: f64,
    pub bounds: [[f64; 3]; 2],
}

impl PourZone {
    pub fn parts(&self) -> BTreeSet<ObjectId> {
        self.pieces.iter().map(|p| p.part).collect()
    }

    pub fn centre(&self) -> [f64; 3] {
        std::array::from_fn(|k| (self.bounds[0][k] + self.bounds[1][k]) / 2.)
    }
}

/// Russian name of a concrete part type.
pub fn concrete_kind_name(code: &str) -> &str {
    match code {
        "column" => "Колонна",
        "beam" => "Балка",
        "panel" => "Стена",
        "slab" => "Плита",
        "footing" => "Фундамент",
        "strip_footing" => "Ленточный фундамент",
        "raft" => "Фундаментная плита",
        other => other,
    }
}

fn centre(b: &[[f64; 3]; 2]) -> [f64; 3] {
    std::array::from_fn(|k| (b[0][k] + b[1][k]) / 2.)
}

fn union(a: &[[f64; 3]; 2], b: &[[f64; 3]; 2]) -> [[f64; 3]; 2] {
    [
        std::array::from_fn(|k| a[0][k].min(b[0][k])),
        std::array::from_fn(|k| a[1][k].max(b[1][k])),
    ]
}

fn find(parent: &mut [usize], i: usize) -> usize {
    let mut r = i;
    while parent[r] != r {
        r = parent[r];
    }
    parent[i] = r;
    r
}

impl Project {
    /// Concrete part type of an element, if it is a concrete part.
    pub fn concrete_type(&self, part: ObjectId) -> Option<&str> {
        match self.elements.get(&part)?.properties.get(CONCRETE_KEY) {
            Some(PropertyValue::Text(t)) => Some(t),
            _ => None,
        }
    }

    /// Concrete parts cast in place (not in a precast cast unit).
    pub fn cast_in_place(&self) -> Vec<ObjectId> {
        let precast: BTreeSet<ObjectId> = self
            .assemblies
            .values()
            .filter(|a| {
                a.properties.get(CAST_UNIT_KEY) == Some(&PropertyValue::Text(PRECAST.into()))
            })
            .flat_map(|a| a.members.iter().copied())
            .collect();
        self.elements
            .keys()
            .filter(|id| self.concrete_type(**id).is_some() && !precast.contains(id))
            .copied()
            .collect()
    }

    /// Concrete class of a part from its material «Бетон В25».
    pub fn concrete_grade(&self, part: ObjectId) -> String {
        self.elements
            .get(&part)
            .and_then(|e| self.materials.get(&e.material))
            .map_or(String::new(), |m| {
                m.name.trim_start_matches("Бетон ").to_string()
            })
    }

    /// The pieces of a part between the joints that cut it.
    fn pour_pieces(&self, part: ObjectId, bounds: [[f64; 3]; 2]) -> Vec<PourPiece> {
        let cutting: Vec<&PourBreak> = self
            .concrete
            .breaks
            .iter()
            .filter(|b| b.cuts(&bounds))
            .collect();
        let whole = || {
            vec![PourPiece {
                part,
                volume_m3: self.element_volume_mm3(part).unwrap_or(0.) / 1e9,
                bounds,
                split: false,
            }]
        };
        if cutting.is_empty() {
            return whole();
        }
        let (Ok(body), Some(element)) = (self.element_body(part), self.elements.get(&part)) else {
            return whole();
        };
        let frame = *element.shape.frame();
        // World plane n·p = d in the part frame: n·(o + A·x) = d.
        let local = |b: &PourBreak| {
            let (n, d) = b.plane();
            let dot = |a: [f64; 3]| (0..3).map(|i| a[i] * n[i]).sum::<f64>();
            let nl: [f64; 3] = std::array::from_fn(|j| dot(frame.axes[j]));
            (nl, d - dot(frame.origin.0))
        };
        let side = |points: &[[f64; 3]], n: [f64; 3], d: f64| {
            points.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
                let s = n[0] * p[0] + n[1] * p[1] + n[2] * p[2] - d;
                (lo.min(s), hi.max(s))
            })
        };
        let mut cells = vec![body];
        for b in cutting {
            let (n, d) = local(b);
            let mut next = Vec::new();
            for cell in cells {
                let Ok((points, _)) = cell.tessellation_f64() else {
                    return whole();
                };
                let (lo, hi) = side(&points, n, d);
                if lo > -TOUCH || hi < TOUCH {
                    next.push(cell);
                    continue;
                }
                let below = cell.clip_plane(n, d);
                let above = cell.clip_plane([-n[0], -n[1], -n[2]], -d);
                match (below, above) {
                    (Ok(a), Ok(b)) => next.extend([a, b]),
                    _ => return whole(),
                }
            }
            cells = next;
        }
        let mut out = Vec::new();
        for cell in cells {
            let (Ok(volume), Ok((points, _))) = (cell.volume_mm3(), cell.tessellation_f64()) else {
                return whole();
            };
            if volume < SLIVER_MM3 || points.is_empty() {
                continue;
            }
            let bounds = points.iter().fold(
                [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]],
                |[lo, hi], p| {
                    let w = frame.world(crate::Point3(*p)).0;
                    [
                        std::array::from_fn(|k| lo[k].min(w[k])),
                        std::array::from_fn(|k| hi[k].max(w[k])),
                    ]
                },
            );
            out.push(PourPiece {
                part,
                volume_m3: volume / 1e9,
                bounds,
                split: true,
            });
        }
        if out.len() < 2 {
            return whole();
        }
        out
    }

    /// Pour zones ordered bottom-up then by X and Y and numbered: a zone
    /// keeps the number of the stored pour unit it shares most parts with,
    /// new zones continue after the largest stored number.
    pub fn pour_zones(&self) -> Vec<PourZone> {
        let mut pieces: Vec<(String, PourPiece)> = Vec::new();
        for id in self.cast_in_place() {
            let Ok(bounds) = self.element_bounds(id) else {
                continue;
            };
            let grade = self.concrete_grade(id);
            pieces.extend(
                self.pour_pieces(id, bounds)
                    .into_iter()
                    .map(|p| (grade.clone(), p)),
            );
        }
        let mut parent: Vec<usize> = (0..pieces.len()).collect();
        for i in 0..pieces.len() {
            for j in i + 1..pieces.len() {
                let ((ga, a), (gb, b)) = (&pieces[i], &pieces[j]);
                if ga != gb || a.part == b.part {
                    continue;
                }
                let touch = (0..3).all(|k| {
                    a.bounds[0][k] <= b.bounds[1][k] + TOUCH
                        && b.bounds[0][k] <= a.bounds[1][k] + TOUCH
                });
                if !touch {
                    continue;
                }
                let (ca, cb) = (centre(&a.bounds), centre(&b.bounds));
                if self.concrete.breaks.iter().any(|br| br.separates(ca, cb)) {
                    continue;
                }
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                parent[ri] = rj;
            }
        }
        let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for i in 0..pieces.len() {
            let r = find(&mut parent, i);
            groups.entry(r).or_default().push(i);
        }
        let mut zones: Vec<PourZone> = groups
            .into_values()
            .map(|members| {
                let grade = pieces[members[0]].0.clone();
                let list: Vec<PourPiece> = members.iter().map(|&i| pieces[i].1.clone()).collect();
                let bounds = list
                    .iter()
                    .skip(1)
                    .fold(list[0].bounds, |b, p| union(&b, &p.bounds));
                PourZone {
                    number: 0,
                    grade,
                    volume_m3: list.iter().map(|p| p.volume_m3).sum(),
                    pieces: list,
                    bounds,
                }
            })
            .collect();
        zones.sort_by(|a, b| {
            let key = |z: &PourZone| [z.bounds[0][2], z.bounds[0][0], z.bounds[0][1]];
            key(a)
                .partial_cmp(&key(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        number_zones(&mut zones, &self.concrete.units);
        zones
    }

    /// Whether the stored pour units differ from the zones of the model.
    pub fn pour_units_outdated(&self) -> bool {
        let zones = self.pour_zones();
        zones.len() != self.concrete.units.len()
            || zones.iter().any(|z| {
                !self
                    .concrete
                    .units
                    .iter()
                    .any(|u| u.number == z.number && u.grade == z.grade && u.parts == z.parts())
            })
    }

    /// Pour units to store for the current zones.
    pub fn pour_units(&self) -> Vec<PourUnit> {
        self.pour_zones()
            .into_iter()
            .map(|z| PourUnit {
                number: z.number,
                grade: z.grade.clone(),
                parts: z.parts(),
                centre: Some(crate::Point3(z.centre())),
            })
            .collect()
    }
}

/// A line of the concrete volume schedule: like pieces of one zone.
#[derive(Clone, Debug, PartialEq)]
pub struct PourRow {
    pub zone: u32,
    pub grade: String,
    /// «Колонна 400x400», «Плита δ=200».
    pub element: String,
    pub count: usize,
    pub volume_m3: f64,
}

impl Project {
    /// Name and section of a concrete part for schedules.
    pub fn concrete_element_name(&self, part: ObjectId) -> String {
        let Some(e) = self.elements.get(&part) else {
            return String::new();
        };
        let kind = self
            .concrete_type(part)
            .map_or(e.name.as_str(), concrete_kind_name);
        let size = match &e.shape {
            crate::ElementShape::Member { profile, .. } => self
                .profiles
                .get(profile)
                .map_or(String::new(), |p| p.name.clone()),
            crate::ElementShape::Plate { thickness, .. } => {
                let t = thickness.value();
                if t.fract().abs() < 1e-9 {
                    format!("δ={t:.0}")
                } else {
                    format!("δ={t:.1}")
                }
            }
        };
        if size.is_empty() {
            kind.to_string()
        } else {
            format!("{kind} {size}")
        }
    }

    /// Schedule lines of the given zones in zone order; like pieces of a
    /// zone (same name) are counted in one line.
    pub fn pour_rows(&self, zones: &[PourZone]) -> Vec<PourRow> {
        let mut out = Vec::new();
        for z in zones {
            let mut lines: BTreeMap<String, (usize, f64)> = BTreeMap::new();
            for p in &z.pieces {
                let line = lines.entry(self.concrete_element_name(p.part)).or_default();
                line.0 += 1;
                line.1 += p.volume_m3;
            }
            out.extend(
                lines
                    .into_iter()
                    .map(|(element, (count, volume_m3))| PourRow {
                        zone: z.number,
                        grade: z.grade.clone(),
                        element,
                        count,
                        volume_m3,
                    }),
            );
        }
        out
    }
}

/// Total volume by concrete class, m³.
pub fn volumes_by_grade(zones: &[PourZone]) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    for z in zones {
        *out.entry(z.grade.clone()).or_default() += z.volume_m3;
    }
    out
}

/// Greedy matching: the pairs with most shared parts first, nearer stored
/// centres among them; then the new zones in order after the largest
/// stored number.
fn number_zones(zones: &mut [PourZone], units: &[PourUnit]) {
    let mut pairs: Vec<(usize, f64, usize, usize)> = Vec::new();
    for (zi, z) in zones.iter().enumerate() {
        let parts = z.parts();
        let c = z.centre();
        for (ui, u) in units.iter().enumerate() {
            let shared = u.parts.intersection(&parts).count();
            if shared > 0 && u.grade == z.grade {
                let distance = u.centre.map_or(0., |p| {
                    (0..3).map(|k| (p.0[k] - c[k]).powi(2)).sum::<f64>().sqrt()
                });
                pairs.push((shared, distance, zi, ui));
            }
        }
    }
    pairs.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(a.1.total_cmp(&b.1))
            .then(a.2.cmp(&b.2))
            .then(a.3.cmp(&b.3))
    });
    let (mut zone_done, mut unit_done) = (BTreeSet::new(), BTreeSet::new());
    for (_, _, zi, ui) in pairs {
        if zone_done.contains(&zi) || unit_done.contains(&ui) {
            continue;
        }
        zones[zi].number = units[ui].number;
        zone_done.insert(zi);
        unit_done.insert(ui);
    }
    let mut next = units.iter().map(|u| u.number).max().unwrap_or(0);
    for (zi, z) in zones.iter_mut().enumerate() {
        if !zone_done.contains(&zi) {
            next += 1;
            z.number = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Element, ElementShape, Frame, LengthMm, Material, Point3, Profile, ProfileShape};

    fn concrete_project() -> Project {
        let mut p = Project::default();
        let material = ObjectId(1);
        p.materials.insert(
            material,
            Material {
                id: material,
                name: "Бетон В25".into(),
                density_kg_m3: 2500.,
            },
        );
        let profile = ObjectId(2);
        p.profiles.insert(
            profile,
            Profile {
                id: profile,
                name: "400x400".into(),
                shape: ProfileShape::Rectangle {
                    height: 400.,
                    width: 400.,
                },
                source: "Бетонное сечение".into(),
                standard: None,
                area_mm2: None,
            },
        );
        p.next_id = 10;
        p
    }

    fn add(p: &mut Project, kind: &str, shape: ElementShape) -> ObjectId {
        let id = ObjectId(p.next_id);
        p.next_id += 1;
        let mut properties = BTreeMap::new();
        properties.insert(CONCRETE_KEY.into(), PropertyValue::Text(kind.into()));
        p.elements.insert(
            id,
            Element {
                id,
                name: kind.into(),
                material: ObjectId(1),
                shape,
                properties,
            },
        );
        id
    }

    fn column(p: &mut Project, x: f64, y: f64, height: f64) -> ObjectId {
        let (frame, length) = Frame::along(Point3([x, y, 0.]), Point3([x, y, height])).unwrap();
        add(
            p,
            "column",
            ElementShape::Member {
                member_kind: crate::MemberKind::Column,
                profile: ObjectId(2),
                length,
                frame,
                position: Default::default(),
            },
        )
    }

    /// A slab 0..10000 × 0..4000, 200 thick, top at `top`.
    fn slab(p: &mut Project, top: f64) -> ObjectId {
        let frame = Frame {
            origin: Point3([0., 0., top - 200.]),
            ..Frame::default()
        };
        add(
            p,
            "slab",
            ElementShape::Plate {
                frame,
                contour: vec![[0., 0.], [10000., 0.], [10000., 4000.], [0., 4000.]],
                thickness: LengthMm::new(200.).unwrap(),
            },
        )
    }

    fn vertical(x: f64) -> PourBreak {
        PourBreak {
            start: Point3([x, -1000., 0.]),
            end: Point3([x, 5000., 0.]),
            horizontal: false,
        }
    }

    #[test]
    fn joint_splits_slab_volume_between_zones() {
        let mut p = concrete_project();
        let s = slab(&mut p, 3200.);
        let zones = p.pour_zones();
        assert_eq!(zones.len(), 1);
        assert!(
            (zones[0].volume_m3 - 8.).abs() < 1e-6,
            "{}",
            zones[0].volume_m3
        );
        p.concrete.breaks.push(vertical(4000.));
        let zones = p.pour_zones();
        assert_eq!(zones.len(), 2);
        let v: Vec<f64> = zones.iter().map(|z| z.volume_m3).collect();
        assert!(
            (v[0] - 3.2).abs() < 1e-3 && (v[1] - 4.8).abs() < 1e-3,
            "{v:?}"
        );
        assert!(zones.iter().all(|z| z.parts() == BTreeSet::from([s])));
        assert!(zones[0].bounds[1][0] <= 4000. + 1e-6);
        let rows = p.pour_rows(&zones);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].element, "Плита δ=200");
        assert_eq!((rows[0].zone, rows[0].count), (1, 1));
        assert_eq!(zones.iter().map(|z| z.number).collect::<Vec<_>>(), [1, 2]);
        // A joint beyond the slab or short of it does not cut.
        p.concrete.breaks = vec![PourBreak {
            start: Point3([4000., 5000., 0.]),
            end: Point3([4000., 6000., 0.]),
            horizontal: false,
        }];
        assert_eq!(p.pour_zones().len(), 1);
    }

    #[test]
    fn horizontal_joint_separates_columns_from_slab() {
        let mut p = concrete_project();
        let c1 = column(&mut p, 200., 200., 3000.);
        let c2 = column(&mut p, 9800., 200., 3000.);
        let s = slab(&mut p, 3200.);
        // Column tops touch the slab bottom: one zone.
        assert_eq!(p.pour_zones().len(), 1);
        p.concrete.breaks.push(PourBreak {
            start: Point3([-500., -500., 3000.]),
            end: Point3([10500., 4500., 3000.]),
            horizontal: true,
        });
        let zones = p.pour_zones();
        // Each column alone below the joint, the slab above it.
        assert_eq!(zones.len(), 3);
        assert_eq!(zones[2].parts(), BTreeSet::from([s]));
        assert!(zones[..2].iter().any(|z| z.parts() == BTreeSet::from([c1])));
        assert!(zones[..2].iter().any(|z| z.parts() == BTreeSet::from([c2])));
        // A joint halfway up the columns cuts both.
        p.concrete.breaks[0].start.0[2] = 1500.;
        let zones = p.pour_zones();
        assert_eq!(zones.len(), 3, "{zones:?}");
        let lower: f64 = zones.iter().take(2).map(|z| z.volume_m3).sum();
        assert!((lower - 2. * 0.4 * 0.4 * 1.5).abs() < 1e-6, "{lower}");
        assert!(
            (zones[2].volume_m3 - (8. + 2. * 0.24)).abs() < 1e-6,
            "{}",
            zones[2].volume_m3
        );
    }

    #[test]
    fn zone_numbers_survive_edits() {
        let mut p = concrete_project();
        slab(&mut p, 3200.);
        p.concrete.breaks.push(vertical(4000.));
        p.concrete.units = p.pour_units();
        assert!(!p.pour_units_outdated());
        // Swap the stored numbers: the zones keep them.
        p.concrete.units[0].number = 7;
        p.concrete.units[1].number = 3;
        // Both zones hold the same slab: the stored centres tell them apart.
        let numbers: Vec<u32> = p.pour_zones().iter().map(|z| z.number).collect();
        assert_eq!(numbers, [7, 3]);
        // Moving the joint changes the volumes, not the numbers.
        p.concrete.breaks[0] = vertical(6000.);
        let zones = p.pour_zones();
        assert_eq!(zones[0].number, 7);
        assert!((zones[0].volume_m3 - 4.8).abs() < 1e-3);
        // A new column apart from the slab is a new zone after the largest.
        column(&mut p, 20000., 0., 3000.);
        assert!(p.pour_units_outdated());
        let zones = p.pour_zones();
        assert_eq!(zones.len(), 3);
        assert!(zones.iter().any(|z| z.number == 8));

        assert!(p.concrete.validate().is_ok());
    }
}
