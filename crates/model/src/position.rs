//! Part position like «Положение»: the profile is placed relative to
//! the reference line (the picked points) by «В плоскости», «Поворот» and
//! «По глубине», and the ends are extended or shortened by Dx.
//!
//! The member's stored `frame` and `length` stay the physical body axis, so
//! bodies, drawings, IFC and clashes need no change; the reference line is
//! derived from them and the position.
use crate::{Frame, LengthMm, ModelError, Point3, ProfileShape};
use serde::{Deserialize, Serialize};

/// «В плоскости» (local x of the reference line).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnPlane {
    #[default]
    Middle,
    Left,
    Right,
}

/// «Поворот» about the member axis in 90° steps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rotation {
    Front,
    #[default]
    Top,
    Back,
    Below,
}

/// «По глубине» (local y of the reference line).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AtDepth {
    #[default]
    Middle,
    Front,
    Behind,
}

impl OnPlane {
    pub const ALL: [Self; 3] = [Self::Middle, Self::Left, Self::Right];
    pub fn label(self) -> &'static str {
        match self {
            Self::Middle => "Посередине",
            Self::Left => "Слева",
            Self::Right => "Справа",
        }
    }
}

impl Rotation {
    pub const ALL: [Self; 4] = [Self::Front, Self::Top, Self::Back, Self::Below];
    pub fn label(self) -> &'static str {
        match self {
            Self::Front => "Спереди",
            Self::Top => "Сверху",
            Self::Back => "Сзади",
            Self::Below => "Снизу",
        }
    }
    /// «Сверху» keeps the profile height along the frame's y (the
    /// orientation every member had before positions existed).
    fn base_deg(self) -> f64 {
        match self {
            Self::Front => -90.0,
            Self::Top => 0.0,
            Self::Back => 90.0,
            Self::Below => 180.0,
        }
    }
}

impl AtDepth {
    pub const ALL: [Self; 3] = [Self::Middle, Self::Front, Self::Behind];
    pub fn label(self) -> &'static str {
        match self {
            Self::Middle => "Посередине",
            Self::Front => "Спереди",
            Self::Behind => "Сзади",
        }
    }
}

/// Position of a beam or column relative to its reference line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PartPosition {
    pub plane: OnPlane,
    /// Extra shift along local x, mm (positive towards «Слева»).
    pub plane_offset: f64,
    pub rotation: Rotation,
    /// Extra rotation about the axis, degrees.
    pub rotation_angle: f64,
    pub depth: AtDepth,
    /// Extra shift along local y, mm (positive towards «Спереди»).
    pub depth_offset: f64,
    /// Dx at the start, mm: positive shortens the part (moves the start
    /// Towards the end).
    pub start_dx: f64,
    /// Dx at the end, mm: positive lengthens the part.
    pub end_dx: f64,
}

impl PartPosition {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        let values = [
            self.plane_offset,
            self.rotation_angle,
            self.depth_offset,
            self.start_dx,
            self.end_dx,
        ];
        if values.iter().all(|v| v.is_finite() && v.abs() <= 1e6) {
            Ok(())
        } else {
            Err(ModelError::InvalidValue(
                "Положение детали: смещения в пределах ±1 000 000 мм".into(),
            ))
        }
    }

    fn angle_rad(&self) -> f64 {
        (self.rotation.base_deg() + self.rotation_angle).to_radians()
    }

    /// Shift of the profile origin in the reference x/y, mm.
    fn offsets(&self, shape: &ProfileShape) -> [f64; 2] {
        let [x0, x1, y0, y1] = rotated_bounds(shape, self.angle_rad());
        let x = match self.plane {
            OnPlane::Middle => 0.0,
            OnPlane::Left => -x0,
            OnPlane::Right => -x1,
        };
        let y = match self.depth {
            AtDepth::Middle => 0.0,
            AtDepth::Front => -y0,
            AtDepth::Behind => -y1,
        };
        [x + self.plane_offset, y + self.depth_offset]
    }
}

/// Profile bounds [min x, max x, min y, max y] after rotating it by `angle`.
fn rotated_bounds(shape: &ProfileShape, angle: f64) -> [f64; 4] {
    let (c, s) = (angle.cos(), angle.sin());
    if let ProfileShape::CircularHollow { diameter, .. } = *shape {
        let r = diameter / 2.0;
        return [-r, r, -r, r];
    }
    let [x0, x1, y0, y1] = match *shape {
        ProfileShape::ISection { height, width, .. }
        | ProfileShape::Angle { height, width, .. }
        | ProfileShape::Channel { height, width, .. }
        | ProfileShape::RectangularHollow { height, width, .. }
        | ProfileShape::Rectangle { height, width } => {
            [-width / 2.0, width / 2.0, -height / 2.0, height / 2.0]
        }
        ProfileShape::Polygon { .. } | ProfileShape::Welded { .. } => {
            shape.bounds().unwrap_or([0.0; 4])
        }
        ProfileShape::CircularHollow { .. } => unreachable!(),
    };
    let mut b = [
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    for (x, y) in [(x0, y0), (x1, y0), (x1, y1), (x0, y1)] {
        let (rx, ry) = (c * x - s * y, s * x + c * y);
        b = [b[0].min(rx), b[1].max(rx), b[2].min(ry), b[3].max(ry)];
    }
    // Snap round-off so «Слева» at 90° lands exactly on the line.
    b.map(|v| (v * 1e9).round() / 1e9)
}

fn add(a: [f64; 3], b: [f64; 3], k: f64) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + b[i] * k)
}

fn turn(axes: [[f64; 3]; 3], angle: f64) -> [[f64; 3]; 3] {
    let (c, s) = (angle.cos(), angle.sin());
    let [x, y, z] = axes;
    [
        std::array::from_fn(|i| c * x[i] + s * y[i]),
        std::array::from_fn(|i| -s * x[i] + c * y[i]),
        z,
    ]
}

/// The reference line of a member: (start, end, reference axes).
pub fn reference_line(
    frame: &Frame,
    length: LengthMm,
    position: &PartPosition,
    shape: &ProfileShape,
) -> (Point3, Point3, [[f64; 3]; 3]) {
    let axes = turn(frame.axes, -position.angle_rad());
    let [ox, oy] = position.offsets(shape);
    let z = axes[2];
    let mut start = add(frame.origin.0, axes[0], -ox);
    start = add(start, axes[1], -oy);
    start = add(start, z, -position.start_dx);
    let ref_length = length.value() - position.end_dx + position.start_dx;
    (Point3(start), Point3(add(start, z, ref_length)), axes)
}

/// Physical frame and length of a member on the reference line `start`→`end`.
/// `keep_axes` (old reference axes) keeps the profile's roll when the
/// direction does not change.
pub fn place(
    start: Point3,
    end: Point3,
    keep_axes: Option<[[f64; 3]; 3]>,
    position: &PartPosition,
    shape: &ProfileShape,
) -> Result<(Frame, LengthMm), ModelError> {
    position.validate()?;
    let (along, ref_length) = Frame::along(start, end)?;
    let axes = match keep_axes {
        Some(a) if (0..3).all(|i| (a[2][i] - along.axes[2][i]).abs() < 1e-9) => a,
        _ => along.axes,
    };
    let length = ref_length.value() + position.end_dx - position.start_dx;
    if length <= 0.001 {
        return Err(ModelError::InvalidValue(
            "Смещения концов Dx дают нулевую длину".into(),
        ));
    }
    let [ox, oy] = position.offsets(shape);
    let mut origin = add(start.0, axes[0], ox);
    origin = add(origin, axes[1], oy);
    origin = add(origin, axes[2], position.start_dx);
    let frame = Frame {
        origin: Point3(origin),
        axes: turn(axes, position.angle_rad()),
    };
    frame.validate()?;
    Ok((
        frame,
        LengthMm::new(length).map_err(|e| ModelError::InvalidValue(e.to_string()))?,
    ))
}

impl crate::Project {
    fn profile_shape(&self, id: crate::ObjectId) -> Result<&ProfileShape, ModelError> {
        self.profiles
            .get(&id)
            .map(|p| &p.shape)
            .ok_or(ModelError::MissingReference(id))
    }

    /// Reference line (start, end, axes) of a beam or column; `None` for
    /// plates and members with a missing profile.
    pub fn member_reference_line(
        &self,
        shape: &crate::ElementShape,
    ) -> Option<(Point3, Point3, [[f64; 3]; 3])> {
        match shape {
            crate::ElementShape::Member {
                profile,
                length,
                frame,
                position,
                ..
            } => {
                let profile = self.profile_shape(*profile).ok()?;
                Some(reference_line(frame, *length, position, profile))
            }
            crate::ElementShape::Plate { .. } => None,
        }
    }

    /// The member placed again on a reference line with a profile and a
    /// position (`None` keeps the member's own values); the profile's roll
    /// is kept while the direction does not change.
    pub fn replace_member(
        &self,
        shape: &crate::ElementShape,
        line: Option<(Point3, Point3)>,
        profile: Option<crate::ObjectId>,
        position: Option<PartPosition>,
    ) -> Result<crate::ElementShape, ModelError> {
        let crate::ElementShape::Member {
            member_kind,
            profile: old_profile,
            ..
        } = shape
        else {
            return Err(ModelError::InvalidValue(
                "Положение задаётся только у балок и колонн".into(),
            ));
        };
        let (start, end, axes) = self
            .member_reference_line(shape)
            .ok_or(ModelError::MissingReference(*old_profile))?;
        let (start, end) = line.unwrap_or((start, end));
        let profile = profile.unwrap_or(*old_profile);
        let position = position.unwrap_or(match shape {
            crate::ElementShape::Member { position, .. } => *position,
            crate::ElementShape::Plate { .. } => unreachable!(),
        });
        let (frame, length) = place(
            start,
            end,
            Some(axes),
            &position,
            self.profile_shape(profile)?,
        )?;
        Ok(crate::ElementShape::Member {
            member_kind: *member_kind,
            profile,
            length,
            frame,
            position,
        })
    }

    /// A new beam or column on the reference line `start`→`end`.
    pub fn new_member(
        &self,
        member_kind: crate::MemberKind,
        profile: crate::ObjectId,
        start: Point3,
        end: Point3,
        position: PartPosition,
    ) -> Result<crate::ElementShape, ModelError> {
        let (frame, length) = place(start, end, None, &position, self.profile_shape(profile)?)?;
        Ok(crate::ElementShape::Member {
            member_kind,
            profile,
            length,
            frame,
            position,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ibeam() -> ProfileShape {
        ProfileShape::ISection {
            height: 300.0,
            width: 150.0,
            web: 8.0,
            flange: 12.0,
        }
    }

    fn close(a: Point3, b: [f64; 3]) -> bool {
        (0..3).all(|i| (a.0[i] - b[i]).abs() < 1e-6)
    }

    #[test]
    fn default_position_is_the_old_placement() {
        let (s, e) = (Point3([0.0, 0.0, 3000.0]), Point3([6000.0, 0.0, 3000.0]));
        let p = PartPosition::default();
        let (frame, length) = place(s, e, None, &p, &ibeam()).unwrap();
        let (old, old_len) = Frame::along(s, e).unwrap();
        assert_eq!(frame.origin, old.origin);
        assert_eq!(length, old_len);
        for i in 0..3 {
            for j in 0..3 {
                assert!((frame.axes[i][j] - old.axes[i][j]).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn behind_puts_the_top_flange_on_the_line_and_round_trips() {
        let (s, e) = (Point3([0.0, 0.0, 3000.0]), Point3([6000.0, 0.0, 3000.0]));
        let p = PartPosition {
            depth: AtDepth::Behind,
            plane: OnPlane::Left,
            start_dx: 100.0,
            end_dx: 50.0,
            ..Default::default()
        };
        let (frame, length) = place(s, e, None, &p, &ibeam()).unwrap();
        // Axis 150 mm below the line, 75 mm to the left (+Y looking along +X).
        assert!(close(frame.origin, [100.0, 75.0, 2850.0]));
        assert!((length.value() - 5950.0).abs() < 1e-9);
        let (rs, re, _) = reference_line(&frame, length, &p, &ibeam());
        assert!(close(rs, s.0) && close(re, e.0));
    }

    #[test]
    fn rotation_front_turns_the_web_and_uses_the_rotated_width() {
        let (s, e) = (Point3([0.0, 0.0, 0.0]), Point3([0.0, 0.0, 3000.0]));
        let p = PartPosition {
            rotation: Rotation::Front,
            plane: OnPlane::Right,
            rotation_angle: 0.0,
            ..Default::default()
        };
        let (frame, length) = place(s, e, None, &p, &ibeam()).unwrap();
        // Rotated −90°: height lies along the reference x, so «Справа»
        // shifts by half the height.
        assert!(close(frame.origin, [-150.0, 0.0, 0.0]));
        let (rs, re, _) = reference_line(&frame, length, &p, &ibeam());
        assert!(close(rs, s.0) && close(re, e.0));
    }

    #[test]
    fn dx_cannot_collapse_the_part() {
        let p = PartPosition {
            start_dx: 3000.0,
            ..Default::default()
        };
        let r = place(
            Point3([0.0; 3]),
            Point3([0.0, 0.0, 3000.0]),
            None,
            &p,
            &ibeam(),
        );
        assert!(r.is_err());
    }

    #[test]
    fn position_is_saved_and_loaded_and_omitted_when_default() {
        let mut project = crate::example_steel_frame().unwrap();
        let plain = project.to_json().unwrap();
        assert!(!plain.contains("\"position\""));
        let id = *project.elements.keys().nth(4).unwrap();
        let shape = project.elements[&id].shape.clone();
        let p = PartPosition {
            depth: AtDepth::Behind,
            rotation_angle: 15.0,
            ..Default::default()
        };
        let moved = project.replace_member(&shape, None, None, Some(p)).unwrap();
        project.elements.get_mut(&id).unwrap().shape = moved.clone();
        let loaded = crate::Project::from_json(&project.to_json().unwrap()).unwrap();
        assert_eq!(loaded.elements[&id].shape, moved);
    }
}
