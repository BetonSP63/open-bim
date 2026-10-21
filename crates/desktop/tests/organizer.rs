//! «Органайзер»: categories of the model tree with counts and masses.
use open_bim_desktop::organizer::{Grouping, Organizer, categories, kg, level_name};
use open_bim_model::{Level, ObjectId, example_steel_frame};

#[test]
fn categories_cover_every_part_with_total_mass() {
    let p = example_steel_frame().unwrap();
    let total: f64 = p
        .elements
        .keys()
        .map(|id| p.element_mass_kg(*id).unwrap())
        .sum();
    assert!(categories(&p, Grouping::None).is_empty());
    for grouping in &Grouping::ALL[1..] {
        let c = categories(&p, *grouping);
        let parts: usize = c.iter().map(|c| c.parts.len()).sum();
        assert_eq!(parts, p.elements.len(), "{grouping:?}");
        let mass: f64 = c.iter().map(|c| c.mass_kg).sum();
        assert!((mass - total).abs() < 1e-6, "{grouping:?}");
    }
    let kinds: Vec<String> = categories(&p, Grouping::Kind)
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert!(kinds.contains(&"Колонны".to_string()));
    assert!(kinds.contains(&"Балки".to_string()));
    // Equal columns share one mark category.
    let marks = categories(&p, Grouping::Mark);
    assert!(marks.iter().any(|c| c.parts.len() >= 2));
    assert!(marks.iter().all(|c| c.name.starts_with("поз. ")));
}

#[test]
fn level_is_the_highest_one_at_or_below_the_part() {
    let mut p = example_steel_frame().unwrap();
    let column = p
        .elements
        .values()
        .find(|e| {
            matches!(
                &e.shape,
                open_bim_model::ElementShape::Member {
                    member_kind: open_bim_model::MemberKind::Column,
                    ..
                }
            )
        })
        .unwrap()
        .id;
    let bottom = {
        let f = p.elements[&column].shape.frame();
        f.origin.0[2]
    };
    p.levels.clear();
    assert_eq!(level_name(&p, column), "Ниже уровней");
    for (id, name, z) in [(900, "0,000", bottom), (901, "+3,600", bottom + 3600.)] {
        p.levels.insert(
            ObjectId(id),
            Level {
                id: ObjectId(id),
                name: name.into(),
                elevation_mm: z,
            },
        );
    }
    assert_eq!(level_name(&p, column), "0,000");
    let levels = categories(&p, Grouping::Level);
    assert_eq!(levels[0].name, "0,000", "levels go bottom up");
}

#[test]
fn cache_follows_revision_and_grouping() {
    let p = example_steel_frame().unwrap();
    let mut o = Organizer::new(Grouping::Kind);
    let a = o.categories(&p);
    let b = o.categories(&p);
    assert!(std::sync::Arc::ptr_eq(&a, &b));
    o.grouping = Grouping::Profile;
    assert!(!std::sync::Arc::ptr_eq(&a, &o.categories(&p)));
    assert_eq!(kg(1234.56), "1\u{202f}234,6");
    assert_eq!(kg(5.0), "5,0");
}

#[test]
fn grouped_tree_is_shown_in_the_model_tree_panel() {
    use eframe::egui;
    use open_bim_desktop::tools::Editor;
    use open_bim_desktop::ui::{Workbench, chrome::SidePanel};
    let mut b = Workbench::new(Editor::new(example_steel_frame().unwrap()).unwrap());
    b.side_panel = Some(SidePanel::Tree);
    b.organizer = Organizer::new(Grouping::Kind);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut names = Vec::new();
    for _ in 0..3 {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280., 800.),
                )),
                ..Default::default()
            },
            |ui| {
                let _ = b.panels(ui);
            },
        );
        output.textures_delta.clear();
        if let Some(update) = output.platform_output.accesskit_update {
            names = update
                .nodes
                .iter()
                .filter_map(|(_, n)| n.label().map(str::to_string))
                .collect();
        }
    }
    assert!(
        names.iter().any(|n| n.starts_with("Колонны — ")),
        "{names:?}"
    );
}
