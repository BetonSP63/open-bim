//! «Сдвоенный профиль»: one beam or column command makes two members side
//! by side on the same reference line, spaced by a gap (for a gusset or
//! packing plates). The second member runs the other way, so its section is
//! the mirror of the first: channels and angles turn their backs or toes to
//! each other.
use open_bim_model::{
    AtDepth, ElementShape, MemberKind, ObjectId, OnPlane, PartPosition, Point3, Project,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Twin {
    #[default]
    Single,
    /// Webs (backs) to each other across the gap, flanges outwards.
    BackToBack,
    /// Flanges (toes) to each other across the gap.
    ToesIn,
    /// One above the other along the profile height.
    Stacked,
}

impl Twin {
    pub const ALL: [Self; 4] = [Self::Single, Self::BackToBack, Self::ToesIn, Self::Stacked];

    pub fn label(self) -> &'static str {
        match self {
            Self::Single => "Одиночный",
            Self::BackToBack => "Спинками друг к другу",
            Self::ToesIn => "Полками друг к другу",
            Self::Stacked => "Один над другим",
        }
    }

    /// Positions of the two members (and whether the member runs end to
    /// start) for the user's position and the gap; `None` for one member.
    /// The side («В плоскости» or «По глубине») is set by the pair, the rest
    /// of the position (rotation, the other side, Dx) stays.
    pub fn pair(self, position: PartPosition, gap: f64) -> Option<[(PartPosition, bool); 2]> {
        let side = |plane, offset| PartPosition {
            plane,
            plane_offset: offset,
            ..position
        };
        match self {
            Self::Single => None,
            Self::BackToBack => {
                let p = side(OnPlane::Left, gap / 2.);
                Some([(p, false), (p, true)])
            }
            Self::ToesIn => {
                let p = side(OnPlane::Right, -gap / 2.);
                Some([(p, false), (p, true)])
            }
            Self::Stacked => {
                let depth = |depth, offset| PartPosition {
                    depth,
                    depth_offset: offset,
                    ..position
                };
                Some([
                    (depth(AtDepth::Front, gap / 2.), false),
                    (depth(AtDepth::Behind, -gap / 2.), false),
                ])
            }
        }
    }
}

/// Member shapes for a beam or column command on `start`→`end`: one, or
/// two for a twin profile.
pub fn member_shapes(
    project: &Project,
    kind: MemberKind,
    profile: ObjectId,
    (start, end): (Point3, Point3),
    position: PartPosition,
    twin: Twin,
    gap: &str,
) -> Result<Vec<ElementShape>, String> {
    let shape = |position, reversed: bool| {
        let (a, b) = if reversed { (end, start) } else { (start, end) };
        let mut position: PartPosition = position;
        if reversed {
            // Dx stays at the same physical end.
            std::mem::swap(&mut position.start_dx, &mut position.end_dx);
            position.start_dx = -position.start_dx;
            position.end_dx = -position.end_dx;
        }
        project
            .new_member(kind, profile, a, b, position)
            .map_err(|e| e.to_string())
    };
    if twin == Twin::Single {
        return Ok(vec![shape(position, false)?]);
    }
    let gap = super::number(&gap.replace(',', "."))?;
    if !(0.0..=1000.0).contains(&gap) {
        return Err("Зазор сдвоенного профиля: от 0 до 1000 мм".into());
    }
    let pair = twin.pair(position, gap).expect("twin");
    pair.into_iter().map(|(p, r)| shape(p, r)).collect()
}
