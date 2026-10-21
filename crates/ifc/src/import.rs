//! IFC import of the declared subset; everything else is listed in the report.
use crate::step::{Entity, File, Value, parse, valid_guid};
use crate::{GOST_PSET_NAME, GUID_PROPERTY, PSET_NAME};
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
];

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
    /// Profile shape (members) or contour (plates) plus its 2D position.
    fn profile(&self, e: &'a Entity) -> Result<(Shape, Transform, Option<String>), String> {
        let name = self.arg(e, 1).as_str().map(str::to_string);
        let position = match self.arg(e, 2).as_ref() {
            Some(r) if e.kind != "IFCARBITRARYCLOSEDPROFILEDEF" => {
                self.placement3d(self.get(r)?)?
            }
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
            "IFCARBITRARYCLOSEDPROFILEDEF" => {
                let curve = self.reference(e, 2)?;
                if curve.kind != "IFCPOLYLINE" {
                    return Err(format!("контур {} не поддерживается", curve.kind));
                }
                let points = self.arg(curve, 0).as_list().ok_or("пустая полилиния")?;
                if points.len() > 100_000 {
                    return Err("слишком много вершин контура".into());
                }
                let mut contour = Vec::new();
                for p in points {
                    let c =
                        self.coordinates(self.get(p.as_ref().ok_or("вершина не ссылка")?)?, true)?;
                    contour.push([c[0], c[1]]);
                }
                if contour.len() > 1 && contour.first() == contour.last() {
                    contour.pop();
                }
                Shape::Contour(contour)
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
            let items = self.arg(rep, 3).as_list().ok_or("пустое представление")?;
            if items.len() != 1 {
                return Err(format!(
                    "тело из {} элементов не поддерживается",
                    items.len()
                ));
            }
            let mut item = self.get(items[0].as_ref().ok_or("элемент не ссылка")?)?;
            let mut depth = 0;
            while item.kind == "IFCBOOLEANCLIPPINGRESULT" && depth < 32 {
                issues.push(format!("{label}: срезы IfcBooleanClippingResult не импортированы, взят исходный профиль"));
                item = self.reference(item, 1)?;
                depth += 1;
            }
            if item.kind != "IFCEXTRUDEDAREASOLID" {
                return Err(format!("тело {} не поддерживается", item.kind));
            }
            let (profile, profile_position, name) = self.profile(self.reference(item, 0)?)?;
            let position = match self.arg(item, 1).as_ref() {
                Some(r) => self.placement3d(self.get(r)?)?,
                None => Transform::IDENTITY,
            };
            let direction = unit(self.coordinates(self.reference(item, 2)?, false)?)
                .ok_or("нулевое направление")?;
            if (direction[2] - 1.).abs() > 1e-9 {
                return Err("выдавливание не по оси Z профиля не поддерживается".into());
            }
            let depth = self.length(self.arg(item, 3))?;
            return Ok((profile, position.then(&profile_position), depth, name));
        }
        Err("нет представления Body".into())
    }
}

enum Shape {
    Profile(ProfileShape),
    Contour(Vec<[f64; 2]>),
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

/// Parse IFC text into a new project; unsupported content goes to the report.
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

pub fn import(text: &str) -> Result<(Project, ImportReport), String> {
    let file = parse(text)?;
    let mut report = ImportReport {
        schema: file.schema.clone(),
        ..Default::default()
    };
    if !file
        .schema
        .iter()
        .any(|s| s.to_ascii_uppercase().starts_with("IFC4"))
    {
        return Err(format!(
            "Схема {:?} не поддерживается; ожидается IFC4",
            file.schema
        ));
    }
    if file
        .schema
        .iter()
        .any(|s| s.to_ascii_uppercase().starts_with("IFC4X3"))
    {
        report.issues.push(
            "IFC4X3 читается как IFC4: специфичные сущности 4.3 перечислены как неподдерживаемые"
                .into(),
        );
    }
    for e in file.entities.values() {
        if !KNOWN.contains(&e.kind.as_str()) {
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
    // Materials and densities by relationship.
    let mut material_of: BTreeMap<u64, u64> = BTreeMap::new();
    let mut density: BTreeMap<u64, f64> = BTreeMap::new();
    for e in file.entities.values() {
        match e.kind.as_str() {
            "IFCRELASSOCIATESMATERIAL" => {
                if let (Some(objects), Some(m)) = (
                    e.args.get(4).and_then(Value::as_list),
                    e.args.get(5).and_then(Value::as_ref),
                ) {
                    if reader.get(m).map(|x| x.kind.as_str()) != Ok("IFCMATERIAL") {
                        report.issues.push(format!(
                            "материал #{m}: {} не поддерживается",
                            reader.get(m).map(|x| x.kind.as_str()).unwrap_or("?")
                        ));
                        continue;
                    }
                    for o in objects.iter().filter_map(Value::as_ref) {
                        material_of.insert(o, m);
                    }
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
            if p.kind != "IFCPROPERTYSINGLEVALUE" {
                report
                    .issues
                    .push(format!("свойство {} не поддерживается", p.kind));
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
    let mut seen_guids = BTreeSet::new();
    for (id, e) in &file.entities {
        let kind = match e.kind.as_str() {
            "IFCBEAM" | "IFCMEMBER" => MemberKind::Beam,
            "IFCCOLUMN" => MemberKind::Column,
            "IFCPLATE" => MemberKind::Beam,
            _ => continue,
        };
        let name = e
            .args
            .get(2)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let label = format!("{} #{id} «{name}»", e.kind);
        let result = (|| -> Result<(), String> {
            let placement = reader.local_placement(reader.reference(e, 5)?, 0)?;
            let (shape, solid, depth, profile_name) = reader.body(e, &mut report.issues, &label)?;
            let world = placement.then(&solid);
            let material = match material_of.get(id) {
                Some(m) => match materials.get(m) {
                    Some(id) => *id,
                    None => {
                        let mat = reader.get(*m)?;
                        let mname = mat
                            .args
                            .first()
                            .and_then(Value::as_str)
                            .unwrap_or("Материал IFC");
                        let existing = project
                            .materials
                            .values()
                            .find(|x| x.name == mname)
                            .map(|x| x.id);
                        let mid = match existing {
                            Some(x) => x,
                            None => {
                                let mid = project.allocate_id().map_err(|e| e.to_string())?;
                                let d = density.get(m).copied().unwrap_or_else(|| {
                                    report.issues.push(format!("материал «{mname}»: плотность не задана, принято 7850 кг/м³"));
                                    7850.
                                });
                                project.materials.insert(
                                    mid,
                                    Material {
                                        id: mid,
                                        name: mname.into(),
                                        density_kg_m3: d,
                                    },
                                );
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
            let shape = match (e.kind.as_str(), shape) {
                ("IFCPLATE", Shape::Contour(contour)) => ElementShape::Plate {
                    contour,
                    thickness: LengthMm::new(depth).map_err(|e| e.to_string())?,
                    frame: world.frame(),
                },
                ("IFCPLATE", Shape::Profile(ProfileShape::Rectangle { width, height })) => {
                    ElementShape::Plate {
                        contour: vec![
                            [-width / 2., -height / 2.],
                            [width / 2., -height / 2.],
                            [width / 2., height / 2.],
                            [-width / 2., height / 2.],
                        ],
                        thickness: LengthMm::new(depth).map_err(|e| e.to_string())?,
                        frame: world.frame(),
                    }
                }
                (_, Shape::Profile(profile_shape)) if e.kind != "IFCPLATE" => {
                    let profile = match project.profiles.values().find(|p| p.shape == profile_shape)
                    {
                        Some(p) => p.id,
                        None => {
                            let pid = project.allocate_id().map_err(|e| e.to_string())?;
                            project.profiles.insert(
                                pid,
                                Profile {
                                    id: pid,
                                    name: profile_name.unwrap_or_else(|| "Профиль IFC".into()),
                                    shape: profile_shape,
                                    source: "Импорт IFC4".into(),
                                    standard: None,
                                    area_mm2: None,
                                },
                            );
                            pid
                        }
                    };
                    ElementShape::Member {
                        member_kind: kind,
                        profile,
                        length: LengthMm::new(depth).map_err(|e| e.to_string())?,
                        frame: world.frame(),
                        position: Default::default(),
                    }
                }
                _ => return Err("сочетание сущности и профиля не поддерживается".into()),
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
            *report.imported.entry(e.kind.clone()).or_default() += 1;
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
                    Shape::Contour(contour) => DetailKind::PrismCut {
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
            report.issues.push(format!("{label}: пропущен — {error}"));
        }
    }
    project
        .validate()
        .map_err(|e| format!("Импортированная модель не прошла проверку: {e}"))?;
    Ok((project, report))
}
