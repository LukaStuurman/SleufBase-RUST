//! A directly reachable account login window, independent of the Jobs workspace.
use super::*;
impl SleufBaseApp {
    pub(super) fn open_kickthemap_login(&mut self) {
        self.shell.login_open = true;
        self.shell.login_focus = true;
        self.shell.login_submitted = false;
    }
    pub(super) fn show_kickthemap_login(&mut self, ctx: &egui::Context) {
        if !self.shell.login_open {
            return;
        }
        let busy = self.live_operation_busy();
        let mut open = true;
        let mut submit = false;
        let mut cancel = false;
        let mut jobs = false;
        let mut launch = false;
        let mut capture = false;
        egui::Window::new("KickTheMap inloggen").id(egui::Id::new("kickthemap-account-login"))
            .open(&mut open).collapsible(false).resizable(false)
            .default_width(480.).constrain_to(ctx.screen_rect().shrink(12.))
            .show(ctx,|ui| {
                settings_ui::control_style(ui);
                ui.heading("Aanmelden bij KickTheMap");
                ui.label("Log in om je live jobs en bestanden te laden.");
                ui.separator();
                ui.add_enabled_ui(!busy,|ui| {
                    ui.label("E-mail");
                    let email=ui.add(egui::TextEdit::singleline(&mut self.live_email)
                        .id_salt("kickthemap-login-email").hint_text("e-mailadres").desired_width(f32::INFINITY));
                    if self.shell.login_focus {email.request_focus();self.shell.login_focus=false;}
                    ui.label("Wachtwoord");
                    let password=ui.add(egui::TextEdit::singleline(&mut self.live_password)
                        .id_salt("kickthemap-login-password").password(true).desired_width(f32::INFINITY));
                    if password.lost_focus() && ui.input(|i|i.key_pressed(egui::Key::Enter)) {submit=true;}
                });
                ui.small("Je wachtwoord wordt niet in projectbestanden of instellingen bewaard.");
                if self.shell.login_submitted {ui.label(&self.message);}
                if busy {ui.horizontal(|ui|{ui.spinner();ui.label("Aanmelding verwerken…");});}
                ui.separator();
                ui.horizontal(|ui| {
                    cancel=ui.button(if busy {"Sluiten"}else{"Annuleren"}).clicked();
                    submit|=ui.add_enabled(!busy,egui::Button::new(egui::RichText::new("Inloggen").color(Color32::WHITE)).fill(ORANGE)).clicked();
                    jobs=ui.add_enabled(!busy&&self.live_session.is_some(),egui::Button::new("Open jobs")).clicked();
                });
                ui.add_space(8.);
                ui.collapsing("Aanmelden via je browser",|ui| {
                    ui.label("Open een eigen KickTheMap-venster in Edge, log daar zelf in en neem daarna de sessie over.");
                    launch=ui.add_enabled(self.browser_ui.browser.is_none()&&!busy,
                        egui::Button::new("Open KickTheMap in Edge")).clicked();
                    capture=ui.add_enabled(self.browser_ui.browser.is_some()&&!busy,
                        egui::Button::new("Sessie overnemen")).clicked();
                });
            });
        if !open || cancel {
            self.shell.login_open = false;
            self.live_password.clear();
            return;
        }
        if submit && !busy {
            self.shell.login_submitted = true;
            self.login_kickthemap();
        }
        if launch {
            self.launch_kickthemap_browser();
        }
        if capture {
            self.shell.login_submitted = true;
            self.capture_kickthemap_browser();
        }
        if jobs {
            self.shell.login_open = false;
            self.live_password.clear();
            self.open_kickthemap_workspace();
        }
    }
}
