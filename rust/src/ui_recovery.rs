use super::*;
impl SleufBaseApp {
    pub(super) fn show_recovery(&mut self, ctx: &egui::Context) {
        let action = self
            .recovery_dialog
            .as_mut()
            .and_then(|dialog| dialog.show(ctx));
        match action {
            Some(sleufbase_rust::recovery_ui::Action::Close) => self.recovery_dialog = None,
            Some(sleufbase_rust::recovery_ui::Action::Restore(path)) => {
                self.recovery_dialog = None;
                self.request_project_action(ProjectAction::Restore(path), ctx);
            }
            Some(sleufbase_rust::recovery_ui::Action::OpenDirectory(path)) => {
                #[cfg(windows)]
                if let Err(e) = std::process::Command::new("explorer.exe")
                    .arg(&path)
                    .spawn()
                {
                    self.message = format!("Back-upmap openen mislukt: {e}");
                }
                #[cfg(not(windows))]
                {
                    self.message = format!("Back-upmap: {}", path.display());
                }
            }
            None => {}
        }
    }
}
