//! Reinforcement schedules written next to the steel CSV lists.
use open_bim_model::{Point3, Project, RebarKind, RebarSet};
use open_bim_reports::csv;

#[test]
fn detail_and_steel_lists_follow_the_bars() {
    let mut project = Project::default();
    let mut set = RebarSet::basic(RebarKind::Single, 16., "А500С");
    set.number = 1;
    set.shape_id = Some("gost.straight".into());
    set.bars = vec![vec![Point3([0.; 3]), Point3([1000., 0., 0.])]; 2];
    project.concrete.rebars.push(set);
    let details = csv::rebar_details(&project);
    assert!(details.starts_with('\u{feff}'));
    assert!(details.contains("Позиция;Форма;Диаметр, мм;Класс;Стандарт"));
    assert!(details.contains("ГОСТ 34028-2016"));
    assert!(details.contains("ПР"));
    assert!(details.contains("A=1000"));
    assert!(details.contains("1,578"));
    assert!(details.contains("3,156"));
    let steel = csv::rebar_steel(&project);
    assert!(steel.contains("А500С"));
    assert!(steel.contains("Всего"));
    assert!(steel.contains("Итого"));
    assert!(steel.contains("3,156"));
    let dir = std::env::temp_dir().join(format!("open-bim-rebar-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let files = csv::write_rebar(&project, &dir).unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.iter().all(|p| p.is_file()));
    let bvbs = csv::write_bvbs(&project, &dir).unwrap();
    assert_eq!(bvbs.len(), 1);
    let text = std::fs::read_to_string(&bvbs[0]).unwrap();
    assert!(text.contains("BF2D@H"));
    assert!(open_bim_model::rebar::bvbs::checksum_matches(
        text.lines().next().unwrap()
    ));
    let _ = std::fs::remove_dir_all(&dir);
}
