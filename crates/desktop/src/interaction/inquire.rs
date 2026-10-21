//! «Запросить объект» and «Сравнить детали»: what the model knows about a
//! picked object, and why two parts get different positions.
use open_bim_model::{DetailKind, ElementShape, MemberKind, ObjectId, Project, PropertyValue};
use serde_json::Value;

pub type Rows = Vec<(String, String)>;

/// Caption of a property key: known keys by name, others as «Свойство «key»».
fn caption(key: &str) -> String {
    match key {
        open_bim_reports::SURFACE_TREATMENT => "Обработка поверхности".into(),
        "class" => "Класс".into(),
        other => format!("Свойство «{other}»"),
    }
}

fn value(v: &PropertyValue) -> String {
    match v {
        PropertyValue::Text(t) => t.clone(),
        PropertyValue::Number(n) => open_bim_reports::fmt(*n),
        PropertyValue::Boolean(b) => if *b { "да" } else { "нет" }.into(),
    }
}

/// Report of one object: a part, a bolt group or weld, a component, an
/// assembly, an axis or a level.
pub fn inquire(project: &Project, id: ObjectId) -> Result<Rows, String> {
    let mut rows: Rows = vec![("ID".into(), id.0.to_string())];
    let mut row = |k: &str, v: String| rows.push((k.into(), v));
    if let Some(e) = project.elements.get(&id) {
        let numbering = open_bim_numbering::number(project).map_err(|e| e.to_string())?;
        row(
            "Тип",
            match &e.shape {
                ElementShape::Member {
                    member_kind: MemberKind::Beam,
                    ..
                } => "Балка",
                ElementShape::Member { .. } => "Колонна",
                ElementShape::Plate { .. } => "Пластина",
            }
            .into(),
        );
        row("Имя", e.name.clone());
        if let Some(p) = numbering.parts.get(&id) {
            row("Позиция", p.clone());
        }
        let (section, length) = open_bim_reports::section(project, id)?;
        row("Сечение", section);
        row("Длина, мм", open_bim_reports::fmt(length));
        if let Some(m) = project.materials.get(&e.material) {
            row("Материал", m.name.clone());
        }
        let mass = project.element_mass_kg(id).map_err(|e| e.to_string())?;
        row("Масса нетто, кг", format!("{mass:.2}"));
        let surface = project.element_surface_mm2(id).map_err(|e| e.to_string())?;
        row("Площадь поверхности, м²", format!("{:.3}", surface / 1e6));
        let o = e.shape.frame().origin.0;
        row("Начало, мм", o.map(open_bim_reports::fmt).join(" · "));
        let holes = project
            .details
            .values()
            .filter(|d| matches!(d.kind, DetailKind::Hole { part, .. } if part == id))
            .count();
        let cuts = project
            .details
            .values()
            .filter(|d| {
                matches!(d.kind, DetailKind::BoxCut { part, .. } | DetailKind::PlaneCut { part, .. } if part == id)
            })
            .count();
        row("Отверстий", holes.to_string());
        row("Вырезов и подрезок", cuts.to_string());
        let bolts = project
            .details
            .values()
            .filter(
                |d| matches!(&d.kind, DetailKind::BoltGroup { parts, .. } if parts.contains(&id)),
            )
            .count();
        let welds = project
            .details
            .values()
            .filter(|d| matches!(&d.kind, DetailKind::Weld { parts, .. } if parts.contains(&id)))
            .count();
        row("Групп болтов", bolts.to_string());
        row("Сварных швов", welds.to_string());
        if let Some(a) = project
            .assemblies
            .values()
            .find(|a| a.members.contains(&id))
        {
            let mark = numbering.assemblies.get(&a.id).cloned().unwrap_or_default();
            let main = if a.main_part == id {
                ", основная деталь"
            } else {
                ""
            };
            row("Сборка", format!("{mark} {}{main}", a.name).trim().into());
            if let Some(v) = a.properties.get(super::control_numbers::CONTROL_NUMBER) {
                row("Контрольный номер", value(v));
            }
        }
        for c in project.connections.values() {
            if c.children.values().any(|child| *child == id) {
                row(
                    "Создана узлом",
                    format!("{} (#{})", c.template.name, c.id.0),
                );
            } else if c.primary == id || c.secondary == Some(id) {
                row("Узел", format!("{} (#{})", c.template.name, c.id.0));
            }
        }
        for (k, v) in &e.properties {
            row(&caption(k), value(v));
        }
    } else if let Some(d) = project.details.get(&id) {
        match &d.kind {
            DetailKind::BoltGroup {
                parts,
                standard,
                grade,
                diameter,
                ..
            } => {
                row("Тип", "Группа болтов".into());
                row(
                    "Болт",
                    format!(
                        "M{} {standard} кл. {grade}",
                        open_bim_reports::fmt(*diameter)
                    ),
                );
                row("Болтов", d.kind.bolt_positions().len().to_string());
                row("Детали", ids(parts.iter()));
            }
            DetailKind::Weld { parts, path, .. } => {
                row("Тип", "Сварной шов".into());
                row(
                    "Номер шва",
                    project
                        .numbering
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(|| "не пронумерован".into()),
                );
                let length: f64 = path.windows(2).map(|w| w[0].distance(w[1])).sum();
                row("Длина, мм", open_bim_reports::fmt(length));
                row("Детали", ids(parts.iter()));
            }
            other => {
                row("Тип", "Обработка детали".into());
                if let Some(p) = other.part() {
                    row("Деталь", format!("#{}", p.0));
                }
            }
        }
    } else if let Some(c) = project.connections.get(&id) {
        row("Тип", "Узел".into());
        row("Имя", c.template.name.clone());
        row("Основная деталь", format!("#{}", c.primary.0));
        if let Some(s) = c.secondary {
            row("Присоединяемая деталь", format!("#{}", s.0));
        }
        row("Создано объектов", c.children.len().to_string());
        row(
            "Состояние",
            if c.children.is_empty() {
                "не создан"
            } else {
                "создан"
            }
            .into(),
        );
        if c.general.locked {
            row("Заблокирован", "да".into());
        }
    } else if let Some(a) = project.assemblies.get(&id) {
        row("Тип", "Сборка".into());
        row("Имя", a.name.clone());
        row("Деталей", a.members.len().to_string());
        row("Основная деталь", format!("#{}", a.main_part.0));
        let mut mass = 0.;
        for m in &a.members {
            mass += project.element_mass_kg(*m).map_err(|e| e.to_string())?;
        }
        row("Масса, кг", format!("{mass:.2}"));
        for (k, v) in &a.properties {
            let k = if k == super::control_numbers::CONTROL_NUMBER {
                "Контрольный номер".to_string()
            } else {
                caption(k)
            };
            row(&k, value(v));
        }
    } else if let Some(a) = project.axes.get(&id) {
        row("Тип", "Ось".into());
        row("Имя", a.name.clone());
    } else if let Some(l) = project.levels.get(&id) {
        row("Тип", "Уровень".into());
        row("Имя", l.name.clone());
    } else {
        return Err(format!("Объект #{} не найден", id.0));
    }
    Ok(rows)
}

fn ids<'a>(ids: impl Iterator<Item = &'a ObjectId>) -> String {
    ids.map(|i| format!("#{}", i.0))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Differences of two parts by the data numbering compares: material,
/// section, length or contour, holes and cuts, properties. Empty when
/// the parts get one position.
pub fn compare(
    project: &Project,
    a: ObjectId,
    b: ObjectId,
) -> Result<Vec<(String, String, String)>, String> {
    let key = |id| open_bim_numbering::part_key_data(project, id).map_err(|e| e.to_string());
    let (ka, kb) = (key(a)?, key(b)?);
    let mut out = Vec::new();
    let text = |v: &Value| match v {
        Value::Null => "—".to_string(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let mut diff = |label: &str, x: &Value, y: &Value| {
        if x != y {
            out.push((label.to_string(), text(x), text(y)));
        }
    };
    diff("Материал", &ka["material"]["name"], &kb["material"]["name"]);
    let (sa, sb) = (&ka["shape"], &kb["shape"]);
    if sa["kind"] != sb["kind"] {
        diff("Вид детали", &sa["kind"], &sb["kind"]);
    } else {
        diff("Сечение", &sa["profile"], &sb["profile"]);
        diff("Длина, мм", &sa["length"], &sb["length"]);
        diff("Контур", &sa["contour"], &sb["contour"]);
        diff("Толщина, мм", &sa["thickness"], &sb["thickness"]);
    }
    let features = |k: &Value| k["features"].as_array().map_or(0, Vec::len);
    if ka["features"] != kb["features"] {
        let (fa, fb) = (features(&ka), features(&kb));
        let (x, y) = if fa == fb {
            (format!("{fa}"), format!("{fb}, другое расположение"))
        } else {
            (fa.to_string(), fb.to_string())
        };
        diff("Отверстия и вырезы", &Value::from(x), &Value::from(y));
    }
    let empty = serde_json::Map::new();
    let (pa, pb) = (
        ka["properties"].as_object().unwrap_or(&empty),
        kb["properties"].as_object().unwrap_or(&empty),
    );
    let keys: std::collections::BTreeSet<&String> = pa.keys().chain(pb.keys()).collect();
    for k in keys {
        let label = caption(k);
        let get = |m: &serde_json::Map<String, Value>| {
            m.get(k)
                .map(|v| {
                    v.as_object()
                        .and_then(|o| o.get("value").or(o.values().next()))
                        .unwrap_or(v)
                        .clone()
                })
                .unwrap_or(Value::Null)
        };
        diff(&label, &get(pa), &get(pb));
    }
    Ok(out)
}

/// Result window of «Запросить объект» or «Сравнить детали».
#[derive(Clone, Debug, PartialEq)]
pub struct Inquiry {
    pub title: String,
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// Line under the table («Детали одинаковые …»).
    pub note: Option<String>,
}

/// «Запросить объект» for the first selected object.
pub fn inquire_selected(
    project: &Project,
    selected: &std::collections::BTreeSet<ObjectId>,
) -> Result<Inquiry, String> {
    let id = *selected
        .first()
        .ok_or("Выберите объект, затем «Запросить объект»")?;
    let rows = inquire(project, id)?;
    Ok(Inquiry {
        title: format!("Объект #{}", id.0),
        header: vec!["Свойство".into(), "Значение".into()],
        rows: rows.into_iter().map(|(k, v)| vec![k, v]).collect(),
        note: (selected.len() > 1)
            .then(|| format!("Показан первый из {} выбранных", selected.len())),
    })
}

/// «Сравнить детали» for exactly two selected parts.
pub fn compare_selected(
    project: &Project,
    selected: &std::collections::BTreeSet<ObjectId>,
) -> Result<Inquiry, String> {
    let parts: Vec<ObjectId> = selected
        .iter()
        .copied()
        .filter(|id| project.elements.contains_key(id))
        .collect();
    let [a, b] = parts[..] else {
        return Err("Выберите две детали, затем «Сравнить детали»".into());
    };
    let diff = compare(project, a, b)?;
    let note = if diff.is_empty() {
        "Детали одинаковые: при нумерации получат одну позицию".to_string()
    } else {
        format!("Отличий: {} — позиции разные", diff.len())
    };
    Ok(Inquiry {
        title: format!("Сравнение #{} и #{}", a.0, b.0),
        header: vec!["Отличие".into(), format!("#{}", a.0), format!("#{}", b.0)],
        rows: diff.into_iter().map(|(k, x, y)| vec![k, x, y]).collect(),
        note: Some(note),
    })
}

/// Angle at `vertex` between the directions to `a` and `b`, degrees.
pub fn angle_deg(
    vertex: open_bim_renderer::DVec3,
    a: open_bim_renderer::DVec3,
    b: open_bim_renderer::DVec3,
) -> Option<f64> {
    let (u, v) = (a - vertex, b - vertex);
    if u.length() < 1e-6 || v.length() < 1e-6 {
        return None;
    }
    Some(u.angle_between(v).to_degrees())
}
