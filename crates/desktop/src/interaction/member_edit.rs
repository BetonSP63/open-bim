//! Member length editing (length in the property pane and start/end
//! handles): a new length moves the start point, the end point or both ends
//! symmetrically along the member axis; dragging a handle in «both» mode
//! moves the whole member.
use open_bim_model::{ElementShape, Frame, LengthMm, Point3, Project};
use open_bim_renderer::DVec3;

/// Which end of a member moves when its length or a handle changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LengthMode {
    Start,
    #[default]
    End,
    Both,
}

impl LengthMode {
    pub const ALL: [LengthMode; 3] = [Self::Start, Self::End, Self::Both];
    pub fn label(self) -> &'static str {
        match self {
            Self::Start => "Начало",
            Self::End => "Конец",
            Self::Both => "Оба конца",
        }
    }
}

fn v(p: Point3) -> DVec3 {
    DVec3::from_array(p.0)
}

/// Member start and end points.
pub fn ends(shape: &ElementShape) -> Option<(DVec3, DVec3)> {
    match shape {
        ElementShape::Member { length, frame, .. } => Some((
            v(frame.origin),
            v(frame.world(Point3([0., 0., length.value()]))),
        )),
        ElementShape::Plate { .. } => None,
    }
}

/// The member with a new length: «Начало» keeps the end, «Конец» keeps the
/// start, «Оба конца» keeps the midpoint.
pub fn resize(
    shape: &ElementShape,
    new_length: f64,
    mode: LengthMode,
) -> Result<ElementShape, String> {
    let ElementShape::Member {
        member_kind,
        profile,
        length,
        frame,
        position,
    } = shape
    else {
        return Err("Длина задаётся только у балок и колонн".into());
    };
    if !(new_length.is_finite() && (1.0..=1e6).contains(&new_length)) {
        return Err("Длина: 1…1 000 000 мм".into());
    }
    let dir = DVec3::from_array(frame.axes[2]);
    let grow = new_length - length.value();
    let shift = match mode {
        LengthMode::Start => -grow,
        LengthMode::End => 0.,
        LengthMode::Both => -grow / 2.,
    };
    let origin = v(frame.origin) + dir * shift;
    Ok(ElementShape::Member {
        member_kind: *member_kind,
        profile: *profile,
        length: LengthMm::new(new_length).map_err(|e| e.to_string())?,
        frame: Frame {
            origin: Point3(origin.to_array()),
            axes: frame.axes,
        },
        position: *position,
    })
}

/// The member moved as a whole by `delta` (handle drag in «both» mode).
pub fn translate(shape: &ElementShape, delta: DVec3) -> ElementShape {
    let mut shape = shape.clone();
    let frame = shape.frame_mut();
    frame.origin = Point3((v(frame.origin) + delta).to_array());
    shape
}

/// Lengths of the selected members (mm), for the pane and the viewport label.
pub fn lengths(project: &Project, ids: &[open_bim_model::ObjectId]) -> Vec<f64> {
    ids.iter()
        .filter_map(|id| project.elements.get(id))
        .filter_map(|e| match &e.shape {
            ElementShape::Member { length, .. } => Some(length.value()),
            ElementShape::Plate { .. } => None,
        })
        .collect()
}

/// Length as shown to the user: «6000», «6000,5».
pub fn length_text(mm: f64) -> String {
    let rounded = (mm * 10.).round() / 10.;
    if rounded.fract() == 0. {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}").replace('.', ",")
    }
}

/// Handle drag: in «Оба конца» mode a member end handle moves the whole
/// member by the drag; otherwise the handle's own point moves.
pub fn drag_handle(
    project: &Project,
    handle: &open_bim_renderer::overlay::Handle,
    target: DVec3,
    mode: LengthMode,
) -> Result<Vec<open_bim_commands::Command>, String> {
    use open_bim_renderer::overlay::HandleKind;
    if mode == LengthMode::Both
        && matches!(handle.kind, HandleKind::Start | HandleKind::End)
        && let Some(e) = project.elements.get(&handle.object)
        && matches!(e.shape, ElementShape::Member { .. })
    {
        if !target.is_finite() {
            return Err("Точка вне диапазона".into());
        }
        let mut element = e.clone();
        element.shape = translate(&e.shape, target - handle.world);
        return Ok(vec![open_bim_commands::Command::ReplaceElement { element }]);
    }
    super::selection::move_handle(project, handle, target)
}
