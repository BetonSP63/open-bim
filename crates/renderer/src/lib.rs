//! GPU display geometry, double-precision camera and visibility-consistent selection.
mod camera;
mod gpu;
pub mod overlay;
mod scene;
pub mod visualize;
pub use camera::{Camera, NamedView, Ray};
pub use glam::{DMat4, DVec2, DVec3};
pub use gpu::{GpuRenderer, ViewportCallback};
pub use scene::Appearance;
pub use scene::{
    Bounds, ClipPlane, Display, DisplayMesh, Hit, Instance, RenderMode, Scene, TransparencyScope,
    ViewState, class_color, feature_edges,
};

#[derive(Debug)]
pub struct RenderError(pub String);
impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for RenderError {}
