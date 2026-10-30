//! Creation dialog for swept metal parts. Dimensions are in millimetres.
use eframe::egui;
use open_bim_commands::Command;
use open_bim_model::{
    Element, ElementShape, Frame, LengthMm, MemberKind, ObjectId, Project, SweptPart,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Arc,
    Poly,
    Plate,
}

pub struct Dialog {
    pub kind: Kind,
    pub profile: ObjectId,
    pub material: ObjectId,
    pub name: String,
    pub origin: [String; 3],
    pub center: String,
    pub radius: String,
    pub start: String,
    pub angle: String,
    pub points: String,
    pub three_points: bool,
    pub width: String,
    pub thickness: String,
    pub first: String,
    pub second: String,
}
impl Dialog {
    pub fn new(kind: Kind, profile: ObjectId, material: ObjectId) -> Self {
        Self {
            kind,
            profile,
            material,
            name: match kind {
                Kind::Arc => "Гнутая балка",
                Kind::Poly => "Полибалка",
                Kind::Plate => "Гнутый лист",
            }
            .into(),
            origin: ["0".into(), "0".into(), "0".into()],
            center: "0,0".into(),
            radius: match kind {
                Kind::Arc => "1000",
                Kind::Poly => "200",
                Kind::Plate => "20",
            }
            .into(),
            start: "0".into(),
            angle: "90".into(),
            points: match kind {
                Kind::Arc => "1000,0; 707.106781,707.106781; 0,1000",
                _ => "0,0; 1000,0; 1000,1000",
            }
            .into(),
            three_points: false,
            width: "500".into(),
            thickness: "10".into(),
            first: "500".into(),
            second: "500".into(),
        }
    }
    pub fn show(&mut self, ctx: &egui::Context, project: &Project) -> (bool, bool) {
        let mut open = true;
        let mut create = false;
        egui::Window::new(self.name.clone()).open(&mut open).collapsible(false).show(ctx, |ui| {
            ui.horizontal(|ui| { ui.label("Наименование"); ui.text_edit_singleline(&mut self.name); });
            ui.horizontal(|ui| { ui.label("Начало локальной системы X, Y, Z, мм"); for v in &mut self.origin { ui.add(egui::TextEdit::singleline(v).desired_width(80.)); } });
            if self.kind != Kind::Plate {
                let profile = project.profiles.get(&self.profile).map_or("не выбран", |p| p.name.as_str());
                ui.label(format!("Профиль: {profile} (задаётся инструментом «Балка»)"));
            }
            egui::ComboBox::from_label("Материал проекта").selected_text(project.materials.get(&self.material).map_or("Выберите материал", |m| m.name.as_str())).show_ui(ui, |ui| { for m in project.materials.values() { ui.selectable_value(&mut self.material, m.id, &m.name); } });
            if self.kind == Kind::Arc { ui.checkbox(&mut self.three_points, "Задать дугу тремя точками"); }
            if self.kind == Kind::Poly || (self.kind == Kind::Arc && self.three_points) {
                ui.label("Точки в плоскости XY: X,Y; X,Y; …, мм"); ui.text_edit_singleline(&mut self.points);
            }
            if self.kind == Kind::Arc && !self.three_points { field(ui, "Центр X,Y, мм", &mut self.center); field(ui, "Начальный угол, градусы", &mut self.start); }
            if self.kind != Kind::Arc || !self.three_points { field(ui, if self.kind == Kind::Poly {"Радиус скругления, мм (0 — без скругления)"} else if self.kind == Kind::Plate {"Внутренний радиус, мм"} else {"Радиус оси, мм"}, &mut self.radius); }
            if self.kind == Kind::Plate || (self.kind == Kind::Arc && !self.three_points) { field(ui, "Угол, градусы", &mut self.angle); }
            if self.kind == Kind::Plate { field(ui, "Ширина, мм", &mut self.width); field(ui, "Толщина, мм", &mut self.thickness); field(ui, "Прямой участок 1, мм", &mut self.first); field(ui, "Прямой участок 2, мм", &mut self.second); }
            ui.label("Траектория задаётся в локальной плоскости XY; вторая координата сечения направлена вдоль Z.");
            create = ui.button("Создать").clicked();
        });
        (open, create)
    }
    pub fn command(&self, project: &Project) -> Result<Command, String> {
        let num = |s: &str| crate::tools::number(s);
        let id = ObjectId(project.next_id);
        let frame = Frame {
            origin: crate::tools::point(&self.origin)?,
            ..Default::default()
        };
        let swept = match self.kind {
            Kind::Arc => {
                let (center, radius, start_angle, sweep_angle) = if self.three_points {
                    arc_three(&crate::tools::parse_contour(&self.points)?)?
                } else {
                    let center = crate::tools::parse_contour(&format!("{};0,0", self.center))?[0];
                    (
                        center,
                        num(&self.radius)?,
                        num(&self.start)?.to_radians(),
                        num(&self.angle)?.to_radians(),
                    )
                };
                SweptPart::CurvedBeam {
                    center,
                    radius,
                    start_angle,
                    sweep_angle,
                }
            }
            Kind::Poly => SweptPart::PolyBeam {
                points: crate::tools::parse_contour(&self.points)?,
                bend_radius: num(&self.radius)?,
            },
            Kind::Plate => SweptPart::BentPlate {
                width: num(&self.width)?,
                first_length: num(&self.first)?,
                second_length: num(&self.second)?,
                radius: num(&self.radius)?,
                angle: num(&self.angle)?.to_radians(),
            },
        };
        let shape = if self.kind == Kind::Plate {
            let t = LengthMm::new(num(&self.thickness)?).map_err(|e| e.to_string())?;
            let len = num(&self.first)?
                + num(&self.second)?
                + (num(&self.radius)? + t.value() / 2.) * num(&self.angle)?.to_radians().abs();
            ElementShape::Plate {
                contour: vec![
                    [0., 0.],
                    [len, 0.],
                    [len, num(&self.width)?],
                    [0., num(&self.width)?],
                ],
                thickness: t,
                frame,
            }
        } else {
            ElementShape::Member {
                member_kind: MemberKind::Beam,
                profile: self.profile,
                length: LengthMm::new(1.).map_err(|e| e.to_string())?,
                frame,
                position: Default::default(),
            }
        };
        let mut element = Element {
            id,
            name: self.name.clone(),
            material: self.material,
            shape,
            properties: Default::default(),
        };
        let mut candidate = project.clone();
        candidate.elements.insert(id, element.clone());
        candidate.swept_parts.insert(id, swept.clone());
        let len = candidate
            .part_developed_length(id)
            .map_err(|e| e.to_string())?;
        if let ElementShape::Member { length, .. } = &mut element.shape {
            *length = LengthMm::new(len).map_err(|e| e.to_string())?;
        }
        Ok(Command::CreateSwept { element, swept })
    }
}
fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.text_edit_singleline(value);
    });
}
fn arc_three(p: &[[f64; 2]]) -> Result<([f64; 2], f64, f64, f64), String> {
    if p.len() != 3 {
        return Err("Для дуги нужны три точки".into());
    }
    let a = p[0];
    let b = [p[1][0] - a[0], p[1][1] - a[1]];
    let c = [p[2][0] - a[0], p[2][1] - a[1]];
    let d = 2. * (b[0] * c[1] - b[1] * c[0]);
    if d.abs() < 1e-9 {
        return Err("Точки дуги лежат на одной прямой".into());
    }
    let q = |v: [f64; 2]| v[0] * v[0] + v[1] * v[1];
    let center = [
        a[0] + (q(b) * c[1] - q(c) * b[1]) / d,
        a[1] + (b[0] * q(c) - c[0] * q(b)) / d,
    ];
    let b = p[1];
    let c = p[2];
    let angle = |v: [f64; 2]| (v[1] - center[1]).atan2(v[0] - center[0]);
    let start = angle(a);
    let mid = (angle(b) - start).rem_euclid(std::f64::consts::TAU);
    let end = (angle(c) - start).rem_euclid(std::f64::consts::TAU);
    let sweep = if mid < end {
        end
    } else {
        end - std::f64::consts::TAU
    };
    Ok((
        center,
        (a[0] - center[0]).hypot(a[1] - center[1]),
        start,
        sweep,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arc_passes_through_middle_point_in_either_direction() {
        let (c, r, start, sweep) = arc_three(&[[1., 0.], [0., 1.], [-1., 0.]]).unwrap();
        assert!(c[0].abs() < 1e-12 && c[1].abs() < 1e-12);
        assert!((r - 1.).abs() < 1e-12 && start.abs() < 1e-12);
        assert!((sweep - std::f64::consts::PI).abs() < 1e-12);
        assert!(arc_three(&[[1., 0.], [0., -1.], [-1., 0.]]).unwrap().3 < 0.);
        assert!(arc_three(&[[0., 0.], [1., 0.], [2., 0.]]).is_err());
    }
    #[test]
    fn default_dialogs_create_valid_parts() {
        for kind in [Kind::Arc, Kind::Poly, Kind::Plate] {
            let p = Project::default();
            let dialog = Dialog::new(kind, ObjectId(7), ObjectId(1));
            let mut doc = open_bim_commands::Document::new(p).unwrap();
            doc.transact("Создание", &[dialog.command(doc.project()).unwrap()])
                .unwrap();
            assert!(doc.project().element_body(ObjectId(8)).is_ok());
        }
    }
}
