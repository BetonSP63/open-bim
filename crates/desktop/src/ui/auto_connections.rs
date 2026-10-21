//! Automatic connections («Автосоединения»): for the selected parts (or the
//! whole model), find the free member ends that meet another part and put
//! the first component of the matching rule that fits there.
use super::Workbench;
use super::components::{Component, catalog};
use crate::tools::ui::Action;
use open_bim_commands::Command;
use open_bim_model::{
    ConnectionInstance, ConnectionTemplate, ElementShape, MemberKind, ObjectId, Point3, Project,
};
use open_bim_renderer::DVec3;
use std::collections::BTreeSet;

/// Farthest a member end may stop from the part it connects to, mm.
pub const REACH: f64 = 400.0;

/// Components tried in order for each kind of joint; the first whose
/// geometry fits is used.
#[derive(Clone, Debug, PartialEq)]
pub struct AutoRules {
    pub beam_to_support: Vec<&'static str>,
    pub beam_to_beam: Vec<&'static str>,
    pub column_base: Vec<&'static str>,
}

impl Default for AutoRules {
    fn default() -> Self {
        Self {
            beam_to_support: vec![
                "Шарнир на пластине",
                "Торцевая пластина",
                "Уголковое соединение",
            ],
            beam_to_beam: vec!["Стык балок на накладках"],
            column_base: vec!["Опорная плита"],
        }
    }
}

/// The connections to create and what was left without one.
#[derive(Clone, Debug, Default)]
pub struct Plan {
    pub commands: Vec<Command>,
    /// (component name, main part) per connection, in command order.
    pub made: Vec<(&'static str, ObjectId)>,
    /// Joints found where no rule's component fits.
    pub skipped: usize,
}

struct Axis {
    id: ObjectId,
    kind: MemberKind,
    start: DVec3,
    end: DVec3,
}

fn axes(project: &Project, scope: &BTreeSet<ObjectId>) -> Vec<Axis> {
    project
        .elements
        .values()
        .filter(|e| scope.is_empty() || scope.contains(&e.id))
        .filter_map(|e| match &e.shape {
            ElementShape::Member {
                member_kind,
                length,
                frame,
                ..
            } => Some(Axis {
                id: e.id,
                kind: *member_kind,
                start: DVec3::from_array(frame.origin.0),
                end: DVec3::from_array(frame.world(Point3([0., 0., length.value()])).0),
            }),
            _ => None,
        })
        .collect()
}

fn distance_to_segment(p: DVec3, a: DVec3, b: DVec3) -> f64 {
    let d = b - a;
    let t = ((p - a).dot(d) / d.dot(d).max(1e-12)).clamp(0., 1.);
    p.distance(a + d * t)
}

/// Plan connections for the free member ends among `scope` (all members
/// when empty), using `template` for bolts, welds and edge distances.
pub fn plan(
    project: &Project,
    scope: &BTreeSet<ObjectId>,
    template: &ConnectionTemplate,
    rules: &AutoRules,
) -> Plan {
    let members = axes(project, scope);
    let mut taken: BTreeSet<(ObjectId, bool)> = project
        .connections
        .values()
        .map(|c| (c.primary, c.at_end))
        .collect();
    let mut plan = Plan::default();
    let mut next = project.next_id;
    let catalog = catalog();
    let component = |name: &str| catalog.iter().find(|c| c.name == name);
    let mut try_rule = |plan: &mut Plan,
                        names: &[&'static str],
                        main: ObjectId,
                        support: Option<ObjectId>,
                        at_end: bool|
     -> bool {
        for name in names {
            let Some(c) = component(name) else { continue };
            let instance = instance(c, template, ObjectId(next), main, support, at_end);
            if open_bim_connections::check_geometry(project, &instance).is_ok() {
                next += 1;
                plan.commands.push(Command::CreateConnection {
                    connection: instance,
                });
                plan.made.push((c.name, main));
                return true;
            }
        }
        plan.skipped += 1;
        false
    };
    for m in &members {
        match m.kind {
            MemberKind::Column => {
                let at_end = m.end.z < m.start.z;
                let foot = if at_end { m.end } else { m.start };
                let stands_on_something = members
                    .iter()
                    .any(|n| n.id != m.id && distance_to_segment(foot, n.start, n.end) < 1.0);
                if taken.contains(&(m.id, at_end)) || stands_on_something {
                    continue;
                }
                if try_rule(&mut plan, &rules.column_base, m.id, None, at_end) {
                    taken.insert((m.id, at_end));
                }
            }
            MemberKind::Beam => {
                for at_end in [false, true] {
                    if taken.contains(&(m.id, at_end)) {
                        continue;
                    }
                    let p = if at_end { m.end } else { m.start };
                    let partner = members
                        .iter()
                        .filter(|n| n.id != m.id)
                        .map(|n| (distance_to_segment(p, n.start, n.end), n))
                        .filter(|(d, _)| *d <= REACH)
                        .min_by(|a, b| a.0.total_cmp(&b.0));
                    let Some((_, n)) = partner else { continue };
                    let along = (m.end - m.start).normalize();
                    let other = (n.end - n.start).normalize();
                    let collinear =
                        n.kind == MemberKind::Beam && along.dot(other).abs() > 1. - 1e-9;
                    if collinear {
                        // The partner's end that meets this one.
                        let partner_end = n.end.distance(p) < n.start.distance(p);
                        if taken.contains(&(n.id, partner_end)) {
                            continue;
                        }
                        if try_rule(&mut plan, &rules.beam_to_beam, m.id, Some(n.id), at_end) {
                            taken.insert((m.id, at_end));
                            taken.insert((n.id, partner_end));
                        }
                    } else if try_rule(&mut plan, &rules.beam_to_support, m.id, Some(n.id), at_end)
                    {
                        taken.insert((m.id, at_end));
                    }
                }
            }
        }
    }
    plan
}

fn instance(
    component: &Component,
    template: &ConnectionTemplate,
    id: ObjectId,
    main: ObjectId,
    support: Option<ObjectId>,
    at_end: bool,
) -> ConnectionInstance {
    let mut template = template.clone();
    template.kind = component.kind.clone();
    template.thickness = component.thickness;
    template.name = component.name.into();
    ConnectionInstance {
        id,
        primary: main,
        secondary: support,
        at_end,
        template,
        children: Default::default(),
    }
}

impl Workbench {
    /// «Автосоединения»: connect the free member ends of the selection, or
    /// of the whole model when nothing is selected.
    pub(super) fn auto_connections(&mut self) -> Option<super::Request> {
        let project = self.editor.document.project();
        let plan = plan(
            project,
            &self.editor.selected,
            &self.tools.connection.template,
            &AutoRules::default(),
        );
        let skipped = if plan.skipped > 0 {
            format!(", без подходящего узла: {}", plan.skipped)
        } else {
            String::new()
        };
        if plan.commands.is_empty() {
            self.message(format!("Автосоединения: новых узлов нет{skipped}"));
            return None;
        }
        let count = plan.commands.len();
        self.message(format!("Автосоединения: узлов {count}{skipped}"));
        Some(super::Request::Action(Action::Commit(
            format!("Автосоединения ({count})"),
            plan.commands,
        )))
    }
}
