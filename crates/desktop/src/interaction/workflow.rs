//! One modeling session instead of a pile of unrelated commands.
//!
//! The resting state is selection. A tool is a temporary mode: its properties
//! are on the side, the status line says the next click, and Esc always
//! returns to selection. While nothing is being picked, the status line says
//! which step of a steel frame is still ahead — axes, columns, beams, plates,
//! connections, drawings — judged from what the model already contains.
use crate::tools::{Draft, ToolKind};
use open_bim_model::{ElementShape, MemberKind, ObjectId, Project};

/// Order of the «МЕТАЛЛ» ribbon. The tab is the modeling path; other tabs
/// keep the same commands for people who already know where they lived.
pub const STEEL_GROUP_ORDER: [&str; 5] = [
    "Оси и уровни",
    "Колонны и балки",
    "Пластины",
    "Узлы",
    "Сборка",
];

/// How far the model has got along the usual frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Grid,
    Columns,
    Beams,
    Plates,
    Connections,
    Drawings,
    Done,
}

/// The command the status line and the empty property pane offer next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Next {
    Grid,
    Column,
    Beam,
    Plate,
    Connections,
    Drawings,
}

/// What to tell the user about the current stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Guide {
    pub stage: Stage,
    pub heading: &'static str,
    pub detail: &'static str,
    /// One line for the status bar.
    pub status: &'static str,
    /// Side-pane button. Not a ribbon caption, so the two are not confused.
    pub action: &'static str,
    pub next: Next,
}

impl Stage {
    pub fn guide(self) -> Guide {
        let (heading, detail, status, action, next) = match self {
            Self::Grid => (
                "Сначала оси и уровни",
                "Без сетки колонну некуда ставить. «Сетка» задаёт оси и отметки сразу; отдельная ось — два щелчка.",
                "Шаг 1 из 6. Задайте оси и уровни — кнопка «Сетка».",
                "Дальше: сетка",
                Next::Grid,
            ),
            Self::Columns => (
                "Колонны по узлам сетки",
                "Один щелчок в узел. Низ и верх берутся с соседних уровней, профиль и сталь уже стоят в панели справа — менять их нужно только если тип другой.",
                "Шаг 2 из 6. Колонна: один щелчок в узел сетки. Низ и верх — соседние уровни.",
                "Дальше: колонна",
                Next::Column,
            ),
            Self::Beams => (
                "Балки между колоннами",
                "Два щелчка: начало и конец. Удобно ловить верх колонны и узел сетки — привязка к концам и пересечениям уже включена.",
                "Шаг 3 из 6. Балка: начало и конец, удобно по верху колонн.",
                "Дальше: балка",
                Next::Beam,
            ),
            Self::Plates => (
                "Пластины, если они нужны",
                "Углы контура и Enter, чтобы замкнуть. Фасонку узла можно не чертить: её поставит компонент на следующем шаге.",
                "Шаг 4 из 6. Пластина — углы контура, Enter замыкает. Либо сразу узлы.",
                "Дальше: пластина",
                Next::Plate,
            ),
            Self::Connections => (
                "Узлы компонентом",
                "Опорная плита, торец, фасонка ставятся из каталога или автосоединением, а не набором отдельных пластин.",
                "Шаг 5 из 6. Узел — «Компоненты» или «Автосоединения», не ручной набор пластин.",
                "Дальше: компоненты",
                Next::Connections,
            ),
            Self::Drawings => (
                "Нумерация и чертежи",
                "Каркас собран. Вкладка «ЧЕРТЕЖИ И ОТЧЁТЫ»: нумерация, лист, выпуск комплекта.",
                "Шаг 6 из 6. Нумерация, чертежи и выпуск — вкладка «ЧЕРТЕЖИ И ОТЧЁТЫ».",
                "Дальше: чертежи",
                Next::Drawings,
            ),
            Self::Done => (
                "Можно править и выпускать снова",
                "Выбор — основной режим. Щелчок по детали открывает её свойства; Esc снимает выделение и возвращает сюда из любой команды.",
                "Модель и листы есть. Выбор — основной режим; чертёж обновляется с модели.",
                "Дальше: чертежи",
                Next::Drawings,
            ),
        };
        Guide {
            stage: self,
            heading,
            detail,
            status,
            action,
            next,
        }
    }
}

/// Earliest missing step. A later object counts as progress, so a model that
/// already has beams is not sent back to «поставьте колонны».
pub fn stage(project: &Project) -> Stage {
    let column = has_kind(project, MemberKind::Column);
    let beam = has_kind(project, MemberKind::Beam);
    let plate = project
        .elements
        .values()
        .any(|e| matches!(e.shape, ElementShape::Plate { .. }));
    let progress = if !project.drawings.is_empty() {
        6
    } else if !project.connections.is_empty() {
        5
    } else if plate {
        4
    } else if beam {
        3
    } else if column {
        2
    } else if !project.axes.is_empty() {
        1
    } else {
        0
    };
    match progress {
        0 => Stage::Grid,
        1 => Stage::Columns,
        2 => Stage::Beams,
        3 => Stage::Plates,
        4 => Stage::Connections,
        5 => Stage::Drawings,
        _ => Stage::Done,
    }
}

fn has_kind(project: &Project, kind: MemberKind) -> bool {
    project.elements.values().any(|e| {
        matches!(
            e.shape,
            ElementShape::Member { member_kind, .. } if member_kind == kind
        )
    })
}

/// Status bar when no command is collecting clicks.
pub fn idle(project: &Project, selected: usize) -> String {
    let status = stage(project).guide().status;
    if selected == 0 {
        status.to_string()
    } else {
        format!("Выбрано {selected}. Свойства справа, кнопка «Изменить». {status}")
    }
}

/// The step of a running command, plus the way back to selection.
pub fn with_escape(step: &str) -> String {
    if step.contains("Esc") {
        step.to_string()
    } else {
        format!("{step}  ·  Esc — вернуться к выбору")
    }
}

/// Idle status: the next step stays readable, a refusal stays in front.
pub fn join_feedback(message: &str, hint: &str) -> String {
    let message = message.trim();
    let hint = hint.trim();
    if message.is_empty() {
        return hint.to_string();
    }
    if hint.is_empty() || message.contains(hint) {
        return message.to_string();
    }
    if refusal(message) {
        format!("{message}  ·  {hint}")
    } else {
        format!("{hint}  ·  {message}")
    }
}

fn refusal(message: &str) -> bool {
    let m = message.to_lowercase();
    [
        "сначала",
        "ничего не",
        "нельзя",
        "ошиб",
        "нет ",
        "подтвердите",
        "не выбран",
    ]
    .iter()
    .any(|s| m.contains(s))
}

/// Help under the title of the create pane. Same rules for every part tool.
pub fn create_help(kind: ToolKind) -> &'static str {
    match kind.base() {
        ToolKind::Column => {
            "Один щелчок — колонна между соседними уровнями. Профиль и материал меняются здесь, до щелчка. Команда повторяется, Esc возвращает к выбору."
        }
        ToolKind::Plate => {
            "Углы контура; Enter или средняя кнопка замыкает. Толщина — в этой панели. Esc возвращает к выбору."
        }
        ToolKind::Axis => {
            "Две точки оси. Всю сетку с уровнями задаёт кнопка «Сетка». Esc возвращает к выбору."
        }
        _ => {
            "Начало и конец. Профиль, материал и положение — в этой панели, до щелчка. Команда повторяется, Esc возвращает к выбору."
        }
    }
}

pub fn selection_note(count: usize) -> String {
    format!(
        "Выбрано {count}. Поля ниже и «Изменить» — один шаг отмены. Перенос — Ctrl+M, копия — Ctrl+C."
    )
}

/// Lowest level and the next one above it. A column is one storey, not the
/// whole building: the top level in the pane changes the rest.
pub fn column_storey(project: &Project) -> Option<(ObjectId, ObjectId)> {
    let mut levels: Vec<_> = project.levels.values().collect();
    levels.sort_by(|a, b| {
        a.elevation_mm
            .partial_cmp(&b.elevation_mm)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.id.cmp(&b.id))
    });
    let bottom = *levels.first()?;
    let top = levels
        .into_iter()
        .find(|l| l.elevation_mm > bottom.elevation_mm + 0.5)?;
    Some((bottom.id, top.id))
}

/// Bind a steel column that still has the untouched 0 / 3600 fields to the
/// storey. A file of properties or a typed height is left alone.
pub fn apply_column_storey(draft: &mut Draft, storey: Option<(ObjectId, ObjectId)>) -> bool {
    let Some((bottom, top)) = storey else {
        return false;
    };
    if draft.kind != ToolKind::Column {
        return false;
    }
    if draft.bottom_level.is_some() || draft.top_level.is_some() {
        return false;
    }
    if draft.bottom.trim() != "0" || draft.top.trim() != "3600" {
        return false;
    }
    draft.bottom_level = Some(bottom);
    draft.top_level = Some(top);
    draft.bottom = "0".into();
    draft.top = "0".into();
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_bim_model::{
        ConnectionInstance, ConnectionTemplate, Drawing, DrawingKind, Element, Frame, GridAxis,
        LengthMm, Level, Point3, SheetSize, TemplateKind,
    };
    use std::collections::BTreeMap;

    fn axis(project: &mut Project) {
        let id = ObjectId(20);
        project.axes.insert(
            id,
            GridAxis {
                id,
                name: "1".into(),
                start: Point3([0., 0., 0.]),
                end: Point3([6000., 0., 0.]),
            },
        );
    }

    fn member(project: &mut Project, id: u64, kind: MemberKind) {
        let id = ObjectId(id);
        project.elements.insert(
            id,
            Element {
                id,
                name: "Деталь".into(),
                material: ObjectId(1),
                shape: ElementShape::Member {
                    member_kind: kind,
                    profile: ObjectId(2),
                    length: LengthMm::new(3000.).unwrap(),
                    frame: Frame::default(),
                    position: Default::default(),
                },
                properties: BTreeMap::new(),
            },
        );
    }

    #[test]
    fn stages_follow_the_frame_and_do_not_send_the_user_backwards() {
        assert_eq!(stage(&Project::default()), Stage::Grid);
        let mut grid = Project::default();
        axis(&mut grid);
        assert_eq!(stage(&grid), Stage::Columns);
        member(&mut grid, 30, MemberKind::Column);
        assert_eq!(stage(&grid), Stage::Beams);
        let mut beams = Project::default();
        member(&mut beams, 31, MemberKind::Beam);
        assert_eq!(
            stage(&beams),
            Stage::Plates,
            "балки без колонн — это уже каркас, не шаг «поставьте колонну»"
        );
        let frame = open_bim_model::example_steel_frame().unwrap();
        assert_eq!(stage(&frame), Stage::Connections);
        assert!(idle(&frame, 0).contains("Компоненты") || idle(&frame, 0).contains("Узел"));
        assert!(idle(&grid, 2).contains("Выбрано 2"));
    }

    #[test]
    fn connections_then_drawings_then_done() {
        let mut project = open_bim_model::example_steel_frame().unwrap();
        project.connections.insert(
            ObjectId(90),
            ConnectionInstance {
                id: ObjectId(90),
                primary: ObjectId(1),
                secondary: None,
                at_end: true,
                template: ConnectionTemplate {
                    version: 1,
                    name: "Опорная плита".into(),
                    kind: TemplateKind::BasePlate { overhang: 20. },
                    thickness: 20.,
                    edge_distance: 40.,
                    bolt_diameter: 20.,
                    clearance: 2.,
                    bolt_standard: "ГОСТ 7798-70".into(),
                    bolt_grade: "8.8".into(),
                    weld_size: 6.,
                },
                children: BTreeMap::new(),
                general: Default::default(),
            },
        );
        assert_eq!(stage(&project), Stage::Drawings);
        project.drawings.insert(
            ObjectId(91),
            Drawing {
                id: ObjectId(91),
                kind: DrawingKind::General {
                    objects: Vec::new(),
                },
                number: "КМД-1".into(),
                title: "Каркас".into(),
                sheet: SheetSize::A1,
                scale: 50.,
                views: Vec::new(),
                dimensions: Vec::new(),
                texts: Vec::new(),
                objects: Vec::new(),
                fingerprints: BTreeMap::new(),
                generated_revision: 0,
                issued_revision: None,
                multiple: 1,
                portrait: false,
            },
        );
        assert_eq!(stage(&project), Stage::Done);
    }

    #[test]
    fn escape_is_appended_once_and_a_refusal_stays_in_front() {
        let step = with_escape("Укажите начальную точку балки");
        assert!(step.contains("Укажите начальную точку балки"));
        assert!(step.contains("Esc — вернуться к выбору"));
        assert_eq!(with_escape(&step), step);
        let hint = "Шаг 3 из 6. Балка.";
        assert!(join_feedback("Сначала выберите объекты", hint).starts_with("Сначала выберите"));
        assert!(join_feedback("Команда прервана", hint).starts_with("Шаг 3"));
        assert_eq!(join_feedback("", hint), hint);
    }

    #[test]
    fn column_storey_is_the_next_level_not_the_roof() {
        let mut project = Project::default();
        for (id, z, name) in [
            (1u64, 0., "0.000"),
            (2, 7200., "+7.200"),
            (3, 3600., "+3.600"),
        ] {
            let id = ObjectId(id);
            project.levels.insert(
                id,
                Level {
                    id,
                    name: name.into(),
                    elevation_mm: z,
                },
            );
        }
        assert_eq!(column_storey(&project), Some((ObjectId(1), ObjectId(3))));
        let mut draft = Draft {
            kind: ToolKind::Column,
            ..Draft::default()
        };
        assert!(apply_column_storey(&mut draft, column_storey(&project)));
        assert_eq!(draft.bottom_level, Some(ObjectId(1)));
        assert_eq!(draft.top_level, Some(ObjectId(3)));
        assert_eq!(draft.top, "0");
        draft.top = "4000".into();
        draft.bottom_level = None;
        draft.top_level = None;
        draft.bottom = "0".into();
        assert!(
            !apply_column_storey(&mut draft, column_storey(&project)),
            "введённая высота не затирается"
        );
        draft.top = "3600".into();
        draft.kind = ToolKind::Beam;
        assert!(!apply_column_storey(&mut draft, column_storey(&project)));
    }

    #[test]
    fn guides_name_a_command_the_ribbon_actually_has() {
        for stage in [
            Stage::Grid,
            Stage::Columns,
            Stage::Beams,
            Stage::Plates,
            Stage::Connections,
            Stage::Drawings,
            Stage::Done,
        ] {
            let g = stage.guide();
            assert_eq!(g.stage, stage);
            assert!(!g.status.is_empty());
            assert!(g.action.starts_with("Дальше:"));
        }
    }
}
