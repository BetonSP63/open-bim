use open_bim_model::rebar::Hook;
use open_bim_model::{Point3, Project, RebarKind, RebarSet};

fn set(number: u32, diameter: f64, bars: Vec<Vec<Point3>>) -> RebarSet {
    RebarSet {
        number,
        kind: RebarKind::Guide,
        host: None,
        diameter,
        grade: "А500С".into(),
        bars,
        hooks: [Hook::default(); 2],
        bend_diameter: None,
        rule: None,
    }
}

fn line(a: [f64; 3], b: [f64; 3]) -> Vec<Point3> {
    vec![Point3(a), Point3(b)]
}

#[test]
fn equal_shapes_share_positions_after_rotation_translation_and_reversal() {
    let mut project = Project::default();
    project.concrete.rebars = vec![
        set(10, 16., vec![line([0., 0., 0.], [1000., 0., 0.])]),
        set(
            20,
            16.,
            vec![
                line([500., 800., 2000.], [500., 1800., 2000.]),
                line([0., 0., 0.], [0., 0., 2000.]),
            ],
        ),
        set(30, 12., vec![line([1000., 0., 0.], [0., 0., 0.])]),
    ];
    let (schedule, positions) = open_bim_reports::rebar::schedule(&project);
    assert_eq!(schedule.rows.len(), 3);
    assert_eq!(schedule.rows[0].diameter, 12.);
    assert_eq!(schedule.rows[1].quantity, 2);
    assert_eq!(schedule.rows[1].sets, [10, 20]);
    assert_eq!(positions.len(), 4);
    let straight_16 = positions
        .iter()
        .find(|p| p.set == 10 && p.bar == 0)
        .unwrap()
        .position;
    assert!(
        positions
            .iter()
            .any(|p| { p.set == 20 && p.bar == 0 && p.position == straight_16 })
    );
    assert_eq!(schedule.rows[1].length_mm, 1000.);
    assert!((schedule.rows[1].unit_mass_kg - 1.578).abs() < 1e-9);
    assert!((schedule.rows[1].total_mass_kg - 3.156).abs() < 1e-9);
    assert!((schedule.total_mass_kg - 7.2).abs() < 0.01);

    let text = open_bim_reports::csv::rebar(&schedule);
    assert!(text.starts_with("\u{feff}Позиция;Класс;Диаметр, мм"));
    assert!(text.contains("2;А500С;16;1000;2;1000;1,578;3,156\r\n"));
    assert!(text.contains("Итого;;;;4;;;"));
}

#[test]
fn bends_use_developed_length_and_mandrel_in_the_position_key() {
    let bent = vec![
        Point3([0., 0., 0.]),
        Point3([1000., 0., 0.]),
        Point3([1000., 500., 0.]),
    ];
    let mut project = Project::default();
    project.concrete.rebars = vec![set(1, 16., vec![bent.clone()])];
    let (schedule, _) = open_bim_reports::rebar::schedule(&project);
    let row = &schedule.rows[0];
    let r = (5. * 16. + 16.) / 2.;
    let expected = 1500. - 2. * r + r * std::f64::consts::FRAC_PI_2;
    assert!((row.length_mm - expected).abs() < 1e-9);
    assert_eq!(row.shape, "1000 / 500");

    project.concrete.rebars[0].bend_diameter = Some(200.);
    let (larger_mandrel, _) = open_bim_reports::rebar::schedule(&project);
    assert!(larger_mandrel.rows[0].length_mm < row.length_mm);

    project.concrete.rebars.push(set(2, 16., vec![bent]));
    let (schedule, positions) = open_bim_reports::rebar::schedule(&project);
    assert_eq!(schedule.rows.len(), 2);
    assert_eq!(positions.iter().filter(|p| p.position == 1).count(), 1);
    assert_eq!(positions.iter().filter(|p| p.position == 2).count(), 1);
}

#[test]
fn positions_are_deterministic_when_sets_are_reordered() {
    let mut project = Project::default();
    project.concrete.rebars = vec![
        set(4, 20., vec![line([0.; 3], [1500., 0., 0.])]),
        set(2, 12., vec![line([0.; 3], [1000., 0., 0.])]),
    ];
    let (before, _) = open_bim_reports::rebar::schedule(&project);
    project.concrete.rebars.reverse();
    let (after, _) = open_bim_reports::rebar::schedule(&project);
    assert_eq!(before, after);
}

#[test]
fn empty_schedule_has_a_zero_total_row() {
    let (schedule, positions) = open_bim_reports::rebar::schedule(&Project::default());
    assert!(schedule.rows.is_empty());
    assert!(positions.is_empty());
    assert!(open_bim_reports::csv::rebar(&schedule).ends_with("Итого;;;;0;;;0,000\r\n"));
}
