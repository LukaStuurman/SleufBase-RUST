//! KickTheMap account and website pane within the SleufBase main window.
use super::*;

impl SleufBaseApp {
    pub(super) fn open_kickthemap_login(&mut self) {
        self.shell.login_open = true;
        self.shell.login_focus = true;
        self.shell.login_remember = sleufbase_rust::kickthemap_session::exists();
    }

    pub(super) fn hide_kickthemap_pane(&mut self) {
        self.browser_ui.web_visible = false;
        if let Some(dock) = &self.browser_ui.dock {
            let _ = dock.set_bounds(None);
        }
        if let Some(browser) = &self.browser_ui.browser {
            let _ = browser.set_visible(false);
        }
    }

    pub(super) fn show_kickthemap_login(&mut self, ctx: &egui::Context) {
        let busy = self.live_operation_busy();
        let connecting = self.browser_ui.capturing.is_some();
        let connected = self.live_session.is_some();
        let web = self.browser_ui.web_visible;
        let mut launch = false;
        let mut close = false;
        let mut jobs = false;
        let mut logout = false;
        egui::CentralPanel::default().show(ctx, |ui| {
            settings_ui::control_style(ui);
            ui.heading("KickTheMap inloggen");
            if connecting {
                ui.horizontal_wrapped(|ui| {
                    ui.spinner();
                    ui.label(&self.browser_ui.status);
                });
            } else if connected {
                ui.label("Je bent aangemeld bij KickTheMap.");
                if web && self.shell.login_open {
                    ui.label("Bevestig desgewenst het opslaan van het wachtwoord op deze pagina en ga daarna naar Jobs.");
                }
            } else if !web {
                ui.label("Meld je aan op de KickTheMap-pagina. Daarna zijn je jobs beschikbaar.");
                ui.add_enabled(
                    !busy,
                    egui::Checkbox::new(&mut self.shell.login_remember, "Aangemeld blijven op dit apparaat"),
                );
                ui.small("WebView2 kan je e-mailadres en wachtwoord bewaren als je dat bevestigt. Uitloggen wist de lokale sessie en het bewaarde account.");
            }
            if !connecting && !self.browser_ui.status.is_empty() {
                ui.label(&self.browser_ui.status);
            }
            ui.horizontal_wrapped(|ui| {
                if connected {
                    jobs = ui.add_enabled(!busy, egui::Button::new("Open jobs")).clicked();
                    logout = ui.add_enabled(!busy, egui::Button::new("Uitloggen")).clicked();
                } else if !connecting && !web {
                    let button = ui.add_enabled(!busy, egui::Button::new("Inloggen"));
                    if self.shell.login_focus {
                        button.request_focus();
                        self.shell.login_focus = false;
                    }
                    launch = button.clicked();
                }
                close = ui.button(if connecting { "Annuleren" } else { "Sluiten" }).clicked();
            });
            if web {
                ui.separator();
                let rect = ui.available_rect_before_wrap();
                let scale = ctx.pixels_per_point();
                if let Some(dock) = &self.browser_ui.dock {
                    let popup_open=ctx.memory(|memory|memory.any_popup_open());
                    let bounds = (!popup_open && rect.width() >= 1. && rect.height() >= 1.).then(|| [
                        (rect.left() * scale).round() as i32,
                        (rect.top() * scale).round() as i32,
                        (rect.width() * scale).round() as i32,
                        (rect.height() * scale).round() as i32,
                    ]);
                    let _ = dock.set_bounds(bounds);
                }
            }
        });
        if close || jobs {
            self.shell.login_open = false;
            if connecting {
                self.cancel_kickthemap_login();
            } else {
                self.hide_kickthemap_pane();
            }
            if jobs {
                self.open_kickthemap_workspace();
            }
        } else if launch {
            self.login_kickthemap();
        } else if logout {
            self.logout_live_jobs();
        }
    }
}
