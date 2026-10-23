//! «Визуализатор»: a ray-traced still of the current view — parts in their
//! colours under a sun with soft sky light, cast shadows on the parts and on
//! a ground plane under the model, edge-smoothed by supersampling.
use crate::{Camera, DVec3, Ray, Scene, ViewState};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub width: u32,
    pub height: u32,
    /// Rays per pixel side (1 = no smoothing, 2 = 4 rays per pixel).
    pub samples: u32,
    pub shadows: bool,
    /// Ground plane under the lowest point of the model.
    pub ground: bool,
    /// Sun direction: azimuth from +X counter-clockwise and elevation, degrees.
    pub sun: [f64; 2],
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            width: 1600,
            height: 1000,
            samples: 2,
            shadows: true,
            ground: true,
            sun: [225., 50.],
        }
    }
}

impl Settings {
    /// Unit vector toward the sun.
    pub fn sun_direction(&self) -> DVec3 {
        let [a, e] = self.sun.map(f64::to_radians);
        DVec3::new(e.cos() * a.cos(), e.cos() * a.sin(), e.sin()).normalize()
    }
}

/// An RGBA image, rows from the top.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

const SKY_TOP: DVec3 = DVec3::new(0.58, 0.72, 0.90);
const SKY_LOW: DVec3 = DVec3::new(0.93, 0.95, 0.97);
const GROUND: DVec3 = DVec3::new(0.80, 0.79, 0.76);

fn sky(direction: DVec3) -> DVec3 {
    let t = direction.z.clamp(0., 1.).sqrt();
    SKY_LOW.lerp(SKY_TOP, t)
}

/// Colour of one camera ray.
fn shade(scene: &Scene, state: &ViewState, settings: &Settings, ground: f64, ray: Ray) -> DVec3 {
    let sun = settings.sun_direction();
    let span = (scene.bounds.max - scene.bounds.min).length().max(1.);
    let lit = |point: DVec3, normal: DVec3| -> f64 {
        let facing = normal.dot(sun);
        if facing <= 0. {
            return 0.;
        }
        if settings.shadows {
            let shadow = Ray {
                origin: point + normal * (span * 1e-6).max(0.05),
                direction: sun,
            };
            if scene.trace(shadow, state).is_some() {
                return 0.;
            }
        }
        facing
    };
    let part = scene.trace(ray, state);
    // The ground plane, when it is nearer than any part.
    let ground_hit = (settings.ground && ray.direction.z < -1e-9)
        .then(|| (ground - ray.origin.z) / ray.direction.z)
        .filter(|t| *t > 0. && part.is_none_or(|h| *t < h.distance));
    if let Some(t) = ground_hit {
        let point = ray.origin + ray.direction * t;
        let light = 0.45 + 0.55 * lit(point, DVec3::Z);
        // Fade to the sky far from the model.
        let centre = (scene.bounds.min + scene.bounds.max) * 0.5;
        let far = ((point - centre).truncate().length() / (span * 3.)).clamp(0., 1.);
        return (GROUND * light).lerp(sky(DVec3::Z * 0.1), far * far);
    }
    let Some(hit) = part else {
        return sky(ray.direction);
    };
    let instance = &scene.instances[hit.instance];
    let look = state.appearance.get(&instance.id);
    let [r, g, b, _] = instance.color;
    let base = look
        .and_then(|a| a.color)
        .map_or(DVec3::new(r as f64, g as f64, b as f64), |c| {
            DVec3::new(c[0] as f64, c[1] as f64, c[2] as f64)
        });
    // Faces are lit on the side the camera sees.
    let normal = if hit.normal.dot(ray.direction) > 0. {
        -hit.normal
    } else {
        hit.normal
    };
    let point = ray.origin + ray.direction * hit.distance;
    let ambient = 0.30 + 0.15 * normal.z.max(0.);
    let diffuse = 0.60 * lit(point, normal);
    // A faint highlight toward the viewer keeps steel from looking flat.
    let half = (sun - ray.direction).normalize_or_zero();
    let specular = if diffuse > 0. {
        0.12 * normal.dot(half).max(0.).powi(24)
    } else {
        0.
    };
    (base * (ambient + diffuse) + DVec3::splat(specular)).min(DVec3::ONE)
}

/// Render the camera's view of the scene; `aspect` follows the settings.
pub fn render(scene: &Scene, camera: &Camera, state: &ViewState, settings: &Settings) -> Image {
    let (w, h) = (settings.width.max(1), settings.height.max(1));
    let n = settings.samples.clamp(1, 4);
    let aspect = w as f64 / h as f64;
    let ground = scene.bounds.min.z;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let rows_per = (h as usize).div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        for (chunk, rows) in rgba.chunks_mut(rows_per * w as usize * 4).enumerate() {
            scope.spawn(move || {
                for (k, px) in rows.chunks_mut(4).enumerate() {
                    let x = (k % w as usize) as f64;
                    let y = (chunk * rows_per + k / w as usize) as f64;
                    let mut sum = DVec3::ZERO;
                    for sy in 0..n {
                        for sx in 0..n {
                            let uv = [
                                (x + (sx as f64 + 0.5) / n as f64) / w as f64,
                                (y + (sy as f64 + 0.5) / n as f64) / h as f64,
                            ];
                            sum += camera
                                .ray(scene.origin, uv, aspect)
                                .map_or(SKY_LOW, |ray| shade(scene, state, settings, ground, ray));
                        }
                    }
                    let c = sum / (n * n) as f64;
                    // sRGB-like gamma.
                    let c = c.clamp(DVec3::ZERO, DVec3::ONE).powf(1. / 1.6) * 255.;
                    px.copy_from_slice(&[c.x as u8, c.y as u8, c.z as u8, 255]);
                }
            });
        }
    });
    Image {
        width: w,
        height: h,
        rgba,
    }
}
