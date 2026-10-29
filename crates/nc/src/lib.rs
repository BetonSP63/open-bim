//! NC data for steel fabrication machines in the DSTV format (`.nc1`).
//!
//! One file per part mark: the `ST` header (order, drawing, phase, mark,
//! steel, quantity, section and its dimensions), holes in `BO`, the outer
//! contour of plates in `AK`, then `EN`. Coordinates are millimetres from the
//! start of the piece. On the web (`v`) and the back wall (`h`) `y` runs up
//! from the underside of the section; on the flanges (`o`, `u`) it runs from
//! the edge on the web side of channels and angles. Plates lie with their
//! long side along `x`. What the format here does not carry yet (copes,
//! skewed cuts, holes in round tubes) is listed as warnings and repeated in
//! the file as `**` comments.
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
    /// Holes: face, x, y, diameter (mm), sorted.
    pub holes: Vec<(Face, f64, f64, f64)>,
    /// Features of the part the file does not carry.
    pub warnings: Vec<String>,
}

/// Files for all parts of the project, one per part mark of the current
/// numbering, with the number of parts of that mark as the quantity.
pub fn export(project: &Project) -> Result<Vec<NcFile>, String> {
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
            Some(true) => batch = welded_files(project, ids[0], mark, ids.len())?,
            // Contour sections have no DSTV profile code.
            Some(false) => {}
            None => batch.push(part_file(project, ids[0], mark, ids.len())?),
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
        let mut file = part_file(&strip, id, &format!("{mark}.{}", i + 1), quantity)?;
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
    let element = project
        .elements
        .get(&id)
        .ok_or_else(|| format!("Нет детали {id}"))?;
    let material = project
        .materials
        .get(&element.material)
        .ok_or_else(|| format!("Нет материала детали {id}"))?;
    let mut warnings = Vec::new();
    let mut holes = Vec::new();
    let details: Vec<&DetailKind> = project
        .details
        .values()
        .map(|d| &d.kind)
        .filter(|k| k.part() == Some(id))
        .collect();
    let (section, length, contour) = match &element.shape {
        ElementShape::Member {
            profile, length, ..
        } => {
            let profile = project
                .profiles
                .get(profile)
                .ok_or_else(|| format!("Нет профиля детали {id}"))?;
            let section = member_section(&profile.name, &profile.shape)
                .map_err(|e| format!("Профиль {}: {e}", profile.name))?;
            let (mut start, mut end) = (0.0_f64, length.value());
            for kind in &details {
                match kind {
                    DetailKind::PlaneCut { normal, offset, .. } => {
                        let n = normal.iter().map(|v| v * v).sum::<f64>().sqrt();
                        let [x, y, z] = normal.map(|v| v / n);
                        if x.abs() < 1e-9 && y.abs() < 1e-9 {
                            if z < 0.0 {
                                start = start.max(-offset / n);
                            } else {
                                end = end.min(offset / n);
                            }
                        } else {
                            warnings.push("косой рез торца не передан".into());
                        }
                    }
                    DetailKind::BoxCut { .. } | DetailKind::PrismCut { .. } => {
                        warnings.push("вырез не передан".into());
                    }
                    _ => {}
                }
            }
            if end - start <= 0.001 {
                return Err(format!("Деталь {id}: резы не оставили длины"));
            }
            for kind in &details {
                if let DetailKind::Hole {
                    frame,
                    diameter,
                    depth,
                    ..
                } = kind
                {
                    let faces = member_faces(&profile.shape, frame.origin.0, frame.axes[2], *depth);
                    if faces.is_empty() {
                        warnings.push(format!("отверстие ⌀{} не передано", fmt(*diameter)));
                    }
                    for (face, y) in faces {
                        holes.push((face, frame.origin.0[2] - start, y, *diameter));
                    }
                }
            }
            (section, end - start, None)
        }
        ElementShape::Plate {
            contour, thickness, ..
        } => {
            let t = thickness.value();
            let (min, max) = bounds(contour);
            let (dx, dy) = (max[0] - min[0], max[1] - min[1]);
            // Long side along x: turn a quarter clockwise when the plate is tall.
            let turn = dy > dx;
            let place = |p: [f64; 2]| {
                if turn {
                    [p[1] - min[1], max[0] - p[0]]
                } else {
                    [p[0] - min[0], p[1] - min[1]]
                }
            };
            let (length, width) = if turn { (dy, dx) } else { (dx, dy) };
            for kind in &details {
                match kind {
                    DetailKind::Hole {
                        frame, diameter, ..
                    } => {
                        let axis = frame.axes[2];
                        if axis[0].abs() < 1e-6 && axis[1].abs() < 1e-6 {
                            let [x, y] = place([frame.origin.0[0], frame.origin.0[1]]);
                            holes.push((Face::Web, x, y, *diameter));
                        } else {
                            warnings.push(format!(
                                "отверстие ⌀{} не по нормали к листу",
                                fmt(*diameter)
                            ));
                        }
                    }
                    _ => warnings.push("вырез не передан".into()),
                }
            }
            let mut points: Vec<[f64; 2]> = contour.iter().map(|p| place(*p)).collect();
            let area: f64 = (0..points.len())
                .map(|i| {
                    let (a, b) = (points[i], points[(i + 1) % points.len()]);
                    a[0] * b[1] - b[0] * a[1]
                })
                .sum();
            if area < 0.0 {
                points.reverse();
            }
            let section = Section {
                profile: format!("PL{}x{}", fmt(t), fmt(width)),
                code: "B",
                height: width,
                width: t,
                flange: t,
                web: t,
                area: width * t,
                surface: 2.0 * (width + t),
            };
            (section, length, Some(points))
        }
    };
    holes.sort_by(|a, b| {
        (a.0, a.1, a.2)
            .partial_cmp(&(b.0, b.1, b.2))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
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
        0.0,
        0.0,
        0.0,
        0.0,
    ] {
        line(&number(v));
    }
    for info in [ascii(&element.name), "-".into(), "-".into(), "-".into()] {
        line(&value(&info));
    }
    if let Some(points) = &contour {
        line("AK");
        for p in points.iter().chain(points.first()) {
            line(&format!("  v {:>10.2} {:>10.2} {:>8.2}", p[0], p[1], 0.0));
        }
    }
    if !holes.is_empty() {
        line("BO");
        for (face, x, y, d) in &holes {
            line(&format!("  {} {x:>10.2} {y:>10.2} {d:>8.2}", face.letter()));
        }
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
        warnings,
    })
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

/// Faces a hole of `depth` from `origin` along `axis` (part coordinates)
/// passes through, with its `y` on each face.
fn member_faces(
    shape: &ProfileShape,
    origin: [f64; 3],
    axis: [f64; 3],
    depth: f64,
) -> Vec<(Face, f64)> {
    // Walls as (face, across-axis, from, to): the slab of the section the
    // hole must cross, normal to local x (0) or y (1).
    let walls: Vec<(Face, usize, f64, f64)> = match *shape {
        ProfileShape::ISection {
            height: h,
            width: _,
            web: tw,
            flange: tf,
        } => vec![
            (Face::Web, 0, -tw / 2.0, tw / 2.0),
            (Face::Top, 1, h / 2.0 - tf, h / 2.0),
            (Face::Bottom, 1, -h / 2.0, -h / 2.0 + tf),
        ],
        ProfileShape::Channel {
            height: h,
            width: w,
            web: tw,
            flange: tf,
        } => vec![
            (Face::Web, 0, -w / 2.0, -w / 2.0 + tw),
            (Face::Top, 1, h / 2.0 - tf, h / 2.0),
            (Face::Bottom, 1, -h / 2.0, -h / 2.0 + tf),
        ],
        ProfileShape::Angle {
            height: h,
            width: w,
            thickness: t,
        } => vec![
            (Face::Web, 0, -w / 2.0, -w / 2.0 + t),
            (Face::Bottom, 1, -h / 2.0, -h / 2.0 + t),
        ],
        ProfileShape::RectangularHollow {
            height: h,
            width: w,
            thickness: t,
        } => vec![
            (Face::Web, 0, -w / 2.0, -w / 2.0 + t),
            (Face::Back, 0, w / 2.0 - t, w / 2.0),
            (Face::Top, 1, h / 2.0 - t, h / 2.0),
            (Face::Bottom, 1, -h / 2.0, -h / 2.0 + t),
        ],
        ProfileShape::Rectangle { width: w, .. } => vec![(Face::Web, 0, -w / 2.0, w / 2.0)],
        ProfileShape::CircularHollow { .. }
        | ProfileShape::Polygon { .. }
        | ProfileShape::Welded { .. } => Vec::new(),
    };
    let (height, width) = match *shape {
        ProfileShape::ISection { height, width, .. }
        | ProfileShape::Channel { height, width, .. }
        | ProfileShape::Angle { height, width, .. }
        | ProfileShape::RectangularHollow { height, width, .. }
        | ProfileShape::Rectangle { height, width } => (height, width),
        ProfileShape::CircularHollow { diameter, .. } => (diameter, diameter),
        ProfileShape::Polygon { .. } | ProfileShape::Welded { .. } => {
            let [h, w, ..] = shape.sizes();
            (h, w)
        }
    };
    let mut faces = Vec::new();
    for (face, across, from, to) in walls {
        let other = 1 - across;
        if (axis[across].abs() - 1.0).abs() > 1e-6 {
            continue;
        }
        let a = origin[across];
        let b = a + axis[across] * depth;
        if a.min(b) < to - 1e-6 && a.max(b) > from + 1e-6 {
            let y = if other == 1 {
                origin[1] + height / 2.0
            } else {
                origin[0] + width / 2.0
            };
            faces.push((face, y));
        }
    }
    faces
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
