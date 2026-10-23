use open_bim_analysis::{build, csv};
use open_bim_model::{
    AnalysisModel, Element, ElementShape, Frame, Load, LoadGroup, LoadKind, LoadShape, MemberKind,
    Members, ObjectId, PartAnalysis, PartRole, Point3, Project, RigidLink,
};

fn member(p: &mut Project, id: u64, a: [f64; 3], b: [f64; 3], kind: MemberKind) -> ObjectId {
    let (frame, length) = Frame::along(Point3(a), Point3(b)).unwrap();
    let id = ObjectId(id);
    p.elements.insert(
        id,
        Element {
            id,
            name: format!("Деталь {}", id.0),
            material: ObjectId(1),
            shape: ElementShape::Member {
                member_kind: kind,
                profile: ObjectId(2),
                length,
                frame,
                position: Default::default(),
            },
            properties: Default::default(),
        },
    );
    id
}

fn model() -> AnalysisModel {
    AnalysisModel {
        number: 1,
        name: "Рама".into(),
        members: Members::All,
        merge_tolerance: 20.,
        load_groups: Default::default(),
    }
}

/// Portal 6 m × 3 m with a middle post: the beam is split in two.
fn portal() -> (Project, [ObjectId; 4]) {
    let mut p = Project::default();
    let c1 = member(
        &mut p,
        10,
        [0., 0., 0.],
        [0., 0., 3000.],
        MemberKind::Column,
    );
    let c2 = member(
        &mut p,
        11,
        [6000., 0., 0.],
        [6000., 0., 3000.],
        MemberKind::Column,
    );
    let mid = member(
        &mut p,
        12,
        [3000., 0., 0.],
        [3000., 0., 2990.],
        MemberKind::Column,
    );
    let beam = member(
        &mut p,
        13,
        [0., 0., 3000.],
        [6000., 0., 3000.],
        MemberKind::Beam,
    );
    (p, [c1, c2, mid, beam])
}

#[test]
fn members_split_at_crossings_with_numbered_nodes() {
    let (p, [c1, _, mid, beam]) = portal();
    let a = build(&p, &model());
    // Three feet and three beam nodes; the post top 10 mm low is merged.
    assert_eq!(a.nodes.len(), 6, "{:?}", a.nodes);
    // Numbered by level, then X: feet first.
    assert_eq!(a.nodes[0], [0., 0., 0.]);
    assert_eq!(a.nodes[5], [6000., 0., 3000.]);
    assert_eq!(a.bars.len(), 5);
    assert_eq!(a.bars.iter().filter(|b| b.part == beam).count(), 2);
    assert_eq!(a.bars.iter().filter(|b| b.part == mid).count(), 1);
    assert!(a.bars.iter().all(|b| !b.hinges[0] && !b.hinges[1]));
    let col = a.bars.iter().find(|b| b.part == c1).unwrap();
    assert_eq!(col.nodes, [0, 3]);
}

#[test]
fn part_properties_links_merges_and_loads() {
    let (mut p, [c1, c2, mid, beam]) = portal();
    let an = &mut p.analysis;
    an.parts.insert(
        beam,
        PartAnalysis {
            role: PartRole::Member,
            hinges: [true, true],
            offset: [0., 0.],
        },
    );
    an.parts.insert(
        mid,
        PartAnalysis {
            role: PartRole::Truss,
            ..Default::default()
        },
    );
    an.parts.insert(
        c2,
        PartAnalysis {
            role: PartRole::Ignore,
            ..Default::default()
        },
    );
    an.nodes.push([0., 0., 1500.]);
    an.links.push(RigidLink {
        number: 1,
        a: [0., 0., 1500.],
        b: [500., 0., 1500.],
    });
    an.load_groups = vec![
        LoadGroup {
            number: 1,
            name: "Собственный вес".into(),
            kind: LoadKind::Dead,
            factor: 1.05,
        },
        LoadGroup {
            number: 2,
            name: "Снег".into(),
            kind: LoadKind::Snow,
            factor: 1.4,
        },
    ];
    for (n, g) in [(1, 1), (2, 2)] {
        an.loads.push(Load {
            number: n,
            group: g,
            shape: LoadShape::Point {
                at: [3000., 0., 3000.],
                force: [0., 0., -10.],
            },
        });
    }
    p.analysis.validate().unwrap();
    let mut m = model();
    m.load_groups.insert(2);
    let a = build(&p, &m);
    // The ignored column is gone; its foot node too.
    assert!(a.bars.iter().all(|b| b.part != c2));
    // The beam is hinged only at its outer ends.
    let beam_bars: Vec<_> = a.bars.iter().filter(|b| b.part == beam).collect();
    assert_eq!(beam_bars.len(), 2);
    assert!(beam_bars[0].hinges[0] && !beam_bars[0].hinges[1]);
    assert!(!beam_bars[1].hinges[0] && beam_bars[1].hinges[1]);
    assert!(
        a.bars
            .iter()
            .filter(|b| b.part == mid)
            .all(|b| b.truss && b.hinges == [true, true])
    );
    // The user node splits the left column; the link joins it to a free node.
    assert_eq!(a.bars.iter().filter(|b| b.part == c1).count(), 2);
    assert_eq!(a.links.len(), 1);
    // Only the snow group is taken.
    assert_eq!(a.loads.len(), 1);
    assert_eq!(a.loads[0].group, 2);
    // Merging the free link node into the column node removes the link.
    p.analysis.merges.push([[0., 0., 1500.], [500., 0., 1500.]]);
    let merged = build(&p, &m);
    assert!(merged.links.is_empty());
    assert_eq!(merged.nodes.len(), a.nodes.len() - 1);
    // CSV tables.
    let files = csv(&p, &a);
    assert_eq!(files.len(), 4);
    assert!(
        files[0]
            .1
            .starts_with("\u{feff}Узел;X, мм;Y, мм;Z, мм\r\n1;0,0;0,0;0,0\r\n")
    );
    assert!(
        files[3]
            .1
            .contains("2;2;сосредоточенная, кН;(3000,0 0,0 3000,0);(0,0 0,0 -10,0)")
    );
}

#[test]
fn listed_members_and_validation() {
    let (mut p, [c1, _, _, beam]) = portal();
    let mut m = model();
    m.members = Members::Listed {
        parts: [c1, beam].into(),
    };
    let a = build(&p, &m);
    assert_eq!(a.bars.len(), 2, "beam unsplit: the post is not taken");
    p.analysis.loads.push(Load {
        number: 1,
        group: 7,
        shape: LoadShape::Point {
            at: [0.; 3],
            force: [0.; 3],
        },
    });
    assert!(p.analysis.validate().is_err(), "no group 7");
}
