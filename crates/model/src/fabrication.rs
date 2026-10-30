//! Developed stock and bending instructions share the model's length calculation.
use crate::{Element, ElementShape, LengthMm, ModelError, ObjectId, Project, SweptPart};
use std::fmt::Write;

impl Project {
    /// Straight stock before bending; the formed element in the project is unchanged.
    pub fn fabrication_blank(&self, id: ObjectId) -> Result<Element, ModelError> {
        let mut element = self
            .elements
            .get(&id)
            .ok_or(ModelError::MissingReference(id))?
            .clone();
        let length = self.part_developed_length(id)?;
        let swept = self
            .swept_parts
            .get(&id)
            .ok_or_else(|| ModelError::InvalidValue("Деталь не имеет параметров гибки".into()))?;
        match (swept, &mut element.shape) {
            (SweptPart::BentPlate { width, .. }, ElementShape::Plate { contour, .. }) => {
                *contour = vec![[0., 0.], [length, 0.], [length, *width], [0., *width]];
            }
            (_, ElementShape::Member { length: stock, .. }) => {
                *stock =
                    LengthMm::new(length).map_err(|e| ModelError::InvalidValue(e.to_string()))?
            }
            _ => {
                return Err(ModelError::InvalidValue(
                    "Нет заготовки для этой детали".into(),
                ));
            }
        }
        Ok(element)
    }

    /// Human-readable card, with signed bend angles and coordinates in local XY.
    /// This describes geometry, not a controller program or a tooling allowance.
    pub fn bending_card(
        &self,
        id: ObjectId,
        mark: &str,
        quantity: usize,
    ) -> Result<String, ModelError> {
        let element = self
            .elements
            .get(&id)
            .ok_or(ModelError::MissingReference(id))?;
        let length = self.part_developed_length(id)?;
        let mut out = format!(
            "КАРТА ГИБКИ\nМарка: {mark}\nДеталь: {}\nКоличество: {quantity}\nДлина заготовки: {length:.3} мм\n",
            element.name
        );
        writeln!(
            out,
            "Материал: {}",
            self.materials
                .get(&element.material)
                .ok_or(ModelError::MissingReference(element.material))?
                .name
        )
        .unwrap();
        let swept = self
            .swept_parts
            .get(&id)
            .ok_or_else(|| ModelError::InvalidValue("Деталь не имеет параметров гибки".into()))?;
        match swept {
            SweptPart::CurvedBeam {
                center,
                radius,
                start_angle,
                sweep_angle,
            } => {
                writeln!(out, "Гнутая балка\nРадиус оси: {radius:.3} мм\nЦентр дуги XY: {:.3}; {:.3} мм\nНачальный угол: {:.6}°\nУгол гибки со знаком: {:.6}°", center[0], center[1], start_angle.to_degrees(), sweep_angle.to_degrees()).unwrap();
            }
            SweptPart::PolyBeam {
                points,
                bend_radius,
            } => {
                writeln!(out, "Полибалка\nРадиус скругления оси: {bend_radius:.3} мм").unwrap();
                for (i, p) in points.iter().enumerate() {
                    writeln!(out, "Точка {}: {:.3}; {:.3} мм", i + 1, p[0], p[1]).unwrap();
                }
                let mut station = 0.;
                let mut previous_trim = 0.;
                for (i, p) in points.windows(3).enumerate() {
                    let a = [p[1][0] - p[0][0], p[1][1] - p[0][1]];
                    let b = [p[2][0] - p[1][0], p[2][1] - p[1][1]];
                    let turn = (a[0] * b[1] - a[1] * b[0]).atan2(a[0] * b[0] + a[1] * b[1]);
                    let trim = bend_radius * (turn.abs() / 2.).tan();
                    station += a[0].hypot(a[1]) - previous_trim - trim;
                    writeln!(
                        out,
                        "Сгиб {}: начало дуги от торца {:.3} мм; угол {:.6}°; длина дуги {:.3} мм",
                        i + 1,
                        station,
                        turn.to_degrees(),
                        bend_radius * turn.abs()
                    )
                    .unwrap();
                    station += bend_radius * turn.abs();
                    previous_trim = trim;
                }
                if *bend_radius == 0. {
                    out.push_str("Острые изломы заданы без радиуса: технология соединения или гибки определяется отдельно.\n");
                }
            }
            SweptPart::BentPlate {
                width,
                first_length,
                second_length,
                radius,
                angle,
            } => {
                let ElementShape::Plate { thickness, .. } = element.shape else {
                    unreachable!()
                };
                let allowance = (radius + thickness.value() / 2.) * angle.abs();
                writeln!(out, "Гнутый лист\nЗаготовка: {length:.3} × {width:.3} × {:.3} мм\nПрямые участки до касания: {first_length:.3}; {second_length:.3} мм\nВнутренний радиус: {radius:.3} мм\nУгол гибки со знаком: {:.6}°\nПрипуск дуги по нейтральному слою: {allowance:.3} мм\nГраницы зоны сгиба от торца: {first_length:.3}; {:.3} мм\nОсь сгиба от торца: {:.3} мм\nКоэффициент нейтрального слоя: 0,5", thickness.value(), angle.to_degrees(), first_length+allowance, first_length+allowance/2.).unwrap();
            }
        }
        if let ElementShape::Member { profile, .. } = element.shape {
            writeln!(
                out,
                "Профиль: {}\nДлина заготовки рассчитана по оси траектории.",
                self.profiles
                    .get(&profile)
                    .ok_or(ModelError::MissingReference(profile))?
                    .name
            )
            .unwrap();
        }
        out.push_str("Траектория и знак угла заданы в локальной плоскости XY детали; положительный угол — против часовой стрелки.\nNC/DSTV содержит раскрой заготовки до гибки. Карта не является программой гибочного станка. Технологические припуски и пружинение не включены.\n");
        Ok(out)
    }
}
