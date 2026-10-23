//! IFC import of the declared subset; everything else is listed in the report.
use crate::step::{Entity, File, Value, parse, valid_guid};
use crate::{GOST_PSET_NAME, GUID_PROPERTY, IFC_ENTITY_PROPERTY, PSET_NAME, mapping};
use open_bim_model::library;
use open_bim_model::{
    Detail, DetailKind, Element, ElementShape, Frame, LengthMm, Material, MemberKind, ObjectId,
    Point3, Profile, ProfileShape, Project, PropertyValue,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ImportReport {
    pub schema: Vec<String>,
    /// Millimetres per file length unit.
    pub length_scale: f64,
    pub imported: BTreeMap<String, usize>,
    /// Entity types present in the file but outside the supported subset.
    pub unsupported_entities: BTreeMap<String, usize>,
    /// Per-object problems: skipped elements, ignored representation parts.
    pub issues: Vec<String>,
    /// Section names of the file → ГОСТ catalogue sections they became.
    pub profiles: BTreeMap<String, String>,
    /// Section names not found in the ГОСТ catalogue (kept as they are).
    pub unmatched_profiles: BTreeSet<String>,
    /// Material names of the file → ГОСТ steels they became.
    pub materials: BTreeMap<String, String>,
    /// Building elements outside the subset, in words («стены» → 12).
    pub not_imported: BTreeMap<String, usize>,
    /// Elements that could not be read.
    pub skipped: usize,
}

impl ImportReport {
    /// Short Russian account for the status line and the import dialog.
    pub fn summary(&self) -> String {
        let n = |k: &str| self.imported.get(k).copied().unwrap_or(0);
        let mut out = format!(
            "Импортировано: балок {}, колонн {}, пластин {}, сборок {}, проёмов {}.",
            n("IFCBEAM") + n("IFCMEMBER"),
            n("IFCCOLUMN"),
            n("IFCPLATE"),
            n("IFCELEMENTASSEMBLY"),
            n("IFCOPENINGELEMENT"),
        );
        let list = |items: Vec<String>| {
            let more = items.len().saturating_sub(6);
            let mut s = items.into_iter().take(6).collect::<Vec<_>>().join(", ");
            if more > 0 {
                s.push_str(&format!(" и ещё {more}"));
            }
            s
        };
        if !self.profiles.is_empty() {
            out.push_str(&format!(
                " Сечения по сортаменту ГОСТ: {}.",
                list(
                    self.profiles
                        .iter()
                        .map(|(a, b)| format!("{a} → {b}"))
                        .collect()
                )
            ));
        }
        if !self.unmatched_profiles.is_empty() {
            out.push_str(&format!(
                " Нет в сортаменте ГОСТ (оставлены как в файле): {}.",
                list(self.unmatched_profiles.iter().cloned().collect())
            ));
        }
        if !self.materials.is_empty() {
            out.push_str(&format!(
                " Стали: {}.",
                list(
                    self.materials
                        .iter()
                        .map(|(a, b)| format!("{a} → {b}"))
                        .collect()
                )
            ));
        }
        if !self.not_imported.is_empty() {
            out.push_str(&format!(
                " Не импортируются: {}.",
                list(
                    self.not_imported
                        .iter()
                        .map(|(k, v)| format!("{k} {v}"))
                        .collect()
                )
            ));
        }
        if self.skipped > 0 {
            out.push_str(&format!(" Пропущено объектов: {}.", self.skipped));
        }
        if !self.issues.is_empty() {
            out.push_str(&format!(" Замечаний: {}.", self.issues.len()));
        }
        out
    }
}

/// Entity types read (or deliberately consumed) by the importer.
const KNOWN: &[&str] = &[
    "IFCCARTESIANPOINT",
    "IFCDIRECTION",
    "IFCAXIS2PLACEMENT3D",
    "IFCAXIS2PLACEMENT2D",
    "IFCLOCALPLACEMENT",
    "IFCGEOMETRICREPRESENTATIONCONTEXT",
    "IFCGEOMETRICREPRESENTATIONSUBCONTEXT",
    "IFCSIUNIT",
    "IFCDERIVEDUNIT",
    "IFCDERIVEDUNITELEMENT",
    "IFCUNITASSIGNMENT",
    "IFCCONVERSIONBASEDUNIT",
    "IFCMEASUREWITHUNIT",
    "IFCDIMENSIONALEXPONENTS",
    "IFCPROJECT",
    "IFCSITE",
    "IFCBUILDING",
    "IFCBUILDINGSTOREY",
    "IFCRELAGGREGATES",
    "IFCRELCONTAINEDINSPATIALSTRUCTURE",
    "IFCSHAPEREPRESENTATION",
    "IFCPRODUCTDEFINITIONSHAPE",
    "IFCEXTRUDEDAREASOLID",
    "IFCISHAPEPROFILEDEF",
    "IFCLSHAPEPROFILEDEF",
    "IFCUSHAPEPROFILEDEF",
    "IFCCIRCLEHOLLOWPROFILEDEF",
    "IFCRECTANGLEHOLLOWPROFILEDEF",
    "IFCRECTANGLEPROFILEDEF",
    "IFCCIRCLEPROFILEDEF",
    "IFCARBITRARYCLOSEDPROFILEDEF",
    "IFCPOLYLINE",
    "IFCMATERIAL",
    "IFCMATERIALPROPERTIES",
    "IFCRELASSOCIATESMATERIAL",
    "IFCPROPERTYSET",
    "IFCPROPERTYSINGLEVALUE",
    "IFCRELDEFINESBYPROPERTIES",
    "IFCOPENINGELEMENT",
    "IFCRELVOIDSELEMENT",
    "IFCCLASSIFICATION",
    "IFCCLASSIFICATIONREFERENCE",
    "IFCRELASSOCIATESCLASSIFICATION",
    "IFCBEAM",
    "IFCCOLUMN",
    "IFCMEMBER",
    "IFCPLATE",
    "IFCOWNERHISTORY",
    "IFCPERSON",
    "IFCORGANIZATION",
    "IFCPERSONANDORGANIZATION",
    "IFCAPPLICATION",
    "IFCBEAMSTANDARDCASE",
    "IFCCOLUMNSTANDARDCASE",
    "IFCMEMBERSTANDARDCASE",
    "IFCPLATESTANDARDCASE",
    "IFCELEMENTASSEMBLY",
    "IFCMAPPEDITEM",
    "IFCREPRESENTATIONMAP",
    "IFCCARTESIANTRANSFORMATIONOPERATOR3D",
    "IFCINDEXEDPOLYCURVE",
    "IFCCARTESIANPOINTLIST2D",
    "IFCCOMPOSITECURVE",
    "IFCCOMPOSITECURVESEGMENT",
    "IFCTRIMMEDCURVE",
    "IFCCIRCLE",
    "IFCLINE",
    "IFCVECTOR",
    "IFCARBITRARYPROFILEDEFWITHVOIDS",
    "IFCBOOLEANCLIPPINGRESULT",
    "IFCMATERIALLIST",
    "IFCMATERIALLAYERSETUSAGE",
    "IFCMATERIALLAYERSET",
    "IFCMATERIALLAYER",
    "IFCMATERIALPROFILESETUSAGE",
    "IFCMATERIALPROFILESETUSAGETAPERING",
    "IFCMATERIALPROFILESET",
    "IFCMATERIALPROFILE",
    "IFCMATERIALCONSTITUENTSET",
    "IFCMATERIALCONSTITUENT",
    "IFCGENERALMATERIALPROPERTIES",
    "IFCRELDEFINESBYTYPE",
    "IFCHALFSPACESOLID",
    "IFCPOLYGONALBOUNDEDHALFSPACE",
    "IFCPLANE",
];

/// Presentation, quantities, types and header data: read by no one, worth no
/// line in the report.
fn silent(kind: &str) -> bool {
    [
        "IFCSURFACESTYLE",
        "IFCCOLOUR",
        "IFCPRESENTATION",
        "IFCSTYLED",
        "IFCCURVESTYLE",
        "IFCFILLAREASTYLE",
        "IFCDRAUGHTINGPRE",
        "IFCMATERIALDEFINITIONREPRESENTATION",
        "IFCQUANTITY",
        "IFCELEMENTQUANTITY",
        "IFCPOSTALADDRESS",
        "IFCTELECOMADDRESS",
        "IFCCALENDARDATE",
        "IFCLOCALTIME",
        "IFCDATEANDTIME",
        "IFCCOORDINATEDUNIVERSALTIMEOFFSET",
        "IFCACTORROLE",
        "IFCRELASSOCIATESDOCUMENT",
        "IFCDOCUMENT",
        "IFCRELSPACEBOUNDARY",
    ]
    .iter()
    .any(|p| kind.starts_with(p))
        || (kind.ends_with("TYPE") && !kind.starts_with("IFCREL"))
}

/// Base kind of a part entity: standard cases count as their entity.
fn base_kind(kind: &str) -> &str {
    kind.strip_suffix("STANDARDCASE").unwrap_or(kind)
}

/// Human name of a building element type the importer does not take.
fn element_word(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "IFCWALL" | "IFCWALLSTANDARDCASE" | "IFCWALLELEMENTEDCASE" => "стены",
        "IFCSLAB" | "IFCSLABSTANDARDCASE" | "IFCSLABELEMENTEDCASE" => "плиты перекрытия",
        "IFCFOOTING" => "фундаменты",
        "IFCPILE" => "сваи",
        "IFCROOF" => "кровли",
        "IFCSTAIR" | "IFCSTAIRFLIGHT" => "лестницы",
        "IFCRAMP" | "IFCRAMPFLIGHT" => "пандусы",
        "IFCRAILING" => "ограждения",
        "IFCDOOR" | "IFCDOORSTANDARDCASE" => "двери",
        "IFCWINDOW" | "IFCWINDOWSTANDARDCASE" => "окна",
        "IFCCOVERING" => "отделка",
        "IFCCURTAINWALL" => "витражи",
        "IFCMECHANICALFASTENER" | "IFCFASTENER" => "болты и крепёж",
        "IFCDISCRETEACCESSORY" => "закладные и аксессуары",
        "IFCREINFORCINGBAR" | "IFCREINFORCINGMESH" | "IFCTENDON" => "арматура",
        "IFCBUILDINGELEMENTPROXY" => "прочие объекты",
        "IFCBUILDINGELEMENTPART" => "части элементов",
        "IFCFURNISHINGELEMENT" => "мебель",
        "IFCSPACE" => "помещения",
        "IFCGRID" => "сетки осей",
        _ => return None,
    })
}

type V3 = [f64; 3];
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn unit(a: V3) -> Option<V3> {
    let l = dot(a, a).sqrt();
    (l > 1e-12 && l.is_finite()).then(|| a.map(|v| v / l))
}

/// Rigid transform: columns x, y, z and origin, all in world millimetres.
#[derive(Clone, Copy, Debug)]
struct Transform {
    origin: V3,
    axes: [V3; 3],
}
impl Transform {
    const IDENTITY: Self = Self {
        origin: [0.; 3],
        axes: [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
    };
    fn apply(&self, p: V3) -> V3 {
        std::array::from_fn(|i| {
            self.origin[i] + (0..3).map(|j| self.axes[j][i] * p[j]).sum::<f64>()
        })
    }
    fn rotate(&self, v: V3) -> V3 {
        std::array::from_fn(|i| (0..3).map(|j| self.axes[j][i] * v[j]).sum::<f64>())
    }
    fn then(&self, child: &Self) -> Self {
        Self {
            origin: self.apply(child.origin),
            axes: child.axes.map(|a| self.rotate(a)),
        }
    }
    fn inverse_point(&self, p: V3) -> V3 {
        let d = std::array::from_fn(|i| p[i] - self.origin[i]);
        self.axes.map(|a| dot(a, d))
    }
    fn frame(&self) -> Frame {
        Frame {
            origin: Point3(self.origin),
            axes: self.axes,
        }
    }
}

struct Reader<'a> {
    file: &'a File,
    scale: f64,
}

impl<'a> Reader<'a> {
    fn get(&self, id: u64) -> Result<&'a Entity, String> {
        self.file
            .entities
            .get(&id)
            .ok_or_else(|| format!("ссылка #{id} не найдена"))
    }
    fn arg(&self, e: &'a Entity, i: usize) -> &'a Value {
        e.args.get(i).unwrap_or(&Value::Null)
    }
    fn reference(&self, e: &'a Entity, i: usize) -> Result<&'a Entity, String> {
        let r = self
            .arg(e, i)
            .as_ref()
            .ok_or_else(|| format!("{}: аргумент {i} не ссылка", e.kind))?;
        self.get(r)
    }
    fn coordinates(&self, e: &Entity, length: bool) -> Result<V3, String> {
        let list = self
            .arg(e, 0)
            .as_list()
            .ok_or_else(|| format!("{}: нет координат", e.kind))?;
        let mut v = [0.; 3];
        for (i, item) in list.iter().take(3).enumerate() {
            let x = item.as_f64().ok_or("координата не число")?;
            v[i] = if length { x * self.scale } else { x };
        }
        if !v.iter().all(|x| x.is_finite() && x.abs() <= 1e12) {
            return Err("координата вне диапазона".into());
        }
        Ok(v)
    }
    fn placement3d(&self, e: &Entity) -> Result<Transform, String> {
        match e.kind.as_str() {
            "IFCAXIS2PLACEMENT3D" => {
                let origin = self.coordinates(self.reference(e, 0)?, true)?;
                let z = match self.arg(e, 1).as_ref() {
                    Some(r) => self.coordinates(self.get(r)?, false)?,
                    None => [0., 0., 1.],
                };
                let x = match self.arg(e, 2).as_ref() {
                    Some(r) => self.coordinates(self.get(r)?, false)?,
                    None => [1., 0., 0.],
                };
                let z = unit(z).ok_or("нулевая ось Z")?;
                let x = unit(std::array::from_fn(|i| x[i] - dot(x, z) * z[i]))
                    .ok_or("ось X параллельна Z")?;
                Ok(Transform {
                    origin,
                    axes: [x, cross(z, x), z],
                })
            }
            "IFCAXIS2PLACEMENT2D" => {
                let p = self.coordinates(self.reference(e, 0)?, true)?;
                let x = match self.arg(e, 1).as_ref() {
                    Some(r) => self.coordinates(self.get(r)?, false)?,
                    None => [1., 0., 0.],
                };
                let x = unit([x[0], x[1], 0.]).ok_or("нулевая ось X")?;
                Ok(Transform {
                    origin: [p[0], p[1], 0.],
                    axes: [x, [-x[1], x[0], 0.], [0., 0., 1.]],
                })
            }
            other => Err(format!("размещение {other} не поддерживается")),
        }
    }
    fn local_placement(&self, e: &Entity, depth: usize) -> Result<Transform, String> {
        if depth > 64 {
            return Err("цикл или слишком глубокая цепочка размещений".into());
        }
        if e.kind != "IFCLOCALPLACEMENT" {
            return Err(format!("размещение {} не поддерживается", e.kind));
        }
        let parent = match self.arg(e, 0).as_ref() {
            Some(r) => self.local_placement(self.get(r)?, depth + 1)?,
            None => Transform::IDENTITY,
        };
        Ok(parent.then(&self.placement3d(self.reference(e, 1)?)?))
    }
    fn length(&self, v: &Value) -> Result<f64, String> {
        let x = v.as_f64().ok_or("размер не число")? * self.scale;
        if !(x.is_finite() && x > 0.) {
            return Err("размер должен быть положительным".into());
        }
        Ok(x)
    }
    /// Vertices of a closed 2D curve: polylines, indexed poly curves and
    /// composite curves of lines and arcs (arcs by their end and middle points).
    fn curve_points(&self, curve: &'a Entity, depth: usize) -> Result<Vec<[f64; 2]>, String> {
        if depth > 8 {
            return Err("слишком глубокая вложенность контура".into());
        }
        let point = |r: &Value| -> Result<[f64; 2], String> {
            let c = self.coordinates(self.get(r.as_ref().ok_or("вершина не ссылка")?)?, true)?;
            Ok([c[0], c[1]])
        };
        let mut out: Vec<[f64; 2]> = Vec::new();
        match curve.kind.as_str() {
            "IFCPOLYLINE" => {
                let points = self.arg(curve, 0).as_list().ok_or("пустая полилиния")?;
                if points.len() > 100_000 {
                    return Err("слишком много вершин контура".into());
                }
                for p in points {
                    out.push(point(p)?);
                }
            }
            "IFCINDEXEDPOLYCURVE" => {
                let list = self.reference(curve, 0)?;
                let coords = self.arg(list, 0).as_list().ok_or("нет координат")?;
                if coords.len() > 100_000 {
                    return Err("слишком много вершин контура".into());
                }
                let mut points = Vec::with_capacity(coords.len());
                for c in coords {
                    let c = c.as_list().ok_or("координата не список")?;
                    let x = c
                        .first()
                        .and_then(Value::as_f64)
                        .ok_or("координата не число")?;
                    let y = c
                        .get(1)
                        .and_then(Value::as_f64)
                        .ok_or("координата не число")?;
                    let p = [x * self.scale, y * self.scale];
                    if !p.iter().all(|v| v.is_finite() && v.abs() <= 1e12) {
                        return Err("координата вне диапазона".into());
                    }
                    points.push(p);
                }
                match self.arg(curve, 1).as_list() {
                    None => out = points,
                    Some(segments) => {
                        for segment in segments {
                            let indices = match segment {
                                Value::Typed(_, inner) => inner.as_list(),
                                other => other.as_list(),
                            }
                            .ok_or("сегмент не список")?;
                            for i in indices {
                                let i = i.as_f64().ok_or("индекс не число")? as usize;
                                let p = *points
                                    .get(i.wrapping_sub(1))
                                    .ok_or("индекс вершины вне списка")?;
                                out.push(p);
                            }
                        }
                    }
                }
            }
            "IFCCOMPOSITECURVE" => {
                let segments = self.arg(curve, 0).as_list().ok_or("пустая кривая")?;
                if segments.len() > 100_000 {
                    return Err("слишком много сегментов контура".into());
                }
                for segment in segments {
                    let segment = self.get(segment.as_ref().ok_or("сегмент не ссылка")?)?;
                    if segment.kind != "IFCCOMPOSITECURVESEGMENT" {
                        return Err(format!("сегмент {} не поддерживается", segment.kind));
                    }
                    let same_sense = self.arg(segment, 1).as_enum() != Some("F");
                    let parent = self.reference(segment, 2)?;
                    let mut points = match parent.kind.as_str() {
                        "IFCTRIMMEDCURVE" => {
                            // Cartesian trimming points (both programs write them).
                            let trim = |i: usize| -> Option<[f64; 2]> {
                                self.arg(parent, i).as_list()?.iter().find_map(|v| {
                                    let e = self.get(v.as_ref()?).ok()?;
                                    (e.kind == "IFCCARTESIANPOINT")
                                        .then(|| self.coordinates(e, true).ok())
                                        .flatten()
                                        .map(|c| [c[0], c[1]])
                                })
                            };
                            let (Some(a), Some(b)) = (trim(1), trim(2)) else {
                                return Err("дуга контура без точек обрезки".into());
                            };
                            let sense = self.arg(parent, 3).as_enum() != Some("F");
                            let basis = self.reference(parent, 0)?;
                            let mut pts = vec![a];
                            if basis.kind == "IFCCIRCLE" {
                                let centre = self.placement3d(self.reference(basis, 0)?)?;
                                let r = self.length(self.arg(basis, 1))?;
                                let c = centre.origin;
                                let angle = |p: [f64; 2]| (p[1] - c[1]).atan2(p[0] - c[0]);
                                let (a0, mut a1) = (angle(a), angle(b));
                                if sense {
                                    while a1 <= a0 {
                                        a1 += std::f64::consts::TAU;
                                    }
                                } else {
                                    while a1 >= a0 {
                                        a1 -= std::f64::consts::TAU;
                                    }
                                }
                                // Chords of at most 15°: fillets keep their area.
                                let steps = ((a1 - a0).abs() / (std::f64::consts::PI / 12.))
                                    .ceil()
                                    .clamp(2., 96.)
                                    as usize;
                                for k in 1..steps {
                                    let t = a0 + (a1 - a0) * k as f64 / steps as f64;
                                    pts.push([c[0] + r * t.cos(), c[1] + r * t.sin()]);
                                }
                            }
                            pts.push(b);
                            pts
                        }
                        _ => self.curve_points(parent, depth + 1)?,
                    };
                    if !same_sense {
                        points.reverse();
                    }
                    for p in points {
                        if out
                            .last()
                            .is_none_or(|q| (q[0] - p[0]).hypot(q[1] - p[1]) > 1e-6)
                        {
                            out.push(p);
                        }
                    }
                }
            }
            other => return Err(format!("контур {other} не поддерживается")),
        }
        if out.len() > 1 && {
            let (a, b) = (out[0], out[out.len() - 1]);
            (a[0] - b[0]).hypot(a[1] - b[1]) <= 1e-6
        } {
            out.pop();
        }
        Ok(out)
    }
    /// IfcCartesianTransformationOperator3D with unit scale.
    fn operator3d(&self, e: &Entity) -> Result<Transform, String> {
        if e.kind != "IFCCARTESIANTRANSFORMATIONOPERATOR3D" {
            return Err(format!("преобразование {} не поддерживается", e.kind));
        }
        let axis = |i: usize, default: V3| -> Result<V3, String> {
            match self.arg(e, i).as_ref() {
                Some(r) => self.coordinates(self.get(r)?, false),
                None => Ok(default),
            }
        };
        if let Some(scale) = self.arg(e, 3).as_f64()
            && (scale - 1.).abs() > 1e-9
        {
            return Err("масштабированные копии геометрии не поддерживаются".into());
        }
        let origin = self.coordinates(self.reference(e, 2)?, true)?;
        let z = unit(axis(4, [0., 0., 1.])?).ok_or("нулевая ось Z")?;
        let x = axis(0, [1., 0., 0.])?;
        let x =
            unit(std::array::from_fn(|i| x[i] - dot(x, z) * z[i])).ok_or("ось X параллельна Z")?;
        Ok(Transform {
            origin,
            axes: [x, cross(z, x), z],
        })
    }
    /// Profile shape (members) or contour (plates) plus its 2D position.
    fn profile(&self, e: &'a Entity) -> Result<(Shape, Transform, Option<String>), String> {
        let name = self.arg(e, 1).as_str().map(str::to_string);
        let position = match self.arg(e, 2).as_ref() {
            Some(r) if !e.kind.starts_with("IFCARBITRARY") => self.placement3d(self.get(r)?)?,
            _ => Transform::IDENTITY,
        };
        let l = |i: usize| self.length(self.arg(e, i));
        let shape = match e.kind.as_str() {
            "IFCISHAPEPROFILEDEF" => Shape::Profile(ProfileShape::ISection {
                width: l(3)?,
                height: l(4)?,
                web: l(5)?,
                flange: l(6)?,
            }),
            "IFCLSHAPEPROFILEDEF" => Shape::Profile(ProfileShape::Angle {
                height: l(3)?,
                width: l(4)?,
                thickness: l(5)?,
            }),
            "IFCUSHAPEPROFILEDEF" => Shape::Profile(ProfileShape::Channel {
                height: l(3)?,
                width: l(4)?,
                web: l(5)?,
                flange: l(6)?,
            }),
            "IFCCIRCLEHOLLOWPROFILEDEF" => Shape::Profile(ProfileShape::CircularHollow {
                diameter: 2. * l(3)?,
                thickness: l(4)?,
            }),
            "IFCRECTANGLEHOLLOWPROFILEDEF" => Shape::Profile(ProfileShape::RectangularHollow {
                width: l(3)?,
                height: l(4)?,
                thickness: l(5)?,
            }),
            "IFCRECTANGLEPROFILEDEF" => Shape::Profile(ProfileShape::Rectangle {
                width: l(3)?,
                height: l(4)?,
            }),
            "IFCCIRCLEPROFILEDEF" => Shape::Circle(l(3)?),
            "IFCARBITRARYCLOSEDPROFILEDEF" | "IFCARBITRARYPROFILEDEFWITHVOIDS" => {
                let outer = self.curve_points(self.reference(e, 2)?, 0)?;
                let mut voids = Vec::new();
                if e.kind == "IFCARBITRARYPROFILEDEFWITHVOIDS" {
                    for inner in self.arg(e, 3).as_list().unwrap_or(&[]) {
                        let inner = self.get(inner.as_ref().ok_or("контур не ссылка")?)?;
                        voids.push(self.curve_points(inner, 0)?);
                    }
                }
                Shape::Contour(outer, voids)
            }
            other => return Err(format!("профиль {other} не поддерживается")),
        };
        Ok((shape, position, name))
    }
    /// Extruded body: profile, solid frame relative to the product, depth.
    fn body(
        &self,
        product: &'a Entity,
        issues: &mut Vec<String>,
        label: &str,
    ) -> Result<(Shape, Transform, f64, Option<String>), String> {
        let shape = self.reference(product, 6)?;
        let reps = self.arg(shape, 2).as_list().ok_or("нет представлений")?;
        for rep in reps {
            let rep = self.get(rep.as_ref().ok_or("представление не ссылка")?)?;
            if self.arg(rep, 1).as_str() != Some("Body") {
                continue;
            }
            return self.representation(rep, issues, label, 0);
        }
        Err("нет представления Body".into())
    }
    fn representation(
        &self,
        rep: &'a Entity,
        issues: &mut Vec<String>,
        label: &str,
        depth: usize,
    ) -> Result<(Shape, Transform, f64, Option<String>), String> {
        let items = self.arg(rep, 3).as_list().ok_or("пустое представление")?;
        if items.len() != 1 {
            return Err(format!(
                "тело из {} элементов не поддерживается",
                items.len()
            ));
        }
        self.solid(
            self.get(items[0].as_ref().ok_or("элемент не ссылка")?)?,
            issues,
            label,
            depth,
        )
    }
    fn solid(
        &self,
        mut item: &'a Entity,
        issues: &mut Vec<String>,
        label: &str,
        depth: usize,
    ) -> Result<(Shape, Transform, f64, Option<String>), String> {
        if depth > 8 {
            return Err("слишком глубокая вложенность геометрии".into());
        }
        let mut clipped = 0;
        while item.kind == "IFCBOOLEANCLIPPINGRESULT" && clipped < 32 {
            if clipped == 0 {
                issues.push(format!("{label}: срезы IfcBooleanClippingResult не импортированы, взят исходный профиль"));
            }
            item = self.reference(item, 1)?;
            clipped += 1;
        }
        if item.kind == "IFCMAPPEDITEM" {
            // A shared geometry placed by a transformation (one per type).
            let map = self.reference(item, 0)?;
            let origin = self.placement3d(self.reference(map, 0)?)?;
            let target = self.operator3d(self.reference(item, 1)?)?;
            let (shape, inner, d, name) =
                self.representation(self.reference(map, 1)?, issues, label, depth + 1)?;
            return Ok((shape, target.then(&origin).then(&inner), d, name));
        }
        if item.kind != "IFCEXTRUDEDAREASOLID" {
            return Err(format!("тело {} не поддерживается", item.kind));
        }
        let (profile, profile_position, name) = self.profile(self.reference(item, 0)?)?;
        let mut position = match self.arg(item, 1).as_ref() {
            Some(r) => self.placement3d(self.get(r)?)?,
            None => Transform::IDENTITY,
        };
        let direction = unit(self.coordinates(self.reference(item, 2)?, false)?)
            .ok_or("нулевое направление")?;
        let depth = self.length(self.arg(item, 3))?;
        if (direction[2] + 1.).abs() <= 1e-9 {
            // Extruded downwards: the same prism, started from its far end.
            position = position.then(&Transform {
                origin: [0., 0., -depth],
                axes: Transform::IDENTITY.axes,
            });
        } else if (direction[2] - 1.).abs() > 1e-9 {
            return Err("выдавливание не по оси Z профиля не поддерживается".into());
        }
        Ok((profile, position.then(&profile_position), depth, name))
    }
}

enum Shape {
    Profile(ProfileShape),
    /// Outer contour and voids.
    Contour(Vec<[f64; 2]>, Vec<Vec<[f64; 2]>>),
    Circle(f64),
}

fn length_scale(file: &File, report: &mut ImportReport) -> f64 {
    for e in file.entities.values().filter(|e| e.kind == "IFCSIUNIT") {
        if e.args.get(1).and_then(Value::as_enum) == Some("LENGTHUNIT") {
            let factor = match e.args.get(2).and_then(Value::as_enum) {
                Some("MILLI") => 1.,
                Some("CENTI") => 10.,
                Some("DECI") => 100.,
                Some("KILO") => 1e6,
                _ => 1000.,
            };
            // A metre unit inside a derived unit must not override the assignment.
            let assigned = file.entities.values().any(|u| {
                u.kind == "IFCUNITASSIGNMENT"
                    && u.args.first().and_then(Value::as_list).is_some_and(|l| {
                        l.iter()
                            .any(|v| v.as_ref().and_then(|r| file.entities.get(&r)) == Some(e))
                    })
            });
            if assigned {
                return factor;
            }
        }
    }
    for e in file
        .entities
        .values()
        .filter(|e| e.kind == "IFCCONVERSIONBASEDUNIT")
    {
        if e.args.get(1).and_then(Value::as_enum) == Some("LENGTHUNIT") {
            match e
                .args
                .get(2)
                .and_then(Value::as_str)
                .map(str::to_ascii_lowercase)
                .as_deref()
            {
                Some("foot") => return 304.8,
                Some("inch") => return 25.4,
                other => report.issues.push(format!(
                    "единица длины {other:?} не поддерживается, принят мм"
                )),
            }
        }
    }
    report
        .issues
        .push("единица длины не задана, принят мм".into());
    1.
}

fn property_value(v: &Value) -> Option<PropertyValue> {
    match v {
        Value::Typed(kind, inner) => match (kind.as_str(), inner.as_ref()) {
            (_, Value::Enum(e)) if e == "T" || e == "F" => Some(PropertyValue::Boolean(e == "T")),
            (_, Value::String(s)) => Some(PropertyValue::Text(s.clone())),
            (_, other) => other.as_f64().map(PropertyValue::Number),
        },
        _ => None,
    }
}

/// Whether a rectangular opening block (centred on its frame origin in XY,
/// extruded `depth` along Z) holds every corner of the part's bounding box
/// that lies above its base plane — i.e. it removes the whole part on that side.
fn covers(
    project: &Project,
    part: ObjectId,
    opening: &Transform,
    width: f64,
    height: f64,
    depth: f64,
) -> bool {
    let Some(element) = project.elements.get(&part) else {
        return false;
    };
    let (section, length) = match &element.shape {
        ElementShape::Plate {
            contour, thickness, ..
        } => (contour.clone(), thickness.value()),
        ElementShape::Member {
            profile, length, ..
        } => match project
            .profiles
            .get(profile)
            .map(|p| p.shape.solid_contour())
        {
            Some(Ok(c)) => (c, length.value()),
            _ => return false,
        },
    };
    let (lo, hi) = section.iter().fold(
        ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]),
        |(lo, hi), p| {
            (
                [lo[0].min(p[0]), lo[1].min(p[1])],
                [hi[0].max(p[0]), hi[1].max(p[1])],
            )
        },
    );
    let corners = [lo[0], hi[0]].into_iter().flat_map(|x| {
        [lo[1], hi[1]]
            .into_iter()
            .flat_map(move |y| [0., length].map(move |z| [x, y, z]))
    });
    let mut any_inside = false;
    for c in corners {
        let d = [
            c[0] - opening.origin[0],
            c[1] - opening.origin[1],
            c[2] - opening.origin[2],
        ];
        let (x, y, z) = (
            dot(d, opening.axes[0]),
            dot(d, opening.axes[1]),
            dot(d, opening.axes[2]),
        );
        if z <= 0. {
            continue;
        }
        any_inside = true;
        if x.abs() > width / 2. || y.abs() > height / 2. || z > depth {
            return false;
        }
    }
    any_inside && width.min(height) > 0.
}

/// The IfcMaterial behind a material select (lists, layer and profile sets,
/// constituents take their first material), and the section a material
/// profile names.
fn resolve_material(file: &File, id: u64, depth: usize) -> (Option<u64>, Option<u64>) {
    let Some(e) = file.entities.get(&id) else {
        return (None, None);
    };
    if depth > 8 {
        return (None, None);
    }
    let first = |i: usize| {
        e.args
            .get(i)
            .and_then(Value::as_list)
            .and_then(|l| l.iter().find_map(Value::as_ref))
    };
    let at = |i: usize| e.args.get(i).and_then(Value::as_ref);
    let next = match e.kind.as_str() {
        "IFCMATERIAL" => return (Some(id), None),
        "IFCMATERIALPROFILE" => {
            let (m, _) = at(2).map_or((None, None), |m| resolve_material(file, m, depth + 1));
            return (m, at(3));
        }
        "IFCMATERIALLIST" | "IFCMATERIALLAYERSET" => first(0),
        "IFCMATERIALPROFILESET" | "IFCMATERIALCONSTITUENTSET" => first(2),
        "IFCMATERIALLAYERSETUSAGE"
        | "IFCMATERIALPROFILESETUSAGE"
        | "IFCMATERIALPROFILESETUSAGETAPERING"
        | "IFCMATERIALLAYER" => at(0),
        "IFCMATERIALCONSTITUENT" => at(2),
        _ => None,
    };
    next.map_or((None, None), |n| resolve_material(file, n, depth + 1))
}

fn regular_polygon(r: f64) -> Vec<[f64; 2]> {
    (0..32)
        .map(|k| {
            let t = std::f64::consts::TAU * k as f64 / 32.;
            [r * t.cos(), r * t.sin()]
        })
        .collect()
}

/// Parse IFC text (IFC2x3 or IFC4) into a new project; unsupported content
/// goes to the report. Sections are matched to the ГОСТ catalogue and steel
/// grades to ГОСТ steels where the file allows it.
pub fn import(text: &str) -> Result<(Project, ImportReport), String> {
    let file = parse(text)?;
    let mut report = ImportReport {
        schema: file.schema.clone(),
        ..Default::default()
    };
    let schema: Vec<String> = file.schema.iter().map(|s| s.to_ascii_uppercase()).collect();
    if !schema
        .iter()
        .any(|s| s.starts_with("IFC4") || s.starts_with("IFC2X3"))
    {
        return Err(format!(
            "Схема {:?} не поддерживается; ожидается IFC2x3 или IFC4",
            file.schema
        ));
    }
    if schema.iter().any(|s| s.starts_with("IFC4X3")) {
        report.issues.push(
            "IFC4X3 читается как IFC4: специфичные сущности 4.3 перечислены как неподдерживаемые"
                .into(),
        );
    }
    for e in file.entities.values() {
        if !KNOWN.contains(&e.kind.as_str()) && !silent(&e.kind) {
            *report
                .unsupported_entities
                .entry(e.kind.clone())
                .or_default() += 1;
        }
    }
    let scale = length_scale(&file, &mut report);
    report.length_scale = scale;
    let reader = Reader { file: &file, scale };
    let mut project = Project::default();
    project.elements.clear();
    if let Some(p) = file.entities.values().find(|e| e.kind == "IFCPROJECT")
        && let Some(name) = p.args.get(2).and_then(Value::as_str)
    {
        project.name = name.chars().take(1024).collect();
    }
    if let Some(site) = file.entities.values().find(|e| e.kind == "IFCSITE")
        && let Some(r) = site.args.get(5).and_then(Value::as_ref)
        && let Ok(t) = reader.get(r).and_then(|p| reader.local_placement(p, 0))
    {
        project.origin = Point3(t.origin);
    }
    // Materials (of elements or of their types), sections named by material
    // profiles, densities.
    let mut material_of: BTreeMap<u64, u64> = BTreeMap::new();
    let mut section_of: BTreeMap<u64, u64> = BTreeMap::new();
    let mut type_of: BTreeMap<u64, u64> = BTreeMap::new();
    let mut density: BTreeMap<u64, f64> = BTreeMap::new();
    let mut parts_of: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    for e in file.entities.values() {
        match e.kind.as_str() {
            "IFCRELASSOCIATESMATERIAL" => {
                if let (Some(objects), Some(m)) = (
                    e.args.get(4).and_then(Value::as_list),
                    e.args.get(5).and_then(Value::as_ref),
                ) {
                    let (material, section) = resolve_material(&file, m, 0);
                    let Some(material) = material else {
                        report.issues.push(format!(
                            "материал #{m}: {} не поддерживается",
                            reader.get(m).map(|x| x.kind.as_str()).unwrap_or("?")
                        ));
                        continue;
                    };
                    for o in objects.iter().filter_map(Value::as_ref) {
                        material_of.insert(o, material);
                        if let Some(s) = section {
                            section_of.insert(o, s);
                        }
                    }
                }
            }
            "IFCRELDEFINESBYTYPE" => {
                if let (Some(objects), Some(t)) = (
                    e.args.get(4).and_then(Value::as_list),
                    e.args.get(5).and_then(Value::as_ref),
                ) {
                    for o in objects.iter().filter_map(Value::as_ref) {
                        type_of.insert(o, t);
                    }
                }
            }
            "IFCRELAGGREGATES" => {
                if let (Some(whole), Some(parts)) = (
                    e.args.get(4).and_then(Value::as_ref),
                    e.args.get(5).and_then(Value::as_list),
                ) && file.entities.get(&whole).map(|w| w.kind.as_str())
                    == Some("IFCELEMENTASSEMBLY")
                {
                    parts_of
                        .entry(whole)
                        .or_default()
                        .extend(parts.iter().filter_map(Value::as_ref));
                }
            }
            "IFCMATERIALPROPERTIES" => {
                if let (Some(props), Some(m)) = (
                    e.args.get(2).and_then(Value::as_list),
                    e.args.get(3).and_then(Value::as_ref),
                ) {
                    for p in props
                        .iter()
                        .filter_map(Value::as_ref)
                        .filter_map(|r| file.entities.get(&r))
                    {
                        if p.args.first().and_then(Value::as_str) == Some("MassDensity")
                            && let Some(v) = p.args.get(2).and_then(Value::as_f64)
                            && v.is_finite()
                            && v > 0.
                        {
                            density.insert(m, v);
                        }
                    }
                }
            }
            // IFC2x3: MassDensity is the fourth attribute, kg/m³.
            "IFCGENERALMATERIALPROPERTIES" => {
                if let (Some(m), Some(v)) = (
                    e.args.first().and_then(Value::as_ref),
                    e.args.get(3).and_then(Value::as_f64),
                ) && (100.0..=30000.).contains(&v)
                {
                    density.insert(m, v);
                }
            }
            _ => {}
        }
    }
    let mut psets: BTreeMap<u64, Vec<(String, PropertyValue)>> = BTreeMap::new();
    for e in file
        .entities
        .values()
        .filter(|e| e.kind == "IFCRELDEFINESBYPROPERTIES")
    {
        let (Some(objects), Some(set)) = (
            e.args.get(4).and_then(Value::as_list),
            e.args.get(5).and_then(Value::as_ref),
        ) else {
            continue;
        };
        let Ok(set) = reader.get(set) else { continue };
        if set.kind != "IFCPROPERTYSET" {
            continue;
        }
        let set_name = set.args.get(2).and_then(Value::as_str).unwrap_or("");
        // Derived from the model on export; nothing to keep on import.
        if set_name == GOST_PSET_NAME {
            continue;
        }
        let mut values = Vec::new();
        for p in set
            .args
            .get(4)
            .and_then(Value::as_list)
            .unwrap_or(&[])
            .iter()
            .filter_map(Value::as_ref)
        {
            let Some(p) = file.entities.get(&p) else {
                continue;
            };
            // Enumerations, lists and tables of other programs carry no
            // model data here.
            if p.kind != "IFCPROPERTYSINGLEVALUE" {
                continue;
            }
            let (Some(name), Some(value)) = (
                p.args.first().and_then(Value::as_str),
                p.args.get(2).and_then(property_value),
            ) else {
                continue;
            };
            let key = if set_name == PSET_NAME {
                name.to_string()
            } else {
                format!("{set_name}.{name}")
            };
            values.push((key, value));
        }
        for o in objects.iter().filter_map(Value::as_ref) {
            psets.entry(o).or_default().extend(values.clone());
        }
    }
    let mut openings: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    for e in file
        .entities
        .values()
        .filter(|e| e.kind == "IFCRELVOIDSELEMENT")
    {
        if let (Some(element), Some(opening)) = (
            e.args.get(4).and_then(Value::as_ref),
            e.args.get(5).and_then(Value::as_ref),
        ) {
            openings.entry(element).or_default().push(opening);
        }
    }
    let mut materials: BTreeMap<u64, ObjectId> = BTreeMap::new();
    let mut element_of: BTreeMap<u64, ObjectId> = BTreeMap::new();
    let mut seen_guids = BTreeSet::new();
    for (id, e) in &file.entities {
        let kind_name = base_kind(&e.kind);
        let (kind, is_plate) = match kind_name {
            "IFCBEAM" | "IFCMEMBER" => (MemberKind::Beam, false),
            "IFCCOLUMN" => (MemberKind::Column, false),
            "IFCPLATE" => (MemberKind::Beam, true),
            _ => continue,
        };
        let name = e
            .args
            .get(2)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let label = format!("{kind_name} #{id} «{name}»");
        let result = (|| -> Result<(), String> {
            let placement = reader.local_placement(reader.reference(e, 5)?, 0)?;
            let (shape, solid, depth, profile_name) = reader.body(e, &mut report.issues, &label)?;
            let world = placement.then(&solid);
            let type_entity = type_of.get(id).and_then(|t| file.entities.get(t));
            let material_ref = material_of
                .get(id)
                .or_else(|| type_of.get(id).and_then(|t| material_of.get(t)));
            // Names that may designate the section, most specific first.
            let section_entity = section_of
                .get(id)
                .or_else(|| type_of.get(id).and_then(|t| section_of.get(t)))
                .and_then(|s| file.entities.get(s));
            let mut names: Vec<String> = Vec::new();
            names.extend(profile_name.clone());
            names.extend(
                section_entity
                    .and_then(|s| s.args.get(1))
                    .and_then(Value::as_str)
                    .map(str::to_string),
            );
            names.extend(
                type_entity
                    .and_then(|t| t.args.get(2))
                    .and_then(Value::as_str)
                    .map(str::to_string),
            );
            names.extend(e.args.get(4).and_then(Value::as_str).map(str::to_string));
            names.push(name.clone());
            names.retain(|n| !n.trim().is_empty());
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            let source_name = names
                .first()
                .map(|n| n.to_string())
                .unwrap_or_else(|| "без имени".into());
            let material = match material_ref {
                Some(m) => match materials.get(m) {
                    Some(id) => *id,
                    None => {
                        let mat = reader.get(*m)?;
                        let mname = mat
                            .args
                            .first()
                            .and_then(Value::as_str)
                            .unwrap_or("Материал IFC");
                        let gost = mapping::steel(ObjectId(project.next_id), mname);
                        let final_name = gost.as_ref().map_or(mname, |g| g.name.as_str());
                        if final_name != mname {
                            report
                                .materials
                                .insert(mname.to_string(), final_name.to_string());
                        }
                        let existing = project
                            .materials
                            .values()
                            .find(|x| x.name == final_name)
                            .map(|x| x.id);
                        let mid = match existing {
                            Some(x) => x,
                            None => {
                                let mid = project.allocate_id().map_err(|e| e.to_string())?;
                                let material = match gost {
                                    Some(mut g) => {
                                        g.id = mid;
                                        g
                                    }
                                    None => {
                                        let d = density.get(m).copied().unwrap_or_else(|| {
                                            let d = mapping::density(mname);
                                            report.issues.push(format!("материал «{mname}»: плотность не задана, принято {d} кг/м³"));
                                            d
                                        });
                                        Material {
                                            id: mid,
                                            name: mname.into(),
                                            density_kg_m3: d,
                                        }
                                    }
                                };
                                project.materials.insert(mid, material);
                                mid
                            }
                        };
                        materials.insert(*m, mid);
                        mid
                    }
                },
                None => {
                    report.issues.push(format!(
                        "{label}: материал не задан, принят первый из каталога"
                    ));
                    *project.materials.keys().next().ok_or("нет материалов")?
                }
            };
            let element_id = project.allocate_id().map_err(|e| e.to_string())?;
            let thickness = |d: f64| LengthMm::new(d).map_err(|e| e.to_string());
            // An outline nothing recognises keeps its geometry as a prism.
            let prism = |contour: Vec<[f64; 2]>,
                         report: &mut ImportReport|
             -> Result<ElementShape, String> {
                report.unmatched_profiles.insert(source_name.clone());
                report.issues.push(format!(
                    "{label}: сечение «{source_name}» не распознано, элемент импортирован как пластина по контуру"
                ));
                Ok(ElementShape::Plate {
                    contour,
                    thickness: thickness(depth)?,
                    frame: world.frame(),
                })
            };
            let mut catalogue =
                |entry: &library::Entry, project: &mut Project| -> Result<ObjectId, String> {
                    if entry.name != source_name {
                        report
                            .profiles
                            .insert(source_name.clone(), entry.name.clone());
                    }
                    match project.profiles.values().find(|p| p.name == entry.name) {
                        Some(p) => Ok(p.id),
                        None => {
                            let pid = project.allocate_id().map_err(|e| e.to_string())?;
                            project.profiles.insert(pid, entry.profile(pid));
                            Ok(pid)
                        }
                    }
                };
            let member = |profile, frame: Transform| ElementShape::Member {
                member_kind: kind,
                profile,
                length: LengthMm::new(depth).expect("checked positive"),
                frame: frame.frame(),
                position: Default::default(),
            };
            let shape = match (is_plate, shape) {
                (true, Shape::Contour(contour, voids)) => {
                    if !voids.is_empty() {
                        report.issues.push(format!(
                            "{label}: вырезы внутри контура пластины не импортированы"
                        ));
                    }
                    ElementShape::Plate {
                        contour,
                        thickness: thickness(depth)?,
                        frame: world.frame(),
                    }
                }
                (true, Shape::Profile(ProfileShape::Rectangle { width, height })) => {
                    ElementShape::Plate {
                        contour: vec![
                            [-width / 2., -height / 2.],
                            [width / 2., -height / 2.],
                            [width / 2., height / 2.],
                            [-width / 2., height / 2.],
                        ],
                        thickness: thickness(depth)?,
                        frame: world.frame(),
                    }
                }
                (true, Shape::Circle(r)) => ElementShape::Plate {
                    contour: regular_polygon(r),
                    thickness: thickness(depth)?,
                    frame: world.frame(),
                },
                (false, Shape::Profile(profile_shape)) => {
                    match mapping::section(&names, &profile_shape) {
                        Some((entry, _)) => member(catalogue(&entry, &mut project)?, world),
                        None => {
                            if !matches!(profile_shape, ProfileShape::Rectangle { .. }) {
                                report.unmatched_profiles.insert(source_name.clone());
                            }
                            let profile = match project
                                .profiles
                                .values()
                                .find(|p| p.shape == profile_shape)
                            {
                                Some(p) => p.id,
                                None => {
                                    let pid = project.allocate_id().map_err(|e| e.to_string())?;
                                    project.profiles.insert(
                                        pid,
                                        Profile {
                                            id: pid,
                                            name: profile_name
                                                .clone()
                                                .unwrap_or_else(|| "Профиль IFC".into()),
                                            shape: profile_shape,
                                            source: "Импорт IFC".into(),
                                            standard: None,
                                            area_mm2: None,
                                        },
                                    );
                                    pid
                                }
                            };
                            member(profile, world)
                        }
                    }
                }
                (false, Shape::Contour(contour, voids)) => {
                    match mapping::outline(&names, &contour, &voids) {
                        Some(found) => {
                            let o = found.orientation;
                            let turn = |v: [f64; 2]| {
                                let p = o.apply(v);
                                world.rotate([p[0], p[1], 0.])
                            };
                            let z = world.axes[2];
                            let sign = if o.mirror { -1. } else { 1. };
                            let mut origin = world.apply([found.centre[0], found.centre[1], 0.]);
                            if o.mirror {
                                origin = std::array::from_fn(|i| origin[i] + z[i] * depth);
                            }
                            let frame = Transform {
                                origin,
                                axes: [turn([1., 0.]), turn([0., 1.]), z.map(|v| v * sign)],
                            };
                            match (found.entry, found.rectangle) {
                                (Some(entry), _) => member(catalogue(&entry, &mut project)?, frame),
                                (None, Some((width, height))) => {
                                    let profile_shape = ProfileShape::Rectangle { width, height };
                                    let profile = match project
                                        .profiles
                                        .values()
                                        .find(|p| p.shape == profile_shape)
                                    {
                                        Some(p) => p.id,
                                        None => {
                                            let pid =
                                                project.allocate_id().map_err(|e| e.to_string())?;
                                            project.profiles.insert(
                                                pid,
                                                Profile {
                                                    id: pid,
                                                    name: format!(
                                                        "{}×{}",
                                                        width.round(),
                                                        height.round()
                                                    ),
                                                    shape: profile_shape,
                                                    source: "Импорт IFC".into(),
                                                    standard: None,
                                                    area_mm2: None,
                                                },
                                            );
                                            pid
                                        }
                                    };
                                    member(profile, frame)
                                }
                                (None, None) => prism(contour, &mut report)?,
                            }
                        }
                        None => prism(contour, &mut report)?,
                    }
                }
                (false, Shape::Circle(r)) => prism(regular_polygon(r), &mut report)?,
                (true, Shape::Profile(_)) => {
                    return Err("сочетание сущности и профиля не поддерживается".into());
                }
            };
            let mut properties: BTreeMap<String, PropertyValue> =
                psets.remove(id).unwrap_or_default().into_iter().collect();
            if let Some(g) = e.args.first().and_then(Value::as_str) {
                if valid_guid(g) && seen_guids.insert(g.to_string()) {
                    properties.insert(GUID_PROPERTY.into(), PropertyValue::Text(g.into()));
                } else {
                    report
                        .issues
                        .push(format!("{label}: неверный или повторный GUID «{g}»"));
                }
            }
            // A member keeps its class on the next export.
            if kind_name == "IFCMEMBER" {
                properties.insert(
                    IFC_ENTITY_PROPERTY.into(),
                    PropertyValue::Text("IfcMember".into()),
                );
            }
            // Openings are placed in the element's own frame.
            let part_frame = match &shape {
                ElementShape::Member { frame, .. } | ElementShape::Plate { frame, .. } => {
                    Transform {
                        origin: frame.origin.0,
                        axes: frame.axes,
                    }
                }
            };
            project.elements.insert(
                element_id,
                Element {
                    id: element_id,
                    name: name.clone(),
                    material,
                    shape,
                    properties,
                },
            );
            element_of.insert(*id, element_id);
            *report.imported.entry(kind_name.to_string()).or_default() += 1;
            let world = part_frame;
            for opening in openings.get(id).into_iter().flatten() {
                let o = reader.get(*opening)?;
                let olabel = format!("IFCOPENINGELEMENT #{opening}");
                let place = match reader.local_placement(reader.reference(o, 5)?, 0) {
                    Ok(p) => p,
                    Err(err) => {
                        report.issues.push(format!("{olabel}: {err}"));
                        continue;
                    }
                };
                let (oshape, osolid, odepth, _) = match reader.body(o, &mut report.issues, &olabel)
                {
                    Ok(b) => b,
                    Err(err) => {
                        report.issues.push(format!("{olabel}: {err}"));
                        continue;
                    }
                };
                let oworld = place.then(&osolid);
                // Opening frame in the element's local coordinates.
                let local = Transform {
                    origin: world.inverse_point(oworld.origin),
                    axes: oworld.axes.map(|a| world.axes.map(|b| dot(a, b))),
                };
                let kind = match oshape {
                    Shape::Circle(r) => DetailKind::Hole {
                        part: element_id,
                        frame: local.frame(),
                        diameter: 2. * r,
                        depth: odepth,
                    },
                    Shape::Profile(ProfileShape::Rectangle { width, height })
                        if (local.axes[0][0] - 1.).abs() < 1e-9
                            && (local.axes[2][2] - 1.).abs() < 1e-9 =>
                    {
                        DetailKind::BoxCut {
                            part: element_id,
                            origin: Point3([
                                local.origin[0] - width / 2.,
                                local.origin[1] - height / 2.,
                                local.origin[2],
                            ]),
                            size: [width, height, odepth],
                        }
                    }
                    // A block covering the whole part beyond its base plane
                    // is a plane cut (that is how plane cuts are exported).
                    Shape::Profile(ProfileShape::Rectangle { width, height })
                        if covers(&project, element_id, &local, width, height, odepth) =>
                    {
                        let z = local.axes[2];
                        DetailKind::PlaneCut {
                            part: element_id,
                            normal: z,
                            offset: dot(local.origin, z),
                        }
                    }
                    Shape::Profile(ProfileShape::Rectangle { width, height }) => {
                        DetailKind::PrismCut {
                            part: element_id,
                            frame: local.frame(),
                            contour: vec![
                                [-width / 2., -height / 2.],
                                [width / 2., -height / 2.],
                                [width / 2., height / 2.],
                                [-width / 2., height / 2.],
                            ],
                            length: odepth,
                        }
                    }
                    Shape::Contour(contour, _) => DetailKind::PrismCut {
                        part: element_id,
                        frame: local.frame(),
                        contour,
                        length: odepth,
                    },
                    _ => {
                        report
                            .issues
                            .push(format!("{olabel}: форма проёма не поддерживается"));
                        continue;
                    }
                };
                let did = project.allocate_id().map_err(|e| e.to_string())?;
                project.details.insert(did, Detail { id: did, kind });
                *report
                    .imported
                    .entry("IFCOPENINGELEMENT".into())
                    .or_default() += 1;
            }
            Ok(())
        })();
        if let Err(error) = result {
            report.skipped += 1;
            report.issues.push(format!("{label}: пропущен — {error}"));
        }
    }
    prune_invalid(&mut project, &mut report);
    assemblies(
        &file,
        &parts_of,
        &element_of,
        &mut psets,
        &mut project,
        &mut report,
    )?;
    // Unsupported building elements, in words.
    for (kind, count) in &report.unsupported_entities {
        if let Some(word) = element_word(kind) {
            *report.not_imported.entry(word.to_string()).or_default() += count;
        }
    }
    project
        .validate()
        .map_err(|e| format!("Импортированная модель не прошла проверку: {e}"))?;
    Ok((project, report))
}

/// Drop elements the model rejects (degenerate outlines, out-of-range
/// sizes) one by one instead of failing the whole file.
fn prune_invalid(project: &mut Project, report: &mut ImportReport) {
    let ids: Vec<ObjectId> = project.elements.keys().copied().collect();
    for id in ids {
        let mut single = project.clone();
        single.elements.retain(|k, _| *k == id);
        single.details.retain(|_, d| d.kind.part() == Some(id));
        single.assemblies.clear();
        if let Err(error) = single.validate() {
            let name = project.elements[&id].name.clone();
            project.elements.remove(&id);
            project.details.retain(|_, d| d.kind.part() != Some(id));
            report.skipped += 1;
            report.issues.push(format!("«{name}»: пропущен — {error}"));
        }
    }
}

/// Assemblies of the file (IfcElementAssembly aggregating parts) become
/// model assemblies; the longest part is the main one.
fn assemblies(
    file: &File,
    parts_of: &BTreeMap<u64, Vec<u64>>,
    element_of: &BTreeMap<u64, ObjectId>,
    psets: &mut BTreeMap<u64, Vec<(String, PropertyValue)>>,
    project: &mut Project,
    report: &mut ImportReport,
) -> Result<(), String> {
    let mut owned = BTreeSet::new();
    let mut assembly_of: BTreeMap<u64, ObjectId> = BTreeMap::new();
    for (whole, parts) in parts_of {
        let members: BTreeSet<ObjectId> = parts
            .iter()
            .filter_map(|p| element_of.get(p))
            .copied()
            .filter(|id| project.elements.contains_key(id) && owned.insert(*id))
            .collect();
        let Some(main_part) = members.iter().copied().max_by(|a, b| {
            let length = |id: &ObjectId| match &project.elements[id].shape {
                ElementShape::Member { length, .. } => length.value(),
                ElementShape::Plate { .. } => 0.,
            };
            length(a).total_cmp(&length(b)).then(b.cmp(a))
        }) else {
            continue;
        };
        let e = &file.entities[whole];
        let name = [2, 4]
            .iter()
            .find_map(|i| e.args.get(*i).and_then(Value::as_str))
            .filter(|n| !n.trim().is_empty())
            .unwrap_or("Сборка")
            .to_string();
        let mut properties: BTreeMap<String, PropertyValue> = psets
            .remove(whole)
            .unwrap_or_default()
            .into_iter()
            .collect();
        if let Some(g) = e.args.first().and_then(Value::as_str)
            && valid_guid(g)
        {
            properties.insert(GUID_PROPERTY.into(), PropertyValue::Text(g.into()));
        }
        let id = project.allocate_id().map_err(|e| e.to_string())?;
        project.assemblies.insert(
            id,
            open_bim_model::Assembly {
                id,
                name,
                main_part,
                members,
                children: BTreeSet::new(),
                properties,
            },
        );
        assembly_of.insert(*whole, id);
        *report
            .imported
            .entry("IFCELEMENTASSEMBLY".into())
            .or_default() += 1;
    }
    // Nested assemblies.
    let mut child_owned = BTreeSet::new();
    for (whole, parts) in parts_of {
        let Some(parent) = assembly_of.get(whole) else {
            continue;
        };
        let children: Vec<ObjectId> = parts
            .iter()
            .filter_map(|p| assembly_of.get(p))
            .copied()
            .filter(|c| c != parent && child_owned.insert(*c))
            .collect();
        if let Some(a) = project.assemblies.get_mut(parent) {
            a.children.extend(children);
        }
    }
    // A cycle in a hostile file would fail validation: drop nesting then.
    if project.validate().is_err() {
        for a in project.assemblies.values_mut() {
            a.children.clear();
        }
    }
    Ok(())
}
