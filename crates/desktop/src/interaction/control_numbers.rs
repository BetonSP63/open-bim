//! «Контрольные номера»: erection order numbers of assemblies. The
//! assemblies are sorted by the position of their main part along the
//! chosen axes and numbered with a prefix, a start and a step. The number
//! is stored in the assembly property `control_number`.
use open_bim_commands::Command;
use open_bim_model::{ObjectId, Project, PropertyValue};
use std::collections::BTreeSet;

pub const CONTROL_NUMBER: &str = "control_number";

/// Sort priority of the axes: the first axis decides, ties go to the next.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Order {
    /// Bottom to top, then along X, then along Y.
    #[default]
    Zxy,
    Zyx,
    Xyz,
    Yxz,
}

impl Order {
    pub const ALL: [Self; 4] = [Self::Zxy, Self::Zyx, Self::Xyz, Self::Yxz];
    pub fn label(self) -> &'static str {
        match self {
            Self::Zxy => "Снизу вверх, по X, по Y",
            Self::Zyx => "Снизу вверх, по Y, по X",
            Self::Xyz => "По X, по Y, снизу вверх",
            Self::Yxz => "По Y, по X, снизу вверх",
        }
    }
    fn axes(self) -> [usize; 3] {
        match self {
            Self::Zxy => [2, 0, 1],
            Self::Zyx => [2, 1, 0],
            Self::Xyz => [0, 1, 2],
            Self::Yxz => [1, 0, 2],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlNumbers {
    pub prefix: String,
    pub start: String,
    pub step: String,
    pub order: Order,
}

impl Default for ControlNumbers {
    fn default() -> Self {
        Self {
            prefix: "М".into(),
            start: "1".into(),
            step: "1".into(),
            order: Order::Zxy,
        }
    }
}

/// Top assemblies holding the selected parts or assemblies; all top
/// assemblies when nothing is selected.
pub fn assemblies_of(project: &Project, selected: &BTreeSet<ObjectId>) -> BTreeSet<ObjectId> {
    let children: BTreeSet<ObjectId> = project
        .assemblies
        .values()
        .flat_map(|a| a.children.iter().copied())
        .collect();
    let top = |mut id: ObjectId| {
        // Walk up the nesting to the top assembly.
        while let Some(parent) = project
            .assemblies
            .values()
            .find(|a| a.children.contains(&id))
        {
            id = parent.id;
        }
        id
    };
    if selected.is_empty() {
        return project
            .assemblies
            .keys()
            .copied()
            .filter(|id| !children.contains(id))
            .collect();
    }
    selected
        .iter()
        .filter_map(|id| {
            if project.assemblies.contains_key(id) {
                Some(top(*id))
            } else {
                project
                    .assemblies
                    .values()
                    .find(|a| a.members.contains(id))
                    .map(|a| top(a.id))
            }
        })
        .collect()
}

impl ControlNumbers {
    /// Commands numbering `assemblies` in the chosen order.
    pub fn commands(
        &self,
        project: &Project,
        assemblies: &BTreeSet<ObjectId>,
    ) -> Result<Vec<Command>, String> {
        let int = |s: &str, what: &str| {
            s.trim()
                .parse::<u64>()
                .map_err(|_| format!("{what}: целое число"))
        };
        let (start, step) = (
            int(&self.start, "Начальный номер")?,
            int(&self.step, "Шаг")?,
        );
        if step == 0 {
            return Err("Шаг: больше 0".into());
        }
        if assemblies.is_empty() {
            return Err("Нет сборок: создайте сборки или выберите их детали".into());
        }
        let axes = self.order.axes();
        let mut keyed: Vec<([i64; 3], ObjectId)> = assemblies
            .iter()
            .map(|id| {
                let a = project.assemblies.get(id).ok_or("Сборка не найдена")?;
                let o = project
                    .elements
                    .get(&a.main_part)
                    .ok_or("Нет основной детали сборки")?
                    .shape
                    .frame()
                    .origin
                    .0;
                // 1 mm steps, so parts on one level sort by the next axis.
                Ok((axes.map(|i| o[i].round() as i64), *id))
            })
            .collect::<Result<_, String>>()?;
        keyed.sort();
        Ok(keyed
            .into_iter()
            .enumerate()
            .map(|(i, (_, id))| Command::ChangeProperty {
                id,
                key: CONTROL_NUMBER.into(),
                value: Some(PropertyValue::Text(format!(
                    "{}{}",
                    self.prefix.trim(),
                    start + step * i as u64
                ))),
            })
            .collect())
    }
}

/// Commands removing the control numbers of `assemblies`.
pub fn clear(project: &Project, assemblies: &BTreeSet<ObjectId>) -> Vec<Command> {
    assemblies
        .iter()
        .filter(|id| {
            project
                .assemblies
                .get(id)
                .is_some_and(|a| a.properties.contains_key(CONTROL_NUMBER))
        })
        .map(|id| Command::ChangeProperty {
            id: *id,
            key: CONTROL_NUMBER.into(),
            value: None,
        })
        .collect()
}
