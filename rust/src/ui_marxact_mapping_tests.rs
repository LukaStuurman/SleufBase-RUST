use super::*;
use sleufbase_rust::{file_loader::PreparedMarXact, marxact::MarXactResult, virtual_trench::TrenchPoint};

fn outcomes() -> Vec<FileOutcome> {
    let mut trench = VirtualTrench::new("PS fixture".into(),
        Vertex { x: 0., y: 0., z: Some(10.) }, Vertex { x: 4., y: 0., z: Some(10.) }, 2.).unwrap();
    trench.source_path = Some("fixture.dxf".into());
    trench.boundary_3d = vec![
        Vertex { x: 0., y: -1., z: Some(10.) }, Vertex { x: 4., y: -1., z: Some(10.) },
        Vertex { x: 4., y: 1., z: Some(10.) }, Vertex { x: 0., y: 1., z: Some(10.) },
    ];
    for (index, name) in ["Gas_lage_druk", "ZZQ"].into_iter().enumerate() {
        trench.add_object(TrenchPoint {
            metadata: serde_json::json!({"fixture": index}), role: "object".into(),
            object_name: format!("Bronmeetpunt {index}"), source_name: name.into(),
            position: Vertex { x: 1. + index as f64, y: 0., z: Some(9. - index as f64) },
            attributes: [format!("Bronmeetpunt {index}"), name.into(), "survey-block".into()],
        }).unwrap();
    }
    let markers = vec![sleufbase_rust::cross_section::markers(&trench)];
    vec![FileOutcome { path: "fixture.dxf".into(), result: Ok(FileContent::MarXact(Box::new(PreparedMarXact {
        result: MarXactResult { trenches: vec![trench], leader_count: 6, insert_count: 2,
            assigned_insert_count: 2, skipped_boundary_count: 0 }, markers,
    }))) }]
}

fn staged() -> SleufBaseApp {
    let mut app = SleufBaseApp::default();
    app.prepare_marxact_mapping(outcomes(), &egui::Context::default());
    app
}

#[test]
fn every_successful_import_waits_for_confirmation_even_with_known_or_no_names() {
    let ctx = egui::Context::default();
    let mut app = SleufBaseApp::default();
    app.migrated_settings.marxact_name_mappings.insert("zzq".into(), "ms".into());
    app.prepare_marxact_mapping(outcomes(), &ctx);
    assert!(app.marxact_mapping.as_ref().unwrap().rows.iter().all(|row| row.target.is_some()));
    assert!(app.virtual_trenches.is_empty() && !app.dirty);
    app.cancel_file_load();
    let mut borders_only = outcomes();
    if let Ok(FileContent::MarXact(prepared)) = &mut borders_only[0].result {
        prepared.result.trenches[0].objects.clear();
        prepared.result.assigned_insert_count = 0;
        prepared.markers = vec![Vec::new()];
    }
    app.prepare_marxact_mapping(borders_only, &ctx);
    assert!(app.marxact_mapping.as_ref().unwrap().rows.is_empty());
    assert_eq!(app.marxact_mapping.as_ref().unwrap().trench_count, 1);
    assert!(app.virtual_trenches.is_empty() && !app.dirty);
    assert!(app.confirm_marxact_mapping_with(&ctx, &mut |_, _| panic!("there are no name mappings to save")));
    assert_eq!(app.virtual_trenches.len(), 1);
    assert!(app.virtual_trenches[0].objects.is_empty());
}

#[test]
fn unknown_names_wait_for_a_choice_and_cancel_does_not_publish_or_save() {
    let mut app = staged();
    let settings = app.migrated_settings.clone();
    let dialog = app.marxact_mapping.as_ref().unwrap();
    assert_eq!(dialog.rows.len(), 2);
    assert_eq!(dialog.rows.iter().find(|row| row.source == "Gas_lage_druk").unwrap().target.as_deref(), Some("ld"));
    assert!(dialog.rows.iter().find(|row| row.source == "ZZQ").unwrap().target.is_none());
    assert!(app.virtual_trenches.is_empty() && !app.dirty);
    assert!(!app.confirm_marxact_mapping_with(&egui::Context::default(), &mut |_, _| panic!("incomplete choices cannot be saved")));
    assert!(app.marxact_mapping.as_ref().unwrap().error.contains("Kies"));
    app.cancel_file_load();
    assert!(app.marxact_mapping.is_none() && app.virtual_trenches.is_empty() && !app.dirty);
    assert_eq!(app.migrated_settings, settings);
}

#[test]
fn failed_save_retains_choices_and_import_without_remembering_keeps_measured_data() {
    let mut app = staged();
    let before = match &app.marxact_mapping.as_ref().unwrap().outcomes[0].result {
        Ok(FileContent::MarXact(prepared)) => prepared.result.trenches[0].clone(), _ => unreachable!(),
    };
    let settings = app.migrated_settings.clone();
    app.marxact_mapping.as_mut().unwrap().rows.iter_mut()
        .find(|row| row.source == "ZZQ").unwrap().target = Some(names::IGNORE.into());
    assert!(!app.confirm_marxact_mapping_with(&egui::Context::default(), &mut |_, _| Err("Disk full".into())));
    assert!(app.marxact_mapping.as_ref().unwrap().error.contains("Disk full"));
    assert!(app.virtual_trenches.is_empty() && !app.dirty);
    assert_eq!(app.migrated_settings, settings);
    app.marxact_mapping.as_mut().unwrap().remember = false;
    assert!(app.confirm_marxact_mapping_with(&egui::Context::default(), &mut |_, _| panic!("remember is disabled")));
    assert!(app.marxact_mapping.is_none() && app.dirty);
    let imported = &app.virtual_trenches[0];
    assert_eq!((imported.start, imported.end, &imported.boundary_3d), (before.start, before.end, &before.boundary_3d));
    assert_eq!(imported.objects.len(), 1);
    assert_eq!(imported.objects[0].source_name, "ld");
    assert_eq!(imported.objects[0].position, before.objects[0].position);
    assert_eq!(imported.objects[0].attributes, before.objects[0].attributes);
    assert_eq!(imported.objects[0].metadata, before.objects[0].metadata);
    assert_eq!(app.trench_markers.len(), app.virtual_trenches.len());
    assert_eq!(app.migrated_settings, settings);
}

#[test]
fn remembered_choices_survive_restart_and_override_an_older_project_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    let mut app = staged();
    app.project_settings = Some(app.migrated_settings.clone());
    app.marxact_mapping.as_mut().unwrap().rows.iter_mut()
        .find(|row| row.source == "ZZQ").unwrap().target = Some("ms".into());
    assert!(app.confirm_marxact_mapping_with(&ctx, &mut |settings, autosave|
        settings_store::save_pair(root.path(), settings, autosave)));
    assert_eq!(app.project_settings.as_ref().unwrap().marxact_name_mappings.get("zzq").map(String::as_str), Some("ms"));
    let (saved, _) = settings_store::read_pair(root.path()).unwrap();
    let mut old_project = MigratedSettings::default();
    old_project.marxact_name_mappings.insert("ZZQ".into(), "ld".into());
    let mut reopened = SleufBaseApp { migrated_settings: saved, project_settings: Some(old_project), ..Default::default() };
    reopened.prepare_marxact_mapping(outcomes(), &ctx);
    let row = reopened.marxact_mapping.as_ref().unwrap().rows.iter().find(|row| row.source == "ZZQ").unwrap();
    assert_eq!(row.target.as_deref(), Some("ms"));
    assert!(row.remembered);
}

#[test]
fn project_switch_discards_staged_measurements_and_late_confirmation() {
    let mut app = staged();
    let settings = app.migrated_settings.clone();
    app.project_generation += 1;
    assert!(!app.confirm_marxact_mapping_with(&egui::Context::default(), &mut |_, _| panic!("stale import cannot save")));
    assert!(app.marxact_mapping.is_none() && app.virtual_trenches.is_empty());
    assert_eq!(app.migrated_settings, settings);
    let mut app = staged();
    app.execute_project_action(ProjectAction::New, &egui::Context::default());
    assert!(app.marxact_mapping.is_none() && app.virtual_trenches.is_empty());
}

#[test]
fn mapping_window_exposes_pending_names_and_escape_cancels() {
    let mut app = staged();
    let ctx = egui::Context::default(); theme::install(&ctx);
    let render = |app: &mut SleufBaseApp, events| ctx.run(egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1000., 760.))), events, ..Default::default()
    }, |ctx| app.show_marxact_mapping(ctx));
    let mut output = render(&mut app, vec![]);
    for _ in 0..3 { output = render(&mut app, vec![]); }
    for name in ["Gas_lage_druk", "ZZQ", "Importeren", "Annuleren"] {
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text)
            if text.galley.text() == name && shape.clip_rect.contains(text.pos + text.galley.rect.center().to_vec2()))), "{name}");
    }
    render(&mut app, vec![egui::Event::Key { key: egui::Key::Escape, physical_key: None,
        pressed: true, repeat: false, modifiers: Default::default() }]);
    assert!(app.marxact_mapping.is_none() && app.virtual_trenches.is_empty() && !app.dirty);
}
