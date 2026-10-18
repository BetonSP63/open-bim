//! Dark engineering theme with WCAG 2.1 contrast ≥ 4.5:1 for text and a
//! clearly visible keyboard-focus ring.
use eframe::egui::{self, Color32, Stroke};

/// WCAG relative luminance of an sRGB colour.
pub fn luminance(c: Color32) -> f64 {
    let channel = |v: u8| {
        let s = v as f64 / 255.;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
}

pub fn contrast(a: Color32, b: Color32) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

pub const FOCUS: Color32 = Color32::from_rgb(110, 180, 255);
pub const WARNING: Color32 = Color32::from_rgb(255, 196, 0);
pub const ERROR: Color32 = Color32::from_rgb(255, 120, 120);
pub const OK: Color32 = Color32::from_rgb(90, 200, 120);

pub fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    let text = Color32::from_gray(225);
    v.override_text_color = None;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1., Color32::from_gray(205));
    v.widgets.inactive.fg_stroke = Stroke::new(1., text);
    v.widgets.inactive.bg_fill = Color32::from_gray(58);
    v.widgets.inactive.weak_bg_fill = Color32::from_gray(58);
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    v.widgets.hovered.weak_bg_fill = Color32::from_gray(75);
    // Keyboard focus uses the active visuals: a 2 px blue ring.
    v.widgets.active.bg_stroke = Stroke::new(2., FOCUS);
    v.widgets.active.fg_stroke = Stroke::new(2., Color32::WHITE);
    v.widgets.active.weak_bg_fill = Color32::from_gray(70);
    v.widgets.active.bg_fill = Color32::from_gray(70);
    v.selection.bg_fill = Color32::from_rgb(20, 90, 160);
    v.selection.stroke = Stroke::new(1., Color32::WHITE);
    v.hyperlink_color = FOCUS;
    v.warn_fg_color = WARNING;
    v.error_fg_color = ERROR;
    v.weak_text_color = Some(Color32::from_gray(170));
    v
}

/// Foreground/background pairs that carry readable text.
pub fn text_pairs(v: &egui::Visuals) -> Vec<(&'static str, Color32, Color32)> {
    vec![
        (
            "текст панели",
            v.widgets.noninteractive.fg_stroke.color,
            v.panel_fill,
        ),
        (
            "текст окна",
            v.widgets.noninteractive.fg_stroke.color,
            v.window_fill,
        ),
        (
            "кнопка",
            v.widgets.inactive.fg_stroke.color,
            v.widgets.inactive.weak_bg_fill,
        ),
        (
            "кнопка под курсором",
            v.widgets.hovered.fg_stroke.color,
            v.widgets.hovered.weak_bg_fill,
        ),
        (
            "кнопка в фокусе",
            v.widgets.active.fg_stroke.color,
            v.widgets.active.weak_bg_fill,
        ),
        (
            "поле ввода",
            v.widgets.inactive.fg_stroke.color,
            v.extreme_bg_color,
        ),
        (
            "выбранный элемент",
            v.selection.stroke.color,
            v.selection.bg_fill,
        ),
        (
            "вторичный текст",
            v.weak_text_color.unwrap_or(Color32::GRAY),
            v.panel_fill,
        ),
        ("предупреждение", WARNING, v.panel_fill),
        ("ошибка", ERROR, v.panel_fill),
        ("готово", OK, v.panel_fill),
        ("ссылка", v.hyperlink_color, v.panel_fill),
    ]
}

/// Default fonts with Hack as a fallback of the proportional family: Ubuntu
/// Light lacks ⇧ ↑ ↓ ↩ used in shortcut captions.
pub fn fonts() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional)
        && !family.iter().any(|f| f == "Hack")
    {
        family.insert(1, "Hack".into());
    }
    fonts
}

pub fn apply(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    ctx.set_visuals(visuals());
}
