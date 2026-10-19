//! Navigation gestures: the rotation centre is the model surface under the
//! cursor when a rotation starts (the view centre over empty space) and stays
//! fixed for the whole gesture; trackpad input is split from the mouse wheel.
use open_bim_renderer::DVec3;

/// A gesture pauses longer than this (seconds) starts a new one.
pub const GESTURE_GAP: f64 = 0.35;

#[derive(Clone, Copy, Debug, Default)]
pub struct Orbit {
    pivot: Option<DVec3>,
    last: f64,
}

impl Orbit {
    /// Rotation centre for a rotation step at time `now`: kept while the
    /// gesture continues, else the surface `hit` under the cursor or the
    /// view `centre`.
    pub fn pivot(&mut self, now: f64, hit: Option<DVec3>, centre: DVec3) -> DVec3 {
        let pivot = match self.pivot {
            Some(p) if now - self.last <= GESTURE_GAP => p,
            _ => hit.unwrap_or(centre),
        };
        self.pivot = Some(pivot);
        self.last = now;
        pivot
    }

    /// Current rotation centre while a gesture is running (drawn on screen).
    pub fn active(&self, now: f64) -> Option<DVec3> {
        self.pivot.filter(|_| now - self.last <= GESTURE_GAP)
    }
}

/// Scroll input of one frame, split by device.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Scroll {
    /// Mouse wheel (line steps converted to points): zooms.
    pub wheel: f64,
    /// Trackpad two-finger scroll in points: rotates (with Shift: pans).
    pub trackpad: [f64; 2],
    pub shift: bool,
}

/// Points per wheel line step (egui's native default).
pub const LINE_POINTS: f64 = 40.;

pub fn split_scroll(events: &[eframe::egui::Event]) -> Scroll {
    use eframe::egui::{Event, MouseWheelUnit};
    let mut s = Scroll::default();
    for e in events {
        if let Event::MouseWheel {
            unit,
            delta,
            modifiers,
            ..
        } = e
        {
            // ⌘/Ctrl + scroll is a zoom gesture, read from `zoom_delta`.
            if modifiers.command || modifiers.ctrl {
                continue;
            }
            match unit {
                MouseWheelUnit::Line => s.wheel += delta.y as f64 * LINE_POINTS,
                MouseWheelUnit::Page => s.wheel += delta.y as f64 * LINE_POINTS * 10.,
                MouseWheelUnit::Point => {
                    s.trackpad[0] += delta.x as f64;
                    s.trackpad[1] += delta.y as f64;
                    s.shift |= modifiers.shift;
                }
            }
        }
    }
    s
}
