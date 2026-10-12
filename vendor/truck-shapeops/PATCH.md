# truck-shapeops 0.4.0 feature patch

Upstream: https://crates.io/crates/truck-shapeops/0.4.0 ; repository https://github.com/ricosjp/truck . License Apache-2.0, see LICENSE.

Source files copied from the published crate. Cargo.toml disables truck-meshalgo default features and explicitly enables analyzers and tessellation. This removes the unused VTK exporter and legacy nom/quick-xml dependencies; shape operations do not use VTK. In src/lib.rs, the private alternative module is guarded with cfg(test), because it is used only by upstream unit tests and otherwise emits a dead-code warning as a local dependency. No mathematical algorithm has been modified. The application's analytical body corpus is required on updates; upstream tests should additionally be run with their upstream resource files.
