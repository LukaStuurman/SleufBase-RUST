//! Connect the staged settings window to the active view and async backup list.
use super::*;

impl SleufBaseApp {
    pub(super) fn open_settings(&mut self) {
        if self.settings_dialog.is_some() {
            return;
        }
        let mut dialog = settings_ui::Dialog::new(&self.migrated_settings, &self.backup_settings);
        if let Some(project_settings) = &self.project_settings {
            dialog.draft.map_export = project_settings.map_export.clone();
        }
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
        if generation == Some(self.project_generation) {
            if let Some(project_settings) = &mut self.project_settings {
                if project_settings.map_export != saved.settings.map_export {
                    project_settings.map_export = saved.settings.map_export.clone();
                    self.dirty = true;
                }
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
    }

}
