use open_bim_commands::{Command, DeletePolicy, Document};
use open_bim_reports::{auto_assemblies, csv, generate, release};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/steel_release")
}

fn assembled() -> Document {
    let project = open_bim_persistence::load(dir().join("source.obim")).unwrap();
    let mut doc = Document::new(project).unwrap();
    let commands = auto_assemblies(doc.project()).unwrap();
    doc.transact("Сборки", &commands).unwrap();
    doc
}

fn bless_or_compare(name: &str, text: &str) {
    let path = dir().join(name);
    if std::env::var_os("OPEN_BIM_BLESS").is_some() {
        std::fs::write(&path, text).unwrap();
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text, "{name}");
}

#[test]
fn lists_match_independent_fixture_totals() {
    let doc = assembled();
    let r = generate(doc.project()).unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir().join("expected.json")).unwrap())
            .unwrap();
    let total = expected["total_mass_kg"].as_f64().unwrap();
    assert!(
        (r.total_part_mass_kg - total).abs() < 1e-3,
        "{}",
        r.total_part_mass_kg
    );
    assert!((r.total_assembly_mass_kg - total).abs() < 1e-3);
    assert!(r.unassembled.is_empty());
    assert_eq!(
        r.parts.iter().map(|p| p.quantity).sum::<usize>(),
        expected["parts"].as_u64().unwrap() as usize
    );
    // Same grouping as the independent script: section + length.
    let mut groups: BTreeMap<String, (usize, f64)> = BTreeMap::new();
    for p in &r.parts {
        let g = groups
            .entry(format!(
                "{}|{}",
                p.section,
                open_bim_reports::fmt(p.length_mm)
            ))
            .or_default();
        g.0 += p.quantity;
        g.1 += p.total_mass_kg;
    }
    let reference = expected["by_section_length"].as_object().unwrap();
    assert_eq!(groups.len(), reference.len());
    for (key, value) in reference {
        let (q, m) = groups[key];
        assert_eq!(q as u64, value["quantity"].as_u64().unwrap(), "{key}");
        assert!(
            (m - value["mass_kg"].as_f64().unwrap()).abs() < 2e-3,
            "{key}: {m}"
        );
    }
    let bolts: usize = r
        .fasteners
        .iter()
        .filter(|f| f.kind == "Болт")
        .map(|f| f.quantity)
        .sum();
    assert_eq!(bolts as u64, expected["bolts"].as_u64().unwrap());
    // Assemblies: shop welds joined 4 pairs; every part is in one assembly.
    assert_eq!(
        r.assemblies.iter().map(|a| a.quantity).sum::<usize>(),
        16 - expected["shop_welds"].as_u64().unwrap() as usize
    );
    bless_or_compare("parts.csv", &csv::parts(&r));
    bless_or_compare("assemblies.csv", &csv::assemblies(&r));
    bless_or_compare("fasteners.csv", &csv::fasteners(&r));
}

#[test]
fn csv_escapes_quotes_line_breaks_commas_and_keeps_cyrillic() {
    assert_eq!(csv::field("Балка Б1"), "Балка Б1");
    assert_eq!(csv::field("Б1, ряд А"), "\"Б1, ряд А\"");
    assert_eq!(csv::field("Балка \"Б1\""), "\"Балка \"\"Б1\"\"\"");
    assert_eq!(csv::field("строка 1\nстрока 2"), "\"строка 1\nстрока 2\"");
    let mut doc = assembled();
    let id = *doc.project().elements.keys().next().unwrap();
    let mut element = doc.project().elements[&id].clone();
    element.name = "Колонна \"К-1\",\nось А".into();
    doc.transact("Имя", &[Command::ReplaceElement { element }])
        .unwrap();
    let text = csv::parts(&generate(doc.project()).unwrap());
    assert!(text.starts_with('\u{feff}'));
    assert!(text.contains("\"Колонна \"\"К-1\"\",\nось А\""));
    assert!(text.ends_with("\r\n"));
    bless_or_compare("escaping.csv", &text);
}

#[test]
fn model_change_regenerates_without_stale_positions() {
    let mut doc = assembled();
    let before = generate(doc.project()).unwrap();
    // Remove one angle cleat: its position quantity drops from 2 to 1.
    let angle = before
        .parts
        .iter()
        .find(|p| p.section.starts_with('L'))
        .unwrap()
        .clone();
    let victim = angle.ids[0];
    let connection = *doc
        .project()
        .connections
        .iter()
        .find(|(_, c)| c.children.values().any(|id| *id == victim))
        .unwrap()
        .0;
    doc.transact(
        "Удалить узел",
        &[Command::Delete {
            id: connection,
            policy: DeletePolicy::Cascade,
        }],
    )
    .unwrap();
    let after = generate(doc.project()).unwrap();
    assert!(after.revision > before.revision);
    let ids: Vec<_> = after.parts.iter().flat_map(|p| p.ids.clone()).collect();
    assert!(!ids.contains(&victim));
    assert!(ids.iter().all(|id| doc.project().elements.contains_key(id)));
    let angles: usize = after
        .parts
        .iter()
        .filter(|p| p.section.starts_with('L'))
        .map(|p| p.quantity)
        .sum();
    assert_eq!(angles, angle.quantity - 1);
    assert!(after.total_part_mass_kg < before.total_part_mass_kg);
}

#[test]
fn release_exports_set_and_refuses_outdated_drawings() {
    let mut doc = assembled();
    let commands = release::drawing_commands(doc.project()).unwrap();
    doc.transact("Комплект", &commands).unwrap();
    let out = std::env::temp_dir().join(format!("open-bim-release-{}", std::process::id()));
    let summary = release::export(doc.project(), &out).unwrap();
    assert_eq!(
        summary.drawings.len(),
        14,
        "GA + 6 assembly + 7 part drawings"
    );
    for file in &summary.files {
        assert!(out.join(file).is_file(), "{file}");
    }
    // A model change outdates drawings: export refuses until refreshed.
    let id = *doc.project().elements.keys().next().unwrap();
    let mut element = doc.project().elements[&id].clone();
    element.shape.frame_mut().origin.0[2] += 10.;
    doc.transact("Сдвиг", &[Command::ReplaceElement { element }])
        .unwrap();
    assert!(release::export(doc.project(), &out).is_err());
    let refresh = release::drawing_commands(doc.project()).unwrap();
    assert!(!refresh.is_empty());
    doc.transact("Обновить", &refresh).unwrap();
    release::export(doc.project(), &out).unwrap();
    std::fs::remove_dir_all(out).unwrap();
}
