//! Sign-in options for the dedicated KickTheMap web view.
use super::*;

impl SleufBaseApp {
    pub(super) fn open_kickthemap_login(&mut self) {
        self.shell.login_open = true;
        self.shell.login_focus = true;
        self.shell.login_remember = sleufbase_rust::kickthemap_session::exists();
    }

    pub(super) fn show_kickthemap_login(&mut self, ctx: &egui::Context) {
        if !self.shell.login_open {
            return;
        }
        let busy = self.live_operation_busy();
        let connecting = self.browser_ui.capturing.is_some();
        let connected = self.live_session.is_some();
        let mut open = true;
        let mut launch = false;
        let mut close = false;
        let mut jobs = false;
        let mut logout = false;
        egui::Window::new("KickTheMap inloggen")
            .id(egui::Id::new("kickthemap-account-login"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.)
            .constrain_to(ctx.screen_rect().shrink(12.))
            .show(ctx, |ui| {
                settings_ui::control_style(ui);
                if connected {
                    ui.label("Je bent aangemeld bij KickTheMap.");
                } else {
                    ui.label("Meld je aan op de KickTheMap-pagina. Je jobs openen automatisch.");
                    ui.add_enabled(
                        !busy,
                        egui::Checkbox::new(&mut self.shell.login_remember, "Aangemeld blijven op dit apparaat"),
                    );
                    ui.small("Windows beveiligt de bewaarde sessie. Na verloop van de sessie meld je opnieuw aan.");
                }
                if connecting {
                    ui.horizontal_wrapped(|ui| {
                        ui.spinner();
                        ui.label(&self.browser_ui.status);
                    });
                } else if !self.browser_ui.status.is_empty() {
                    ui.label(&self.browser_ui.status);
                }
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    if connected {
                        jobs = ui.add_enabled(!busy, egui::Button::new("Open jobs")).clicked();
                        logout = ui.add_enabled(!busy, egui::Button::new("Uitloggen")).clicked();
                    } else if !connecting {
                        let button = ui.add_enabled(
                            !busy,
                            egui::Button::new(egui::RichText::new("Inloggen").color(Color32::WHITE)).fill(ORANGE),
                        );
                        if self.shell.login_focus {
                            button.request_focus();
                            self.shell.login_focus = false;
                        }
                        launch = button.clicked();
                    }
                    close = ui.button(if connecting { "Annuleren" } else { "Sluiten" }).clicked();
                });
            });
        if !open || close {
            self.shell.login_open = false;
            if connecting {
                self.cancel_kickthemap_login();
            }
        } else if launch {
            self.login_kickthemap();
        } else if logout {
            self.logout_live_jobs();
        } else if jobs {
            self.shell.login_open = false;
            self.open_kickthemap_workspace();
        }
    }
}
