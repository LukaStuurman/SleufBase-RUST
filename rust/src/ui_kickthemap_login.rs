//! Native KickTheMap sign-in with opt-in automatic session restoration.
//! Interactive provider checks stay inside SleufBase and require user action.
use super::*;
#[cfg(not(test))]
use sleufbase_rust::kickthemap_session as stored;

impl SleufBaseApp {
    pub(super) fn open_kickthemap_login(&mut self) {
        let already_open = self.shell.login_open;
        if !already_open { self.browser_ui.show_password=false; }
        self.shell.login_open = true;
        self.shell.login_focus = true;
        #[cfg(not(test))]
        {
            if !already_open {
                self.shell.login_remember |= stored::account_exists() || stored::exists();
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
        if !self.shell.login_open { return; }
        let busy = self.live_operation_busy();
        let connecting = self.browser_ui.capturing.is_some();
        let connected = self.live_session.is_some();
        #[cfg(not(test))]
        let saved = stored::account_exists();
        #[cfg(test)]
        let saved = false;
        let needs_verification=self.browser_ui.status==sleufbase_rust::kickthemap_live::INTERACTIVE_LOGIN_REQUIRED;
        let submit_pressed=ctx.input(|i|i.key_pressed(egui::Key::Enter));
        let mut form_has_focus=false;
        let mut open=true;
        let (mut login,mut browser,mut close,mut jobs,mut logout)=(false,false,false,false,false);
        egui::Window::new("KickTheMap inloggen")
            .id(egui::Id::new("kickthemap-account-login"))
            .open(&mut open).collapsible(false).resizable(false)
            .default_width(470.)
            .constrain_to(ctx.screen_rect().shrink(12.))
            .show(ctx, |ui| {
                settings_ui::control_style(ui);
                egui::ScrollArea::vertical().id_salt("kickthemap-login-body")
                    .max_height((ctx.screen_rect().height()-180.).clamp(160.,520.))
                    .show(ui, |ui| {
                        theme::card(12,14,12).show(ui, |ui| {
                            if connected {
                                ui.heading("Je bent verbonden");
                                ui.label("Open je jobs om proefsleuven en bestanden op te halen.");
                                if !self.shell.login_email.is_empty() {
                                    ui.small(&self.shell.login_email);
                                }
                            } else {
                                ui.heading("Je KickTheMap-account");
                                ui.label("Meld je aan om je jobs in SleufBase te gebruiken.");
                                ui.add_space(10.);
                                ui.label("E-mailadres");
                                let email=ui.add_enabled(!busy,
                                    egui::TextEdit::singleline(&mut self.shell.login_email)
                                        .hint_text("naam@voorbeeld.nl").desired_width(f32::INFINITY));
                                if self.shell.login_focus {
                                    email.request_focus();self.shell.login_focus=false;
                                }
                                ui.add_space(4.);
                                ui.label("Wachtwoord");
                                let password=ui.add_enabled(!busy,
                                    egui::TextEdit::singleline(&mut self.shell.login_password)
                                        .password(!self.browser_ui.show_password)
                                        .hint_text(if saved { "Leeg laten om het bewaarde wachtwoord te gebruiken" } else { "Wachtwoord" })
                                        .desired_width(f32::INFINITY));
                                form_has_focus=email.has_focus()||email.lost_focus()||password.has_focus()||password.lost_focus();
                                ui.add_enabled(!busy,egui::Checkbox::new(&mut self.browser_ui.show_password,"Toon wachtwoord"));
                                ui.add_space(8.);
                                ui.add_enabled(!busy,egui::Checkbox::new(&mut self.shell.login_remember,
                                    "Inloggegevens onthouden en automatisch aanmelden"));
                                ui.small("Je aanmelding blijft versleuteld op dit apparaat. Uitloggen zet automatisch aanmelden uit en verwijdert de bewaarde aanmelding.");
                                if saved && self.shell.login_password.is_empty() {
                                    ui.small("Vul alleen een nieuw wachtwoord in als je het bewaarde wachtwoord wilt vervangen.");
                                }
                            }
                        });
                        if busy {
                            ui.add_space(8.);
                            ui.horizontal_wrapped(|ui| {
                                ui.spinner();
                                ui.label(if self.browser_ui.restoring_login { "Je bewaarde aanmelding herstellen…" }
                                    else if connecting { "Rond je aanmelding af in SleufBase…" }
                                    else { "Aanmelden en jobs ophalen…" });
                            });
                        }
                        if !self.browser_ui.status.is_empty() {
                            ui.add_space(6.);
                            ui.label(&self.browser_ui.status);
                        }
                        if needs_verification && !busy {
                            ui.label("Klik op Inloggen om de extra controle binnen SleufBase af te ronden. Daarna opent je joblijst vanzelf.");
                        }
                        if !connected && !needs_verification {
                            ui.add_space(8.);
                            ui.collapsing("Andere manier van aanmelden", |ui| {
                                ui.small("Gebruik de ingebouwde KickTheMap-pagina als de gewone aanmelding niet lukt. Je blijft in SleufBase.");
                                browser=ui.add_enabled(!busy,egui::Button::new("Aanmelden via KickTheMap-pagina")).clicked();
                            });
                        }
                    });
                ui.add_space(8.);
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    if connected {
                        jobs=ui.add_enabled(!busy,egui::Button::new("Open jobs").fill(ORANGE)).clicked();
                        logout=ui.add_enabled(!busy,egui::Button::new("Uitloggen")).clicked();
                    } else {
                        let can_login=needs_verification||!self.shell.login_email.trim().is_empty();
                        let clicked=ui.add_enabled(!busy&&can_login,
                            egui::Button::new(egui::RichText::new("Inloggen").color(Color32::WHITE)).fill(ORANGE)).clicked();
                        if clicked||(!busy&&can_login&&form_has_focus&&submit_pressed) {
                            if needs_verification { browser=true; } else { login=true; }
                        }
                    }
                    close=ui.button(if busy { "Annuleren" } else { "Sluiten" }).clicked();
                });
            });
        if !open || close {
            self.cancel_native_kickthemap_login();
            if connecting { self.cancel_kickthemap_login(); }
            self.shell.login_open=false;
            self.shell.login_password.clear();
            self.browser_ui.show_password=false;
        } else if login {
            self.login_kickthemap();
        } else if browser {
            self.shell.login_password.clear();
            self.browser_ui.show_password=false;
            self.start_kickthemap_login(self.shell.login_remember);
        } else if logout {
            self.logout_live_jobs();
        } else if jobs {
            self.shell.login_open=false;
            self.open_kickthemap_workspace();
        }
    }

    fn cancel_native_kickthemap_login(&mut self) {
        self.browser_ui.startup_login_checked=true;
        self.shell.login_password.clear();
        self.browser_ui.show_password=false;
        if self.shell.login_is_native && self.live_rx.is_some() {
            // Drop the only receiver before clearing pending credentials. A late
            // result can no longer install a session or persist an account.
            self.live_rx=None;
            self.live_busy=false;
            self.shell.login_pending_account=None;
            self.shell.login_is_native=false;
            self.browser_ui.restoring_login=false;
            self.browser_ui.status="Aanmelden geannuleerd.".into();
            self.message=self.browser_ui.status.clone();
        }
    }

}

impl SleufBaseApp {
    /// Embedded website is an explicit fallback for mandatory KickTheMap checks.
    /// It uses the parent SleufBase window and never launches an external browser.
    pub(super) fn hide_kickthemap_pane(&mut self) {
        self.browser_ui.web_visible = false;
        if let Some(dock) = &self.browser_ui.dock {
            let _ = dock.set_bounds(None);
        }
        if let Some(browser) = &self.browser_ui.browser {
            let _ = browser.set_visible(false);
        }
    }

    pub(super) fn show_kickthemap_website(&mut self, ctx: &egui::Context) {
        if !self.browser_ui.web_visible {
            return;
        }
        let busy = self.live_operation_busy();
        let connected = self.live_session.is_some();
        let mut close = false;
        let mut jobs = false;
        let mut logout = false;
        egui::CentralPanel::default().show(ctx, |ui| {
            settings_ui::control_style(ui);
            ui.heading("KickTheMap beveiligde aanmelding");
            if busy {
                ui.horizontal_wrapped(|ui| {
                    ui.spinner();
                    ui.label("Meld je aan op de KickTheMap-pagina. Rond eventuele verificatie zelf af.");
                });
            } else if connected {
                ui.label("Aangemeld bij KickTheMap.");
            }
            if !self.browser_ui.status.is_empty() {
                ui.label(&self.browser_ui.status);
            }
            ui.horizontal_wrapped(|ui| {
                if connected {
                    jobs = ui.add_enabled(!busy, egui::Button::new("Open jobs")).clicked();
                    logout = ui.add_enabled(!busy, egui::Button::new("Uitloggen")).clicked();
                }
                close = ui.button(if busy { "Annuleren" } else { "Sluiten" }).clicked();
            });
            ui.separator();
            let rect = ui.available_rect_before_wrap();
            let scale = ctx.pixels_per_point();
            if let Some(dock) = &self.browser_ui.dock {
                let popup_open = ctx.memory(|memory| memory.any_popup_open());
                let bounds = (!popup_open && rect.width() >= 1. && rect.height() >= 1.)
                    .then(|| [
                        (rect.left() * scale).round() as i32,
                        (rect.top() * scale).round() as i32,
                        (rect.width() * scale).round() as i32,
                        (rect.height() * scale).round() as i32,
                    ]);
                let _ = dock.set_bounds(bounds);
            }
        });
        if close {
            if busy { self.cancel_kickthemap_login(); }
            else { self.hide_kickthemap_pane(); }
            self.shell.login_open = false;
        } else if jobs {
            self.hide_kickthemap_pane();
            self.shell.login_open = false;
            self.open_kickthemap_workspace();
        } else if logout {
            self.logout_live_jobs();
        }
    }
}

#[cfg(test)]
mod login_lifecycle_tests {
    use super::*;

    fn session()->KickTheMapLiveClient {
        let cookie=sleufbase_rust::kickthemap_browser::BrowserCookie{
            name:"fixture".into(),value:"local-only".into(),domain:"my.kickthemap.com".into(),
            path:"/".into(),secure:true,
        };
        KickTheMapLiveClient::from_authenticated_browser(&[cookie],"fixture-csrf").unwrap()
    }

    #[test]
    fn successful_automatic_native_login_keeps_workspace_and_login_closed() {
        let mut app=SleufBaseApp::default();
        let (tx,rx)=mpsc::channel();
        app.live_busy=true;app.live_rx=Some(rx);app.shell.login_is_native=true;
        app.browser_ui.restoring_login=true;
        tx.send(Ok((session(),Vec::new()))).unwrap();
        app.poll_live_jobs();
        assert!(app.live_session.is_some());
        assert!(!app.live_operation_busy());
        assert!(!app.shell.login_open && !app.browser_ui.web_visible && !app.browser_ui.open);
        assert!(!app.browser_ui.restoring_login);
    }

    #[test]
    fn manual_login_opens_jobs_and_replaces_the_previous_account_selection() {
        let mut app=SleufBaseApp::default();
        app.live_jobs_owner=Some(session().snapshot_owner().clone());
        app.browser_ui.selected.insert(41);
        let (tx,rx)=mpsc::channel();
        app.live_busy=true;app.live_rx=Some(rx);app.shell.login_is_native=true;
        tx.send(Ok((session(),Vec::new()))).unwrap();
        app.poll_live_jobs();
        assert!(app.live_session.is_some() && app.browser_ui.open);
        assert!(app.browser_ui.selected.is_empty());
        assert!(!app.shell.login_open);
    }

    #[test]
    fn cancelled_native_login_discards_a_late_success_and_pending_credentials() {
        let mut app=SleufBaseApp::default();
        let (tx,rx)=mpsc::channel();
        app.live_busy=true;app.live_rx=Some(rx);app.shell.login_is_native=true;
        app.browser_ui.restoring_login=true;
        app.shell.login_pending_account=Some(sleufbase_rust::kickthemap_session::Account{
            email:"fixture@example.test".into(),password:"test-only".into(),
        });
        app.shell.login_password="test-only".into();
        app.browser_ui.show_password=true;
        // Success already queued when the user cancels must still be rejected.
        tx.send(Ok((session(),Vec::new()))).unwrap();
        app.cancel_native_kickthemap_login();
        app.poll_live_jobs();
        assert!(app.live_session.is_none() && app.live_jobs_owner.is_none());
        assert!(app.shell.login_pending_account.is_none());
        assert!(!app.live_operation_busy());
        assert!(!app.browser_ui.restoring_login && !app.browser_ui.open);
        assert_eq!(app.browser_ui.status,"Aanmelden geannuleerd.");
        assert!(app.browser_ui.startup_login_checked);
        assert!(app.shell.login_password.is_empty() && !app.browser_ui.show_password);
    }

    #[test]
    fn disconnected_native_login_returns_to_a_usable_login_form() {
        let mut app=SleufBaseApp::default();
        let (tx,rx)=mpsc::channel();
        app.live_busy=true;app.live_rx=Some(rx);app.shell.login_is_native=true;
        app.browser_ui.restoring_login=true;drop(tx);
        app.poll_live_jobs();
        assert!(app.shell.login_open);
        assert!(!app.live_operation_busy());
        assert!(app.browser_ui.status.contains("Probeer opnieuw"));
        assert!(!app.browser_ui.web_visible && !app.browser_ui.restoring_login);
    }

    #[test]
    fn logout_prevents_startup_resume_and_clears_active_account_state() {
        let mut app=SleufBaseApp::default();
        let live=session();app.live_jobs_owner=Some(live.snapshot_owner().clone());
        app.live_session=Some(live);app.browser_ui.selected.insert(7);
        app.shell.login_email="fixture@example.test".into();
        app.shell.login_password="test-only".into();
        app.browser_ui.show_password=true;
        app.logout_live_jobs();
        assert!(app.live_session.is_none() && app.live_jobs_owner.is_none());
        assert!(app.live_jobs.is_empty() && app.browser_ui.selected.is_empty());
        assert!(app.shell.login_email.is_empty() && app.shell.login_password.is_empty());
        assert!(app.browser_ui.startup_login_checked);
        assert!(!app.browser_ui.show_password);
        assert!(app.browser_ui.status.contains("Automatisch aanmelden staat uit"));
    }
}
