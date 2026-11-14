use open_bim_model::rebar::Hook;
use open_bim_model::{Point3, Project, RebarKind, RebarSet};

fn project(diameter: f64, bar: Vec<Point3>) -> Project {
    let mut project = Project {
        name: "TestPDF".into(),
        ..Project::default()
    };
    project.document.base_designation = "417".into();
    project.concrete.rebars = vec![RebarSet {
        number: 1,
        kind: RebarKind::Guide,
        host: None,
        diameter,
        grade: "B500A".into(),
        bars: vec![bar],
        hooks: [Hook::default(); 2],
        bend_diameter: Some(4. * diameter),
        rule: None,
    }];
    project
}

fn checksum(line: &str) -> u32 {
    let prefix = line.split_once("@C").unwrap().0;
    96 - format!("{prefix}@C").bytes().map(u32::from).sum::<u32>() % 32
}

#[test]
fn straight_bar_and_checksum_follow_the_bvbs_example() {
    let project = project(25., vec![Point3([0.; 3]), Point3([6000., 0., 0.])]);
    let text = open_bim_reports::bvbs::export(&project).unwrap();
    let line = text.trim_end();
    assert!(
        line.starts_with(
            "BF2D@HjTestPDF@r417@i0@p1@l6000@n1@e23.118@d25@gB500A@s100@v@Gl6000@w0@C"
        )
    );
    assert!(line.ends_with(&format!("{}@", checksum(line))));
    assert!(text.ends_with("\r\n"));
    assert!(text.is_ascii());
}

#[test]
fn right_angle_uses_outer_dimensions() {
    let project = project(
        12.,
        vec![
            Point3([0., 0., 0.]),
            Point3([394., 0., 0.]),
            Point3([394., 594., 0.]),
        ],
    );
    let text = open_bim_reports::bvbs::export(&project).unwrap();
    assert!(text.contains("@p1@l1000@n1@e0.888@d12@gB500A@s48@v@"));
    assert!(text.contains("@Gl400@w90@l600@w0@C"));
}

#[test]
fn two_right_angle_corners_become_one_180_degree_hook() {
    let project = project(
        12.,
        vec![
            Point3([-64., 0., 0.]),
            Point3([0., 0., 0.]),
            Point3([0., -60., 0.]),
            Point3([-528., -60., 0.]),
            Point3([-528., 0., 0.]),
            Point3([-464., 0., 0.]),
        ],
    );
    let text = open_bim_reports::bvbs::export(&project).unwrap();
    assert!(
        text.contains("@p1@l800@n1@e0.710@d12@gB500A@s48@v@"),
        "{text}"
    );
    assert!(
        text.contains("@Gl100@w180@l600@w180@l100@w0@C72@"),
        "{text}"
    );
}

#[test]
fn non_planar_geometry_is_not_flattened() {
    let project = project(
        12.,
        vec![
            Point3([0., 0., 0.]),
            Point3([400., 0., 0.]),
            Point3([400., 400., 0.]),
            Point3([400., 400., 400.]),
        ],
    );
    let error = open_bim_reports::bvbs::export(&project).unwrap_err();
    assert!(error.contains("BF3D"));
}

#[test]
fn empty_project_produces_an_empty_abs_file() {
    assert_eq!(
        open_bim_reports::bvbs::export(&Project::default()).unwrap(),
        ""
    );
}
