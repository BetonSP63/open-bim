//! Sizes of a connection picked from the section of its main part
//! («подбор по профилю»): bolt diameter by the section height, plate
//! thickness from the web and flange, plate heights from the web between
//! the flanges. Rounded to usual sheet thicknesses and 5/10 mm steps; the
//! result still goes through the geometry check of the connection.
use crate::norms::{hole_clearance, min_weld};
use open_bim_model::{
    ConnectionInstance, ConnectionTemplate, ElementShape, ProfileShape, Project, TemplateKind,
};

/// Usual sheet thicknesses, mm.
pub const SHEETS: [f64; 15] = [
    6., 8., 10., 12., 14., 16., 18., 20., 22., 25., 28., 30., 32., 36., 40.,
];

fn sheet(min: f64) -> f64 {
    SHEETS
        .into_iter()
        .find(|t| *t >= min - 1e-9)
        .unwrap_or(SHEETS[SHEETS.len() - 1])
}

fn up(x: f64, step: f64) -> f64 {
    (x / step).ceil() * step
}

fn down(x: f64, step: f64) -> f64 {
    (x / step).floor() * step
}

/// Section of the main part as (height, width, web, flange) and whether it
/// is an angle (its thickness then stands for web and flange).
fn section(project: &Project, instance: &ConnectionInstance) -> Option<([f64; 4], bool)> {
    let ElementShape::Member { profile, .. } = &project.elements.get(&instance.primary)?.shape
    else {
        return None;
    };
    match project.profiles.get(profile)?.shape {
        ProfileShape::ISection {
            height,
            width,
            web,
            flange,
            ..
        } => Some(([height, width, web, flange], false)),
        ProfileShape::Angle {
            height,
            width,
            thickness,
            ..
        } => Some(([height, width, thickness, thickness], true)),
        _ => None,
    }
}

/// Bolt diameter for a section `h` high, mm.
pub fn bolt_for_height(h: f64) -> f64 {
    match h {
        h if h < 220. => 16.,
        h if h < 450. => 20.,
        h if h < 650. => 24.,
        _ => 27.,
    }
}

/// The template of `instance` with sizes picked from its main part, or
/// `None` when the main part has no I-section (an angle for a gusset).
pub fn fit_to_profile(
    project: &Project,
    instance: &ConnectionInstance,
) -> Option<ConnectionTemplate> {
    let ([h, b, tw, tf], angle) = section(project, instance)?;
    if angle != matches!(instance.template.kind, TemplateKind::Gusset { .. }) {
        return None;
    }
    let mut t = instance.template.clone();
    let d = if angle {
        if h < 90. { 16. } else { 20. }
    } else {
        bolt_for_height(h)
    };
    t.bolt_diameter = d;
    t.clearance = hole_clearance(&t.bolt_standard, d);
    let hole = d + t.clearance;
    let e = up(2. * hole, 5.);
    t.edge_distance = e;
    let pitch = up(2.5 * hole, 5.).max(up(3. * d, 10.));
    // Web between the flanges, less the root fillets.
    let web = h - 2. * tf;
    let rows_in = |height: f64| (((height - 2. * e) / pitch).floor() as u32 + 1).clamp(1, 12);
    match &mut t.kind {
        TemplateKind::BasePlate { overhang } => {
            *overhang = up(e + d, 5.);
            t.thickness = sheet((1.5 * tf).max(20.));
        }
        TemplateKind::EndPlate { overhang } => {
            *overhang = up(e + d / 2. + 5., 5.);
            t.thickness = sheet(d.max(10.));
        }
        TemplateKind::ClipAngle {
            leg,
            height,
            pitch: p,
        } => {
            *height = down(0.7 * web, 10.).max(2. * e + pitch);
            *p = pitch;
            t.thickness = sheet(tw.max(8.));
            // Room for the bolts to both parts on each leg.
            *leg = up(2. * e + t.thickness + 1., 5.);
        }
        TemplateKind::FinPlate {
            height,
            lap,
            pitch: p,
            rows,
            ..
        } => {
            *lap = (2. * e).max(*lap);
            *height = down(0.7 * web, 10.).max(2. * e);
            *p = pitch;
            *rows = rows_in(*height);
            *height = (*height).max(2. * e + pitch * f64::from(*rows - 1));
            t.thickness = sheet((tw + 2.).max(8.));
        }
        TemplateKind::ColumnStiffeners { chamfer } => {
            *chamfer = up(tf + 5., 5.).min(100.);
            t.thickness = sheet(tf.max(8.));
        }
        TemplateKind::BeamSplice { rows, pitch: p } => {
            *p = pitch;
            *rows = rows_in(web - 20.);
            t.thickness = sheet((tw * 0.75).max(tf * 0.6).max(8.));
        }
        TemplateKind::Haunch { length, depth } => {
            *length = up(1.5 * h, 10.);
            *depth = up(h / 2., 10.);
            t.thickness = sheet(tw.max(8.));
        }
        TemplateKind::Gusset { bolts, pitch: p } => {
            *p = pitch;
            *bolts = (*bolts).max(2);
            t.thickness = sheet((tw + 2.).max(8.));
        }
        TemplateKind::Seat {
            length,
            width,
            depth,
        } => {
            *length = up(2. * e + 40., 10.).max(*length);
            *width = up(b + 40., 10.);
            *depth = up(h / 2., 10.);
            t.thickness = sheet((tf + 4.).max(12.));
        }
    }
    t.weld_size = min_weld(t.thickness).max(t.weld_size.min(1.2 * t.thickness).floor());
    Some(t)
}
