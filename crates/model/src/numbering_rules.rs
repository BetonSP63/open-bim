//! Naming rules of ГОСТ Р 21.101-2026 for grids and levels.
//!
//! - 5.3.2: axis letters are capital Russian letters without Е, З, Й, О, Х,
//!   Ц, Ч, Щ, Ъ, Ы, Ь (Ё is not used either), or Latin letters without I and
//!   O; no gaps; when the alphabet runs out, two letters (АА, ББ, ВВ…); digits
//!   go along the side with more axes.
//! - 5.3.8: the model origin is at axes 1/А on the zero level, X towards
//!   increasing digits, Y towards increasing letters.
//! - 5.4.3: level marks in metres with three decimals and a comma, «+» above
//!   zero, «-» below, no sign for zero («0,000», «+3,600», «-2,800»).

/// Letters allowed for grid axes, in order.
pub const AXIS_CYRILLIC: &str = "АБВГДЖИКЛМНПРСТУФШЭЮЯ";
/// Latin letters allowed for grid axes, in order.
pub const AXIS_LATIN: &str = "ABCDEFGHJKLMNPQRSTUVWXYZ";
/// Russian letters never used for axes.
pub const FORBIDDEN_CYRILLIC: &str = "ЕЁЗЙОХЦЧЩЪЫЬ";
/// Latin letters never used for axes.
pub const FORBIDDEN_LATIN: &str = "IO";

/// Letter label of the `index`-th lettered axis (0 → «А»): single letters,
/// then doubled ones (АА, ББ…), then tripled, and so on.
pub fn axis_letter(index: usize) -> String {
    let letters: Vec<char> = AXIS_CYRILLIC.chars().collect();
    let n = letters.len();
    let repeat = index / n + 1;
    std::iter::repeat_n(letters[index % n], repeat).collect()
}

/// Label after `label` in its sequence: «5» → «6», «Г» → «Д», «Я» → «АА»,
/// «АА» → «ББ», «Y» → «Z». `None` for labels that are not part of one.
pub fn next_axis_label(label: &str) -> Option<String> {
    if let Ok(n) = label.parse::<u64>() {
        return Some((n + 1).to_string());
    }
    let first = label.chars().next()?;
    if !label.chars().all(|c| c == first) {
        return None;
    }
    let repeat = label.chars().count();
    for alphabet in [AXIS_CYRILLIC, AXIS_LATIN] {
        let letters: Vec<char> = alphabet.chars().collect();
        if let Some(i) = letters.iter().position(|l| *l == first) {
            let (c, r) = match letters.get(i + 1) {
                Some(c) => (*c, repeat),
                None => (letters[0], repeat + 1),
            };
            return Some(std::iter::repeat_n(c, r).collect());
        }
    }
    None
}

/// Check an axis name: digits, allowed letters, or a fraction for an
/// additional axis («1/1», «Б/2»).
pub fn validate_axis_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Пустое обозначение оси".into());
    }
    if name.chars().count() > 8 {
        return Err("Обозначение оси длиннее 8 знаков".into());
    }
    for part in name.split('/') {
        if part.is_empty() {
            return Err("Дробь без числителя или знаменателя".into());
        }
        for c in part.chars() {
            if FORBIDDEN_CYRILLIC.contains(c) || FORBIDDEN_LATIN.contains(c) {
                return Err(format!(
                    "Буква «{c}» не применяется для осей (ГОСТ Р 21.101, 5.3.2: исключены {FORBIDDEN_CYRILLIC}, латинские {FORBIDDEN_LATIN})"
                ));
            }
            if !(c.is_ascii_digit() || AXIS_CYRILLIC.contains(c) || AXIS_LATIN.contains(c)) {
                return Err(format!(
                    "Знак «{c}» недопустим: цифры или прописные буквы (ГОСТ Р 21.101, 5.3.2)"
                ));
            }
        }
    }
    Ok(())
}

/// Whether digits go along X (lines across X are numbered) for a grid with
/// `along_x` lines across X and `along_y` lines across Y: digits label the
/// side with more axes (5.3.2), X when equal.
pub fn digits_along_x(along_x: usize, along_y: usize) -> bool {
    along_x >= along_y
}

/// Level mark for an elevation in millimetres (5.4.3): «0,000», «+3,600»,
/// «-2,800».
pub fn level_mark(elevation_mm: f64) -> String {
    let metres = (elevation_mm / 1000.0 * 1000.0).round() / 1000.0;
    let text = format!("{:.3}", metres.abs()).replace('.', ",");
    if metres == 0.0 {
        text
    } else if metres > 0.0 {
        format!("+{text}")
    } else {
        format!("-{text}")
    }
}

/// Parse a level mark in metres («+3,600», «-2.8», «0,000») to millimetres.
pub fn parse_level_mark(text: &str) -> Option<f64> {
    let t = text.trim().replace(',', ".");
    let v: f64 = t.strip_prefix('+').unwrap_or(&t).parse().ok()?;
    v.is_finite()
        .then(|| (v * 1000.0 * 1000.0).round() / 1000.0)
}

/// Plan title by its level (5.5.12): «План на отм. +3,600».
pub fn plan_title(elevation_mm: f64) -> String {
    format!("План на отм. {}", level_mark(elevation_mm))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_skip_forbidden_and_double_after_ya() {
        let first: Vec<String> = (0..21).map(axis_letter).collect();
        assert_eq!(first.concat(), AXIS_CYRILLIC);
        assert_eq!(axis_letter(21), "АА");
        assert_eq!(axis_letter(22), "ББ");
        assert_eq!(next_axis_label("Я").as_deref(), Some("АА"));
        assert_eq!(next_axis_label("АА").as_deref(), Some("ББ"));
        assert_eq!(next_axis_label("12").as_deref(), Some("13"));
        assert_eq!(next_axis_label("H").as_deref(), Some("J"));
        assert_eq!(next_axis_label("АБ"), None);
    }

    #[test]
    fn marks_have_comma_three_decimals_and_sign() {
        assert_eq!(level_mark(0.0), "0,000");
        assert_eq!(level_mark(-0.0), "0,000");
        assert_eq!(level_mark(3600.0), "+3,600");
        assert_eq!(level_mark(-2800.0), "-2,800");
        assert_eq!(level_mark(12345.4), "+12,345");
        assert_eq!(parse_level_mark("+3,600"), Some(3600.0));
        assert_eq!(parse_level_mark("-2.8"), Some(-2800.0));
        assert_eq!(parse_level_mark("абв"), None);
        assert_eq!(plan_title(3600.0), "План на отм. +3,600");
    }
}
