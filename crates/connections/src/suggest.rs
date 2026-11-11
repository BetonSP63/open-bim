//! Up to three connection recipes for a selected pair. The order comes
//! from `data/connections/pair-rules.json` and does not change with use.
//! Nothing is inserted until the engineer clicks.
use open_bim_model::{ElementShape, MemberKind, ObjectId, Point3, Project};
use serde::Deserialize;

const REACH_MM: f64 = 400.0;

#[derive(Deserialize)]
struct Rule {
    meet: String,
    recipes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub name: String,
    /// Column or the member the other one frames into.
    pub primary: ObjectId,
    pub secondary: ObjectId,
    /// The secondary's end that meets the primary.
    pub at_end: bool,
}

fn rules() -> Vec<Rule> {
    serde_json::from_str(include_str!("../../../data/connections/pair-rules.json"))
        .expect("pair-rules.json")
}

struct Bar {
    id: ObjectId,
    kind: MemberKind,
    start: [f64; 3],
    end: [f64; 3],
}

fn bars(project: &Project, a: ObjectId, b: ObjectId) -> Option<(Bar, Bar)> {
    let bar = |id| {
        let element = project.elements.get(&id)?;
        let ElementShape::Member {
            member_kind,
            length,
            frame,
            ..
        } = &element.shape
        else {
            return None;
        };
        Some(Bar {
            id,
            kind: *member_kind,
            start: frame.origin.0,
            end: frame.world(Point3([0.0, 0.0, length.value()])).0,
        })
    };
    Some((bar(a)?, bar(b)?))
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}

fn distance_to_segment(p: [f64; 3], a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let len2 = d.iter().map(|v| v * v).sum::<f64>().max(1e-9);
    let t = ((0..3).map(|i| (p[i] - a[i]) * d[i]).sum::<f64>() / len2).clamp(0.0, 1.0);
    dist(p, [a[0] + d[0] * t, a[1] + d[1] * t, a[2] + d[2] * t])
}

fn near_end(point: [f64; 3], bar: &Bar) -> bool {
    dist(point, bar.start) <= REACH_MM || dist(point, bar.end) <= REACH_MM
}

/// Recipes for the pair, at most three, in the file's order.
pub fn suggest(project: &Project, a: ObjectId, b: ObjectId) -> Vec<Suggestion> {
    let Some((left, right)) = bars(project, a, b) else {
        return Vec::new();
    };
    let Some((meet, primary, secondary, at_end)) = classify(&left, &right).or_else(|| {
        // The file lists the column as primary; try the other order.
        classify(&right, &left)
    }) else {
        return Vec::new();
    };
    let Some(rule) = rules().into_iter().find(|r| r.meet == meet) else {
        return Vec::new();
    };
    rule.recipes
        .into_iter()
        .take(3)
        .map(|name| Suggestion {
            name,
            primary,
            secondary,
            at_end,
        })
        .collect()
}

fn classify(main: &Bar, other: &Bar) -> Option<(&'static str, ObjectId, ObjectId, bool)> {
    let end_meets = |bar: &Bar, support: &Bar| -> Option<bool> {
        let start = distance_to_segment(bar.start, support.start, support.end) <= REACH_MM;
        let end = distance_to_segment(bar.end, support.start, support.end) <= REACH_MM;
        match (start, end) {
            (true, false) => Some(false),
            (false, true) => Some(true),
            (true, true) => Some(dist(bar.end, support.start) <= dist(bar.start, support.start)),
            _ => None,
        }
    };
    if main.kind == MemberKind::Column && other.kind == MemberKind::Beam {
        let at_end = end_meets(other, main)?;
        return Some(("beam_to_column", main.id, other.id, at_end));
    }
    if main.kind == MemberKind::Beam && other.kind == MemberKind::Beam {
        let ends_touch = near_end(main.start, other) && near_end(main.end, other)
            || (near_end(other.start, main) && dist(other.start, main.end) <= REACH_MM)
            || (near_end(other.end, main) && dist(other.end, main.end) <= REACH_MM);
        if ends_touch && dist(other.end, main.end) <= REACH_MM {
            return Some(("beam_to_beam_end", main.id, other.id, true));
        }
        if let Some(at_end) = end_meets(other, main) {
            let on_span = match at_end {
                true => dist(other.end, main.start) > REACH_MM && dist(other.end, main.end) > 50.0,
                false => {
                    dist(other.start, main.start) > REACH_MM && dist(other.start, main.end) > 50.0
                }
            };
            if on_span {
                return Some(("beam_on_beam", main.id, other.id, at_end));
            }
        }
    }
    None
}
