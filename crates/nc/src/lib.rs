//! NC data for steel fabrication machines in the DSTV format (`.nc1`).
//!
//! One file per part mark: the `ST` header (order, drawing, phase, mark,
//! steel, quantity, section and its dimensions, angles of skewed end cuts),
//! face contours in `AK` (outer) and `IK` (inner), holes and slotted holes
//! in `BO`, the part mark in `SI`, then `EN`. Coordinates are millimetres
//! from the start of the piece. On the web (`v`) and the back wall (`h`)
//! `y` runs up from the underside of the section; on the flanges (`o`, `u`)
//! it runs from the edge on the web side of channels and angles. Plates lie
//! with their long side along `x`. Plates always get their contour; members
//! get a face contour where copes, openings or cuts at an angle to the axis
//! change that face. What the format here does not carry (cuts through part
//! of a wall, cuts at an angle to a wall, holes and cuts in round tubes) is
//! listed as warnings and repeated in the file as `**` comments.
mod faces;

use faces::{Effect, Wall};
use open_bim_model::{DetailKind, ElementShape, ObjectId, ProfileShape, Project};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Face of the piece in DSTV terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Face {
    /// `v`: web, front wall, plate surface, vertical leg of an angle.
    Web,
    /// `o`: top flange or wall.
    Top,
    /// `u`: bottom flange or wall, horizontal leg of an angle.
    Bottom,
    /// `h`: back wall of a rectangular tube.
    Back,
}

impl Face {
    pub fn letter(self) -> char {
        match self {
            Self::Web => 'v',
            Self::Top => 'o',
            Self::Bottom => 'u',
            Self::Back => 'h',
        }
    }
}

#[derive(Clone, Debug)]
pub struct NcFile {
    /// File name, `<mark>.nc1`.
    pub name: String,
    pub text: String,
    /// Piece length, mm.
    pub length: f64,
    /// Round holes: face, x, y, diameter (mm), sorted.
    pub holes: Vec<(Face, f64, f64, f64)>,
    /// Slotted holes: face, x and y of the middle, diameter, slot length
    /// (centre distance of the half circles), angle of the slot to x (°).
    pub slots: Vec<(Face, f64, f64, f64, f64, f64)>,
    /// Face contours written as `AK` (outer) and `IK` (inner, `true`).
    pub contours: Vec<(Face, bool, Vec<[f64; 2]>)>,
    /// Part mark (`SI`): face, lower left corner of the text, text.
    pub marking: Option<(Face, f64, f64, String)>,
    /// Cut angles of the header: web start, web end, flange start, flange end (°).
    pub angles: [f64; 4],
    /// Features of the part the file does not carry.
    pub warnings: Vec<String>,
    /// Separate UTF-8 bending card for developed stock.
    pub bending_card: Option<String>,
}

/// Export settings.
#[derive(Clone, Debug)]
pub struct NcOptions {
    /// Write the part mark for hard stamping (`SI`).
    pub marking: bool,
    /// Letter height of the mark, mm.
    pub text_height: f64,
}

impl Default for NcOptions {
    fn default() -> Self {
        Self {
            marking: true,
            text_height: 10.0,
        }
    }
}

/// Files for all parts of the project, one per part mark of the current
/// numbering, with the number of parts of that mark as the quantity.
pub fn export(project: &Project) -> Result<Vec<NcFile>, String> {
    export_with(project, &NcOptions::default())
}

/// [`export`] with the given settings.
pub fn export_with(project: &Project, options: &NcOptions) -> Result<Vec<NcFile>, String> {
    project.validate().map_err(|e| e.to_string())?;
    let numbering = open_bim_numbering::number(project).map_err(|e| e.to_string())?;
    let mut by_mark: BTreeMap<&String, Vec<ObjectId>> = BTreeMap::new();
    for (id, mark) in &numbering.parts {
        by_mark.entry(mark).or_default().push(*id);
    }
    let mut files = Vec::new();
    let mut names = std::collections::BTreeSet::new();
    for (mark, ids) in by_mark {
        let mut batch = Vec::new();
        match user_section(project, ids[0]) {
            Some(true) => batch = welded_files(project, ids[0], mark, ids.len(), options)?,
            // Contour sections have no DSTV profile code.
            Some(false) => {}
            None => batch.push(part_file_with(project, ids[0], mark, ids.len(), options)?),
        }
        for mut file in batch {
            if !names.insert(file.name.clone()) {
                let stem = file.name.trim_end_matches(".nc1").to_string();
                file.name = format!("{stem}_{}.nc1", ids[0]);
                names.insert(file.name.clone());
            }
            files.push(file);
        }
    }
    Ok(files)
}

/// `Some(true)` for a member with a welded section, `Some(false)` for a
/// contour section, `None` otherwise.
fn user_section(project: &Project, id: ObjectId) -> Option<bool> {
    let ElementShape::Member { profile, .. } = &project.elements.get(&id)?.shape else {
        return None;
    };
    match project.profiles.get(profile)?.shape {
        ProfileShape::Welded { .. } => Some(true),
        ProfileShape::Polygon { .. } => Some(false),
        _ => None,
    }
}

/// Plate files of a welded member, marks «5.1», «5.2», …: each plate as a
/// strip of the member length. Holes and cuts of the member are not passed.
fn welded_files(
    project: &Project,
    id: ObjectId,
    mark: &str,
    quantity: usize,
    options: &NcOptions,
) -> Result<Vec<NcFile>, String> {
    let pieces = project
        .welded_pieces(id)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let featured = project.details.values().any(|d| d.kind.part() == Some(id));
    let mut out = Vec::new();
    for (i, piece) in pieces.iter().enumerate() {
        let mut strip = project.clone();
        strip.details.retain(|_, d| d.kind.part() != Some(id));
        let element = strip.elements.get_mut(&id).ok_or("Нет детали")?;
        let frame = *element.shape.frame();
        element.shape = ElementShape::Plate {
            contour: vec![
                [0.0, 0.0],
                [piece.length, 0.0],
                [piece.length, piece.breadth],
                [0.0, piece.breadth],
            ],
            thickness: open_bim_model::LengthMm::new(piece.thickness).map_err(|e| e.to_string())?,
            frame,
        };
        let mut file = part_file_with(&strip, id, &format!("{mark}.{}", i + 1), quantity, options)?;
        if featured {
            file.warnings
                .push("отверстия и резы сварного сечения не переданы".into());
        }
        out.push(file);
    }
    Ok(out)
}

/// Section data of the header.
struct Section {
    profile: String,
    code: &'static str,
    height: f64,
    width: f64,
    flange: f64,
    web: f64,
    area: f64,
    surface: f64,
}

/// File of one part with the given mark and quantity.
pub fn part_file(
    project: &Project,
    id: ObjectId,
    mark: &str,
    quantity: usize,
) -> Result<NcFile, String> {
    part_file_with(project, id, mark, quantity, &NcOptions::default())
}

/// [`part_file`] with the given settings.
pub fn part_file_with(
    project: &Project,
    id: ObjectId,
    mark: &str,
    quantity: usize,
    options: &NcOptions,
) -> Result<NcFile, String> {
    let element = project
        .elements
        .get(&id)
        .ok_or_else(|| format!("Нет детали {id}"))?;
    if project.swept_parts.contains_key(&id) {
        let blank = project.fabrication_blank(id).map_err(|e| e.to_string())?;
        let mut card = project
            .bending_card(id, mark, quantity)
            .map_err(|e| e.to_string())?;
        let mut stock = project.clone();
        stock.swept_parts.remove(&id);
        stock.elements.insert(id, blank);
        let mut file = part_file_with(&stock, id, mark, quantity, options)?;
        file.text = file.text.replacen(
            "ST\n",
            "ST\n** DEVELOPED STOCK BEFORE BENDING; SEE BENDING CARD\n",
            1,
        );
        file.warnings
            .push("Раскрой заготовки до гибки; см. отдельную карту гибки".into());
        if let Some(open_bim_model::SweptPart::BentPlate {
            width,
            first_length,
            radius,
            angle,
            ..
        }) = project.swept_parts.get(&id)
            && let ElementShape::Plate { thickness, .. } = element.shape
        {
            let developed = project
                .part_developed_length(id)
                .map_err(|e| e.to_string())?;
            let station = first_length + (radius + thickness.value() / 2.) * angle.abs() / 2.;
            let (axis, coordinate) = if *width > developed {
                ("Y", developed - station)
            } else {
                ("X", station)
            };
            writeln!(card,"Ось сгиба в координатах файла NC: {axis} = {coordinate:.3} мм (длинная сторона заготовки направлена по X).").unwrap();
        }
        file.bending_card = Some(card);
        return Ok(file);
    }
    let material = project
        .materials
        .get(&element.material)
        .ok_or_else(|| format!("Нет материала детали {id}"))?;
    let mut warnings = Vec::new();
    let details: Vec<&DetailKind> = project
        .details
        .values()
        .map(|d| &d.kind)
        .filter(|k| k.part() == Some(id))
        .collect();
    let planes: Vec<([f64; 3], f64)> = details
        .iter()
        .filter_map(|k| match k {
            DetailKind::PlaneCut { normal, offset, .. } => {
                let n = normal.iter().map(|v| v * v).sum::<f64>().sqrt();
                Some((normal.map(|v| v / n), offset / n))
            }
            _ => None,
        })
        .collect();
    // Piece start in wall `u` and `w`, length and angles of the end cuts.
    let (section, walls, plate, start, length, angles) = match &element.shape {
        ElementShape::Member {
            profile, length, ..
        } => {
            let profile = project
                .profiles
                .get(profile)
                .ok_or_else(|| format!("Нет профиля детали {id}"))?;
            let section = member_section(&profile.name, &profile.shape)
                .map_err(|e| format!("Профиль {}: {e}", profile.name))?;
            let (start, end) = member_extent(&profile.shape, length.value(), &planes)?;
            let walls = member_walls(&profile.shape, length.value());
            if walls.is_empty()
                && details
                    .iter()
                    .any(|k| matches!(k, DetailKind::BoxCut { .. } | DetailKind::PrismCut { .. }))
            {
                warnings.push("вырез в круглой трубе не передан".into());
            }
            let angles = end_angles(&planes, &mut warnings);
            (section, walls, false, [start, 0.0], end - start, angles)
        }
        ElementShape::Plate {
            contour, thickness, ..
        } => {
            let t = thickness.value();
            let (min, max) = bounds(contour);
            let (dx, dy) = (max[0] - min[0], max[1] - min[1]);
            // Long side along x: turn a quarter clockwise when the plate is tall.
            let (origin, u, w) = if dy > dx {
                ([max[0], min[1], 0.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0])
            } else {
                ([min[0], min[1], 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0])
            };
            let mut wall = Wall {
                face: Face::Web,
                origin,
                u,
                w,
                c: [0.0, 0.0, 1.0],
                through: (0.0, t),
                base: Vec::new(),
            };
            wall.base = contour
                .iter()
                .map(|p| {
                    let q = wall.project([p[0], p[1], 0.0]);
                    [q[0], q[1]]
                })
                .collect();
            if faces::area(&wall.base) < 0.0 {
                wall.base.reverse();
            }
            let section = Section {
                profile: String::new(),
                code: "B",
                height: 0.0,
                width: t,
                flange: t,
                web: t,
                area: 0.0,
                surface: 0.0,
            };
            (section, vec![wall], true, [0.0; 2], 0.0, [0.0; 4])
        }
    };
    let (mut section, mut start, mut length) = (section, start, length);

    // Face regions.
    let mut contours: Vec<(Face, bool, Vec<[f64; 2]>)> = Vec::new();
    let mut regions: Vec<Vec<Vec<[f64; 2]>>> = Vec::new();
    for wall in &walls {
        let (mut halves, mut removed, mut gone, mut touched) =
            (Vec::new(), Vec::new(), false, false);
        for kind in &details {
            match wall.effect(kind) {
                Effect::None => {}
                Effect::Half(a, k) => {
                    halves.push((a, k));
                    // Cuts square to the face are carried by the length and the angles.
                    touched |= a[1].abs() > 1e-9;
                }
                Effect::Remove(polys) => {
                    removed.extend(polys);
                    touched = true;
                }
                Effect::Gone => gone = true,
                Effect::Unsupported(why) => warnings.push(why.into()),
            }
        }
        if gone {
            regions.push(Vec::new());
            continue;
        }
        let loops = faces::region(&wall.base, &halves, &removed);
        if loops.iter().filter(|l| faces::area(l) > 0.0).count() > 1 {
            warnings.push(format!(
                "грань {} разделена резами на части",
                wall.face.letter()
            ));
        }
        if touched || plate {
            for l in &loops {
                contours.push((wall.face, faces::area(l) < 0.0, l.clone()));
            }
        }
        regions.push(loops);
    }
    if plate {
        let outer: Vec<[f64; 2]> = regions[0]
            .iter()
            .filter(|l| faces::area(l) > 0.0)
            .flatten()
            .copied()
            .collect();
        if outer.is_empty() {
            return Err(format!("Деталь {id}: резы не оставили листа"));
        }
        let (min, max) = bounds(&outer);
        start = min;
        length = max[0] - min[0];
        let width = max[1] - min[1];
        let t = section.width;
        section.profile = format!("PL{}x{}", fmt(t), fmt(width));
        section.height = width;
        section.area = width * t;
        section.surface = 2.0 * (width + t);
    }
    if length <= 0.001 {
        return Err(format!("Деталь {id}: резы не оставили длины"));
    }
    let shift = |p: [f64; 2]| [p[0] - start[0], p[1] - start[1]];
    for c in &mut contours {
        c.2 = c.2.iter().map(|p| shift(*p)).collect();
    }
    for r in &mut regions {
        for l in r.iter_mut() {
            *l = l.iter().map(|p| shift(*p)).collect();
        }
    }

    // Holes on the walls they cross.
    let mut holes = Vec::new();
    let mut slots = Vec::new();
    for kind in &details {
        let DetailKind::Hole {
            frame,
            diameter,
            depth,
            slot,
            ..
        } = kind
        else {
            continue;
        };
        let mut placed = false;
        for wall in &walls {
            let along = faces_dot(frame.axes[2], wall.c);
            if (along.abs() - 1.0).abs() > 1e-6 {
                continue;
            }
            let q = wall.project(frame.origin.0);
            let (a, b) = (q[2], q[2] + along * depth);
            let (from, to) = wall.through;
            if a.min(b) < to - 1e-6 && a.max(b) > from + 1e-6 {
                let [x, y] = shift([q[0], q[1]]);
                if *slot > 0.0 {
                    let (cu, cw) = (
                        faces_dot(frame.axes[0], wall.u),
                        faces_dot(frame.axes[0], wall.w),
                    );
                    let mut angle = cw.atan2(cu).to_degrees();
                    if angle > 90.0 + 1e-9 {
                        angle -= 180.0;
                    } else if angle <= -90.0 + 1e-9 {
                        angle += 180.0;
                    }
                    let angle = if angle.abs() < 1e-9 { 0.0 } else { angle };
                    slots.push((wall.face, x, y, *diameter, *slot, angle));
                } else {
                    holes.push((wall.face, x, y, *diameter));
                }
                placed = true;
            }
        }
        if !placed {
            warnings.push(if plate {
                format!("отверстие ⌀{} не по нормали к листу", fmt(*diameter))
            } else {
                format!("отверстие ⌀{} не передано", fmt(*diameter))
            });
        }
    }
    holes.sort_by(|a, b| {
        (a.0, a.1, a.2)
            .partial_cmp(&(b.0, b.1, b.2))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    slots.sort_by(|a, b| {
        (a.0, a.1, a.2)
            .partial_cmp(&(b.0, b.1, b.2))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Part mark on the front face, clear of holes and cuts.
    let marking = if options.marking {
        let text: String = ascii(mark)
            .chars()
            .filter(|c| !c.is_ascii_control())
            .take(40)
            .collect();
        let place = walls
            .first()
            .zip(regions.first())
            .and_then(|(wall, loops)| {
                mark_place(
                    wall,
                    loops,
                    &start,
                    &holes,
                    &slots,
                    &text,
                    options.text_height,
                    length,
                )
            });
        match place {
            Some((x, y)) => Some((walls[0].face, x, y, text)),
            None => {
                // Round tubes have no flat face for the mark.
                if !text.is_empty() && !walls.is_empty() {
                    warnings.push("маркировка не помещается на грани".into());
                }
                None
            }
        }
    } else {
        None
    };
    warnings.sort();
    warnings.dedup();

    let steel = material.name.split(" · ").next().unwrap_or_default();
    let mut text = String::new();
    let mut line = |s: &str| {
        text.push_str(s);
        text.push('\n');
    };
    line("ST");
    line(&format!("** Open BIM, {}", ascii(&element.name)));
    let value = |s: &str| format!("  {}", one_line(s));
    let number = |v: f64| format!("  {v:>10.2}");
    for field in [
        &ascii(&project.name),
        &ascii(mark),
        "1",
        &ascii(mark),
        &ascii(steel),
        &quantity.to_string(),
        &ascii(&section.profile),
        section.code,
    ] {
        line(&value(field));
    }
    for v in [
        length,
        section.height,
        section.width,
        section.flange,
        section.web,
        0.0,
        section.area * material.density_kg_m3 / 1e6,
        section.surface / 1000.0,
        angles[0],
        angles[1],
        angles[2],
        angles[3],
    ] {
        line(&number(v));
    }
    for info in [ascii(&element.name), "-".into(), "-".into(), "-".into()] {
        line(&value(&info));
    }
    for inner in [false, true] {
        for (face, _, points) in contours.iter().filter(|c| c.1 == inner) {
            line(if inner { "IK" } else { "AK" });
            for p in points.iter().chain(points.first()) {
                line(&format!(
                    "  {} {:>10.2} {:>10.2} {:>8.2}",
                    face.letter(),
                    p[0],
                    p[1],
                    0.0
                ));
            }
        }
    }
    if !holes.is_empty() || !slots.is_empty() {
        line("BO");
        for (face, x, y, d) in &holes {
            line(&format!("  {} {x:>10.2} {y:>10.2} {d:>8.2}", face.letter()));
        }
        for (face, x, y, d, slot, angle) in &slots {
            line(&format!(
                "  {} {x:>10.2} {y:>10.2} {d:>8.2} {:>8.2}l {slot:>8.2} {:>8.2} {angle:>8.2}",
                face.letter(),
                0.0,
                0.0
            ));
        }
    }
    if let Some((face, x, y, mark_text)) = &marking {
        line("SI");
        line(&format!(
            "  {} {x:>10.2} {y:>10.2} {:>8.2} {} {mark_text}",
            face.letter(),
            0.0,
            fmt(options.text_height)
        ));
    }
    for warning in &warnings {
        line(&format!("** {}", ascii(warning)));
    }
    line("EN");
    Ok(NcFile {
        name: format!("{}.nc1", file_stem(mark)),
        text,
        length,
        holes,
        slots,
        contours,
        marking,
        angles,
        warnings,
        bending_card: None,
    })
}

fn faces_dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Start and end of a member along its axis after the plane cuts: the
/// extreme points of the section outline between the cut planes.
fn member_extent(
    shape: &ProfileShape,
    length: f64,
    planes: &[([f64; 3], f64)],
) -> Result<(f64, f64), String> {
    let outline: Vec<[f64; 2]> = match *shape {
        ProfileShape::CircularHollow { diameter, .. } => (0..64)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / 64.0;
                [diameter / 2.0 * a.cos(), diameter / 2.0 * a.sin()]
            })
            .collect(),
        ProfileShape::RectangularHollow { height, width, .. } => vec![
            [-width / 2.0, -height / 2.0],
            [width / 2.0, -height / 2.0],
            [width / 2.0, height / 2.0],
            [-width / 2.0, height / 2.0],
        ],
        _ => shape.solid_contour().map_err(|e| e.to_string())?,
    };
    let (mut start, mut end) = (f64::MAX, f64::MIN);
    for p in outline {
        let (mut lo, mut hi) = (0.0_f64, length);
        for (n, k) in planes {
            let b = k - n[0] * p[0] - n[1] * p[1];
            if n[2].abs() < 1e-9 {
                if b < -1e-9 {
                    hi = lo - 1.0;
                }
            } else if n[2] > 0.0 {
                hi = hi.min(b / n[2]);
            } else {
                lo = lo.max(b / n[2]);
            }
        }
        if hi > lo {
            start = start.min(lo);
            end = end.max(hi);
        }
    }
    if end - start <= 0.001 {
        return Err("резы не оставили длины".into());
    }
    Ok((start, end))
}

/// Cut angles of the header, degrees from square: web start, web end,
/// flange start, flange end. Positive where the piece is longer on the
/// side of larger `y` of the face (top of the web; the flange edge away
/// from the reference edge).
fn end_angles(planes: &[([f64; 3], f64)], warnings: &mut Vec<String>) -> [f64; 4] {
    let mut out = [0.0; 4];
    for (end, sign) in [(0, -1.0), (1, 1.0)] {
        let skew: Vec<_> = planes
            .iter()
            .filter(|(n, _)| n[2] * sign > 0.1 && (n[0].abs() > 1e-9 || n[1].abs() > 1e-9))
            .collect();
        match skew.as_slice() {
            [] => {}
            [(n, _)] => {
                let at = |v: f64| {
                    let a = (-sign * v / n[2]).atan().to_degrees();
                    if a.abs() < 1e-9 { 0.0 } else { a }
                };
                out[end] = at(n[1]);
                out[2 + end] = at(n[0]);
            }
            _ => warnings
                .push("несколько косых резов на одном торце: углы в заголовке не заданы".into()),
        }
    }
    out
}

/// Walls of a section along a member of `length`, `y` on the web and back
/// wall from the underside, on the flanges from the −x edge.
fn member_walls(shape: &ProfileShape, length: f64) -> Vec<Wall> {
    let x = [1.0, 0.0, 0.0];
    let y = [0.0, 1.0, 0.0];
    let z = [0.0, 0.0, 1.0];
    let rect = |w: f64| vec![[0.0, 0.0], [length, 0.0], [length, w], [0.0, w]];
    // Web-like wall across x: w = y + h/2; flange-like across y: w = x + b/2.
    let across_x = |face, h: f64, from: f64, to: f64| Wall {
        face,
        origin: [0.0, -h / 2.0, 0.0],
        u: z,
        w: y,
        c: x,
        through: (from, to),
        base: rect(h),
    };
    let across_y = |face, b: f64, from: f64, to: f64| Wall {
        face,
        origin: [-b / 2.0, 0.0, 0.0],
        u: z,
        w: x,
        c: y,
        through: (from, to),
        base: rect(b),
    };
    match *shape {
        ProfileShape::ISection {
            height: h,
            width: b,
            web: tw,
            flange: tf,
        } => vec![
            across_x(Face::Web, h, -tw / 2.0, tw / 2.0),
            across_y(Face::Top, b, h / 2.0 - tf, h / 2.0),
            across_y(Face::Bottom, b, -h / 2.0, -h / 2.0 + tf),
        ],
        ProfileShape::Channel {
            height: h,
            width: b,
            web: tw,
            flange: tf,
        } => vec![
            across_x(Face::Web, h, -b / 2.0, -b / 2.0 + tw),
            across_y(Face::Top, b, h / 2.0 - tf, h / 2.0),
            across_y(Face::Bottom, b, -h / 2.0, -h / 2.0 + tf),
        ],
        ProfileShape::Angle {
            height: h,
            width: b,
            thickness: t,
        } => vec![
            across_x(Face::Web, h, -b / 2.0, -b / 2.0 + t),
            across_y(Face::Bottom, b, -h / 2.0, -h / 2.0 + t),
        ],
        ProfileShape::RectangularHollow {
            height: h,
            width: b,
            thickness: t,
        } => vec![
            across_x(Face::Web, h, -b / 2.0, -b / 2.0 + t),
            across_y(Face::Top, b, h / 2.0 - t, h / 2.0),
            across_y(Face::Bottom, b, -h / 2.0, -h / 2.0 + t),
            across_x(Face::Back, h, b / 2.0 - t, b / 2.0),
        ],
        ProfileShape::Rectangle {
            height: h,
            width: b,
        } => vec![across_x(Face::Web, h, -b / 2.0, b / 2.0)],
        ProfileShape::CircularHollow { .. }
        | ProfileShape::Polygon { .. }
        | ProfileShape::Welded { .. } => Vec::new(),
    }
}

/// Lower left corner of the mark text on the face: the first place from the
/// start of the piece, along the middle of the face, where the text lies on
/// material away from holes.
#[allow(clippy::too_many_arguments)]
fn mark_place(
    wall: &Wall,
    loops: &[Vec<[f64; 2]>],
    start: &[f64; 2],
    holes: &[(Face, f64, f64, f64)],
    slots: &[(Face, f64, f64, f64, f64, f64)],
    text: &str,
    height: f64,
    length: f64,
) -> Option<(f64, f64)> {
    if text.is_empty() || height <= 0.0 {
        return None;
    }
    let width = height * 0.8 * text.chars().count() as f64;
    let (lo, hi) = bounds(&wall.base);
    let middle = (lo[1] + hi[1]) / 2.0 - start[1];
    let y = middle - height / 2.0;
    let circles: Vec<([f64; 2], f64)> = holes
        .iter()
        .filter(|h| h.0 == wall.face)
        .map(|h| ([h.1, h.2], h.3 / 2.0))
        .chain(
            slots
                .iter()
                .filter(|s| s.0 == wall.face)
                .map(|s| ([s.1, s.2], (s.3 + s.4) / 2.0)),
        )
        .collect();
    let gap = height / 2.0;
    let mut x = 2.0 * height;
    while x + width + 2.0 * height <= length + 1e-9 {
        let corners = [
            [x - gap, y - gap],
            [x + width + gap, y - gap],
            [x + width + gap, y + height + gap],
            [x - gap, y + height + gap],
            [x + width / 2.0, y + height / 2.0],
        ];
        let on_material = corners.iter().all(|p| faces::in_region(loops, *p));
        let clear = circles.iter().all(|(c, r)| {
            let dx = (c[0] - (x + width / 2.0)).abs() - width / 2.0 - gap;
            let dy = (c[1] - (y + height / 2.0)).abs() - height / 2.0 - gap;
            dx.max(0.0).hypot(dy.max(0.0)) > *r && !(dx < 0.0 && dy < 0.0)
        });
        if on_material && clear {
            return Some((x, y));
        }
        x += height;
    }
    None
}

fn member_section(name: &str, shape: &ProfileShape) -> Result<Section, String> {
    let area = shape.area_mm2().map_err(|e| e.to_string())?;
    let perimeter = |c: Vec<[f64; 2]>| {
        (0..c.len())
            .map(|i| {
                let (a, b) = (c[i], c[(i + 1) % c.len()]);
                ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
            })
            .sum::<f64>()
    };
    let solid = || {
        shape
            .solid_contour()
            .map(perimeter)
            .map_err(|e| e.to_string())
    };
    let section = |code, height, width, flange, web, surface| Section {
        profile: name.into(),
        code,
        height,
        width,
        flange,
        web,
        area,
        surface,
    };
    Ok(match *shape {
        ProfileShape::Polygon { .. } => {
            return Err("сечение по контуру не передаётся в DSTV".into());
        }
        ProfileShape::Welded { .. } => {
            return Err("сварное сечение изготавливается из листов, в DSTV не передаётся".into());
        }
        ProfileShape::ISection {
            height,
            width,
            web,
            flange,
        } => section("I", height, width, flange, web, solid()?),
        ProfileShape::Channel {
            height,
            width,
            web,
            flange,
        } => section("U", height, width, flange, web, solid()?),
        ProfileShape::Angle {
            height,
            width,
            thickness,
        } => section("L", height, width, thickness, thickness, solid()?),
        ProfileShape::CircularHollow {
            diameter,
            thickness,
        } => section(
            "RU",
            diameter,
            diameter,
            thickness,
            thickness,
            std::f64::consts::PI * diameter,
        ),
        ProfileShape::RectangularHollow {
            height,
            width,
            thickness,
        } => section(
            "M",
            height,
            width,
            thickness,
            thickness,
            2.0 * (height + width),
        ),
        ProfileShape::Rectangle { height, width } => {
            section("B", height, width, width, width, solid()?)
        }
    })
}

fn bounds(points: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
    let mut min = [f64::MAX; 2];
    let mut max = [f64::MIN; 2];
    for p in points {
        for i in 0..2 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    (min, max)
}

fn fmt(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.1}")
    }
}

fn one_line(s: &str) -> String {
    let s = s.trim();
    if s.is_empty() { "-".into() } else { s.into() }
}

fn file_stem(mark: &str) -> String {
    let stem: String = ascii(mark)
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    if stem.is_empty() { "part".into() } else { stem }
}

/// Machine controllers read plain ASCII: Russian letters are transliterated,
/// quotes and signs replaced, control characters dropped.
pub fn ascii(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        let lower = c.to_lowercase().next().unwrap_or(c);
        let latin = match lower {
            'а' => "a",
            'б' => "b",
            'в' => "v",
            'г' => "g",
            'д' => "d",
            'е' | 'ё' | 'э' => "e",
            'ж' => "zh",
            'з' => "z",
            'и' => "i",
            'й' | 'ы' => "y",
            'к' => "k",
            'л' => "l",
            'м' => "m",
            'н' => "n",
            'о' => "o",
            'п' => "p",
            'р' => "r",
            'с' => "s",
            'т' => "t",
            'у' => "u",
            'ф' => "f",
            'х' => "kh",
            'ц' => "ts",
            'ч' => "ch",
            'ш' => "sh",
            'щ' => "shch",
            'ъ' | 'ь' => "",
            'ю' => "yu",
            'я' => "ya",
            _ => {
                match c {
                    '«' | '»' | '“' | '”' | '„' => out.push('"'),
                    '×' => out.push('x'),
                    '·' | '—' | '–' => out.push('-'),
                    '⌀' => out.push('D'),
                    c if c.is_ascii() && !c.is_ascii_control() => out.push(c),
                    c if c.is_whitespace() => out.push(' '),
                    c if c.is_ascii_control() => {}
                    _ => out.push('?'),
                }
                continue;
            }
        };
        if c != lower {
            let mut chars = latin.chars();
            if let Some(first) = chars.next() {
                let _ = write!(out, "{}{}", first.to_ascii_uppercase(), chars.as_str());
            }
        } else {
            out.push_str(latin);
        }
    }
    out
}
