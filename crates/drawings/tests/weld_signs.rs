//! Weld signs of ГОСТ 2.312 on drawings: the designation on the shelf, the
//! circle of a weld all around and the flag of a site weld on the leader
//! break, the designation under the shelf for the reverse side.
use open_bim_drawings::{generate, sample::control_set};
use open_bim_model::{DetailKind, DrawingKind, Intermittent, ObjectId, WeldGost};

fn weld_view(
    edit: impl Fn(&mut DetailKind),
) -> (
    ObjectId,
    Vec<open_bim_drawings::Polyline>,
    Vec<open_bim_drawings::Label>,
) {
    let mut p = control_set().unwrap();
    let id = *p
        .details
        .iter()
        .find(|(_, d)| matches!(d.kind, DetailKind::Weld { .. }))
        .unwrap()
        .0;
    edit(&mut p.details.get_mut(&id).unwrap().kind);
    let drawing = p
        .drawings
        .values()
        .find(|d| matches!(d.kind, DrawingKind::Assembly { .. }))
        .unwrap()
        .clone();
    let sheet = generate(&p, &drawing).unwrap();
    let view = &sheet.views[0];
    let mine = |o: Option<ObjectId>| o == Some(id);
    (
        id,
        view.lines
            .iter()
            .filter(|l| mine(l.object))
            .cloned()
            .collect(),
        view.labels
            .iter()
            .filter(|l| mine(l.object))
            .cloned()
            .collect(),
    )
}

#[test]
fn all_around_and_site_welds_get_their_signs_on_the_leader() {
    let (_, lines, labels) = weld_view(|k| {
        if let DetailKind::Weld {
            around, shop, gost, ..
        } = k
        {
            *around = true;
            *shop = false;
            *gost = WeldGost {
                method: "УП".into(),
                ..Default::default()
            };
        }
    });
    let text: String = labels.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(text, "ГОСТ 5264-80-Т1-УП-6");
    assert!(!text.contains("контуру") && !text.contains("монт"));
    // Circle Ø3 at the break, and the «┐» flag.
    let circle = lines.iter().find(|l| l.points.len() == 25).expect("circle");
    let c = circle.points[0];
    let r = circle
        .points
        .iter()
        .map(|p| ((p[0] - c[0] + 1.5).powi(2) + (p[1] - c[1]).powi(2)).sqrt())
        .fold(0f64, f64::max);
    assert!((r - 1.5).abs() < 1e-6, "{r}");
    let flag = lines
        .iter()
        .find(|l| l.points.len() == 3 && (l.points[1][1] - l.points[0][1] - 4.).abs() < 1e-9)
        .expect("flag");
    assert!((flag.points[0][0] - flag.points[1][0]).abs() < 1e-9);
    assert!((flag.points[2][0] - flag.points[1][0]).abs() > 2.);
}

#[test]
fn reverse_side_goes_under_the_shelf() {
    let shelf_y = |lines: &[open_bim_drawings::Polyline]| {
        // Leader with the shelf: three points, the last two level.
        lines
            .iter()
            .find(|l| l.points.len() == 3 && (l.points[1][1] - l.points[2][1]).abs() < 1e-9)
            .unwrap()
            .points[1][1]
    };
    let (_, lines, labels) = weld_view(|_| {});
    assert!(labels[0].position[1] > shelf_y(&lines));
    let (_, lines, labels) = weld_view(|k| {
        if let DetailKind::Weld { gost, around, .. } = k {
            *around = false;
            gost.back = true;
            gost.intermittent = Some(Intermittent {
                length: 50.,
                pitch: 100.,
                staggered: false,
            });
        }
    });
    assert!(labels[0].position[1] < shelf_y(&lines));
    assert_eq!(labels.last().unwrap().text, "6-50/100");
}
