use crate::{Bounds, DMat4, DVec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NamedView {
    Isometric,
    Front,
    Top,
    Right,
}
#[derive(Clone, Copy, Debug)]
pub struct Ray {
    pub origin: DVec3,
    pub direction: DVec3,
}

#[derive(Clone, Debug)]
pub struct Camera {
    pub target: DVec3,
    pub distance: f64,
    pub yaw: f64,
    pub pitch: f64,
    pub orthographic: bool,
    pub ortho_height: f64,
    /// Model extent (parts and grid): the clip planes always enclose it, so
    /// zooming in never cuts the model away.
    pub extent: Option<Bounds>,
}

/// Closest the camera gets to the point it zooms to (mm).
pub const MIN_DISTANCE: f64 = 50.0;
impl Default for Camera {
    fn default() -> Self {
        Self {
            target: DVec3::ZERO,
            distance: 12000.0,
            yaw: -2.35,
            pitch: 0.52,
            orthographic: false,
            ortho_height: 9000.0,
            extent: None,
        }
    }
}
impl Camera {
    pub fn eye(&self) -> DVec3 {
        self.target
            + DVec3::new(
                self.pitch.cos() * self.yaw.cos(),
                self.pitch.cos() * self.yaw.sin(),
                self.pitch.sin(),
            ) * self.distance
    }
    fn up(&self) -> DVec3 {
        if self.pitch.cos().abs() < 1e-5 {
            DVec3::Y
        } else {
            DVec3::Z
        }
    }
    pub fn matrix(&self, origin: DVec3, aspect: f64) -> DMat4 {
        let view = glam::dcamera::rh::view::look_at_mat4(
            self.eye() - origin,
            self.target - origin,
            self.up(),
        );
        let (near, far) = self.clip_range();
        // Reversed-Z: near maps to depth 1 and far to 0, which keeps f32 depth
        // precise over the whole range (no z-fighting «ripple» far away).
        let projection = if self.orthographic {
            let half = self.ortho_height * 0.5;
            glam::dcamera::rh::proj::directx::orthographic(
                -half * aspect,
                half * aspect,
                -half,
                half,
                far,
                near,
            )
        } else {
            glam::dcamera::rh::proj::directx::perspective(45_f64.to_radians(), aspect, far, near)
        };
        projection * view
    }
    /// Near and far clip distances along the view direction: from the model
    /// extent and the target, never from the orbit distance alone.
    pub fn clip_range(&self) -> (f64, f64) {
        let eye = self.eye();
        let forward = (self.target - eye).normalize_or_zero();
        let Some(b) = self.extent else {
            let near = (self.distance * 0.0001).max(0.01);
            return (near, (self.distance * 100.0).max(near + 100.0));
        };
        let size = (b.max - b.min).length().max(1.0);
        let margin = size * 0.01 + 10.0;
        let (mut lo, mut hi) = (self.distance, self.distance);
        for i in 0..8 {
            let c = DVec3::new(
                if i & 1 == 0 { b.min.x } else { b.max.x },
                if i & 2 == 0 { b.min.y } else { b.max.y },
                if i & 4 == 0 { b.min.z } else { b.max.z },
            );
            let d = (c - eye).dot(forward);
            lo = lo.min(d);
            hi = hi.max(d);
        }
        let far = hi + margin;
        let near = if self.orthographic {
            // Parallel projection: geometry behind the eye position is still seen.
            lo - margin
        } else {
            (lo - margin).max(far * 1e-7).max(1.0)
        };
        (near, far.max(near + 1.0))
    }

    pub fn fit(&mut self, bounds: Bounds) {
        self.extent = Some(bounds);
        self.target = (bounds.min + bounds.max) * 0.5;
        let radius = ((bounds.max - bounds.min).length() * 0.5).max(100.0);
        self.distance = (radius / (22.5_f64.to_radians().tan()) * 1.3).clamp(1.0, 1e9);
        self.ortho_height = (radius * 2.6).clamp(1.0, 1e9);
    }
    pub fn set_view(&mut self, view: NamedView) {
        match view {
            NamedView::Isometric => {
                self.yaw = -2.35;
                self.pitch = 0.52;
                self.orthographic = false;
            }
            NamedView::Front => {
                self.yaw = -std::f64::consts::FRAC_PI_2;
                self.pitch = 0.0;
                self.orthographic = true;
            }
            NamedView::Top => {
                self.yaw = 0.0;
                self.pitch = std::f64::consts::FRAC_PI_2;
                self.orthographic = true;
            }
            NamedView::Right => {
                self.yaw = 0.0;
                self.pitch = 0.0;
                self.orthographic = true;
            }
        }
    }
    pub fn orbit(&mut self, dx: f64, dy: f64) {
        if dx.is_finite() && dy.is_finite() {
            self.yaw -= dx * 0.008;
            self.pitch = (self.pitch + dy * 0.008).clamp(-1.55, 1.55);
        }
    }
    /// Orbit about a world point (the rotation centre under the
    /// cursor): yaw about the vertical through `pivot`, pitch about the
    /// horizontal axis across the view; the pivot keeps its place on screen.
    pub fn orbit_about(&mut self, pivot: DVec3, dx: f64, dy: f64) {
        if !(dx.is_finite() && dy.is_finite() && pivot.is_finite()) {
            return;
        }
        let yaw = -dx * 0.008;
        let pitch = (self.pitch + dy * 0.008).clamp(-1.55, 1.55) - self.pitch;
        let dir = (self.eye() - self.target).normalize_or_zero();
        let q_yaw = glam::DQuat::from_rotation_z(yaw);
        let turned = q_yaw * dir;
        let across = DVec3::Z.cross(turned).normalize_or_zero();
        // Rotating about Z × dir by a negative angle raises the eye.
        let q = glam::DQuat::from_axis_angle(across, -pitch) * q_yaw;
        self.target = pivot + q * (self.target - pivot);
        self.yaw -= dx * 0.008;
        self.pitch += pitch;
    }
    pub fn zoom(&mut self, delta: f64) {
        if !delta.is_finite() {
            return;
        }
        let factor = (-delta.clamp(-1000.0, 1000.0) * 0.002).exp();
        self.distance = (self.distance * factor).clamp(MIN_DISTANCE, 1e9);
        self.ortho_height = (self.ortho_height * factor).clamp(MIN_DISTANCE, 1e9);
    }
    /// Zoom keeping the world point under the cursor fixed (wheel zoom):
    /// a homothety of eye and target about that point. `hit` is the model
    /// surface under the cursor; without it the point is where the cursor ray
    /// meets the plane through the target. The eye never gets closer than
    /// `MIN_DISTANCE` to that point, so zooming in cannot fly through the model.
    pub fn zoom_at(&mut self, delta: f64, ray: Option<Ray>) {
        self.zoom_to(delta, ray, None);
    }

    pub fn zoom_to(&mut self, delta: f64, ray: Option<Ray>, hit: Option<DVec3>) {
        if !delta.is_finite() {
            return;
        }
        let mut factor = (-delta.clamp(-1000.0, 1000.0) * 0.002).exp();
        let eye = self.eye();
        let forward = (self.target - eye).normalize_or_zero();
        let anchor = hit.filter(|h| h.is_finite()).or_else(|| {
            ray.and_then(|ray| {
                let denominator = ray.direction.dot(forward);
                (denominator.abs() > 1e-12).then(|| {
                    ray.origin
                        + ray.direction * ((self.target - ray.origin).dot(forward) / denominator)
                })
            })
        });
        if let Some(anchor) = anchor.filter(|a| a.is_finite()) {
            let gap = (eye - anchor).length();
            if gap > 0.0 && factor < 1.0 {
                factor = factor.max((MIN_DISTANCE / gap).min(1.0));
            }
            let distance = (self.distance * factor).clamp(1.0, 1e9);
            let applied = distance / self.distance;
            self.target = anchor + (self.target - anchor) * applied;
            self.distance = distance;
            self.ortho_height = (self.ortho_height * applied).clamp(MIN_DISTANCE, 1e9);
        } else {
            self.zoom(delta);
        }
    }
    /// «Облёт»: move the eye and the target together along the view
    /// direction by `step` mm (negative backs away).
    pub fn fly(&mut self, step: f64) {
        if !step.is_finite() {
            return;
        }
        let forward = (self.target - self.eye()).normalize_or_zero();
        self.target += forward * step.clamp(-1e7, 1e7);
    }

    /// «Центр вращения»: the eye stays, the view turns to `point`, which
    /// becomes the centre of rotation and zoom.
    pub fn look_at(&mut self, point: DVec3) {
        if !point.is_finite() {
            return;
        }
        let eye = self.eye();
        let d = eye - point;
        let length = d.length();
        if length < 1e-6 {
            return;
        }
        self.pitch = (d.z / length).clamp(-1.0, 1.0).asin().clamp(-1.55, 1.55);
        if d.x.hypot(d.y) > 1e-9 {
            self.yaw = d.y.atan2(d.x);
        }
        self.distance = length.clamp(1.0, 1e9);
        self.target = point;
    }

    pub fn pan(&mut self, dx: f64, dy: f64, viewport_height: f64) {
        if !dx.is_finite() || !dy.is_finite() || viewport_height <= 0.0 {
            return;
        }
        let forward = (self.target - self.eye()).normalize();
        let right = forward.cross(self.up()).normalize();
        let up = right.cross(forward).normalize();
        let height = if self.orthographic {
            self.ortho_height
        } else {
            2.0 * self.distance * 22.5_f64.to_radians().tan()
        };
        self.target += (-dx * right + dy * up) * height / viewport_height;
    }
    pub fn ray(&self, origin: DVec3, uv: [f64; 2], aspect: f64) -> Option<Ray> {
        if !uv.iter().all(|v| v.is_finite()) || !aspect.is_finite() || aspect <= 0.0 {
            return None;
        }
        let inverse = self.matrix(origin, aspect).inverse();
        let x = uv[0] * 2.0 - 1.0;
        let y = 1.0 - uv[1] * 2.0;
        // Reversed-Z: depth 1 is the near plane, 0 the far plane.
        let near = inverse.project_point3(DVec3::new(x, y, 1.0)) + origin;
        let far = inverse.project_point3(DVec3::new(x, y, 0.0)) + origin;
        let direction = (far - near).try_normalize()?;
        Some(Ray {
            origin: near,
            direction,
        })
    }
    pub fn project(&self, origin: DVec3, point: DVec3, aspect: f64) -> Option<[f64; 2]> {
        let p = self.matrix(origin, aspect) * (point - origin).extend(1.0);
        if p.w <= 0.0 {
            return None;
        }
        let ndc = p.truncate() / p.w;
        if !(0.0..=1.0).contains(&ndc.z) {
            return None;
        }
        Some([(ndc.x + 1.0) * 0.5, (1.0 - ndc.y) * 0.5])
    }
}
