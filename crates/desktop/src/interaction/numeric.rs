//! Numeric location input during picks («Enter a Numeric Location»).
//! The default mode is relative: `x,y,z` without a prefix is relative to
//! the previous pick (absolute from the work plane origin when there is none);
//! `@` relative, `$` absolute in work plane coordinates, `!` global; polar
//! `d<a` / `d<a<b` (distance, angle on the XY plane, angle from the plane, in
//! degrees, counter-clockwise); a single value is a distance along the pointer.
use super::workplane::Workplane;
use open_bim_renderer::DVec3;

pub const MAX_INPUT: usize = 128;

fn numbers(text: &str) -> Result<Vec<f64>, String> {
    let parts: Vec<&str> = text
        .split([',', ';', ' '])
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() || parts.len() > 3 {
        return Err("Введите 1–3 числа через запятую".into());
    }
    parts
        .iter()
        .map(|p| {
            p.parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && v.abs() <= 1e9)
                .ok_or_else(|| format!("«{p}» — не число"))
        })
        .collect()
}

/// Resolve typed text to a world point.
pub fn parse(
    text: &str,
    workplane: &Workplane,
    previous: Option<DVec3>,
    direction: Option<DVec3>,
) -> Result<DVec3, String> {
    let text = text.trim();
    if text.is_empty() || text.len() > MAX_INPUT {
        return Err("Пустой или слишком длинный ввод".into());
    }
    let (mode, body) = match text.chars().next() {
        Some(c @ ('@' | '$' | '!')) => (c, text[1..].trim()),
        _ => ('r', text),
    };
    let local = |v: &[f64]| DVec3::new(v[0], *v.get(1).unwrap_or(&0.), *v.get(2).unwrap_or(&0.));
    if body.contains('<') {
        let v: Vec<f64> = body
            .split('<')
            .map(|p| {
                p.trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite() && v.abs() <= 1e9)
                    .ok_or_else(|| format!("«{p}» — не число"))
            })
            .collect::<Result<_, _>>()?;
        if !(2..=3).contains(&v.len()) {
            return Err("Полярный ввод: расстояние<угол или расстояние<угол<угол".into());
        }
        let (d, a, b) = (
            v[0],
            v[1].to_radians(),
            v.get(2).copied().unwrap_or(0.).to_radians(),
        );
        let offset = DVec3::new(d * b.cos() * a.cos(), d * b.cos() * a.sin(), d * b.sin());
        return match (mode, previous) {
            ('$', _) | ('r', None) => Ok(workplane.to_world(offset)),
            ('!', _) => Ok(offset),
            (_, Some(p)) => Ok(p + workplane.vector_to_world(offset)),
            ('@', None) => Err("Относительный ввод (@) требует предыдущей точки".into()),
            _ => unreachable!(),
        };
    }
    let v = numbers(body)?;
    match mode {
        '$' if v.len() >= 2 => Ok(workplane.to_world(local(&v))),
        '!' if v.len() >= 2 => Ok(local(&v)),
        '$' | '!' => Err("Абсолютный ввод требует 2–3 координаты".into()),
        _ if v.len() == 1 => {
            let previous = previous.ok_or("Расстояние требует предыдущей точки")?;
            let direction = direction
                .and_then(|d| d.try_normalize())
                .ok_or("Укажите направление курсором")?;
            Ok(previous + direction * v[0])
        }
        '@' => {
            let previous = previous.ok_or("Относительный ввод (@) требует предыдущей точки")?;
            Ok(previous + workplane.vector_to_world(local(&v)))
        }
        _ => Ok(match previous {
            Some(p) => p + workplane.vector_to_world(local(&v)),
            None => workplane.to_world(local(&v)),
        }),
    }
}
