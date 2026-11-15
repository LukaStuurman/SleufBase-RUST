use super::*;

struct WorkspaceUi {
    app: SleufBaseApp,
    ctx: egui::Context,
    size: Vec2,
}

impl WorkspaceUi {
    fn new(mut app: SleufBaseApp, size: Vec2) -> Self {
        app.open_virtual_editor(0);
        assert!(app.trench_editor.is_some(), "{}", app.message);
        let ctx = egui::Context::default();
        theme::install(&ctx);
        Self { app, ctx, size }
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let app = &mut self.app;
        self.ctx.run(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, self.size)),
            events,
            ..Default::default()
        }, |ctx| app.show_virtual_editor(ctx))
    }

    fn settle(&mut self) -> egui::FullOutput {
        for _ in 0..4 { self.frame(vec![]); }
        self.frame(vec![])
    }

    fn click_at(&mut self, pos: Pos2) -> egui::FullOutput {
        for pressed in [true, false] {
            self.frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos, button: egui::PointerButton::Primary, pressed,
                    modifiers: Default::default(),
                },
            ]);
        }
        self.settle()
    }

    fn visible_label(&self, out: &egui::FullOutput, label: &str) -> egui::Rect {
        let (rect, clip) = out.shapes.iter().rev().find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == label =>
                Some((text.galley.rect.translate(text.pos.to_vec2()), shape.clip_rect)),
            _ => None,
        }).unwrap_or_else(|| panic!("Ontbrekende bediening: {label}"));
        assert!(clip.contains_rect(rect), "{label} valt buiten de clip: {rect:?} / {clip:?}");
        assert!(egui::Rect::from_min_size(Pos2::ZERO, self.size).contains_rect(rect),
            "{label} valt buiten het venster van {:?}: {rect:?}", self.size);
        rect
    }

    fn click(&mut self, label: &str) -> egui::FullOutput {
        let out = self.settle();
        let pos = self.visible_label(&out, label).center();
        self.click_at(pos)
    }

    fn place(&mut self, along_fraction: f32, depth_fraction: f32) {
        let out = self.settle();
        let plot = out.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if rect.fill == Color32::from_rgb(239, 230, 218) => Some(rect.rect),
            _ => None,
        }).expect("doorsnede");
        self.click_at(plot.left_top() + egui::vec2(
            plot.width() * along_fraction, plot.height() * depth_fraction,
        ));
    }

    fn editor(&self) -> &Editor { self.app.trench_editor.as_ref().unwrap() }
    fn editor_mut(&mut self) -> &mut Editor { self.app.trench_editor.as_mut().unwrap() }
}

fn trench() -> VirtualTrench {
    VirtualTrench::new("PS12".into(),
        Vertex { x: 155000., y: 463000., z: Some(2.) },
        Vertex { x: 155010., y: 463000., z: Some(2.) }, 0.6).unwrap()
}

fn app_with(trench: VirtualTrench) -> SleufBaseApp {
    SleufBaseApp { virtual_trenches: vec![trench], ..Default::default() }
}

fn snapshot(value: &impl serde::Serialize) -> serde_json::Value {
    serde_json::to_value(value).unwrap()
}

#[test]
fn repeat_button_saves_only_positioned_cables_and_keeps_the_next_properties_ready() {
    for place_second in [false, true] {
        let mut ui = WorkspaceUi::new(app_with(trench()), egui::vec2(1200., 760.));
        ui.place(0.25, 0.4);
        let form = &mut ui.editor_mut().object;
        form.name = "Waterleiding".into();
        form.source = "water".into();
        form.role = "leiding".into();
        form.attributes = ["PE100".into(), "110".into(), "3".into()];
        form.diameter = 110.;
        form.quantity = 3;
        form.angle = 25.;
        ui.click("Toevoegen en herhalen");

        assert_eq!(ui.editor().draft.objects.len(), 1);
        assert!(ui.app.virtual_trenches[0].objects.is_empty());
        assert!(ui.editor().prepared_entry.is_some());
        assert!(!ui.editor().entry_changed());
        assert_eq!(ui.editor().object.attributes, ["PE100", "110", "3"]);
        assert_eq!(ui.editor().object.chainage, 0.);

        // Repeated clicks without choosing a position cannot create a cable at zero.
        ui.click("Toevoegen en herhalen");
        assert_eq!(ui.editor().draft.objects.len(), 1);
        assert!(!ui.editor().entry_changed());
        if place_second {
            ui.place(0.75, 0.5);
            assert!(ui.editor().entry_changed());
            ui.click("Toevoegen en herhalen");
            assert_eq!(ui.editor().draft.objects.len(), 2);
            assert!(!ui.editor().entry_changed());
        }
        assert!(ui.app.virtual_trenches[0].objects.is_empty());
        ui.click("Opslaan in project");
        assert!(ui.app.trench_editor.is_none());
        let objects = &ui.app.virtual_trenches[0].objects;
        assert_eq!(objects.len(), if place_second { 2 } else { 1 });
        for (index, object) in objects.iter().enumerate() {
            let (along, depth) = if index == 0 { (2.5, 0.6) } else { (7.5, 0.75) };
            assert!((object.position.x - 155000. - along).abs() < 1e-5);
            assert!((object.position.z.unwrap() - (2. - depth)).abs() < 1e-5);
            assert_eq!(object.source_name, "water");
            assert_eq!(object.attributes, ["PE100", "110", "3"]);
            assert_eq!(object.metadata["quantity"], 3);
            assert_eq!(object.metadata["crossing_angle_degrees"], 25.);
        }
    }
}

#[test]
fn repeating_an_unchanged_selected_measurement_preserves_exact_xyz_and_source_metadata() {
    let mut source = trench();
    source.boundary_3d = vec![source.start, source.end];
    source.objects.push(TrenchPoint {
        role: "leiding".into(), object_name: "Waterleiding".into(), source_name: "water".into(),
        position: Vertex { x: 155006.23456789, y: 463000.12345678, z: Some(-2.123456789) },
        attributes: ["PVC".into(), "110".into(), "2".into()],
        metadata: serde_json::json!({
            "point_id": "MX-042", "surface_mode": "manual", "surface_status": "measured",
            "crossing_angle_degrees": 17., "display_rgb": [17, 24, 39],
            "source_record": {"id": 42, "quality": 0.012, "description": "Ø gemeten"}
        }),
    });
    let original = snapshot(&source);
    let mut ui = WorkspaceUi::new(app_with(source), egui::vec2(1200., 760.));
    assert!(ui.editor_mut().select_object(0));
    assert!(!ui.editor().entry_changed());
    ui.click("Bewaren en herhalen");
    assert_eq!(snapshot(&ui.editor().draft), original);
    assert!(ui.editor().selected.is_none());
    assert!(ui.editor().prepared_entry.is_some());
    assert!(!ui.editor().object.has_elevation);
    assert_eq!(ui.editor().object.offset, 0.);
    assert_eq!(ui.editor().object.chainage, 0.);
    assert_eq!(ui.editor().object.attributes, ["PVC", "110", "2"]);
    ui.click("Opslaan in project");
    assert!(ui.app.trench_editor.is_none());
    assert_eq!(snapshot(&ui.app.virtual_trenches[0]), original);
}

#[test]
fn reset_discards_only_active_input_and_restores_the_current_draft_after_ahn() {
    let mut source = trench();
    source.start_metadata = serde_json::json!({"surface_mode": "auto"});
    source.end_metadata = serde_json::json!({"surface_mode": "auto"});
    let original = snapshot(&source);
    let mut ui = WorkspaceUi::new(app_with(source), egui::vec2(1200., 760.));
    {
        let editor = ui.editor_mut();
        editor.object = ObjectForm { chainage: 3., use_depth: true, depth: 0.7,
            name: "Eerste invoer".into(), ..Default::default() };
        editor.add_or_replace().unwrap();
        assert!(editor.select_object(0));
        editor.object.name = "Eerder gewijzigd".into();
        editor.object.depth = 0.85;
        editor.add_or_replace().unwrap();
        editor.object.name = "Nog niet toegevoegde kabel".into();
        editor.object.chainage = 8.;
        editor.entry_started = true;
        assert!(editor.receive_map_ahn([editor.start, editor.end],
            [editor.draft.start_metadata.clone(), editor.draft.end_metadata.clone()],
            [Some(Some(5.)), Some(Some(5.))]).unwrap());
    }
    let draft_after_ahn = snapshot(&ui.editor().draft);
    let last_entry = ui.editor().last_entry.clone();
    ui.click("Invoer wissen");
    assert!(ui.editor().object == ObjectForm::default());
    assert!(ui.editor().prepared_entry.is_none());
    assert!(!ui.editor().entry_started);
    assert!(ui.editor().last_entry == last_entry);
    assert_eq!(snapshot(&ui.editor().draft), draft_after_ahn);
    assert_eq!(snapshot(&ui.app.virtual_trenches[0]), original);

    // An invalid form must be discardable without first committing or validating it.
    ui.editor_mut().object.offset = f64::NAN;
    ui.editor_mut().entry_started = true;
    ui.click("Kabel/leiding toevoegen");
    assert!(!ui.editor().error.is_empty());
    assert_eq!(snapshot(&ui.editor().draft), draft_after_ahn);
    ui.click("Invoer wissen");
    assert!(ui.editor().error.is_empty());
    assert!(ui.editor().object == ObjectForm::default());
    assert!(ui.editor().last_entry == last_entry);
    assert_eq!(snapshot(&ui.editor().draft), draft_after_ahn);

    {
        let editor = ui.editor_mut();
        assert!(editor.select_object(0));
        editor.object.name = "Deze wijziging vervalt".into();
        editor.object.chainage = 9.;
        editor.object.depth = 1.2;
        assert!(editor.receive_map_ahn([editor.start, editor.end],
            [editor.draft.start_metadata.clone(), editor.draft.end_metadata.clone()],
            [Some(Some(7.)), None]).unwrap());
    }
    let current_draft = snapshot(&ui.editor().draft);
    ui.click("Wijzigingen herstellen");
    let editor = ui.editor();
    assert_eq!(editor.selected, Some(0));
    assert_eq!(editor.object.name, "Eerder gewijzigd");
    assert_eq!(editor.object.chainage, 3.);
    assert_eq!(editor.object.depth, 0.85);
    assert!((editor.object.elevation - 6.15).abs() < 1e-10);
    assert!(editor.object.use_depth);
    assert!(!editor.entry_changed());
    assert!(editor.last_entry == last_entry);
    assert_eq!(snapshot(&editor.draft), current_draft);
    assert_eq!(snapshot(&ui.app.virtual_trenches[0]), original);
    ui.click("Opslaan in project");
    assert_eq!(snapshot(&ui.app.virtual_trenches[0]), current_draft);
}

#[test]
fn native_and_legacy_project_dropdowns_use_project_types_and_materials() {
    for legacy in [false, true] {
        let mut app = app_with(trench());
        app.migrated_settings.object_rules = vec![sleufbase_rust::Rule::new(
            vec!["ls".into()], "ALGEMEEN".into(), 1, "Algemeen stroom".into(),
        )];
        app.migrated_settings.kickthemap_material_choices = vec!["Algemeen PVC".into()];
        let global = app.migrated_settings.clone();
        if legacy {
            app.legacy_archive = Some(sleufbase_rust::legacy_project::LegacyArchive {
                source_path: "project.klicproject".into(), notices: vec![],
                original: std::sync::Arc::new(serde_json::json!({
                    "version": 1,
                    "template_export_settings": {
                        "kickthemap_object_layer_rules": [
                            ["ls", "PROJECT_STROOM", 3, "Project stroom"],
                            ["project_water", "PROJECT_WATER", 5, "Projectleiding"]
                        ],
                        "kickthemap_material_choices": ["Project PE100", "Project RVS"]
                    }
                })),
            });
        } else {
            let mut project = global.clone();
            project.object_rules = vec![
                sleufbase_rust::Rule::new(vec!["ls".into()], "PROJECT_STROOM".into(), 3, "Project stroom".into()),
                sleufbase_rust::Rule::new(vec!["project_water".into()], "PROJECT_WATER".into(), 5, "Projectleiding".into()),
            ];
            project.kickthemap_material_choices = vec!["Project PE100".into(), "Project RVS".into()];
            app.project_settings = Some(project);
        }
        let mut ui = WorkspaceUi::new(app, egui::vec2(1200., 760.));
        // The selected LS label must already come from the effective project rules.
        ui.click("Project stroom");
        ui.click("Projectleiding");
        assert_eq!(ui.editor().object.source, "project_water", "legacy={legacy}");
        ui.click("Onbekend");
        ui.click("Project PE100");
        assert_eq!(ui.editor().object.attributes[0], "Project PE100", "legacy={legacy}");
        ui.click("Opslaan in project");
        assert!(ui.app.trench_editor.is_none());
        let saved = &ui.app.virtual_trenches[0].objects;
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].object_name, "Projectleiding");
        assert_eq!(saved[0].source_name, "project_water");
        assert_eq!(saved[0].attributes[0], "Project PE100");
        assert_eq!(saved[0].role, "leiding");
        assert_eq!(ui.app.migrated_settings, global);
    }
}

#[test]
fn clicking_a_section_number_selects_its_cable_even_outside_the_circle() {
    for height in [Some(1.25), None] {
        let mut source = trench();
        // Keep source order different from chainage order to exercise real identity.
        for (index, along) in [8., 3.].into_iter().enumerate() {
            let mut point = ObjectForm { chainage: along, has_elevation: height.is_some(),
                elevation: height.unwrap_or(0.), ..Default::default() }.to_object(&source).unwrap();
            point.metadata["point_id"] = serde_json::json!(format!("source-{index}"));
            source.objects.push(point);
        }
        let target = snapshot(&source.objects[1]);
        let mut ui = WorkspaceUi::new(app_with(source), egui::vec2(1200., 760.));
        let out = ui.settle();
        let label = ui.visible_label(&out, "2").center();
        let (center, radius) = out.shapes.iter().filter_map(|shape| match &shape.shape {
            egui::Shape::Circle(circle) if circle.fill == Color32::from_rgb(37, 99, 235) =>
                Some((circle.center, circle.radius)),
            _ => None,
        }).min_by(|a, b| a.0.distance(label).total_cmp(&b.0.distance(label))).unwrap();
        assert!(center.distance(label) > radius.max(9.),
            "Deze klik moet de nummerhitbox testen, niet de bestaande cirkelhitbox");
        ui.click_at(label);
        assert_eq!(ui.editor().selected, Some(1), "hoogte={height:?}");
        assert!(!ui.editor().entry_started);
        assert!(!ui.editor().entry_changed());
        assert_eq!(ui.editor().draft.objects.len(), 2);
        assert_eq!(snapshot(&ui.editor().draft.objects[1]), target);
        ui.click("Opslaan in project");
        let saved = &ui.app.virtual_trenches[0].objects;
        assert_eq!(saved.len(), 2);
        assert_eq!(snapshot(saved.iter().find(|p| p.metadata["point_id"] == "source-1").unwrap()), target);
    }
}

#[test]
fn compact_footer_keeps_reset_and_project_actions_clickable_after_a_validation_error() {
    for size in [egui::vec2(680., 420.), egui::vec2(520., 420.)] {
        let mut ui = WorkspaceUi::new(app_with(trench()), size);
        ui.editor_mut().object.chainage = 4.;
        ui.editor_mut().add_or_replace().unwrap();
        let saved_draft = snapshot(&ui.editor().draft);
        ui.editor_mut().object.offset = f64::NAN;
        ui.editor_mut().entry_started = true;
        ui.click("Kabel/leiding toevoegen");
        assert!(!ui.editor().error.is_empty());
        ui.editor_mut().error.push_str(&" Controleer de invoer; het bestaande concept is bewaard.".repeat(12));
        ui.editor_mut().notice = "Een eerdere melding over de ontvangen AHN-hoogtes blijft beschikbaar.".into();
        let out = ui.settle();
        for label in ["Kabel/leiding toevoegen", "Toevoegen en herhalen", "Invoer wissen",
            "Opslaan in project", "Sluiten zonder opslaan"] {
            ui.visible_label(&out, label);
        }
        ui.click("Invoer wissen");
        assert!(ui.editor().error.is_empty());
        assert!(!ui.editor().entry_changed());
        assert_eq!(snapshot(&ui.editor().draft), saved_draft);
        assert!(ui.app.virtual_trenches[0].objects.is_empty());
        ui.click("Opslaan in project");
        assert!(ui.app.trench_editor.is_none());
        assert_eq!(snapshot(&ui.app.virtual_trenches[0]), saved_draft);
    }
}
