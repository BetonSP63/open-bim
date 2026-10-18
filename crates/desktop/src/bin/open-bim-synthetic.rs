//! Synthetic benchmark corpus: N simple steel members (columns and beams on a
//! storey grid, four catalogue sections, three lengths), no connections.
//!
//! Usage: open-bim-synthetic <count> <out.obim>
use open_bim_model::{
    Element, ElementShape, Frame, LengthMm, MemberKind, ObjectId, Point3, Project,
};
use std::collections::BTreeMap;

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let count: usize = args
        .next()
        .and_then(|v| v.parse().ok())
        .filter(|v| (1..=1_000_000).contains(v))
        .ok_or("Укажите число объектов 1…1 000 000")?;
    let path = args.next().ok_or("Укажите файл .obim")?;
    let mut project = Project {
        name: format!("Синтетический корпус {count}"),
        ..Project::default()
    };
    let profiles: Vec<ObjectId> = project.profiles.keys().copied().take(4).collect();
    let material = *project.materials.keys().next().ok_or("нет материала")?;
    let side = ((count as f64 / 40.).sqrt().ceil() as usize).max(1);
    let (bay, storey) = (6000., 3600.);
    for i in 0..count {
        let id = project.allocate_id().map_err(|e| e.to_string())?;
        let level = i / (side * side * 2);
        let cell = i % (side * side * 2);
        let (x, y) = ((cell / 2) % side, (cell / 2) / side);
        let column = cell.is_multiple_of(2);
        let origin = Point3([x as f64 * bay, y as f64 * bay, level as f64 * storey]);
        let (kind, frame, length) = if column {
            (
                MemberKind::Column,
                Frame {
                    origin,
                    ..Frame::default()
                },
                storey - 300.,
            )
        } else {
            (
                MemberKind::Beam,
                Frame {
                    origin: Point3([origin.0[0], origin.0[1], origin.0[2] + storey - 150.]),
                    axes: [[0., 1., 0.], [0., 0., 1.], [1., 0., 0.]],
                },
                [bay, bay - 300., bay - 600.][i % 3],
            )
        };
        project.elements.insert(
            id,
            Element {
                id,
                name: format!(
                    "{} {i}",
                    if column {
                        "Колонна"
                    } else {
                        "Балка"
                    }
                ),
                material,
                shape: ElementShape::Member {
                    member_kind: kind,
                    profile: profiles[i % profiles.len()],
                    length: LengthMm::new(length).map_err(|e| e.to_string())?,
                    frame,
                },
                properties: BTreeMap::new(),
            },
        );
    }
    project.validate().map_err(|e| e.to_string())?;
    open_bim_persistence::save_atomic(&path, &project).map_err(|e| e.to_string())?;
    println!("{count} объектов → {path}");
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Ошибка: {e}");
        std::process::exit(1);
    }
}
