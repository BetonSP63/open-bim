use crate::{ElementShape, ObjectId, Point3, Project, PropertyValue};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelError {
    DuplicateId(ObjectId),
    MissingReference(ObjectId),
    InvalidValue(String),
    OwnershipCycle(ObjectId),
    MultipleOwners(ObjectId),
    Serialization(String),
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateId(id) => write!(f, "Повторный или неверный ID: {id}"),
            Self::MissingReference(id) => write!(f, "Объект по ссылке не найден: {id}"),
            Self::InvalidValue(message) => f.write_str(message),
            Self::OwnershipCycle(id) => write!(f, "Цикл владения сборками: {id}"),
            Self::MultipleOwners(id) => write!(f, "Объект входит в несколько сборок: {id}"),
            Self::Serialization(message) => write!(f, "Ошибка данных проекта: {message}"),
        }
    }
}
impl std::error::Error for ModelError {}

pub(crate) fn valid_point(point: Point3) -> Result<(), ModelError> {
    if !point.0.iter().all(|v| v.is_finite() && v.abs() <= 1e9) {
        return Err(ModelError::InvalidValue(
            "Координаты вне допустимого диапазона".into(),
        ));
    }
    Ok(())
}

fn text(value: &str) -> Result<(), ModelError> {
    if value.len() > 16384 || value.contains('\0') {
        return Err(ModelError::InvalidValue("Недопустимый текст".into()));
    }
    Ok(())
}

fn properties(values: &BTreeMap<String, PropertyValue>) -> Result<(), ModelError> {
    if values.len() > 1024 {
        return Err(ModelError::InvalidValue("Слишком много свойств".into()));
    }
    for (key, value) in values {
        text(key)?;
        match value {
            PropertyValue::Text(value) => text(value)?,
            PropertyValue::Number(value) if !value.is_finite() => {
                return Err(ModelError::InvalidValue(
                    "Нечисловое значение свойства".into(),
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Independent polygon validation and shoelace area, without CAD tessellation.
pub fn polygon_area(points: &[[f64; 2]]) -> Result<f64, ModelError> {
    let error =
        || ModelError::InvalidValue("Вырожденный или самопересекающийся контур плиты".into());
    if points.len() < 3
        || points.len() > 10000
        || !points
            .iter()
            .flatten()
            .all(|v| v.is_finite() && v.abs() <= 1e7)
    {
        return Err(error());
    }
    let orient = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    let contact = |a: [f64; 2], b: [f64; 2], p: [f64; 2]| {
        orient(a, b, p).abs() < 1e-8
            && p[0] >= a[0].min(b[0]) - 1e-4
            && p[0] <= a[0].max(b[0]) + 1e-4
            && p[1] >= a[1].min(b[1]) - 1e-4
            && p[1] <= a[1].max(b[1]) + 1e-4
    };
    let mut area = 0.0;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        if (b[0] - a[0]).hypot(b[1] - a[1]) <= 0.001 {
            return Err(error());
        }
        area += a[0] * b[1] - a[1] * b[0];
        for j in i + 1..points.len() {
            if j == i + 1 || (i == 0 && j == points.len() - 1) {
                continue;
            }
            let c = points[j];
            let d = points[(j + 1) % points.len()];
            if (orient(a, b, c) * orient(a, b, d) < 0.0 && orient(c, d, a) * orient(c, d, b) < 0.0)
                || contact(a, b, c)
                || contact(a, b, d)
                || contact(c, d, a)
                || contact(c, d, b)
            {
                return Err(error());
            }
        }
    }
    if area.abs() < 1e-8 {
        return Err(error());
    }
    Ok(area.abs() / 2.0)
}

impl Project {
    pub fn validate(&self) -> Result<(), ModelError> {
        text(&self.name)?;
        valid_point(self.origin)?;
        self.document.validate()?;
        self.sections.validate()?;
        if self.model_views.len() > 256 {
            return Err(ModelError::InvalidValue(
                "Слишком много видов модели".into(),
            ));
        }
        for view in &self.model_views {
            view.validate()?;
        }
        // Changes of each set are numbered 1, 2, 3… in order (7.3.1).
        for mark in crate::SET_MARKS {
            let numbers: Vec<u32> = self
                .changes
                .iter()
                .filter(|c| c.mark == mark)
                .map(|c| c.number)
                .collect();
            if numbers.iter().zip(1..).any(|(n, k)| *n != k) {
                return Err(ModelError::InvalidValue(format!(
                    "Изменения комплекта {mark}: номера не по порядку"
                )));
            }
        }
        if self.changes.len() > 10_000 {
            return Err(ModelError::InvalidValue("Слишком много изменений".into()));
        }
        for change in &self.changes {
            change.validate(|id| self.drawings.get(&id).map(|d| d.sheet_mm()))?;
        }
        let mut ids = BTreeSet::new();
        let mut register = |key: ObjectId, id: ObjectId| {
            if key != id || id.0 == 0 || !ids.insert(id) {
                Err(ModelError::DuplicateId(id))
            } else {
                Ok(())
            }
        };
        let mut generated = BTreeSet::new();
        for (key, connection) in &self.connections {
            register(*key, connection.id)?;
            connection.template.validate()?;
            connection.validate_child_roles(self)?;
            text(&connection.general.class)?;
            text(&connection.general.code)?;
            if connection.general.class.len() > 256 || connection.general.code.len() > 256 {
                return Err(ModelError::InvalidValue(
                    "Слишком длинный класс или код узла".into(),
                ));
            }
            for parent in std::iter::once(connection.primary).chain(connection.secondary) {
                if !self.elements.contains_key(&parent) {
                    return Err(ModelError::MissingReference(parent));
                }
            }
            if connection.secondary == Some(connection.primary) {
                return Err(ModelError::InvalidValue(
                    "Родители соединения должны различаться".into(),
                ));
            }
            for (role, child) in &connection.children {
                text(role)?;
                if role.is_empty() || !generated.insert(*child) {
                    return Err(ModelError::MultipleOwners(*child));
                }
                if !self.elements.contains_key(child)
                    && !self.details.contains_key(child)
                    && !self.profiles.contains_key(child)
                {
                    return Err(ModelError::MissingReference(*child));
                }
            }
        }
        // Initial built-in connections attach to source parts. Generated parts
        // cannot become parents until a dependency graph is explicitly supported.
        for connection in self.connections.values() {
            if generated.contains(&connection.primary)
                || connection
                    .secondary
                    .is_some_and(|id| generated.contains(&id))
            {
                return Err(ModelError::OwnershipCycle(connection.id));
            }
        }
        for (key, value) in &self.materials {
            register(*key, value.id)?;
            text(&value.name)?;
            if !value.density_kg_m3.is_finite()
                || value.density_kg_m3 <= 0.0
                || value.density_kg_m3 > 1e6
            {
                return Err(ModelError::InvalidValue("Плотность материала".into()));
            }
        }
        for (key, value) in &self.profiles {
            register(*key, value.id)?;
            text(&value.name)?;
            text(&value.source)?;
            value.area_mm2()?;
        }
        for (key, value) in &self.elements {
            register(*key, value.id)?;
            text(&value.name)?;
            properties(&value.properties)?;
            if !self.materials.contains_key(&value.material) {
                return Err(ModelError::MissingReference(value.material));
            }
            value.shape.frame().validate()?;
            match &value.shape {
                ElementShape::Member {
                    profile,
                    length,
                    position,
                    ..
                } => {
                    position.validate()?;
                    if generated.contains(profile) && !generated.contains(&value.id) {
                        return Err(ModelError::InvalidValue("Профиль принадлежит соединению; используйте самостоятельный профиль или копию шаблона узла".into()));
                    }
                    if !self.profiles.contains_key(profile) {
                        return Err(ModelError::MissingReference(*profile));
                    }
                    if length.value() <= 0.001 || length.value() > 1e7 {
                        return Err(ModelError::InvalidValue("Длина элемента".into()));
                    }
                    valid_point(
                        value
                            .shape
                            .frame()
                            .world(Point3([0.0, 0.0, length.value()])),
                    )?;
                }
                ElementShape::Plate {
                    contour, thickness, ..
                } => {
                    polygon_area(contour)?;
                    if thickness.value() <= 0.001 || thickness.value() > 1e7 {
                        return Err(ModelError::InvalidValue("Толщина плиты".into()));
                    }
                }
            }
        }
        for (id, swept) in &self.swept_parts {
            let element = self
                .elements
                .get(id)
                .ok_or(ModelError::MissingReference(*id))?;
            swept.validate(&element.shape)?;
            if let ElementShape::Member { profile, .. } = &element.shape {
                let profile = self
                    .profiles
                    .get(profile)
                    .ok_or(ModelError::MissingReference(*profile))?;
                crate::swept::validate_swept_profile(profile)?;
            }
            self.element_body(*id)?;
        }
        for (key, value) in &self.axes {
            register(*key, value.id)?;
            text(&value.name)?;
            valid_point(value.start)?;
            valid_point(value.end)?;
            if value.start.distance(value.end) <= 0.001 {
                return Err(ModelError::InvalidValue("Нулевая длина оси".into()));
            }
        }
        for (key, value) in &self.levels {
            register(*key, value.id)?;
            text(&value.name)?;
            valid_point(Point3([0.0, 0.0, value.elevation_mm]))?;
        }
        for (key, value) in &self.details {
            if value
                .kind
                .part()
                .is_some_and(|id| self.swept_parts.contains_key(&id))
            {
                return Err(ModelError::InvalidValue(
                    "Отверстия и вырезы гнутых деталей пока не поддерживаются".into(),
                ));
            }
            register(*key, value.id)?;
            value.kind.validate(self)?;
        }
        for (key, value) in &self.drawings {
            register(*key, value.id)?;
            value.validate()?;
        }
        for (key, value) in &self.construction {
            register(*key, value.id)?;
            value.validate()?;
        }
        for (key, value) in &self.dimensions {
            register(*key, value.id)?;
            value.validate(self)?;
        }
        let mut plugin_outputs = BTreeSet::new();
        for (key, value) in &self.plugins {
            register(*key, value.id)?;
            value.validate(self)?;
            for id in &value.outputs {
                if !plugin_outputs.insert(*id) {
                    return Err(ModelError::MultipleOwners(*id));
                }
            }
        }
        for (key, value) in &self.families {
            value.validate()?;
            if *key != value.name {
                return Err(ModelError::InvalidValue(format!(
                    "Семейство «{}» записано под именем «{key}»",
                    value.name
                )));
            }
        }
        let mut family_parts = BTreeSet::new();
        for (key, value) in &self.family_instances {
            register(*key, value.id)?;
            value.validate(self)?;
            for id in value.children.values() {
                if !family_parts.insert(*id) {
                    return Err(ModelError::MultipleOwners(*id));
                }
            }
        }
        let mut owners = BTreeMap::new();
        for (key, value) in &self.assemblies {
            register(*key, value.id)?;
            text(&value.name)?;
            properties(&value.properties)?;
            if !value.members.contains(&value.main_part) {
                return Err(ModelError::InvalidValue(
                    "Главный элемент должен входить в сборку".into(),
                ));
            }
            for member in &value.members {
                if !self.elements.contains_key(member) {
                    return Err(ModelError::MissingReference(*member));
                }
                if owners.insert(*member, value.id).is_some() {
                    return Err(ModelError::MultipleOwners(*member));
                }
            }
            for child in &value.children {
                if !self.assemblies.contains_key(child) {
                    return Err(ModelError::MissingReference(*child));
                }
                if owners.insert(*child, value.id).is_some() {
                    return Err(ModelError::MultipleOwners(*child));
                }
            }
        }
        if ids.len() > 1_000_000
            || self.next_id == 0
            // Only the counter's own ID range must be free above it.
            || ids
                .range(ObjectId(self.next_id)..)
                .next()
                .is_some_and(|id| id.0 / crate::ID_BLOCK == self.next_id / crate::ID_BLOCK)
        {
            return Err(ModelError::InvalidValue(
                "Счётчик ID или количество объектов".into(),
            ));
        }
        // Iterative DFS avoids stack overflow on deeply nested imported assemblies.
        let mut colors = BTreeMap::<ObjectId, u8>::new();
        for root in self.assemblies.keys() {
            if colors.get(root) == Some(&2) {
                continue;
            }
            let mut stack = vec![(*root, false)];
            while let Some((id, exit)) = stack.pop() {
                if exit {
                    colors.insert(id, 2);
                    continue;
                }
                match colors.get(&id) {
                    Some(1) => return Err(ModelError::OwnershipCycle(id)),
                    Some(2) => continue,
                    _ => {}
                }
                colors.insert(id, 1);
                stack.push((id, true));
                for child in self.assemblies[&id].children.iter().rev() {
                    stack.push((*child, false));
                }
            }
        }
        Ok(())
    }
}
