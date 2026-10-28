//! One supported login flow: the embedded WebView2 window handles reCAPTCHA.
use super::*;
impl SleufBaseApp {
    pub(super) fn open_kickthemap_login(&mut self){
        self.shell.login_open=true;self.shell.login_focus=true;self.shell.login_submitted=false;

    }
    pub(super) fn show_kickthemap_login(&mut self,ctx:&egui::Context){
        if !self.shell.login_open{return;}
        let busy=self.live_operation_busy();
        let mut open=true;let mut launch=false;let mut capture=false;let mut close=false;let mut jobs=false;let mut restart=false;
        egui::Window::new("KickTheMap inloggen").id(egui::Id::new("kickthemap-account-login"))
            .open(&mut open).collapsible(false).resizable(false).default_width(500.)
            .constrain_to(ctx.screen_rect().shrink(12.)).show(ctx,|ui|{
                settings_ui::control_style(ui);ui.heading("Aanmelden bij KickTheMap");
                ui.label("1. Open het inlogvenster in SleufBase.");
                ui.label("2. Meld je daar aan en rond reCAPTCHA zelf af.");
                ui.label("3. Klik hier op Sessie overnemen om je live jobs te laden.");
                ui.small("Er wordt geen apart Edge-browservenster gestart. Je wachtwoord vul je uitsluitend op de KickTheMap-pagina in.");
                ui.separator();
                ui.horizontal_wrapped(|ui|{
                    let button=ui.add_enabled(!busy&&self.browser_ui.browser.is_none(),egui::Button::new(egui::RichText::new("Inloggen").color(Color32::WHITE)).fill(ORANGE));
                    if self.shell.login_focus{button.request_focus();self.shell.login_focus=false;}
                    launch=button.clicked();
                    capture=ui.add_enabled(!busy&&self.browser_ui.browser.is_some(),egui::Button::new("Sessie overnemen")).clicked();
                });
                if busy{ui.horizontal(|ui|{ui.spinner();ui.label("Inlogvenster of sessie verwerken…");});}
                if !self.browser_ui.status.is_empty(){ui.label(&self.browser_ui.status);}
                ui.separator();
                ui.horizontal_wrapped(|ui|{
                    close=ui.button("Sluiten").clicked();
                    jobs=ui.add_enabled(!busy&&self.live_session.is_some(),egui::Button::new("Open jobs")).clicked();
                    if ui.add_enabled(!busy&&self.browser_ui.browser.is_some(),egui::Button::new("Inlogvenster tonen")).clicked(){
                        if let Some(browser)=self.browser_ui.browser.as_ref(){if let Err(error)=browser.set_visible(true){self.browser_ui.status=error;}}
                    }
                    restart=ui.add_enabled(!busy&&self.browser_ui.browser.is_some(),egui::Button::new("Nieuw inlogvenster")).clicked();
                });
                ui.small("Dit venster gebruikt Microsoft WebView2 Runtime, de ingebouwde webweergave voor Windows.");
            });
        if !open||close{self.shell.login_open=false;return;}
        if launch{self.login_kickthemap();}
        if restart{self.browser_ui.browser=None;self.login_kickthemap();}
        if capture{self.capture_kickthemap_browser();}
        if jobs{self.shell.login_open=false;self.open_kickthemap_workspace();}
    }
}
