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

/// Tekla-like light palette: light grey chrome, white panels, blue accents.
pub const LIGHT_FOCUS: Color32 = Color32::from_rgb(31, 95, 173);
/// Status bar (blue band at the bottom of the window).
pub const STATUS_BG: Color32 = Color32::from_rgb(30, 90, 168);
pub const STATUS_TEXT: Color32 = Color32::WHITE;
/// Ribbon background in the light theme.
pub const RIBBON_BG: Color32 = Color32::from_rgb(247, 248, 250);

/// Ribbon background: Tekla-like light band, or the faint panel colour in dark theme.
pub fn ribbon_bg(v: &egui::Visuals) -> Color32 {
    if v.dark_mode {
        v.faint_bg_color
    } else {
        RIBBON_BG
    }
}

pub fn ok(visuals: &egui::Visuals) -> Color32 {
    if visuals.dark_mode {
        OK
    } else {
        Color32::from_rgb(25, 115, 50)
    }
}

pub fn light_visuals() -> egui::Visuals {
    let mut v = egui::Visuals::light();
    let text = Color32::from_rgb(29, 31, 35);
    v.panel_fill = Color32::from_rgb(238, 240, 243);
    v.window_fill = Color32::WHITE;
    v.extreme_bg_color = Color32::WHITE;
    v.faint_bg_color = Color32::from_rgb(245, 246, 248);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1., text);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1., Color32::from_rgb(205, 209, 216));
    v.widgets.inactive.fg_stroke = Stroke::new(1., text);
    v.widgets.inactive.bg_fill = Color32::from_rgb(226, 229, 234);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(226, 229, 234);
    // Text fields are white like windows: an outline keeps them visible.
    v.widgets.inactive.bg_stroke = Stroke::new(1., Color32::from_rgb(178, 184, 194));
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, Color32::BLACK);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(214, 228, 245);
    v.widgets.hovered.bg_fill = Color32::from_rgb(214, 228, 245);
    v.widgets.active.bg_stroke = Stroke::new(2., LIGHT_FOCUS);
    v.widgets.active.fg_stroke = Stroke::new(2., Color32::BLACK);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(196, 218, 243);
    v.widgets.active.bg_fill = Color32::from_rgb(196, 218, 243);
    v.selection.bg_fill = Color32::from_rgb(31, 111, 197);
    v.selection.stroke = Stroke::new(1., Color32::WHITE);
    v.hyperlink_color = LIGHT_FOCUS;
    v.warn_fg_color = Color32::from_rgb(138, 90, 0);
    v.error_fg_color = Color32::from_rgb(180, 35, 24);
    v.weak_text_color = Some(Color32::from_rgb(90, 96, 105));
    v
}

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
        ("предупреждение", v.warn_fg_color, v.panel_fill),
        ("ошибка", v.error_fg_color, v.panel_fill),
        ("готово", ok(v), v.panel_fill),
        ("строка состояния", STATUS_TEXT, STATUS_BG),
        ("лента", v.widgets.inactive.fg_stroke.color, ribbon_bg(v)),
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

/// Fonts plus the chosen theme (light Tekla-like by default).
pub fn apply(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    set_theme(ctx, true);
}

/// Both theme slots get our visuals and the theme is pinned, so popups and
/// menus never fall back to the system (dark/light) style.
pub fn set_theme(ctx: &egui::Context, light: bool) {
    ctx.set_visuals_of(egui::Theme::Light, light_visuals());
    ctx.set_visuals_of(egui::Theme::Dark, visuals());
    ctx.set_theme(if light {
        egui::Theme::Light
    } else {
        egui::Theme::Dark
    });
}
