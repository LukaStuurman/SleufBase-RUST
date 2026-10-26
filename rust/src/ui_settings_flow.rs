//! Connect the staged settings window to the active view and async backup list.
use super::*;

impl SleufBaseApp {
    pub(super) fn open_settings(&mut self) {
        if self.settings_dialog.is_some() {
            return;
        }
        let mut dialog = settings_ui::Dialog::new(&self.migrated_settings, &self.backup_settings);
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
}
