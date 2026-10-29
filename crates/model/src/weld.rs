//! Welds by the Russian standards: joint types of ГОСТ 5264, 14771, 8713
//! and the joints under acute and obtuse angles (ГОСТ 11534, 11533, 23518),
//! welding methods and the weld designation of ГОСТ 2.312.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeldType {
    /// Тавровое соединение без скоса кромок (угловой шов).
    Fillet,
    /// Тавровое со скосом одной кромки.
    TeeBevel,
    /// Тавровое с двумя симметричными скосами одной кромки (K).
    TeeK,
    /// Угловое соединение без скоса кромок.
    Corner,
    /// Угловое со скосом одной кромки.
    CornerBevel,
    /// Стыковой со скосом двух кромок (V).
    Butt,
    /// Стыковой без скоса кромок.
    ButtSquare,
    /// Стыковой со скосом одной кромки.
    ButtBevel,
    /// Стыковой с двумя симметричными скосами одной кромки (K).
    ButtK,
    /// Стыковой с двумя симметричными скосами (X), двусторонний.
    ButtX,
    /// Нахлёсточный.
    Lap,
}

/// Which sides of the joint are welded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeldSides {
    #[default]
    One,
    Both,
}

impl WeldSides {
    pub fn label(self) -> &'static str {
        match self {
            Self::One => "односторонний",
            Self::Both => "двусторонний",
        }
    }
}

/// Joint kind letter of the standards: С, У, Т, Н.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JointKind {
    Butt,
    Corner,
    Tee,
    Lap,
}

impl JointKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Butt => "Стыковое (С)",
            Self::Corner => "Угловое (У)",
            Self::Tee => "Тавровое (Т)",
            Self::Lap => "Нахлёсточное (Н)",
        }
    }
}

impl WeldType {
    pub const ALL: [WeldType; 11] = [
        Self::Fillet,
        Self::TeeBevel,
        Self::TeeK,
        Self::Corner,
        Self::CornerBevel,
        Self::ButtSquare,
        Self::ButtBevel,
        Self::Butt,
        Self::ButtK,
        Self::ButtX,
        Self::Lap,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Fillet => "угловой (тавровое соединение)",
            Self::TeeBevel => "тавровое со скосом одной кромки",
            Self::TeeK => "тавровое с двумя скосами одной кромки (K)",
            Self::Corner => "угловое соединение без скоса кромок",
            Self::CornerBevel => "угловое соединение со скосом одной кромки",
            Self::ButtSquare => "стыковой без скоса кромок",
            Self::ButtBevel => "стыковой со скосом одной кромки",
            Self::Butt => "стыковой V-образный",
            Self::ButtK => "стыковой K-образный",
            Self::ButtX => "стыковой X-образный",
            Self::Lap => "нахлёсточный",
        }
    }
    /// Short name for lists («угловой», «стыковой»…).
    pub fn short(self) -> &'static str {
        match self {
            Self::Fillet => "угловой",
            Self::TeeBevel => "тавровый со скосом",
            Self::TeeK => "тавровый K",
            Self::Corner => "угловое соединение",
            Self::CornerBevel => "угловое соединение со скосом",
            Self::Butt => "стыковой",
            Self::ButtSquare => "стыковой без скоса",
            Self::ButtBevel => "стыковой со скосом одной кромки",
            Self::ButtK => "стыковой K",
            Self::ButtX => "стыковой X",
            Self::Lap => "нахлёсточный",
        }
    }
    pub fn kind(self) -> JointKind {
        match self {
            Self::Fillet | Self::TeeBevel | Self::TeeK => JointKind::Tee,
            Self::Corner | Self::CornerBevel => JointKind::Corner,
            Self::Lap => JointKind::Lap,
            _ => JointKind::Butt,
        }
    }
    /// Whether the designation carries the leg «⊿k» (angle welds).
    pub fn has_leg(self) -> bool {
        matches!(self, Self::Fillet | Self::Corner | Self::Lap)
    }
    /// Joint designation in the standard of `process` for the sides; empty
    /// for the standards of joints under angles, where the engineer picks
    /// the designation (their numbering depends on the angle).
    pub fn joint(self, sides: WeldSides, process: WeldProcess) -> &'static str {
        let catalog = process.catalog();
        let find = |s: WeldSides| {
            catalog
                .iter()
                .find(|j| j.weld_type == self && j.sides == s)
                .map(|j| j.code)
        };
        // A joint the standard has for the other side only (ГОСТ 8713 has
        // only two-sided У and Т joints with bevels): the nearest one.
        find(sides)
            .or_else(|| find(WeldSides::Both))
            .or_else(|| find(WeldSides::One))
            .unwrap_or("")
    }
}

/// One joint type of a standard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Joint {
    pub code: &'static str,
    pub weld_type: WeldType,
    pub sides: WeldSides,
    pub edges: &'static str,
}

const fn j(
    code: &'static str,
    weld_type: WeldType,
    sides: WeldSides,
    edges: &'static str,
) -> Joint {
    Joint {
        code,
        weld_type,
        sides,
        edges,
    }
}

use WeldSides::{Both, One};
use WeldType::*;

/// Main joint types of ГОСТ 5264-80; ГОСТ 14771-76 uses the same
/// designations for them.
const MANUAL_JOINTS: [Joint; 19] = [
    j("С2", ButtSquare, One, "без скоса кромок"),
    j("С7", ButtSquare, Both, "без скоса кромок"),
    j("С8", ButtBevel, One, "со скосом одной кромки"),
    j("С12", ButtBevel, Both, "со скосом одной кромки"),
    j(
        "С15",
        ButtK,
        Both,
        "с двумя симметричными скосами одной кромки",
    ),
    j("С17", Butt, One, "со скосом кромок"),
    j("С21", Butt, Both, "со скосом кромок"),
    j("С25", ButtX, Both, "с двумя симметричными скосами кромок"),
    j("У4", Corner, One, "без скоса кромок"),
    j("У5", Corner, Both, "без скоса кромок"),
    j("У6", CornerBevel, One, "со скосом одной кромки"),
    j("У7", CornerBevel, Both, "со скосом одной кромки"),
    j("Т1", Fillet, One, "без скоса кромок"),
    j("Т3", Fillet, Both, "без скоса кромок"),
    j("Т6", TeeBevel, One, "со скосом одной кромки"),
    j("Т7", TeeBevel, Both, "со скосом одной кромки"),
    j(
        "Т8",
        TeeK,
        Both,
        "с двумя симметричными скосами одной кромки",
    ),
    j("Н1", Lap, One, "без скоса кромок"),
    j("Н2", Lap, Both, "без скоса кромок"),
];

/// Main joint types of ГОСТ 8713-79: one-sided butt joints on a flux bed
/// (С4, С9, С18), У and Т joints with bevels two-sided only.
const SUBMERGED_JOINTS: [Joint; 16] = [
    j(
        "С4",
        ButtSquare,
        One,
        "без скоса кромок, на флюсовой подушке",
    ),
    j("С7", ButtSquare, Both, "без скоса кромок"),
    j(
        "С9",
        ButtBevel,
        One,
        "со скосом одной кромки, на флюсовой подушке",
    ),
    j("С12", ButtBevel, Both, "со скосом одной кромки"),
    j(
        "С15",
        ButtK,
        Both,
        "с двумя симметричными скосами одной кромки",
    ),
    j("С18", Butt, One, "со скосом кромок, на флюсовой подушке"),
    j("С21", Butt, Both, "со скосом кромок"),
    j("С25", ButtX, Both, "с двумя симметричными скосами кромок"),
    j("У5", Corner, Both, "без скоса кромок"),
    j("У7", CornerBevel, Both, "со скосом одной кромки"),
    j("Т1", Fillet, One, "без скоса кромок"),
    j("Т3", Fillet, Both, "без скоса кромок"),
    j("Т7", TeeBevel, Both, "со скосом одной кромки"),
    j(
        "Т8",
        TeeK,
        Both,
        "с двумя симметричными скосами одной кромки",
    ),
    j("Н1", Lap, One, "без скоса кромок"),
    j("Н2", Lap, Both, "без скоса кромок"),
];

/// Welding process; it names the standard of the joint types.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeldProcess {
    /// Ручная дуговая сварка, ГОСТ 5264-80.
    #[default]
    Manual,
    /// Дуговая сварка в защитном газе, ГОСТ 14771-76.
    GasShielded,
    /// Автоматическая и механизированная сварка под флюсом, ГОСТ 8713-79.
    Submerged,
    /// Ручная дуговая сварка, соединения под острыми и тупыми углами, ГОСТ 11534-75.
    ManualAngle,
    /// Дуговая сварка в защитных газах, соединения под углами, ГОСТ 23518-79.
    GasAngle,
    /// Сварка под флюсом, соединения под углами, ГОСТ 11533-75.
    SubmergedAngle,
}

impl WeldProcess {
    pub const ALL: [WeldProcess; 6] = [
        Self::Manual,
        Self::GasShielded,
        Self::Submerged,
        Self::ManualAngle,
        Self::GasAngle,
        Self::SubmergedAngle,
    ];
    pub fn standard(self) -> &'static str {
        match self {
            Self::Manual => "ГОСТ 5264-80",
            Self::GasShielded => "ГОСТ 14771-76",
            Self::Submerged => "ГОСТ 8713-79",
            Self::ManualAngle => "ГОСТ 11534-75",
            Self::GasAngle => "ГОСТ 23518-79",
            Self::SubmergedAngle => "ГОСТ 11533-75",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Manual => "ручная дуговая",
            Self::GasShielded => "в защитном газе",
            Self::Submerged => "под флюсом",
            Self::ManualAngle => "ручная дуговая, под углом",
            Self::GasAngle => "в защитном газе, под углом",
            Self::SubmergedAngle => "под флюсом, под углом",
        }
    }
    /// Joints under acute and obtuse angles (only Т and У joints).
    pub fn angled(self) -> bool {
        matches!(
            self,
            Self::ManualAngle | Self::GasAngle | Self::SubmergedAngle
        )
    }
    /// Title of the standard for the list of referenced documents.
    pub fn title(self) -> &'static str {
        match self {
            Self::Manual => {
                "Ручная дуговая сварка. Соединения сварные. Основные типы, конструктивные элементы и размеры"
            }
            Self::GasShielded => {
                "Дуговая сварка в защитном газе. Соединения сварные. Основные типы, конструктивные элементы и размеры"
            }
            Self::Submerged => {
                "Сварка под флюсом. Соединения сварные. Основные типы, конструктивные элементы и размеры"
            }
            Self::ManualAngle => {
                "Ручная дуговая сварка. Соединения сварные под острыми и тупыми углами. Основные типы, конструктивные элементы и размеры"
            }
            Self::GasAngle => {
                "Дуговая сварка в защитных газах. Соединения сварные под острыми и тупыми углами. Основные типы, конструктивные элементы и размеры"
            }
            Self::SubmergedAngle => {
                "Автоматическая и полуавтоматическая дуговая сварка под флюсом. Соединения сварные под острыми и тупыми углами. Основные типы, конструктивные элементы и размеры"
            }
        }
    }
    /// Main joint types with automatic designations; empty for the joints
    /// under angles, whose designation the engineer picks from the standard.
    pub fn catalog(self) -> Vec<Joint> {
        match self {
            Self::Manual | Self::GasShielded => MANUAL_JOINTS.to_vec(),
            Self::Submerged => SUBMERGED_JOINTS.to_vec(),
            _ => Vec::new(),
        }
    }
    /// Welding methods of the standard with their designations (the
    /// designation may be left out, ГОСТ 2.312).
    pub fn methods(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Manual | Self::ManualAngle => &[],
            Self::GasShielded | Self::GasAngle => &[
                (
                    "ИН",
                    "в инертных газах неплавящимся электродом без присадочного металла",
                ),
                (
                    "ИНп",
                    "в инертных газах неплавящимся электродом с присадочным металлом",
                ),
                ("ИП", "в инертных газах и их смесях плавящимся электродом"),
                ("УП", "в углекислом газе и его смесях плавящимся электродом"),
            ],
            Self::Submerged => &[
                ("АФ", "автоматическая на весу"),
                ("АФф", "автоматическая на флюсовой подушке"),
                ("АФм", "автоматическая на флюсомедной подкладке"),
                ("АФо", "автоматическая на остающейся подкладке"),
                ("АФш", "автоматическая с предварительным подварочным швом"),
                (
                    "АФк",
                    "автоматическая с предварительной подваркой корня шва",
                ),
                ("МФ", "механизированная на весу"),
                ("МФо", "механизированная на остающейся подкладке"),
                ("МФш", "механизированная с предварительным подварочным швом"),
                (
                    "МФк",
                    "механизированная с предварительной подваркой корня шва",
                ),
            ],
            Self::SubmergedAngle => &[
                ("А", "автоматическая"),
                ("П", "полуавтоматическая"),
                ("Ас", "автоматическая на подкладке"),
                ("Пс", "полуавтоматическая на подкладке"),
                ("Апш", "автоматическая с подварочным швом"),
                ("Ппш", "полуавтоматическая с подварочным швом"),
            ],
        }
    }
}

/// Intermittent weld: welded length and pitch of the segments.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Intermittent {
    pub length: f64,
    pub pitch: f64,
    /// Шахматное расположение (two-sided welds); цепное otherwise.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub staggered: bool,
}

/// Designation details of ГОСТ 2.312 beyond the joint type: the joint
/// picked by the engineer, the welding method, an intermittent weld and the
/// side the weld is seen from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WeldGost {
    /// Joint designation («Т8», «У4»…); empty — by the weld type and sides.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub joint: String,
    /// Welding method of the standard («УП», «АФ»…); may be empty.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intermittent: Option<Intermittent>,
    /// Seen from the reverse side: the designation goes under the shelf.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub back: bool,
}

impl WeldGost {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

pub(crate) fn size_text(size: f64) -> String {
    if size.fract().abs() < 1e-9 {
        format!("{size:.0}")
    } else {
        format!("{size}").replace('.', ",")
    }
}

/// Leg sign of ГОСТ 2.312 (right triangle). The sheet font has no glyph for
/// it, so drawings draw it as lines and the screen shows «Δ».
pub const LEG_SIGN: char = '⊿';

/// Weld designation of ГОСТ 2.312 for the shelf of the leader: standard,
/// joint, method, leg «⊿k» (angle welds), intermittent «l/t» or «lZt». The
/// signs of a weld all around and of a site weld go on the leader break.
pub fn weld_mark(
    process: WeldProcess,
    weld_type: WeldType,
    sides: WeldSides,
    size: f64,
    gost: &WeldGost,
) -> String {
    let joint = gost.joint.trim();
    let joint = if joint.is_empty() {
        weld_type.joint(sides, process)
    } else {
        joint
    };
    let mut mark = process.standard().to_string();
    for part in [joint, gost.method.trim()] {
        if !part.is_empty() {
            mark.push('-');
            mark.push_str(part);
        }
    }
    if weld_type.has_leg() {
        mark.push_str(&format!("-{LEG_SIGN}{}", size_text(size)));
    }
    if let Some(i) = gost.intermittent {
        let sign = if i.staggered { "Z" } else { "/" };
        mark.push_str(&format!(
            "-{}{sign}{}",
            size_text(i.length),
            size_text(i.pitch)
        ));
    }
    mark
}

/// Words for the signs drawn on the leader, for lists: « по контуру»,
/// « (монт.)».
pub fn weld_notes(around: bool, shop: bool) -> String {
    let mut out = String::new();
    if around {
        out.push_str(" по контуру");
    }
    if !shop {
        out.push_str(" (монт.)");
    }
    out
}

/// Welded length of a weld of `length` along its path: an intermittent
/// weld is welded on l of every t.
pub fn welded_length(length: f64, gost: &WeldGost) -> f64 {
    match gost.intermittent {
        Some(i) if i.pitch > 0. => {
            // Segments start at each end of the path: n segments of l.
            let n = ((length - i.length) / i.pitch).floor().max(0.) + 1.;
            (n * i.length).min(length)
        }
        _ => length,
    }
}

/// What the standards and ГОСТ 2.312 say against the weld settings, for the
/// properties panel; empty when everything fits.
pub fn weld_advice(
    process: WeldProcess,
    weld_type: WeldType,
    sides: WeldSides,
    gost: &WeldGost,
) -> Vec<String> {
    let mut out = Vec::new();
    let kind = weld_type.kind();
    if process.angled() {
        if matches!(kind, JointKind::Butt | JointKind::Lap) {
            out.push(format!(
                "{} — только тавровые и угловые соединения под острыми и тупыми углами",
                process.standard()
            ));
        }
        if gost.joint.trim().is_empty() {
            out.push(format!(
                "Укажите обозначение соединения по {} (зависит от угла и подготовки кромок)",
                process.standard()
            ));
        }
    } else if gost.joint.trim().is_empty() {
        let code = weld_type.joint(sides, process);
        let exact = process
            .catalog()
            .iter()
            .any(|j| j.weld_type == weld_type && j.sides == sides);
        if !exact && !code.is_empty() {
            out.push(format!(
                "В {} нет {} соединения «{}», принято {code}",
                process.standard(),
                sides.label().trim_end_matches("ий").to_owned() + "его",
                weld_type.label()
            ));
        }
    }
    let joint = gost.joint.trim();
    if let Some(letter) = joint.chars().next() {
        let expected = match kind {
            JointKind::Butt => 'С',
            JointKind::Corner => 'У',
            JointKind::Tee => 'Т',
            JointKind::Lap => 'Н',
        };
        if letter != expected {
            out.push(format!(
                "Обозначение {joint} не того вида: для типа «{}» — {}…",
                weld_type.label(),
                expected
            ));
        }
    }
    if !gost.method.trim().is_empty()
        && !process
            .methods()
            .iter()
            .any(|(m, _)| *m == gost.method.trim())
    {
        out.push(format!(
            "Способа «{}» нет в {}",
            gost.method.trim(),
            process.standard()
        ));
    }
    if let Some(i) = gost.intermittent {
        if !weld_type.has_leg() {
            out.push("Прерывистым бывает только угловой шов".into());
        }
        if i.length <= 0. || i.pitch <= i.length {
            out.push("Шаг прерывистого шва должен быть больше длины участка".into());
        }
        if i.staggered && sides == WeldSides::One {
            out.push("Шахматное расположение — у двустороннего шва".into());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_follow_gost_2_312() {
        let g = WeldGost::default();
        let mark = |p, t, s| weld_mark(p, t, s, 6.0, &g);
        use WeldProcess::*;
        assert_eq!(mark(Manual, Fillet, One), "ГОСТ 5264-80-Т1-⊿6");
        assert_eq!(mark(GasShielded, Fillet, Both), "ГОСТ 14771-76-Т3-⊿6");
        assert_eq!(mark(GasShielded, Butt, One), "ГОСТ 14771-76-С17");
        // ГОСТ 8713 has no С2 and С17.
        assert_eq!(mark(Submerged, ButtSquare, One), "ГОСТ 8713-79-С4");
        assert_eq!(mark(Submerged, Butt, One), "ГОСТ 8713-79-С18");
        assert_eq!(mark(Submerged, Butt, Both), "ГОСТ 8713-79-С21");
        assert_eq!(mark(Submerged, Lap, Both), "ГОСТ 8713-79-Н2-⊿6");
        assert_eq!(mark(Manual, TeeK, Both), "ГОСТ 5264-80-Т8");
        assert_eq!(mark(Manual, Corner, One), "ГОСТ 5264-80-У4-⊿6");
        assert_eq!(mark(Manual, ButtBevel, Both), "ГОСТ 5264-80-С12");
        // Only the two-sided Т7 in ГОСТ 8713.
        assert_eq!(mark(Submerged, TeeBevel, One), "ГОСТ 8713-79-Т7");
        let g = WeldGost {
            method: "УП".into(),
            intermittent: Some(Intermittent {
                length: 50.,
                pitch: 100.,
                staggered: false,
            }),
            ..Default::default()
        };
        assert_eq!(
            weld_mark(GasShielded, Fillet, One, 5., &g),
            "ГОСТ 14771-76-Т1-УП-⊿5-50/100"
        );
        let g = WeldGost {
            joint: "Т3".into(),
            method: "ИП".into(),
            intermittent: Some(Intermittent {
                length: 50.,
                pitch: 100.,
                staggered: true,
            }),
            ..Default::default()
        };
        assert_eq!(
            weld_mark(GasAngle, Fillet, Both, 4.5, &g),
            "ГОСТ 23518-79-Т3-ИП-⊿4,5-50Z100"
        );
        assert_eq!(weld_notes(true, false), " по контуру (монт.)");
    }

    #[test]
    fn advice_names_what_does_not_fit() {
        let g = WeldGost::default();
        assert!(weld_advice(WeldProcess::Manual, Fillet, One, &g).is_empty());
        let a = weld_advice(WeldProcess::Submerged, TeeBevel, One, &g);
        assert!(
            a[0].contains("одностороннего") && a[0].contains("Т7"),
            "{a:?}"
        );
        let a = weld_advice(WeldProcess::ManualAngle, Butt, One, &g);
        assert_eq!(a.len(), 2, "{a:?}");
        let g = WeldGost {
            joint: "С2".into(),
            method: "АФ".into(),
            intermittent: Some(Intermittent {
                length: 100.,
                pitch: 50.,
                staggered: true,
            }),
            ..Default::default()
        };
        assert_eq!(weld_advice(WeldProcess::Manual, Fillet, One, &g).len(), 4);
    }

    #[test]
    fn intermittent_welds_count_the_welded_segments() {
        let g = WeldGost {
            intermittent: Some(Intermittent {
                length: 50.,
                pitch: 150.,
                staggered: false,
            }),
            ..Default::default()
        };
        // Segments at 0, 150, 300, 450 on 500 mm.
        assert_eq!(welded_length(500., &g), 200.);
        assert_eq!(welded_length(30., &g), 30.);
        assert_eq!(welded_length(500., &WeldGost::default()), 500.);
    }
}
