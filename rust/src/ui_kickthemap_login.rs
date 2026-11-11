//! Native KickTheMap sign-in. The website view is a user-invoked fallback only
//! for mandatory provider challenges (for example reCAPTCHA).
use super::*;
#[cfg(not(test))]
use sleufbase_rust::kickthemap_session as stored;

impl SleufBaseApp {
    pub(super) fn open_kickthemap_login(&mut self) {
        #[cfg(not(test))]
        let already_open = self.shell.login_open;
        self.shell.login_open = true;
        self.shell.login_focus = true;
        #[cfg(not(test))]
        {
            if !already_open {
                self.shell.login_remember = stored::account_exists() || stored::exists();
            }
            if self.shell.login_email.is_empty() {
                if let Ok(Some(account)) = stored::load_account() {
                    // Show only the email, never the decrypted password.
                    self.shell.login_email = account.email;
                }
            }
        }
    }

    pub(super) fn show_kickthemap_login(&mut self, ctx: &egui::Context) {
        if !self.shell.login_open {
            return;
        }
        let busy = self.live_operation_busy();
        let connecting = self.browser_ui.capturing.is_some();
        let connected = self.live_session.is_some();
        #[cfg(not(test))]
        let saved = stored::account_exists();
        #[cfg(test)]
        let saved = false;
        let mut open = true;
        let mut login = false;
        let mut browser = false;
        let mut close = false;
        let mut jobs = false;
        let mut logout = false;
        egui::Window::new("KickTheMap inloggen")
            .id(egui::Id::new("kickthemap-account-login"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(430.)
            .constrain_to(ctx.screen_rect().shrink(12.))
            .show(ctx, |ui| {
                settings_ui::control_style(ui);
                if connected {
                    ui.label("Je bent aangemeld bij KickTheMap.");
                } else {
                    ui.label("Log rechtstreeks in bij KickTheMap vanuit SleufBase.");
                    ui.add_space(8.);
                    ui.label("E-mailadres");
                    let email = ui.add_enabled(
                        !busy,
                        egui::TextEdit::singleline(&mut self.shell.login_email)
                            .hint_text("naam@voorbeeld.nl")
                            .desired_width(f32::INFINITY),
                    );
                    if self.shell.login_focus {
                        email.request_focus();
                        self.shell.login_focus = false;
                    }
                    ui.add_space(4.);
                    ui.label("Wachtwoord");
                    ui.add_enabled(
                        !busy,
                        egui::TextEdit::singleline(&mut self.shell.login_password)
                            .password(true)
                            .hint_text(if saved { "Leeg laten om het onthouden wachtwoord te gebruiken" } else { "Wachtwoord" })
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(6.);
                    ui.add_enabled(
                        !busy,
                        egui::Checkbox::new(
                            &mut self.shell.login_remember,
                            "Inloggegevens onthouden en automatisch aanmelden",
                        ),
                    );
                    ui.small("Inloggegevens worden alleen op dit apparaat versleuteld met Windows DPAPI bewaard. Uitloggen wist ze.");
                    if saved && self.shell.login_password.is_empty() {
                        ui.small("Een wachtwoord is veilig opgeslagen. Vul een nieuw wachtwoord in om het te vervangen.");
                    }
                }
                if busy {
                    ui.horizontal_wrapped(|ui| {
                        ui.spinner();
                        ui.label(if connecting { "Beveiligde website-aanmelding…" } else { "Aanmelden en jobs ophalen…" });
                    });
                }
                if !self.browser_ui.status.is_empty() {
                    ui.label(&self.browser_ui.status);
                }
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    if connected {
                        jobs = ui.add_enabled(!busy, egui::Button::new("Open jobs")).clicked();
                        logout = ui.add_enabled(!busy, egui::Button::new("Uitloggen")).clicked();
                    } else {
                        let button = ui.add_enabled(
                            !busy && !self.shell.login_email.trim().is_empty(),
                            egui::Button::new(egui::RichText::new("Inloggen").color(Color32::WHITE)).fill(ORANGE),
                        );
                        login = button.clicked();
                        if !busy && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            login = true;
                        }
                    }
                    close = ui.add_enabled(!busy, egui::Button::new("Sluiten")).clicked();
                });
                if !connected {
                    ui.add_space(8.);
                    ui.collapsing("Als KickTheMap een beveiligingscontrole vereist", |ui| {
                        ui.small("Bij een verplichte reCAPTCHA kan de rechtstreekse aanmelding worden geweigerd. Gebruik dan de beveiligde KickTheMap-weergave binnen SleufBase. Deze stap kan niet automatisch worden overgeslagen.");
                        browser = ui.add_enabled(!busy, egui::Button::new("Beveiligde website-aanmelding")).clicked();
                    });
                }
            });
        if (!open || close) && !busy {
            self.shell.login_open = false;
            self.shell.login_password.clear();
        } else if login {
            self.login_kickthemap();
        } else if browser {
            self.shell.login_password.clear();
            self.start_kickthemap_login(self.shell.login_remember);
        } else if logout {
            self.logout_live_jobs();
        } else if jobs {
            self.shell.login_open = false;
            self.open_kickthemap_workspace();
        }
    }
}
