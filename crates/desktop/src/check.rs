//! Quick model checks. The list never blocks a commit and is not the
//! clash report: it names free ends, duplicate members, bolts that do not
//! join two parts, stale sheets and column ends that left their level.
use eframe::egui;
use open_bim_commands::{Command, DeletePolicy};
use open_bim_drawings::{self, Status};
use open_bim_model::{DetailKind, ElementShape, MemberEnd, ObjectId, Project};
use open_bim_renderer::DVec3;

const MEET_MM: f64 = 80.0;

#[derive(Clone, Debug)]
pub struct Issue {
    pub text: String,
    pub target: ObjectId,
    pub fix: Option<Vec<Command>>,
}

pub fn issues(project: &Project) -> Vec<Issue> {
    let mut out = Vec::new();
    out.extend(free_ends(project));
    out.extend(duplicates(project));
    out.extend(lonely_bolts(project));
    out.extend(stale_sheets(project));
    out.extend(broken_levels(project));
    out
}

fn member_line(project: &Project, id: ObjectId) -> Option<(DVec3, DVec3)> {
    let element = project.elements.get(&id)?;
    let (a, b, _) = project.member_reference_line(&element.shape)?;
    Some((DVec3::from_array(a.0), DVec3::from_array(b.0)))
}

fn free_ends(project: &Project) -> Vec<Issue> {
    let lines: Vec<(ObjectId, DVec3, DVec3)> = project
        .elements
        .keys()
        .filter_map(|id| member_line(project, *id).map(|(a, b)| (*id, a, b)))
        .collect();
    let mut out = Vec::new();
    for (id, a, b) in &lines {
        for (at_end, point) in [(false, *a), (true, *b)] {
            let meets = lines
                .iter()
                .any(|(other, c, d)| other != id && distance_to_segment(point, *c, *d) <= MEET_MM);
            let joined = project
                .connections
                .values()
                .any(|c| (c.primary == *id && c.at_end == at_end) || c.secondary == Some(*id));
            if meets || joined {
                continue;
            }
            let name = &project.elements[id].name;
            out.push(Issue {
                text: format!(
                    "Свободный {}: «{name}»",
                    if at_end { "конец" } else { "начало" }
                ),
                target: *id,
                fix: None,
            });
        }
    }
    out
}

fn distance_to_segment(p: DVec3, a: DVec3, b: DVec3) -> f64 {
    let d = b - a;
    let t = ((p - a).dot(d) / d.dot(d).max(1e-9)).clamp(0.0, 1.0);
    p.distance(a + d * t)
}

fn duplicates(project: &Project) -> Vec<Issue> {
    let mut members: Vec<(ObjectId, ObjectId, DVec3, DVec3)> = Vec::new();
    for (id, element) in &project.elements {
        let ElementShape::Member { profile, .. } = &element.shape else {
            continue;
        };
        let Some((a, b)) = member_line(project, *id) else {
            continue;
        };
        members.push((*id, *profile, a, b));
    }
    let mut out = Vec::new();
    let mut reported = std::collections::BTreeSet::new();
    for (i, (id, profile, a, b)) in members.iter().enumerate() {
        for (other, other_profile, c, d) in members.iter().skip(i + 1) {
            if profile != other_profile || reported.contains(other) {
                continue;
            }
            let same = a.distance(*c) < 1.0 && b.distance(*d) < 1.0
                || a.distance(*d) < 1.0 && b.distance(*c) < 1.0;
            if !same {
                continue;
            }
            reported.insert(*other);
            out.push(Issue {
                text: format!(
                    "Дубль «{}» совпадает с «{}»",
                    project.elements[other].name, project.elements[id].name
                ),
                target: *other,
                fix: Some(vec![Command::Delete {
                    id: *other,
                    policy: DeletePolicy::Cascade,
                }]),
            });
        }
    }
    out
}

fn lonely_bolts(project: &Project) -> Vec<Issue> {
    let mut out = Vec::new();
    for detail in project.details.values() {
        let DetailKind::BoltGroup { parts, .. } = &detail.kind else {
            continue;
        };
        if parts.len() >= 2 {
            continue;
        }
        out.push(Issue {
            text: format!("Болты #{} не соединяют две детали", detail.id),
            target: detail.id,
            fix: Some(vec![Command::Delete {
                id: detail.id,
                policy: DeletePolicy::Cascade,
            }]),
        });
    }
    out
}

fn stale_sheets(project: &Project) -> Vec<Issue> {
    let mut out = Vec::new();
    for drawing in project.drawings.values() {
        if matches!(
            open_bim_drawings::status(project, drawing),
            Status::UpToDate
        ) {
            continue;
        }
        out.push(Issue {
            text: format!(
                "Лист «{}» устарел",
                if drawing.title.is_empty() {
                    &drawing.number
                } else {
                    &drawing.title
                }
            ),
            target: drawing.id,
            fix: Some(vec![Command::ReplaceDrawing {
                drawing: open_bim_drawings::updated(project, drawing),
            }]),
        });
    }
    out
}

pub enum Click {
    Select(ObjectId),
    Fix(Vec<Command>),
}

/// The check list. Closing it does not change the model.
pub fn window(ctx: &egui::Context, open: &mut bool, project: &Project) -> Option<Click> {
    if !*open {
        return None;
    }
    let found = issues(project);
    let mut click = None;
    egui::Window::new("Проверка модели")
        .id(egui::Id::new("model check"))
        .open(open)
        .collapsible(false)
        .resizable(true)
        .default_width(420.)
        .show(ctx, |ui| {
            ui.small("Список ничего не блокирует. Коллизии — отдельная проверка.");
            if found.is_empty() {
                ui.label("Замечаний нет");
                return;
            }
            egui::ScrollArea::vertical()
                .max_height(360.)
                .show(ui, |ui| {
                    for issue in &found {
                        ui.horizontal(|ui| {
                            if ui.button(&issue.text).clicked() {
                                click = Some(Click::Select(issue.target));
                            }
                            if let Some(fix) = &issue.fix
                                && ui.button("Исправить").clicked()
                            {
                                click = Some(Click::Fix(fix.clone()));
                            }
                        });
                    }
                });
        });
    click
}

fn broken_levels(project: &Project) -> Vec<Issue> {
    let mut by_part: std::collections::BTreeMap<ObjectId, (Option<f64>, Option<f64>)> =
        Default::default();
    for end in open_bim_model::level_ends::broken(project) {
        let Some(level) = project.levels.get(&end.level) else {
            continue;
        };
        let z = level.elevation_mm + end.offset_mm;
        let slot = by_part.entry(end.part).or_default();
        match end.end {
            MemberEnd::Bottom => slot.0 = Some(z),
            MemberEnd::Top => slot.1 = Some(z),
        }
    }
    let mut out = Vec::new();
    for (part, (bottom, top)) in by_part {
        let Some(element) = project.elements.get(&part) else {
            continue;
        };
        let Ok(element) = open_bim_model::level_ends::with_end_zs(project, element, bottom, top)
        else {
            continue;
        };
        out.push(Issue {
            text: format!("«{}»: конец ушёл с уровня", element.name),
            target: part,
            fix: Some(vec![Command::ReplaceElement { element }]),
        });
    }
    out
}
