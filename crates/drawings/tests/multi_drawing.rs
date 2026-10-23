//! «Комплексный чертёж»: several parts or assemblies on one sheet, each view
//! drawn in the frame of its own object.
use open_bim_drawings::sample::control_project;
use open_bim_drawings::{
    anchor_candidates, generate, multi_drawing, part_drawing, referenced_objects, set_mark,
};
use open_bim_model::{Drawing, DrawingKind, ObjectId};

#[test]
fn parts_get_their_own_views_on_one_sheet() {
    let f = control_project();
    let p = &f.project;
    let d = multi_drawing(p, ObjectId(900), vec![f.beam, f.plate], "КМД-9").unwrap();
    d.validate().unwrap();
    assert_eq!(set_mark(&d.kind), "КМД");
    // The beam has a front view and a view from above, the plate one view.
    let sources: Vec<_> = d.views.iter().map(|v| v.source).collect();
    assert_eq!(
        sources,
        vec![Some(f.beam), Some(f.beam), Some(f.plate)],
        "{:?}",
        d.views
    );
    assert_eq!(d.dimensions.len(), 1, "beam length");
    let sheet = generate(p, &d).unwrap();
    assert_eq!(sheet.views.len(), 3);
    let [w, h] = d.sheet_mm();
    for v in &sheet.views {
        assert!(!v.lines.is_empty(), "view {} is empty", v.id);
        let [lo, hi] = v.bounds;
        assert!(lo[0] >= 0. && lo[1] >= 0. && hi[0] <= w && hi[1] <= h);
    }
    // Views do not overlap.
    for (i, a) in sheet.views.iter().enumerate() {
        for b in &sheet.views[i + 1..] {
            let apart = a.bounds[1][0] <= b.bounds[0][0]
                || b.bounds[1][0] <= a.bounds[0][0]
                || a.bounds[1][1] <= b.bounds[0][1]
                || b.bounds[1][1] <= a.bounds[0][1];
            assert!(apart, "views {} and {} overlap", a.id, b.id);
        }
    }
    // The beam front view looks like the beam on its own part drawing.
    let single = part_drawing(p, ObjectId(901), f.beam, "КМД-1").unwrap();
    let alone = generate(p, &single).unwrap();
    let size = |b: [[f64; 2]; 2], s: f64| [(b[1][0] - b[0][0]) * s, (b[1][1] - b[0][1]) * s];
    let (m, s) = (&sheet.views[0], &alone.views[0]);
    let (a, b) = (size(m.bounds, m.scale), size(s.bounds, s.scale));
    assert!(
        (a[0] - b[0]).abs() < 1. && (a[1] - b[1]).abs() < 1.,
        "{a:?} {b:?}"
    );
    // Anchors are offered only on the views of their own object.
    let anchors = anchor_candidates(p, &d).unwrap();
    assert!(
        anchors
            .iter()
            .all(|(view, a, _)| d.views[*view as usize - 1].source == Some(a.object))
    );
    assert!(referenced_objects(p, &d).contains(&f.plate));
}

#[test]
fn assemblies_and_validation() {
    let f = control_project();
    let d = multi_drawing(&f.project, ObjectId(900), vec![f.assembly], "КМД-9").unwrap();
    assert!(d.views.iter().all(|v| v.source == Some(f.assembly)));
    generate(&f.project, &d).unwrap();
    // Saved and read back with the view sources.
    let json = serde_json::to_string(&d).unwrap();
    let back: Drawing = serde_json::from_str(&json).unwrap();
    assert_eq!(back, d);
    // A view without its object, or no objects, is refused.
    let mut bad = d.clone();
    bad.views[0].source = None;
    assert!(bad.validate().is_err());
    let mut bad = d.clone();
    bad.kind = DrawingKind::Multi {
        objects: Vec::new(),
    };
    assert!(bad.validate().is_err());
    assert!(multi_drawing(&f.project, ObjectId(900), Vec::new(), "К").is_err());
}
