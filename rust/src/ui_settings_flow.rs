//! Connect the staged settings window to the active view and async backup list.
use super::*;

impl SleufBaseApp {
    fn has_project_settings_scope(&self) -> bool {
        self.project_settings.is_some() || self.legacy_archive.as_ref()
            .is_some_and(|archive| archive.original.get("template_export_settings").is_some())
    }

    pub(super) fn open_settings(&mut self) {
        if self.settings_dialog.is_some() {
            return;
        }
        let effective = match self.project_export_settings() {
            Ok(settings) => settings,
            Err(error) => { self.message = error; return; }
        };
        let mut dialog = settings_ui::Dialog::new(&effective, &self.backup_settings);
        dialog.project_scoped = self.has_project_settings_scope();
        dialog.viewer = Some(settings_ui::ViewerDraft {
            background_source: self.project_view.background_source.clone(),
            use_background: self.project_view.use_background,
        });
        self.settings_dialog = Some(dialog);
        self.shell.settings_generation = Some(self.project_generation);
        #[cfg(not(test))]
        let directories = sleufbase_rust::recovery::default_directories();
        #[cfg(test)]
        let directories = Vec::new(); // UI tests never enumerate personal backups.
        self.shell.settings_scan = Some(sleufbase_rust::recovery::ScanWorker::start(directories));
    }

    fn apply_saved_settings(&mut self, saved: settings_ui::Saved, generation: Option<u64>) {
        if generation == Some(self.project_generation) && self.has_project_settings_scope() {
            if self.project_settings.as_ref() != Some(&saved.settings) {
                self.project_settings = Some(saved.settings.clone());
                self.dirty = true;
            }
        }
        self.migrated_settings = saved.settings;
        self.backup_settings = saved.autosave;
        self.rule_editor = None;
        self.message = "Instellingen opgeslagen".into();
        if let Some(viewer) = saved.viewer {
            if generation != Some(self.project_generation) {
                self.message
                    .push_str("; kaartkeuzes horen bij het vorige project en zijn niet toegepast.");
                return;
            }
            if self.project_view.background_source != viewer.background_source
                || self.project_view.use_background != viewer.use_background
            {
                self.project_view.background_source = viewer.background_source;
                self.project_view.use_background = viewer.use_background;
                self.dirty = true;
                self.request_selected_background();
            }
        }
    }

    pub(super) fn show_settings_window(&mut self, ctx: &egui::Context) {
        if self.settings_dialog.is_none() {
            self.shell.settings_scan = None;
            self.shell.settings_generation = None;
            return;
        }
        if let Some(result) = self
            .shell
            .settings_scan
            .as_ref()
            .and_then(sleufbase_rust::recovery::ScanWorker::poll)
        {
            self.shell.settings_scan = None;
            if let Some(dialog) = self.settings_dialog.as_mut() {
                match result {
                    Ok(catalog) => dialog.autosave_count = Some(catalog.entries.len()),
                    Err(error) => dialog.error = error,
                }
            }
        }
        if self.shell.settings_scan.is_some() {
            ctx.request_repaint_after(Duration::from_millis(40));
        }
        let generation = self.shell.settings_generation;
        if let Some(saved) =
            settings_ui::show(ctx, &mut self.settings_dialog, |settings, autosave| {
                settings_store::save_pair(&project::app_data_directory(), settings, autosave)
            })
        {
            self.apply_saved_settings(saved, generation);
        }
        let restore = self
            .settings_dialog
            .as_mut()
            .is_some_and(|dialog| std::mem::take(&mut dialog.restore_autosave_requested));
        if restore {
            self.restore_backup(ctx);
        }
        if self.settings_dialog.is_none() {
            self.shell.settings_scan = None;
            self.shell.settings_generation = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_and_failed_settings_drafts_never_change_the_live_map() {
        let mut app = SleufBaseApp::default();
        let before = app.project_view.clone();
        app.open_settings();
        let draft = app.settings_dialog.as_mut().unwrap();
        draft.viewer.as_mut().unwrap().background_source = "OpenStreetMap".into();
        assert!(draft
            .try_save(&mut |_, _| Err("Disk full".into()))
            .is_none());
        assert_eq!(app.project_view, before);
        assert!(!app.dirty);
        app.settings_dialog = None;
        app.show_settings_window(&egui::Context::default());
        assert_eq!(app.project_view, before);
        assert!(app.shell.settings_scan.is_none());
    }
    #[test]
    fn successful_save_applies_the_map_choice_and_marks_its_project_dirty() {
        let mut app = SleufBaseApp::default();
        app.open_settings();
        let draft = app.settings_dialog.as_mut().unwrap();
        draft.viewer.as_mut().unwrap().background_source = "OpenStreetMap".into();
        let saved = draft.try_save(&mut |_, _| Ok(())).unwrap();
        app.apply_saved_settings(saved, app.shell.settings_generation);
        assert_eq!(app.project_view.background_source, "OpenStreetMap");
        assert!(app.osm_visible);
        assert!(app.dirty);
    }
    #[test]
    fn old_settings_view_draft_cannot_retarget_a_restored_project() {
        let mut app = SleufBaseApp::default();
        app.open_settings();
        let generation = app.shell.settings_generation;
        app.settings_dialog
            .as_mut()
            .unwrap()
            .viewer
            .as_mut()
            .unwrap()
            .background_source = "OpenStreetMap".into();
        let saved = app
            .settings_dialog
            .as_mut()
            .unwrap()
            .try_save(&mut |_, _| Ok(()))
            .unwrap();
        app.project_generation += 1;
        let before = app.project_view.clone();
        app.apply_saved_settings(saved, generation);
        assert_eq!(app.project_view, before);
        assert!(!app.dirty);
        assert!(app.message.contains("vorige project"));
    }
    #[test]
    fn saved_defaults_survive_disk_roundtrip_and_new_projects_but_not_override_saved_views() {
        let root = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut app = SleufBaseApp::default();
        app.open_settings();
        let draft = app.settings_dialog.as_mut().unwrap();
        let viewer = draft.viewer.as_mut().unwrap();
        viewer.background_source = "OpenStreetMap".into();
        viewer.use_background = false;
        draft.draft.map_export.background = "white".into();
        draft.draft.map_export.dpi = 150;
        draft.draft.map_export.scale = 500.;
        let saved = draft.try_save(&mut |settings, autosave|
            settings_store::save_pair(root.path(), settings, autosave)).unwrap();
        app.apply_saved_settings(saved, app.shell.settings_generation);
        let (settings, _) = settings_store::read_pair(root.path()).unwrap();
        assert_eq!(settings.viewer, app.migrated_settings.viewer);
        assert_eq!(settings.map_export.background, "white");
        assert_eq!(settings.map_export.dpi, 150);
        assert_eq!(settings.map_export.scale, 500.);
        let mut restarted = SleufBaseApp {migrated_settings: settings, ..Default::default()};
        restarted.apply_project(NativeProject::default(), None, false, Vec::new(), &ctx);
        assert_eq!(restarted.project_view.background_source, "OpenStreetMap");
        assert!(!restarted.project_view.use_background);
        let saved_view = sleufbase_rust::project_view::ProjectView::default();
        restarted.apply_project(NativeProject {view:Some(saved_view.clone()), ..Default::default()},
            None, false, Vec::new(), &ctx);
        assert_eq!(restarted.project_view, saved_view);
    }

    #[test]
    fn map_settings_edit_the_active_project_scope_and_keep_other_export_preferences() {
        let mut app = SleufBaseApp::default();
        let mut project_settings = app.migrated_settings.clone();
        project_settings.map_export.dpi = 150;
        project_settings.dxf_label_gap = 12.;
        app.project_settings = Some(project_settings);
        app.open_settings();
        let draft = app.settings_dialog.as_mut().unwrap();
        assert_eq!(draft.draft.map_export.dpi, 150);
        draft.draft.map_export.dpi = 200;
        let saved = draft.try_save(&mut |_, _| Ok(())).unwrap();
        app.apply_saved_settings(saved, app.shell.settings_generation);
        assert_eq!(app.project_settings.as_ref().unwrap().map_export.dpi, 200);
        assert_eq!(app.project_settings.as_ref().unwrap().dxf_label_gap, 12.);
        assert_eq!(app.migrated_settings.map_export.dpi, 200);
        assert!(app.dirty);
        let path=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters/gray16.tiff");
        app.geotiffs.push(sleufbase_rust::geotiff::read_geotiff_metadata(&path).unwrap());
        let (sent,received)=std::sync::mpsc::channel();
        app.exporter=Some(sleufbase_rust::export_worker::Exporter::with_processor(move |_,request,_| {
            sent.send(request.maps.clone().unwrap()).unwrap();
            Ok(sleufbase_rust::export_worker::Exported {point_updates:Vec::new(),point_candidates:None,
                skipped:Vec::new(),companion:None,warnings:Vec::new()})
        }).unwrap());
        let directory=tempfile::tempdir().unwrap();
        assert!(app.start_maps_export(directory.path().to_owned()));
        assert_eq!(received.recv_timeout(Duration::from_secs(5)).unwrap().dpi,200);
        let deadline=Instant::now()+Duration::from_secs(5);
        while app.export_intent.is_some() {
            app.poll_export_jobs(&egui::Context::default());
            assert!(Instant::now()<deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn project_dxf_and_trench_settings_are_staged_applied_and_used_by_export_dialogs() {
        let mut app=SleufBaseApp::default();
        let mut scoped=app.migrated_settings.clone();
        scoped.dxf_label_gap=12.;
        scoped.export_preferences.template_include_cross_sections=false;
        scoped.export_preferences.dxf_centerline_color="#123456".into();
        app.project_settings=Some(scoped.clone());
        app.open_settings();
        let generation=app.shell.settings_generation;
        let draft=app.settings_dialog.as_mut().unwrap();
        assert!(draft.project_scoped);
        assert_eq!(draft.draft.dxf_label_gap,12.);
        assert!(!draft.draft.export_preferences.template_include_cross_sections);
        draft.draft.dxf_label_gap=7.;
        draft.draft.export_preferences.template_include_cross_sections=true;
        draft.draft.export_preferences.dxf_centerline_color="#654321".into();
        assert!(draft.try_save(&mut |_,_|Err("disk full".into())).is_none());
        assert_eq!(app.project_settings.as_ref(),Some(&scoped));
        assert!(!app.dirty);
        let saved=app.settings_dialog.as_mut().unwrap().try_save(&mut |_,_|Ok(())).unwrap();
        app.apply_saved_settings(saved,generation);
        let effective=app.project_export_settings().unwrap();
        assert_eq!(effective.dxf_label_gap,7.);
        assert!(effective.export_preferences.template_include_cross_sections);
        assert_eq!(effective.export_preferences.dxf_centerline_color,"#654321");
        assert!(app.dirty);
        let path=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters/gray16.tiff");
        app.geotiffs.push(sleufbase_rust::geotiff::read_geotiff_metadata(&path).unwrap());
        app.open_template_export();
        let state=&app.template_order_dialog.as_ref().unwrap().state;
        assert_eq!(state.label_gap,7.);
        assert!(state.profiles);
        assert_eq!(state.overview_settings.centerline_color,[0x65,0x43,0x21]);
        app.save_dxf();
        assert_eq!(app.export_dialog.as_ref().unwrap().label_gap,7.);
        let restored:NativeProject=serde_json::from_value(serde_json::to_value(app.snapshot()).unwrap()).unwrap();
        assert_eq!(restored.project_settings.unwrap(),effective);
    }
    #[test]
    fn cancelled_or_stale_settings_cannot_replace_another_projects_export_values() {
        let mut app=SleufBaseApp::default();
        app.project_settings=Some(app.migrated_settings.clone());
        app.open_settings();
        app.settings_dialog.as_mut().unwrap().draft.dxf_label_gap=13.;
        app.settings_dialog=None;
        assert_eq!(app.project_settings.as_ref().unwrap().dxf_label_gap,3.5);
        app.open_settings();
        let generation=app.shell.settings_generation;
        app.settings_dialog.as_mut().unwrap().draft.dxf_label_gap=14.;
        let saved=app.settings_dialog.as_mut().unwrap().try_save(&mut |_,_|Ok(())).unwrap();
        app.project_generation+=1;
        app.project_settings.as_mut().unwrap().dxf_label_gap=20.;
        app.apply_saved_settings(saved,generation);
        assert_eq!(app.project_settings.as_ref().unwrap().dxf_label_gap,20.);
        assert_eq!(app.migrated_settings.dxf_label_gap,14.);
        assert!(!app.dirty);
    }

    #[test]
    fn legacy_export_overrides_are_shown_and_saving_materializes_the_native_scope() {
        let mut app=SleufBaseApp::default();
        app.legacy_archive=Some(sleufbase_rust::legacy_project::LegacyArchive {
            source_path:PathBuf::from("legacy.klicproject"),
            original:std::sync::Arc::new(serde_json::json!({"version":1,
                "template_export_settings":{"dxf_label_gap":11.,"template_include_cross_sections":false}})),
            notices:Vec::new(),
        });
        let original=app.legacy_archive.as_ref().unwrap().original.clone();
        app.open_settings();
        let draft=app.settings_dialog.as_mut().unwrap();
        assert!(draft.project_scoped);
        assert_eq!(draft.draft.dxf_label_gap,11.);
        assert!(!draft.draft.export_preferences.template_include_cross_sections);
        draft.draft.dxf_label_gap=8.;
        let saved=draft.try_save(&mut |_,_|Ok(())).unwrap();
        app.apply_saved_settings(saved,app.shell.settings_generation);
        assert_eq!(app.project_settings.as_ref().unwrap().dxf_label_gap,8.);
        assert_eq!(app.project_export_settings().unwrap().dxf_label_gap,8.);
        assert_eq!(app.legacy_archive.as_ref().unwrap().original,original);
        assert!(app.dirty);
    }

}
