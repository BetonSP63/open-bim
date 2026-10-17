//! Scripted GUI input for reproducible verification of the real application.
//! Steps are converted into ordinary egui raw input events (pointer, keys,
//! text), so they pass through the same widgets as a person's input.
use eframe::egui;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Step {
    /// Idle frames.
    Wait(u32),
    /// Click at logical points.
    Click([f32; 2]),
    /// Secondary button click at logical points.
    RightClick([f32; 2]),
    /// Move the pointer without clicking.
    Hover([f32; 2]),
    /// Primary-button drag in logical points: [x0, y0, x1, y1].
    Drag([f32; 4]),
    /// Scroll by the given logical delta at a point: [x, y, dy].
    Scroll([f32; 3]),
    /// Key by egui name, e.g. "K", "Enter", "Tab", with optional modifiers.
    Key {
        name: String,
        #[serde(default)]
        command: bool,
        #[serde(default)]
        shift: bool,
    },
    Text(String),
    Capture(String),
    Note(String),
    Quit,
}

pub const MAX_STEPS: usize = 10_000;

pub fn parse(json: &str) -> Result<Vec<Step>, String> {
    if json.len() > 4 << 20 {
        return Err("Сценарий больше 4 МиБ".into());
    }
    let steps: Vec<Step> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if steps.len() > MAX_STEPS {
        return Err("Сценарий содержит слишком много шагов".into());
    }
    for step in &steps {
        if let Step::Key { name, .. } = step
            && egui::Key::from_name(name).is_none()
        {
            return Err(format!("Неизвестная клавиша: {name}"));
        }
    }
    Ok(steps)
}

/// Effect a step has outside raw input.
#[derive(Debug, PartialEq)]
pub enum Effect {
    Capture(String),
    Quit,
}

#[derive(Default)]
pub struct Runner {
    steps: Vec<Step>,
    index: usize,
    phase: u32,
    wait: u32,
}

impl Runner {
    pub fn new(steps: Vec<Step>) -> Self {
        Self {
            steps,
            ..Default::default()
        }
    }
    pub fn finished(&self) -> bool {
        self.index >= self.steps.len()
    }
    /// Feed one frame of events; returns an effect when the step needs one.
    pub fn frame(&mut self, input: &mut egui::RawInput) -> Option<Effect> {
        // The script owns the pointer: drop the physical mouse so a cursor
        // resting over the window cannot move or cancel scripted clicks.
        input.events.retain(|e| {
            !matches!(
                e,
                egui::Event::PointerMoved(_)
                    | egui::Event::PointerButton { .. }
                    | egui::Event::PointerGone
                    | egui::Event::MouseWheel { .. }
                    | egui::Event::MouseMoved(_)
            )
        });
        if self.wait > 0 {
            self.wait -= 1;
            return None;
        }
        let step = self.steps.get(self.index)?.clone();
        let modifiers = egui::Modifiers::default();
        let advance = |runner: &mut Self, wait: u32| {
            eprintln!("SCRIPT: step {} {:?}", runner.index + 1, step);
            runner.index += 1;
            runner.phase = 0;
            runner.wait = wait;
        };
        let button = |pos: [f32; 2], button, pressed| egui::Event::PointerButton {
            pos: egui::pos2(pos[0], pos[1]),
            button,
            pressed,
            modifiers,
        };
        match &step {
            Step::Wait(frames) => advance(self, *frames),
            Step::Hover(p) => {
                input
                    .events
                    .push(egui::Event::PointerMoved(egui::pos2(p[0], p[1])));
                advance(self, 2);
            }
            Step::Click(p) | Step::RightClick(p) => {
                let kind = if matches!(step, Step::Click(_)) {
                    egui::PointerButton::Primary
                } else {
                    egui::PointerButton::Secondary
                };
                match self.phase {
                    0 => input
                        .events
                        .push(egui::Event::PointerMoved(egui::pos2(p[0], p[1]))),
                    1 => input.events.push(button(*p, kind, true)),
                    _ => {
                        input.events.push(button(*p, kind, false));
                        advance(self, 3);
                        return None;
                    }
                }
                self.phase += 1;
            }
            Step::Drag([x0, y0, x1, y1]) => {
                const MOVES: u32 = 8;
                let at = |k: u32| {
                    let t = k as f32 / MOVES as f32;
                    egui::pos2(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t)
                };
                match self.phase {
                    0 => input.events.push(egui::Event::PointerMoved(at(0))),
                    1 => input
                        .events
                        .push(button([*x0, *y0], egui::PointerButton::Primary, true)),
                    k if k < MOVES + 2 => input.events.push(egui::Event::PointerMoved(at(k - 1))),
                    _ => {
                        input.events.push(egui::Event::PointerMoved(at(MOVES)));
                        input
                            .events
                            .push(button([*x1, *y1], egui::PointerButton::Primary, false));
                        advance(self, 3);
                        return None;
                    }
                }
                self.phase += 1;
            }
            Step::Scroll(p) => {
                input
                    .events
                    .push(egui::Event::PointerMoved(egui::pos2(p[0], p[1])));
                input.events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0., p[2]),
                    phase: egui::TouchPhase::Move,
                    modifiers,
                });
                advance(self, 3);
            }
            Step::Key {
                name,
                command,
                shift,
            } => {
                let key = egui::Key::from_name(name)?;
                let modifiers = egui::Modifiers {
                    shift: *shift,
                    command: *command,
                    mac_cmd: *command && cfg!(target_os = "macos"),
                    ctrl: *command && !cfg!(target_os = "macos"),
                    ..Default::default()
                };
                if self.phase == 0 {
                    input.events.push(egui::Event::ModifiersChanged(modifiers));
                    for pressed in [true, false] {
                        input.events.push(egui::Event::Key {
                            key,
                            physical_key: None,
                            pressed,
                            repeat: false,
                            modifiers,
                        });
                    }
                    self.phase = 1;
                } else {
                    input
                        .events
                        .push(egui::Event::ModifiersChanged(egui::Modifiers::default()));
                    advance(self, 3);
                }
            }
            Step::Text(text) => {
                input.events.push(egui::Event::Text(text.clone()));
                advance(self, 2);
            }
            Step::Note(_) => advance(self, 0),
            Step::Capture(path) => {
                advance(self, 6);
                return Some(Effect::Capture(path.clone()));
            }
            Step::Quit => {
                advance(self, 0);
                return Some(Effect::Quit);
            }
        }
        None
    }
}
