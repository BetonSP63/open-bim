//! ГОСТ Р 21.101-2026 conventions on drawings: 45° ticks on КМ dimensions
//! (arrows on КМД), level marks, coordination axes with bubbles, marks on
//! shelves, view titles and section lines with arrows.
use open_bim_drawings::conventions::{AXIS_TEXT, BUBBLE, DIM_TEXT, LEVEL_STROKE, TICK};
use open_bim_drawings::sample::{control_building, control_project};
use open_bim_drawings::{LineStyle, Sheet, general_drawing, generate, part_drawing};
use open_bim_model::{Drawing, DrawingView, ObjectId, Project, SheetSize, ViewDirection};

/// КМ-1 on A1: elevation with its automatic chain over the row-А columns,
/// plan and section 1-1.
pub fn ga(project: &Project) -> Drawing {
    let mut d = general_drawing(project, ObjectId(9000), vec![], "КМ-1", SheetSize::A1).unwrap();
    d.views.push(DrawingView {
        id: 3,
        name: "Разрез".into(),
        direction: ViewDirection::Front,
        section: Some(-3000.),
        position: [620., 500.],
        scale: Some(100.),
        node: None,
        source: None,
        section_host: None,
        appearance: Default::default(),
        unfolded: false,
    });
    // «Узел 1»: the joint of column 1/А and the beam at +3,600 at 1:20.
    d.views.push(DrawingView {
        id: 4,
        name: "Узел".into(),
        direction: ViewDirection::Front,
        section: None,
        position: [700., 330.],
        scale: Some(20.),
        node: Some(open_bim_model::NodeRegion {
            centre: [0., 3600.],
            radius: 500.,
        }),
        source: None,
        section_host: None,
        appearance: Default::default(),
        unfolded: false,
    });
    d
}

fn view(s: &Sheet, id: u32) -> &open_bim_drawings::ViewGraphic {
    s.views.iter().find(|v| v.id == id).unwrap()
}

fn seg_len(p: &[[f64; 2]]) -> f64 {
    (p[1][0] - p[0][0]).hypot(p[1][1] - p[0][1])
}

#[test]
fn view_titles_follow_5_5_and_are_not_underlined() {
    let p = control_building();
    let d = ga(&p);
    let s = generate(&p, &d).unwrap();
    let title = |id| view(&s, id).labels[0].clone();
    assert_eq!(title(1).text, "Фасад 1-5");
    assert_eq!(title(2).text, "План на отм. +3,600");
    assert_eq!(title(3).text, "Разрез 1-1");
    for id in [1, 2, 3] {
        let t = title(id);
        assert!(t.height >= 1.5 * DIM_TEXT && t.height <= 2. * DIM_TEXT);
        let underline = view(&s, id).lines.iter().any(|l| {
            l.points.len() == 2
                && (l.points[0][1] - l.points[1][1]).abs() < 1e-6
                && (l.points[0][1] - (t.position[1] - 1.)).abs() < 1.5
                && (l.points[0][0] - t.position[0]).abs() < 30.
        });
        assert!(!underline, "title {} underlined", t.text);
    }
    // Section line on the plan (the vertical cut is seen edge-on there):
    // thick strokes with arrows and «1».
    let main = view(&s, 2);
    assert_eq!(
        main.lines
            .iter()
            .filter(|l| l.style == LineStyle::Section && l.object.is_none())
            .count(),
        2
    );
    // Two section numbers plus the bubble of axis 1.
    assert_eq!(main.labels.iter().filter(|l| l.text == "1").count(), 3);
    assert!(
        main.lines
            .iter()
            .filter(|l| l.points.len() == 3 && l.object.is_none())
            .count()
            >= 2,
        "arrows"
    );
}

#[test]
fn km_dimensions_use_45_degree_ticks_and_kmd_keeps_arrows() {
    let p = control_building();
    let s = generate(&p, &ga(&p)).unwrap();
    let dim = &s.dimensions[0];
    assert_eq!(dim.values_mm, vec![6000.; 4]);
    let ticks: Vec<_> = dim
        .lines
        .iter()
        .filter(|l| l.style == LineStyle::Visible)
        .collect();
    assert_eq!(ticks.len(), 5, "a tick at every extension line");
    for t in &ticks {
        let d = [
            t.points[1][0] - t.points[0][0],
            t.points[1][1] - t.points[0][1],
        ];
        assert!(((d[1].atan2(d[0])).to_degrees().abs() - 45.).abs() < 1e-9);
        assert!((seg_len(&t.points) - TICK).abs() < 1e-9 && (2. ..=4.).contains(&TICK));
    }
    for l in &dim.labels {
        assert!(
            l.text.chars().all(|c| c.is_ascii_digit()),
            "no units: {}",
            l.text
        );
        assert_eq!(l.height, DIM_TEXT);
    }
    // КМД part drawing: ГОСТ 2.307 arrows, no ticks.
    let f = control_project();
    let part = part_drawing(&f.project, ObjectId(9100), f.beam, "КМД-1").unwrap();
    let ps = generate(&f.project, &part).unwrap();
    assert!(
        ps.dimensions
            .iter()
            .flat_map(|d| &d.lines)
            .all(|l| l.style != LineStyle::Visible)
    );
    assert!(
        ps.dimensions
            .iter()
            .flat_map(|d| &d.lines)
            .any(|l| l.points.len() == 3)
    );
}

#[test]
fn axes_bubbles_and_level_marks() {
    let p = control_building();
    let s = generate(&p, &ga(&p)).unwrap();
    // Plan: all 9 grid lines dash-dot with bubbles 6–12 mm, labels 1–5, А–Г.
    let plan = view(&s, 2);
    assert_eq!(
        plan.lines
            .iter()
            .filter(|l| l.style == LineStyle::DashDot)
            .count(),
        9
    );
    let names: Vec<&str> = plan.labels.iter().map(|l| l.text.as_str()).collect();
    for n in ["1", "2", "3", "4", "5", "А", "Б", "В", "Г"] {
        assert!(names.contains(&n), "{n}");
    }
    assert!((6. ..=12.).contains(&BUBBLE));
    let axis_labels: Vec<_> = plan
        .labels
        .iter()
        .filter(|l| l.height == AXIS_TEXT)
        .collect();
    assert!(axis_labels.len() >= 9);
    assert!((1.5 * DIM_TEXT..=2. * DIM_TEXT).contains(&AXIS_TEXT));
    // Framed plan level mark.
    assert!(names.contains(&"+3,600"));
    // Elevation: vertical axes 1–5, level signs «0,000» and «+3,600».
    let front = view(&s, 1);
    assert_eq!(
        front
            .lines
            .iter()
            .filter(|l| l.style == LineStyle::DashDot)
            .count(),
        5
    );
    let texts: Vec<&str> = front.labels.iter().map(|l| l.text.as_str()).collect();
    assert!(texts.contains(&"0,000") && texts.contains(&"+3,600"));
    // Each sign: two 45° strokes of LEVEL_STROKE mm meeting on the level line.
    let signs = front
        .lines
        .iter()
        .filter(|l| l.points.len() == 3 && l.object.is_none())
        .filter(|l| {
            let (a, v, b) = (l.points[0], l.points[1], l.points[2]);
            (a[1] - b[1]).abs() < 1e-9
                && (seg_len(&[a, v]) - LEVEL_STROKE).abs() < 1e-9
                && ((a[1] - v[1]) - (v[0] - a[0])).abs() < 1e-9
        });
    assert_eq!(signs.count(), 2);
}

#[test]
fn part_marks_sit_on_shelves_with_a_dot() {
    let f = control_project();
    let d = part_drawing(&f.project, ObjectId(9100), f.beam, "КМД-1").unwrap();
    let s = generate(&f.project, &d).unwrap();
    let main = &s.views[0];
    let mark = main
        .labels
        .iter()
        .find(|l| l.object.is_some())
        .expect("part mark");
    assert!(mark.height >= 1.5 * DIM_TEXT);
    let shelf = main.lines.iter().any(|l| {
        l.points.len() == 2
            && (l.points[0][1] - l.points[1][1]).abs() < 1e-9
            && (l.points[0][1] - (mark.position[1] - 0.6)).abs() < 1e-9
    });
    assert!(shelf, "shelf under the mark");
    assert!(
        main.lines
            .iter()
            .any(|l| l.points.len() == 49 && l.style == LineStyle::Visible),
        "dot"
    );
}

/// The control building as a project file for the native run in
/// tests/gost/conventions/native.
#[test]
fn control_building_project_file() {
    let bytes = open_bim_persistence::encode(&control_building()).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/gost/conventions/native/control-building.obim");
    if std::env::var_os("OPEN_BIM_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &bytes).unwrap();
    }
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn node_view_keeps_its_region_and_is_marked_on_the_main_view() {
    let p = control_building();
    let s = generate(&p, &ga(&p)).unwrap();
    let node = view(&s, 4);
    assert_eq!(node.labels[0].text, "Узел 1");
    let [lo, hi] = node.bounds;
    assert!(
        (hi[0] - lo[0] - 1000. / 20.).abs() < 1e-6,
        "область 1000 мм в 1:20"
    );
    for l in &node.lines {
        for q in &l.points {
            assert!(
                q[0] >= lo[0] - 1e-6
                    && q[0] <= hi[0] + 1e-6
                    && q[1] >= lo[1] - 1e-6
                    && q[1] <= hi[1] + 1e-6
            );
        }
    }
    assert!(node.lines.len() > 4, "узел показывает колонну и балку");
    // Main view (elevation, 1:100): a thin circle of 500/100 = 5 mm with «1».
    let main = view(&s, 1);
    let circle = main
        .lines
        .iter()
        .find(|l| {
            l.style == LineStyle::Thin && l.points.len() == 49 && {
                let xs: Vec<f64> = l.points.iter().map(|p| p[0]).collect();
                let w = xs.iter().cloned().fold(f64::MIN, f64::max)
                    - xs.iter().cloned().fold(f64::MAX, f64::min);
                (w - 10.).abs() < 0.05
            }
        })
        .expect("окружность узла");
    let c = circle.points[0];
    assert!(
        main.labels
            .iter()
            .any(|l| l.text == "1" && (l.position[0] - c[0]).abs() < 20.)
    );
    // Sections and nodes are numbered separately (5.5.6).
    assert!(view(&s, 3).labels[0].text == "Разрез 1-1");
}
