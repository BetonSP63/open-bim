use crate::{Frame, GridAxis, LengthMm, Level, Material, ModelError, Point3, Profile};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObjectId(pub u64);

impl std::fmt::Display for ObjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Units {
    Millimetres,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberKind {
    Beam,
    Column,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ElementShape {
    Member {
        member_kind: MemberKind,
        profile: ObjectId,
        length: LengthMm,
        frame: Frame,
        /// «Положение»; `frame`/`length` are the resulting body axis.
        #[serde(default, skip_serializing_if = "crate::PartPosition::is_default")]
        position: crate::PartPosition,
    },
    Plate {
        contour: Vec<[f64; 2]>,
        thickness: LengthMm,
        frame: Frame,
    },
}

impl ElementShape {
    pub fn frame(&self) -> &Frame {
        match self {
            Self::Member { frame, .. } | Self::Plate { frame, .. } => frame,
        }
    }
    pub fn frame_mut(&mut self) -> &mut Frame {
        match self {
            Self::Member { frame, .. } | Self::Plate { frame, .. } => frame,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum PropertyValue {
    Text(String),
    Number(f64),
    Boolean(bool),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Element {
    pub id: ObjectId,
    pub name: String,
    pub material: ObjectId,
    pub shape: ElementShape,
    pub properties: BTreeMap<String, PropertyValue>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assembly {
    pub id: ObjectId,
    pub name: String,
    pub main_part: ObjectId,
    pub members: BTreeSet<ObjectId>,
    pub children: BTreeSet<ObjectId>,
    pub properties: BTreeMap<String, PropertyValue>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub name: String,
    pub units: Units,
    pub origin: Point3,
    pub revision: u64,
    pub next_id: u64,
    #[serde(deserialize_with = "unique_map")]
    pub materials: BTreeMap<ObjectId, Material>,
    #[serde(deserialize_with = "unique_map")]
    pub profiles: BTreeMap<ObjectId, Profile>,
    #[serde(deserialize_with = "unique_map")]
    pub elements: BTreeMap<ObjectId, Element>,
    #[serde(deserialize_with = "unique_map")]
    pub assemblies: BTreeMap<ObjectId, Assembly>,
    #[serde(deserialize_with = "unique_map")]
    pub axes: BTreeMap<ObjectId, GridAxis>,
    #[serde(deserialize_with = "unique_map")]
    pub levels: BTreeMap<ObjectId, Level>,
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "unique_map"
    )]
    pub details: BTreeMap<ObjectId, crate::Detail>,
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "unique_map"
    )]
    pub connections: BTreeMap<ObjectId, crate::ConnectionInstance>,
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "unique_map"
    )]
    pub drawings: BTreeMap<ObjectId, crate::Drawing>,
    /// Construction lines and circles (snap targets, never on drawings).
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "unique_map"
    )]
    pub construction: BTreeMap<ObjectId, crate::Construction>,
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "unique_map"
    )]
    pub dimensions: BTreeMap<ObjectId, crate::ModelDimension>,
    /// Placed plugins with their inputs and outputs.
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "unique_map"
    )]
    pub plugins: BTreeMap<ObjectId, crate::PluginInstance>,
    /// Main inscription attributes (ГОСТ Р 21.101-2026, appendix Е).
    #[serde(default, skip_serializing_if = "crate::DocumentInfo::is_empty")]
    pub document: crate::DocumentInfo,
    /// Changes of the issued sets in order (ГОСТ Р 21.101-2026, section 7).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<crate::Change>,
    /// Part positions and shipping marks fixed by the last numbering run
    /// («Нумеровать»), by object. Later runs keep these marks where the
    /// objects are still alike; new and changed objects get new numbers.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub numbering: BTreeMap<ObjectId, String>,
    /// Grid colour, label size and magnetic grid planes.
    #[serde(default, skip_serializing_if = "crate::GridSettings::is_default")]
    pub grid: crate::GridSettings,
    /// Phases, lots and sequences («Управление»).
    #[serde(default, skip_serializing_if = "crate::Management::is_default")]
    pub management: crate::Management,
    /// Analysis models, loads and analysis properties («Анализ»).
    #[serde(default, skip_serializing_if = "crate::Analysis::is_default")]
    pub analysis: crate::Analysis,
}

impl Default for Project {
    fn default() -> Self {
        let (materials, profiles) = crate::catalog::builtins();
        Self {
            name: "Новый проект".into(),
            units: Units::Millimetres,
            origin: Point3([0.0; 3]),
            revision: 0,
            next_id: 8,
            materials: materials.into_iter().map(|v| (v.id, v)).collect(),
            profiles: profiles.into_iter().map(|v| (v.id, v)).collect(),
            elements: BTreeMap::new(),
            assemblies: BTreeMap::new(),
            axes: BTreeMap::new(),
            levels: BTreeMap::new(),
            details: BTreeMap::new(),
            connections: BTreeMap::new(),
            drawings: BTreeMap::new(),
            construction: BTreeMap::new(),
            dimensions: BTreeMap::new(),
            plugins: BTreeMap::new(),
            document: crate::DocumentInfo::default(),
            changes: Vec::new(),
            numbering: BTreeMap::new(),
            grid: Default::default(),
            management: Default::default(),
            analysis: Default::default(),
        }
    }
}

impl Project {
    pub fn allocate_id(&mut self) -> Result<ObjectId, ModelError> {
        let id = ObjectId(self.next_id);
        let next = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| ModelError::InvalidValue("Исчерпан диапазон ID".into()))?;
        if id.0 == 0 || self.contains_id(id) {
            return Err(ModelError::DuplicateId(id));
        }
        self.next_id = next;
        Ok(id)
    }

    pub fn contains_id(&self, id: ObjectId) -> bool {
        self.materials.contains_key(&id)
            || self.profiles.contains_key(&id)
            || self.elements.contains_key(&id)
            || self.assemblies.contains_key(&id)
            || self.axes.contains_key(&id)
            || self.levels.contains_key(&id)
            || self.details.contains_key(&id)
            || self.connections.contains_key(&id)
            || self.drawings.contains_key(&id)
            || self.construction.contains_key(&id)
            || self.dimensions.contains_key(&id)
            || self.plugins.contains_key(&id)
    }

    pub fn element_volume_mm3(&self, id: ObjectId) -> Result<f64, ModelError> {
        if self.details.values().any(|d| d.kind.part() == Some(id)) {
            return self
                .element_body(id)?
                .volume_mm3()
                .map_err(|e| ModelError::InvalidValue(e.to_string()));
        }
        let element = self
            .elements
            .get(&id)
            .ok_or(ModelError::MissingReference(id))?;
        match &element.shape {
            ElementShape::Member {
                profile, length, ..
            } => Ok(self
                .profiles
                .get(profile)
                .ok_or(ModelError::MissingReference(*profile))?
                .area_mm2()?
                * length.value()),
            ElementShape::Plate {
                contour, thickness, ..
            } => Ok(crate::polygon_area(contour)? * thickness.value()),
        }
    }

    pub fn element_mass_kg(&self, id: ObjectId) -> Result<f64, ModelError> {
        let element = self
            .elements
            .get(&id)
            .ok_or(ModelError::MissingReference(id))?;
        let density = self
            .materials
            .get(&element.material)
            .ok_or(ModelError::MissingReference(element.material))?
            .density_kg_m3;
        if !density.is_finite() || density <= 0.0 {
            return Err(ModelError::InvalidValue("Плотность материала".into()));
        }
        Ok(self.element_volume_mm3(id)? / 1e9 * density)
    }

    pub fn to_json(&self) -> Result<String, ModelError> {
        self.validate()?;
        serde_json::to_string_pretty(self)
            .map_err(|error| ModelError::Serialization(error.to_string()))
    }

    pub fn from_json(json: &str) -> Result<Self, ModelError> {
        let project: Self = serde_json::from_str(json)
            .map_err(|error| ModelError::Serialization(error.to_string()))?;
        project.validate()?;
        Ok(project)
    }
}

fn unique_map<'de, D, T>(deserializer: D) -> Result<BTreeMap<ObjectId, T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Visitor<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Visitor<T> {
        type Value = BTreeMap<ObjectId, T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("map with unique object IDs")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut access: A,
        ) -> Result<Self::Value, A::Error> {
            let mut map = BTreeMap::new();
            while let Some((key, value)) = access.next_entry::<ObjectId, T>()? {
                if map.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom(format!(
                        "Duplicate object ID: {key}"
                    )));
                }
                if map.len() > 1_000_000 {
                    return Err(serde::de::Error::custom("Object count limit exceeded"));
                }
            }
            Ok(map)
        }
    }
    deserializer.deserialize_map(Visitor(std::marker::PhantomData))
}

/// Reproducible test structure: four columns, two beams and a contour plate.
pub fn example_steel_frame() -> Result<Project, ModelError> {
    let mut project = Project {
        name: "Контрольный стальной каркас".into(),
        ..Default::default()
    };
    for (i, (x, y)) in [(0.0, 0.0), (6000.0, 0.0), (0.0, 4000.0), (6000.0, 4000.0)]
        .into_iter()
        .enumerate()
    {
        let id = project.allocate_id()?;
        project.elements.insert(
            id,
            Element {
                id,
                name: format!("Колонна {}", i + 1),
                material: ObjectId(1),
                shape: ElementShape::Member {
                    member_kind: MemberKind::Column,
                    profile: ObjectId(2),
                    length: LengthMm::new(3000.0)
                        .map_err(|e| ModelError::InvalidValue(e.to_string()))?,
                    frame: Frame {
                        origin: Point3([x, y, 0.0]),
                        ..Default::default()
                    },
                    position: Default::default(),
                },
                properties: BTreeMap::new(),
            },
        );
    }
    for (i, y) in [0.0, 4000.0].into_iter().enumerate() {
        let (frame, length) = Frame::along(Point3([0.0, y, 3000.0]), Point3([6000.0, y, 3000.0]))?;
        let id = project.allocate_id()?;
        project.elements.insert(
            id,
            Element {
                id,
                name: format!("Балка {}", i + 1),
                material: ObjectId(1),
                shape: ElementShape::Member {
                    member_kind: MemberKind::Beam,
                    profile: ObjectId(2),
                    length,
                    frame,
                    position: Default::default(),
                },
                properties: BTreeMap::new(),
            },
        );
    }
    let id = project.allocate_id()?;
    project.elements.insert(
        id,
        Element {
            id,
            name: "Опорная пластина".into(),
            material: ObjectId(1),
            shape: ElementShape::Plate {
                contour: vec![
                    [-200.0, -200.0],
                    [200.0, -200.0],
                    [200.0, 200.0],
                    [-200.0, 200.0],
                ],
                thickness: LengthMm::new(20.0)
                    .map_err(|e| ModelError::InvalidValue(e.to_string()))?,
                frame: Frame::default(),
            },
            properties: BTreeMap::from([("Марка".into(), PropertyValue::Text("П-1".into()))]),
        },
    );
    let assembly_id = project.allocate_id()?;
    project.assemblies.insert(
        assembly_id,
        Assembly {
            id: assembly_id,
            name: "Колонна с опорной пластиной".into(),
            main_part: ObjectId(8),
            members: BTreeSet::from([ObjectId(8), id]),
            children: BTreeSet::new(),
            properties: BTreeMap::from([("Этап".into(), PropertyValue::Number(1.0))]),
        },
    );
    for (name, x) in [("А", 0.0), ("Б", 6000.0)] {
        let id = project.allocate_id()?;
        project.axes.insert(
            id,
            GridAxis {
                id,
                name: name.into(),
                start: Point3([x, -1000.0, 0.0]),
                end: Point3([x, 5000.0, 0.0]),
            },
        );
    }
    for (name, z) in [("Основание", 0.0), ("Верх", 3000.0)] {
        let id = project.allocate_id()?;
        project.levels.insert(
            id,
            Level {
                id,
                name: name.into(),
                elevation_mm: z,
            },
        );
    }
    project.validate()?;
    Ok(project)
}
