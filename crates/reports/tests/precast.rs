use open_bim_model::precast::PrecastRow;
use open_bim_reports::csv;
use std::collections::BTreeSet;

#[test]
fn precast_csv_uses_form_seven_columns_and_russian_number_format() {
    let text = csv::precast(&[PrecastRow {
        mark: "П1".into(),
        designation: "Серия; А".into(),
        name: "Плита δ=200 П1".into(),
        count: 2,
        unit_mass_kg: 960.,
        unit_volume_m3: 0.4,
        grades: BTreeSet::from(["В25".into()]),
    }]);
    assert_eq!(
        text,
        "\u{feff}Поз.;Обозначение;Наименование;Кол.;Масса ед., кг;Примечание\r\nП1;\"Серия; А\";Плита δ=200 П1;2;960,000;В25, 0,400 м³\r\n"
    );
}
