//! Reinforcement positions and schedules. Equal bars are recognised by their
//! local geometry, independently of placement and direction in the model.
use open_bim_model::{Point3, Project, RebarSet};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarPosition {
    pub set: u32,
    pub bar: usize,
    pub position: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RebarPositionRow {
    pub position: u32,
    pub grade: String,
    pub diameter: f64,
    /// Straight-leg lengths in the direction of the bar, mm.
    pub shape: String,
    pub quantity: usize,
    pub length_mm: f64,
    pub unit_mass_kg: f64,
    pub total_mass_kg: f64,
    pub sets: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RebarSchedule {
    pub rows: Vec<RebarPositionRow>,
    pub total_mass_kg: f64,
}

fn quantize(value: f64) -> i64 {
    (value * 1000.).round() as i64
}

fn geometry_signature(set: &RebarSet, bar: &[Point3]) -> String {
    let directed = |reverse: bool| {
        let at = |i: usize| {
            if reverse {
                &bar[bar.len() - 1 - i]
            } else {
                &bar[i]
            }
        };
        let direction = |a: &Point3, b: &Point3| {
            let v: [f64; 3] = std::array::from_fn(|k| b.0[k] - a.0[k]);
            let length = v.iter().map(|x| x * x).sum::<f64>().sqrt();
            (length > 1e-9).then(|| v.map(|x| x / length))
        };
        let mut values = Vec::new();
        for i in 0..bar.len() - 1 {
            values.push(quantize(RebarSet::bar_length(&[*at(i), *at(i + 1)])));
        }
        for i in 1..bar.len() - 1 {
            if let (Some(a), Some(b)) = (direction(at(i), at(i - 1)), direction(at(i), at(i + 1))) {
                let dot = (0..3).map(|k| a[k] * b[k]).sum::<f64>().clamp(-1., 1.);
                values.push(quantize(dot.acos().to_degrees()));
            }
        }
        values.push(quantize(set.developed_length(bar)));
        values
            .into_iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(",")
    };
    directed(false).min(directed(true))
}

#[derive(Default)]
struct Group {
    grade: String,
    diameter: f64,
    shape: String,
    length_mm: f64,
    unit_mass_kg: f64,
    sets: Vec<u32>,
    bars: Vec<(u32, usize)>,
}

/// Number equal bars across all sets. The same position is retained when an
/// unrelated set is inserted before it; placement and reversal do not matter.
pub fn schedule(project: &Project) -> (RebarSchedule, Vec<BarPosition>) {
    let mut groups: BTreeMap<String, Group> = BTreeMap::new();
    for set in &project.concrete.rebars {
        let mass_per_mm = open_bim_model::rebar::mass_per_m(set.diameter) / 1000.;
        for (bar_index, bar) in set.bars.iter().enumerate() {
            let length = set.developed_length(bar);
            let key = format!(
                "{}|{}|{}|{}",
                set.grade,
                quantize(set.diameter),
                quantize(set.mandrel()),
                geometry_signature(set, bar)
            );
            let group = groups.entry(key).or_insert_with(|| Group {
                grade: set.grade.clone(),
                diameter: set.diameter,
                shape: bar
                    .windows(2)
                    .map(|leg| crate::fmt(RebarSet::bar_length(leg)))
                    .collect::<Vec<_>>()
                    .join(" / "),
                length_mm: length,
                unit_mass_kg: length * mass_per_mm,
                ..Group::default()
            });
            if !group.sets.contains(&set.number) {
                group.sets.push(set.number);
            }
            group.bars.push((set.number, bar_index));
        }
    }
    let mut groups: Vec<Group> = groups.into_values().collect();
    groups.sort_by(|a, b| {
        a.grade
            .cmp(&b.grade)
            .then(a.diameter.total_cmp(&b.diameter))
            .then(a.length_mm.total_cmp(&b.length_mm))
            .then(a.shape.cmp(&b.shape))
    });
    let mut rows = Vec::with_capacity(groups.len());
    let mut positions = Vec::new();
    let mut total_mass_kg = 0.;
    for (index, group) in groups.into_iter().enumerate() {
        let position = index as u32 + 1;
        let total = group.unit_mass_kg * group.bars.len() as f64;
        total_mass_kg += total;
        positions.extend(
            group
                .bars
                .iter()
                .map(|&(set, bar)| BarPosition { set, bar, position }),
        );
        rows.push(RebarPositionRow {
            position,
            grade: group.grade,
            diameter: group.diameter,
            shape: group.shape,
            quantity: group.bars.len(),
            length_mm: group.length_mm,
            unit_mass_kg: group.unit_mass_kg,
            total_mass_kg: total,
            sets: group.sets,
        });
    }
    (
        RebarSchedule {
            rows,
            total_mass_kg,
        },
        positions,
    )
}
