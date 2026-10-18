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
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            target: DVec3::ZERO,
            distance: 12000.0,
            yaw: -2.35,
            pitch: 0.52,
            orthographic: false,
            ortho_height: 9000.0,
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
        let near = (self.distance * 0.0001).max(0.01);
        let far = (self.distance * 100.0).max(near + 100.0);
        let projection = if self.orthographic {
            let half = self.ortho_height * 0.5;
            glam::dcamera::rh::proj::directx::orthographic(
                -half * aspect,
                half * aspect,
                -half,
                half,
                near,
                far,
            )
        } else {
            glam::dcamera::rh::proj::directx::perspective(45_f64.to_radians(), aspect, near, far)
        };
        projection * view
    }
    pub fn fit(&mut self, bounds: Bounds) {
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
    pub fn zoom(&mut self, delta: f64) {
        if !delta.is_finite() {
            return;
        }
        let factor = (-delta.clamp(-1000.0, 1000.0) * 0.002).exp();
        self.distance = (self.distance * factor).clamp(0.01, 1e9);
        self.ortho_height = (self.ortho_height * factor).clamp(0.01, 1e9);
    }
    /// Zoom keeping the world point under the cursor fixed (Tekla wheel zoom):
    /// a homothety about the point where the cursor ray meets the plane
    /// through the target perpendicular to the view direction.
    pub fn zoom_at(&mut self, delta: f64, ray: Option<Ray>) {
        if !delta.is_finite() {
            return;
        }
        let factor = (-delta.clamp(-1000.0, 1000.0) * 0.002).exp();
        let forward = (self.target - self.eye()).normalize_or_zero();
        let anchor = ray.and_then(|ray| {
            let denominator = ray.direction.dot(forward);
            (denominator.abs() > 1e-12).then(|| {
                ray.origin + ray.direction * ((self.target - ray.origin).dot(forward) / denominator)
            })
        });
        let distance = (self.distance * factor).clamp(0.01, 1e9);
        let applied = distance / self.distance;
        if let Some(anchor) = anchor.filter(|a| a.is_finite()) {
            self.target = anchor + (self.target - anchor) * applied;
        }
        self.distance = distance;
        self.ortho_height = (self.ortho_height * applied).clamp(0.01, 1e9);
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
        let near = inverse.project_point3(DVec3::new(x, y, 0.0)) + origin;
        let far = inverse.project_point3(DVec3::new(x, y, 1.0)) + origin;
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
