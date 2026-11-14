//! BVBS 3.1 two-dimensional bending shapes (`.abs`), with outer leg
//! dimensions and the checksum defined by Bundesverband Bausoftware.
use open_bim_model::{Point3, Project, RebarSet};

fn ascii(value: &str, fallback: &str) -> String {
    let value: String = value
        .chars()
        .map(|c| {
            if c.is_ascii() && c != '@' && !c.is_control() {
                c
            } else {
                '_'
            }
        })
        .collect();
    let value = value.trim();
    if value.is_empty() {
        fallback.into()
    } else {
        value.into()
    }
}

fn number(value: f64) -> String {
    let rounded = value.round();
    if (value - rounded).abs() < 1e-6 {
        format!("{rounded:.0}")
    } else {
        format!("{value:.3}")
    }
}

fn sub(a: Point3, b: Point3) -> [f64; 3] {
    std::array::from_fn(|k| a.0[k] - b.0[k])
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|k| a[k] * b[k]).sum()
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit(v: [f64; 3]) -> Option<[f64; 3]> {
    let length = dot(v, v).sqrt();
    (length > 1e-9).then(|| v.map(|x| x / length))
}

fn direction(a: Point3, b: Point3) -> Option<[f64; 3]> {
    unit(sub(b, a))
}

/// Outer leg lengths and signed bend angles of a planar centre-line polyline.
fn geometry(set: &RebarSet, bar: &[Point3]) -> Result<(Vec<f64>, Vec<f64>), String> {
    let directions: Vec<[f64; 3]> = bar
        .windows(2)
        .map(|leg| direction(leg[0], leg[1]).ok_or("BVBS: участок нулевой длины"))
        .collect::<Result<_, _>>()?;
    let normal = directions
        .windows(2)
        .map(|d| cross(d[0], d[1]))
        .find_map(unit);
    if let Some(normal) = normal
        && directions.iter().any(|d| dot(*d, normal).abs() > 1e-7)
    {
        return Err("BVBS BF2D: пространственный стержень требует BF3D".into());
    }
    let r = set.bend_radius();
    let mut lengths: Vec<f64> = bar.windows(2).map(RebarSet::bar_length).collect();
    let mut angles = vec![0.; lengths.len()];
    let mut chord_of_semicircle = vec![false; lengths.len()];
    let signed_angle = |i: usize| {
        let (u, v) = (directions[i - 1], directions[i]);
        let angle = dot(u, v).clamp(-1., 1.).acos();
        let sign = normal.map_or(1., |n| if dot(cross(u, v), n) < 0. { -1. } else { 1. });
        angle * sign
    };
    let mut i = 1;
    while i < bar.len() - 1 {
        let angle = signed_angle(i);
        if i + 1 < bar.len() - 1 {
            let next = signed_angle(i + 1);
            if (angle.abs() - std::f64::consts::FRAC_PI_2).abs() < 1e-9
                && (next - angle).abs() < 1e-9
                && (lengths[i] - 2. * r).abs() < 1e-6
            {
                let adjustment = r + set.diameter / 2.;
                lengths[i - 1] += adjustment;
                lengths[i + 1] += adjustment;
                angles[i - 1] = 180. * angle.signum();
                chord_of_semicircle[i] = true;
                i += 2;
                continue;
            }
        }
        angles[i - 1] = angle.to_degrees();
        let adjustment = if (angle.abs() - std::f64::consts::PI).abs() < 1e-9 {
            r + set.diameter / 2.
        } else {
            set.diameter / 2. * (angle.abs() / 2.).tan()
        };
        lengths[i - 1] += adjustment;
        lengths[i] += adjustment;
        i += 1;
    }
    let mut outer = Vec::new();
    let mut bends = Vec::new();
    for i in 0..lengths.len() {
        if !chord_of_semicircle[i] {
            outer.push(lengths[i]);
            bends.push(angles[i]);
        }
    }
    Ok((outer, bends))
}

fn record(
    project: &Project,
    row: &crate::rebar::RebarPositionRow,
    set: &RebarSet,
    bar: &[Point3],
) -> Result<String, String> {
    let (lengths, angles) = geometry(set, bar)?;
    let total: f64 = lengths.iter().sum();
    let mass = total / 1000. * open_bim_model::rebar::mass_per_m(set.diameter);
    let designation = if project.document.base_designation.is_empty() {
        "1"
    } else {
        &project.document.base_designation
    };
    let mut text = format!(
        "BF2D@Hj{}@r{}@i{}@p{}@l{}@n{}@e{}@d{}@g{}@s{}@v@G",
        ascii(&project.name, "OpenBIM"),
        ascii(designation, "1"),
        project.revision,
        row.position,
        number(total),
        row.quantity,
        number(mass),
        number(set.diameter),
        ascii(&set.grade, "A500C"),
        number(set.mandrel()),
    );
    for (length, angle) in lengths.into_iter().zip(angles) {
        text.push_str(&format!("l{}@w{}@", number(length), number(angle)));
    }
    text.push('C');
    let checksum = 96 - text.bytes().map(u32::from).sum::<u32>() % 32;
    text.push_str(&format!("{checksum}@\r\n"));
    if text.len() > 1000 {
        return Err(format!(
            "BVBS: позиция {} длиннее 1000 символов",
            row.position
        ));
    }
    Ok(text)
}

/// One BF2D record per automatic reinforcement position. Non-planar bars are
/// rejected explicitly; no record with substituted or flattened geometry is
/// written.
pub fn export(project: &Project) -> Result<String, String> {
    let (schedule, positions) = crate::rebar::schedule(project);
    let mut text = String::new();
    for row in &schedule.rows {
        let reference = positions
            .iter()
            .find(|p| p.position == row.position)
            .ok_or("BVBS: позиция без стержня")?;
        let set = project
            .concrete
            .rebars
            .iter()
            .find(|s| s.number == reference.set)
            .ok_or("BVBS: набор стержня не найден")?;
        let bar = set
            .bars
            .get(reference.bar)
            .ok_or("BVBS: стержень не найден")?;
        text.push_str(&record(project, row, set, bar)?);
    }
    Ok(text)
}
