//! Numeric location input during picks (Tekla «Ввод координат»):
//! `x,y,z` absolute in work plane coordinates, `@dx,dy,dz` relative to the
//! previous pick (work plane axes), `d` — distance along the rubber band.
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
    if let Some(rest) = text.strip_prefix('@') {
        let previous = previous.ok_or("Относительный ввод (@) требует предыдущей точки")?;
        let v = numbers(rest)?;
        let local = DVec3::new(v[0], *v.get(1).unwrap_or(&0.), *v.get(2).unwrap_or(&0.));
        return Ok(previous + workplane.vector_to_world(local));
    }
    let v = numbers(text)?;
    if v.len() == 1 {
        let previous = previous.ok_or("Расстояние требует предыдущей точки")?;
        let direction = direction
            .and_then(|d| d.try_normalize())
            .ok_or("Укажите направление курсором")?;
        return Ok(previous + direction * v[0]);
    }
    Ok(workplane.to_world(DVec3::new(v[0], v[1], *v.get(2).unwrap_or(&0.))))
}
