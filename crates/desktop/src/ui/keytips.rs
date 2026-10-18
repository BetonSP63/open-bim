//! KeyTips (Office/Tekla style): Alt pressed and released alone, or F10,
//! shows a letter on every ribbon tab; typing it opens the tab and shows
//! letters on its buttons; typing a button's letter runs the command. Esc
//! steps back. Letters are Russian; the Latin key at the same place on the
//! ЙЦУКЕН layout works too, so the layout does not matter.
use super::ribbon::{Item, RibbonTab, groups};
use super::{Mode, Request, Workbench};
use eframe::egui;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyTips {
    #[default]
    Off,
    Tabs,
    Buttons,
}

/// Latin key at the same position as a Russian letter (ЙЦУКЕН ↔ QWERTY).
pub fn latin_of(c: char) -> Option<char> {
    const RU: &str = "ЙЦУКЕНГШЩЗХЪФЫВАПРОЛДЖЭЯЧСМИТЬБЮ";
    const EN: &str = "QWERTYUIOP[]ASDFGHJKL;'ZXCVBNM,.";
    RU.chars()
        .position(|r| r == c)
        .and_then(|i| EN.chars().nth(i))
}

/// Whether a typed character selects the tip.
pub fn matches(tip: char, typed: char) -> bool {
    let up = typed.to_uppercase().next().unwrap_or(typed);
    up == tip || latin_of(tip) == Some(up)
}

pub fn tab_tip(tab: RibbonTab) -> Option<char> {
    match tab {
        RibbonTab::Steel => Some('М'),
        RibbonTab::Edit => Some('П'),
        RibbonTab::View => Some('В'),
        RibbonTab::Drawings => Some('Ч'),
        RibbonTab::Manage => Some('У'),
        RibbonTab::Concrete | RibbonTab::Rebar => None,
    }
}

/// One distinct letter per available button: the first unused letter of its
/// caption, else a digit.
pub fn button_tips(items: &[&Item]) -> Vec<Option<char>> {
    let mut used: Vec<char> = Vec::new();
    let mut digit = 1;
    items
        .iter()
        .map(|it| {
            it.cmd?;
            let letter = it
                .label
                .chars()
                .filter(|c| c.is_alphabetic())
                .map(|c| c.to_uppercase().next().unwrap_or(c))
                .find(|c| !used.contains(c) && latin_of(*c).is_some());
            let tip = letter.unwrap_or_else(|| {
                let d = char::from_digit(digit % 10, 10).unwrap_or('0');
                digit += 1;
                d
            });
            used.push(tip);
            Some(tip)
        })
        .collect()
}

/// Items of a tab in drawing order.
pub fn tab_items(tab: RibbonTab) -> Vec<Item> {
    groups(tab)
        .into_iter()
        .flat_map(|(_, items)| items)
        .collect()
}

impl Workbench {
    /// Handle Alt/F10, the typed letters and Esc while KeyTips are shown.
    /// Runs before the global shortcuts so typed letters never trigger them.
    pub(super) fn keytips_input(&mut self, ctx: &egui::Context, requests: &mut Vec<Request>) {
        let (alt, any_key) = ctx.input(|i| {
            (
                i.modifiers.alt,
                i.events
                    .iter()
                    .any(|e| matches!(e, egui::Event::Key { pressed: true, .. })),
            )
        });
        let mut toggle = false;
        if alt && !self.alt_down {
            self.alt_down = true;
            self.alt_clean = true;
        }
        if self.alt_down && any_key {
            self.alt_clean = false;
        }
        if !alt && self.alt_down {
            self.alt_down = false;
            toggle |= self.alt_clean;
        }
        if !ctx.egui_wants_keyboard_input() {
            toggle |= ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F10));
        }
        if toggle {
            self.keytips = if self.keytips == KeyTips::Off {
                self.message("Подсказки клавиш: буква вкладки (Esc — выход)");
                KeyTips::Tabs
            } else {
                KeyTips::Off
            };
        }
        if self.keytips == KeyTips::Off {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.keytips = match self.keytips {
                KeyTips::Buttons => KeyTips::Tabs,
                _ => KeyTips::Off,
            };
        }
        // Letters go to the tips only: drop them so no shortcut or field sees them.
        let typed: Vec<char> = ctx.input_mut(|i| {
            let typed = i
                .events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Text(t) => t.chars().next(),
                    _ => None,
                })
                .collect();
            i.events
                .retain(|e| !matches!(e, egui::Event::Text(_) | egui::Event::Key { .. }));
            typed
        });
        for c in typed {
            match self.keytips {
                KeyTips::Tabs => {
                    if let Some(tab) = RibbonTab::ALL
                        .into_iter()
                        .find(|t| tab_tip(*t).is_some_and(|tip| matches(tip, c)))
                    {
                        self.ribbon_tab = tab;
                        if tab != RibbonTab::Drawings {
                            self.mode = Mode::Model;
                        }
                        self.keytips = KeyTips::Buttons;
                        self.message(format!("{}: буква команды (Esc — назад)", tab.title()));
                    }
                }
                KeyTips::Buttons => {
                    let items = tab_items(self.ribbon_tab);
                    let refs: Vec<&Item> = items.iter().collect();
                    let tips = button_tips(&refs);
                    if let Some(cmd) = items
                        .iter()
                        .zip(&tips)
                        .find(|(_, t)| t.is_some_and(|tip| matches(tip, c)))
                        .and_then(|(it, _)| it.cmd)
                    {
                        self.keytips = KeyTips::Off;
                        self.run(cmd, requests);
                    }
                }
                KeyTips::Off => {}
            }
        }
    }
}

/// Paint a tip badge at the top-left of a ribbon control.
pub fn badge(ctx: &egui::Context, rect: egui::Rect, tip: char) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("keytips"),
    ));
    let r = egui::Rect::from_min_size(
        rect.left_bottom() - egui::vec2(-2., 16.),
        egui::vec2(16., 16.),
    );
    painter.rect_filled(r, 3., egui::Color32::from_rgb(30, 40, 55));
    painter.text(
        r.center(),
        egui::Align2::CENTER_CENTER,
        tip,
        egui::FontId::proportional(12.),
        egui::Color32::WHITE,
    );
}
