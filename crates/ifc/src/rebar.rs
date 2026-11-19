//! IFC4.3 reinforcing bars. One `IfcReinforcingBarType` per schedule
//! position (the bending parameters and the bar length live on the type),
//! one `IfcReinforcingBar` per bar. The body is an `IfcSweptDiskSolidPolygonal`
//! along the centreline stored in the model: the directrix is the polyline
//! through the bend corners, and `FilletRadius` is the centreline bend
//! radius `(mandrel + diameter) / 2`.
//!
//! A bar whose host was exported is aggregated into that host
//! (`IfcRelAggregates`) and contained in the host's storey. A bar without a
//! host is only contained in the spatial structure. Meshes are written as
//! the bars they are: openings and remainder steps make them not a regular
//! `IfcReinforcingMesh`.
use std::collections::BTreeMap;

use open_bim_model::concrete::ZoneRole;
use open_bim_model::rebar::{
    BarPlace, BendLeg, DetailRow, bar_places, bend_legs, detail_schedule, resolve_shape,
};
use open_bim_model::{BarSurface, ObjectId, Point3, Project, RebarKind, RebarSet, rebar_class};

use crate::export::{HostProduct, Writer, derived_guid, refs};
use crate::step::{encode, real};
use crate::{GOST_PSET_NAME, PSET_NAME};

/// Ids the bar geometry hangs from.
pub(crate) struct RebarSpace {
    pub body: u64,
    pub storey_lp: u64,
    pub default_storey: u64,
    pub identity: u64,
}

/// Write the reinforcement of `project` and return how many bars were
/// written. Contained products are pushed onto `contained` as
/// `(storey, product)` for the caller's spatial structure.
pub(crate) fn write(
    w: &mut Writer,
    project: &Project,
    space: &RebarSpace,
    hosts: &BTreeMap<ObjectId, HostProduct>,
    contained: &mut Vec<(u64, u64)>,
) -> usize {
    let rows = detail_schedule(project);
    if rows.is_empty() {
        return 0;
    }
    let places = bar_places(project);
    let sets: BTreeMap<u32, &RebarSet> = project
        .concrete
        .rebars
        .iter()
        .map(|set| (set.number, set))
        .collect();
    let mut types = BTreeMap::new();
    for row in &rows {
        let set = sets[&row.sets[0]];
        types.insert(row.mark, write_type(w, project, row, set));
    }
    let mut of_mark: BTreeMap<u32, Vec<u64>> = BTreeMap::new();
    let mut of_host: BTreeMap<ObjectId, Vec<u64>> = BTreeMap::new();
    let mut of_grade: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    let mut count = 0;
    for place in &places {
        let Some(set) = sets.get(&place.set_number) else {
            continue;
        };
        let Some(row) = rows.iter().find(|r| r.mark == place.mark) else {
            continue;
        };
        let Some((product, storey)) = write_bar(w, project, space, hosts, set, row, place) else {
            continue;
        };
        let storey = if storey == 0 {
            space.default_storey
        } else {
            storey
        };
        count += 1;
        of_mark.entry(place.mark).or_default().push(product);
        of_grade.entry(row.grade.clone()).or_default().push(product);
        contained.push((storey, product));
        if let Some(host) = set.host
            && hosts.contains_key(&host)
        {
            of_host.entry(host).or_default().push(product);
        }
    }
    for (mark, bars) in &of_mark {
        let row = rows.iter().find(|r| r.mark == *mark).unwrap();
        let ty = types[mark];
        w.add(format!(
            "IFCRELDEFINESBYTYPE('{}',$,$,$,({}),#{ty})",
            derived_guid("rebar-type-rel", u64::from(*mark)),
            refs(bars)
        ));
        write_psets(w, row, bars);
    }
    for (host, bars) in &of_host {
        w.add(format!(
            "IFCRELAGGREGATES('{}',$,$,$,#{},({}))",
            derived_guid("rebar-aggregate", host.0),
            hosts[host].product,
            refs(bars)
        ));
    }
    for (grade, bars) in &of_grade {
        let mat = w.add(format!("IFCMATERIAL({},$,$)", encode(grade)));
        let prop = w.add(format!(
            "IFCPROPERTYSINGLEVALUE('MassDensity',$,IFCMASSDENSITYMEASURE({}),$)",
            real(open_bim_model::concrete::REBAR_DENSITY)
        ));
        w.add(format!(
            "IFCMATERIALPROPERTIES('Pset_MaterialCommon',$,(#{prop}),#{mat})"
        ));
        w.add(format!(
            "IFCRELASSOCIATESMATERIAL('{}',$,$,$,({}),#{mat})",
            derived_guid(&format!("rebar-material-{grade}"), 0),
            refs(bars)
        ));
    }
    count
}

/// `(product, storey)`. Storey 0 means the caller’s default storey.
fn write_bar(
    w: &mut Writer,
    project: &Project,
    space: &RebarSpace,
    hosts: &BTreeMap<ObjectId, HostProduct>,
    set: &RebarSet,
    row: &DetailRow,
    place: &BarPlace,
) -> Option<(u64, u64)> {
    let bar = set.bars.get(place.bar_index)?;
    let (points, parent, storey) = locate(project, hosts, space.storey_lp, set, bar)?;
    let points = collapse(&points);
    if points.len() < 2 {
        return None;
    }
    let lp = w.add(format!("IFCLOCALPLACEMENT(#{parent},#{})", space.identity));
    let disk = set.diameter / 2.;
    let mandrel = open_bim_model::rebar::mandrel_diameter(project, set);
    let bend = (mandrel + set.diameter) / 2.;
    // The directrix is the centreline. Bends are circular arcs of the
    // mandrel radius; IfcSweptDiskSolidPolygonal.FilletRadius is left unused
    // because the geometry kernel does not mesh it.
    let curve = centreline(w, &points, if points.len() >= 3 { bend } else { 0. });
    let solid = w.add(format!("IFCSWEPTDISKSOLID(#{curve},{},$,$,$)", real(disk)));
    let rep = w.add(format!(
        "IFCSHAPEREPRESENTATION(#{},'Body','AdvancedSweptSolid',(#{solid}))",
        space.body
    ));
    let shape = w.add(format!("IFCPRODUCTDEFINITIONSHAPE($,$,(#{rep}))"));
    let predefined = predefined(project, set);
    let surface = surface(&row.grade);
    let area = open_bim_model::rebar::bar_area(set.diameter) * 1e-6;
    let product = w.add(format!(
        "IFCREINFORCINGBAR('{}',$,{},{},$,#{lp},#{shape},'{}',{},{},{},{},.{predefined}.,.{surface}.)",
        derived_guid(
            &format!("rebar-bar-{}", set.number),
            place.bar_index as u64
        ),
        encode(&format!("Поз. {}", row.mark)),
        encode(&row.code),
        row.mark,
        encode(&row.grade),
        real(set.diameter),
        real(area),
        real(row.length_mm.max(1.)),
    ));
    Some((product, storey))
}

fn write_type(w: &mut Writer, project: &Project, row: &DetailRow, set: &RebarSet) -> u64 {
    let legs = bend_legs(row);
    let predefined = predefined(project, set);
    let surface = surface(&row.grade);
    let area = open_bim_model::rebar::bar_area(row.diameter) * 1e-6;
    let code = if row.code.is_empty() {
        "С".to_string()
    } else {
        row.code.clone()
    };
    w.add(format!(
        "IFCREINFORCINGBARTYPE('{}',$,{},{},'IfcReinforcingBar',$,$,'{}',$,.{predefined}.,{},{},{},.{surface}.,{},{})",
        derived_guid("rebar-type", u64::from(row.mark)),
        encode(&format!("Поз. {} {code}", row.mark)),
        encode(&row.shape_id),
        row.mark,
        real(row.diameter),
        real(area),
        real(row.length_mm.max(1.)),
        encode(&code),
        parameters(&legs),
    ))
}

fn parameters(legs: &[BendLeg]) -> String {
    let mut parts = Vec::new();
    for leg in legs {
        parts.push(format!("IFCLENGTHMEASURE({})", real(leg.length_mm as f64)));
        parts.push(format!(
            "IFCPLANEANGLEMEASURE({})",
            real((leg.angle_deg as f64).to_radians())
        ));
    }
    format!("({})", parts.join(","))
}

fn write_psets(w: &mut Writer, row: &DetailRow, bars: &[u64]) {
    let length = row.length_mm.max(1.);
    let area = open_bim_model::rebar::bar_area(row.diameter) * 1e-6;
    let common = vec![
        text_prop(w, "Reference", &row.mark.to_string(), "IFCIDENTIFIER"),
        text_prop(w, "SteelGrade", &row.grade, "IFCLABEL"),
        measure(
            w,
            "NominalDiameter",
            "IFCPOSITIVELENGTHMEASURE",
            row.diameter,
        ),
        measure(w, "CrossSectionArea", "IFCAREAMEASURE", area),
        measure(w, "BarLength", "IFCPOSITIVELENGTHMEASURE", length),
    ];
    define(
        w,
        "rebar-common",
        row.mark,
        "Pset_ReinforcingBarCommon",
        &common,
        bars,
    );
    let sets = row
        .sets
        .iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let open = vec![
        text_prop(w, "Позиция", &row.mark.to_string(), "IFCLABEL"),
        text_prop(w, "Форма", &row.code, "IFCLABEL"),
        text_prop(w, "Тип формы", &row.shape_id, "IFCLABEL"),
        text_prop(w, "Класс", &row.grade, "IFCLABEL"),
        text_prop(w, "Наборы", &sets, "IFCLABEL"),
    ];
    define(w, "rebar-open", row.mark, PSET_NAME, &open, bars);
    let gost = vec![
        text_prop(w, "Сталь", &row.grade, "IFCLABEL"),
        text_prop(w, "Сортамент", &row.standard, "IFCLABEL"),
        measure(w, "Масса, кг", "IFCMASSMEASURE", row.unit_mass_kg),
    ];
    define(w, "rebar-gost", row.mark, GOST_PSET_NAME, &gost, bars);
}

fn define(w: &mut Writer, namespace: &str, mark: u32, name: &str, props: &[u64], bars: &[u64]) {
    let pset = w.add(format!(
        "IFCPROPERTYSET('{}',$,{},$,({}))",
        derived_guid(namespace, u64::from(mark)),
        encode(name),
        refs(props)
    ));
    w.add(format!(
        "IFCRELDEFINESBYPROPERTIES('{}',$,$,$,({}),#{pset})",
        derived_guid(&format!("{namespace}-rel"), u64::from(mark)),
        refs(bars)
    ));
}

fn text_prop(w: &mut Writer, name: &str, value: &str, kind: &str) -> u64 {
    w.add(format!(
        "IFCPROPERTYSINGLEVALUE({},$,{kind}({}),$)",
        encode(name),
        encode(value)
    ))
}

fn measure(w: &mut Writer, name: &str, kind: &str, value: f64) -> u64 {
    w.add(format!(
        "IFCPROPERTYSINGLEVALUE({},$,{kind}({}),$)",
        encode(name),
        real(value)
    ))
}

/// Local centreline, the placement the bar hangs from, and the storey that
/// contains it. Hosted bars hang from the host and use its frame.
fn locate(
    project: &Project,
    hosts: &BTreeMap<ObjectId, HostProduct>,
    storey_lp: u64,
    set: &RebarSet,
    bar: &[Point3],
) -> Option<(Vec<[f64; 3]>, u64, u64)> {
    if let Some(host) = set.host
        && let Some(part) = hosts.get(&host)
        && let Some(element) = project.elements.get(&host)
    {
        let frame = element.shape.frame();
        let points = bar.iter().map(|p| frame.local(*p).0).collect();
        return Some((points, part.placement, part.storey));
    }
    let origin = project.origin.0;
    let points = bar
        .iter()
        .map(|p| [p.0[0] - origin[0], p.0[1] - origin[1], p.0[2] - origin[2]])
        .collect();
    Some((points, storey_lp, 0))
}

/// Straight bars are an `IfcPolyline`. A bend becomes a line, a circular
/// arc and a line, joined in one `IfcCompositeCurve`. The arc radius is the
/// centreline radius, reduced when a leg is shorter than the tangent setback.
fn centreline(w: &mut Writer, points: &[[f64; 3]], radius: f64) -> u64 {
    let arcs = bend_arcs(points, radius);
    if arcs.is_empty() {
        let ids: Vec<u64> = points.iter().copied().map(|p| w.point(p)).collect();
        return w.add(format!("IFCPOLYLINE(({}))", refs(&ids)));
    }
    let mut segments = Vec::new();
    let mut cursor = points[0];
    for arc in &arcs {
        if dist(cursor, arc.pin) > 0.05 {
            segments.push(line_segment(w, cursor, arc.pin));
        }
        segments.push(arc_segment(w, arc));
        cursor = arc.pout;
    }
    let end = *points.last().unwrap();
    if dist(cursor, end) > 0.05 {
        segments.push(line_segment(w, cursor, end));
    }
    if segments.is_empty() {
        let ids: Vec<u64> = points.iter().copied().map(|p| w.point(p)).collect();
        return w.add(format!("IFCPOLYLINE(({}))", refs(&ids)));
    }
    w.add(format!("IFCCOMPOSITECURVE(({}),.U.)", refs(&segments)))
}

fn line_segment(w: &mut Writer, a: [f64; 3], b: [f64; 3]) -> u64 {
    let (a, b) = (w.point(a), w.point(b));
    let poly = w.add(format!("IFCPOLYLINE((#{a},#{b}))"));
    w.add(format!(
        "IFCCOMPOSITECURVESEGMENT(.CONTSAMEGRADIENT.,.T.,#{poly})"
    ))
}

fn arc_segment(w: &mut Writer, arc: &BendArc) -> u64 {
    let place = w.placement(arc.center, arc.axis, arc.ref_dir);
    let circle = w.add(format!("IFCCIRCLE(#{place},{})", real(arc.radius)));
    let trim = w.add(format!(
        "IFCTRIMMEDCURVE(#{circle},(IFCPARAMETERVALUE(0.)),(IFCPARAMETERVALUE({})),.T.,.PARAMETER.)",
        real(arc.sweep)
    ));
    w.add(format!(
        "IFCCOMPOSITECURVESEGMENT(.CONTSAMEGRADIENT.,.T.,#{trim})"
    ))
}

struct BendArc {
    pin: [f64; 3],
    pout: [f64; 3],
    center: [f64; 3],
    axis: [f64; 3],
    ref_dir: [f64; 3],
    radius: f64,
    sweep: f64,
}

fn bend_arcs(points: &[[f64; 3]], radius: f64) -> Vec<BendArc> {
    if points.len() < 3 || radius < 1. {
        return Vec::new();
    }
    let mut arcs = Vec::new();
    for i in 1..points.len() - 1 {
        let Some(arc) = bend_arc(points[i - 1], points[i], points[i + 1], radius) else {
            continue;
        };
        arcs.push(arc);
    }
    arcs
}

fn bend_arc(a: [f64; 3], b: [f64; 3], c: [f64; 3], radius: f64) -> Option<BendArc> {
    let (din, lin) = unit(sub(b, a))?;
    let (dout, lout) = unit(sub(c, b))?;
    let axis = cross(din, dout);
    let (axis, _) = unit(axis)?;
    let cos = dot(din, dout).clamp(-1., 1.);
    let theta = cos.acos();
    if !(1f64.to_radians()..170f64.to_radians()).contains(&theta) {
        return None;
    }
    let setback = (radius * (theta / 2.).tan()).min(0.45 * lin.min(lout));
    if setback < 0.5 {
        return None;
    }
    let radius = setback / (theta / 2.).tan();
    let pin = add(b, mul(din, -setback));
    let pout = add(b, mul(dout, setback));
    let normal = cross(axis, din);
    let (normal, _) = unit(normal)?;
    let center = add(pin, mul(normal, radius));
    let ref_dir = unit(sub(pin, center))?.0;
    Some(BendArc {
        pin,
        pout,
        center,
        axis,
        ref_dir,
        radius,
        sweep: theta,
    })
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    dot(sub(a, b), sub(a, b)).sqrt()
}
fn unit(a: [f64; 3]) -> Option<([f64; 3], f64)> {
    let l = dot(a, a).sqrt();
    (l > 1e-9).then(|| (mul(a, 1. / l), l))
}

fn collapse(points: &[[f64; 3]]) -> Vec<[f64; 3]> {
    let mut out = Vec::new();
    for p in points {
        if !p.iter().all(|v| v.is_finite()) {
            continue;
        }
        if out.last().is_none_or(|q: &[f64; 3]| {
            (0..3)
                .map(|i| p[i] - q[i])
                .map(|v| v * v)
                .sum::<f64>()
                .sqrt()
                > 0.05
        }) {
            out.push(*p);
        }
    }
    out
}

fn predefined(project: &Project, set: &RebarSet) -> &'static str {
    match set.role {
        Some(ZoneRole::Edge | ZoneRole::Opening) => "EDGE",
        Some(ZoneRole::Frog) => "SPACEBAR",
        Some(
            ZoneRole::BottomMain | ZoneRole::BottomCross | ZoneRole::TopMain | ZoneRole::TopCross,
        ) => "MAIN",
        None => match set.kind {
            // A closed stirrup is a ligature. A straight tie, sized from the
            // bars it crosses, is a stud.
            RebarKind::Stirrup => {
                if straight_tie(project, set) {
                    "STUD"
                } else {
                    "LIGATURE"
                }
            }
            RebarKind::Edge => "EDGE",
            RebarKind::Frog => "SPACEBAR",
            RebarKind::Longitudinal | RebarKind::Mesh | RebarKind::Group => "MAIN",
            RebarKind::Starter => "ANCHORING",
            RebarKind::Guide | RebarKind::Single | RebarKind::Shape => "NOTDEFINED",
        },
    }
}

fn straight_tie(project: &Project, set: &RebarSet) -> bool {
    set.shape_id.as_deref().is_some_and(|id| {
        resolve_shape(project, id)
            .is_some_and(|shape| shape.segments.len() == 1 && shape.segments[0].bend_deg.abs() < 1.)
    })
}

fn surface(grade: &str) -> &'static str {
    match rebar_class(grade).map(|c| c.surface) {
        Some(BarSurface::Smooth) => "PLAIN",
        _ => "TEXTURED",
    }
}
