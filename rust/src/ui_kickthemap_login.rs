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
            // The initial choice is set by the shell. Reopening the form must
            // not override an explicit choice to sign in for this session only.
            if self.shell.login_email.is_empty() {
                if let Ok(Some(account)) = stored::load_account() {
                    // Show only the email, never the decrypted password.
                    self.shell.login_email = account.email;
                }
            }
        }
    }

    fn kickthemap_account_owner(&self)->Option<&sleufbase_rust::jobs_cache::SnapshotOwner> {
        if self.browser_ui.capturing.is_some()||(self.shell.login_is_native&&self.live_rx.is_some()) {
            return None;
        }
        if let Some(session)=&self.live_session {return Some(session.snapshot_owner());}
        if !self.live_busy {return None;}
        if let Some(refresh)=&self.live_refresh {
            return (self.live_jobs_owner.as_ref()==Some(&refresh.owner)).then_some(&refresh.owner);
        }
        if self.live_download_rx.is_some() {self.live_jobs_owner.as_ref()} else {None}
    }

    pub(super) fn show_kickthemap_login(&mut self, ctx: &egui::Context) {
        if !self.shell.login_open { return; }
        let busy = self.live_operation_busy();
        let connecting = self.browser_ui.capturing.is_some();
        let signing_in=connecting||(self.shell.login_is_native&&self.live_rx.is_some());
        let account_owner=self.kickthemap_account_owner().cloned();
        let connected=account_owner.is_some();
        #[cfg(not(test))]
        let saved = stored::account_exists();
        #[cfg(test)]
        let saved = false;
        let mut needs_verification=self.browser_ui.status==sleufbase_rust::kickthemap_live::INTERACTIVE_LOGIN_REQUIRED;
        let submit_pressed=ctx.input(|i|i.key_pressed(egui::Key::Enter));
        let mut form_has_focus=false;
        let mut open=true;
        let (mut login,mut browser,mut close,mut jobs,mut logout)=(false,false,false,false,false);
        egui::Window::new("KickTheMap inloggen")
            .id(egui::Id::new("kickthemap-account-login"))
            .open(&mut open).collapsible(false).resizable(false)
            .default_width(470.)
            .default_height((ctx.screen_rect().height()-180.).clamp(160.,520.)+64.)
            .constrain_to(ctx.screen_rect().shrink(12.))
            .show(ctx, |ui| {
                settings_ui::control_style(ui);
                // The window starts with egui's current Resize budget, which can
                // be smaller than the screen. Keep the actions inside that budget
                // even while a new font or a long status changes the body height.
                let footer_height=(ui.text_style_height(&egui::TextStyle::Button)
                    +2.*ui.spacing().button_padding.y+3.*ui.spacing().item_spacing.y+14.).max(64.);
                let body_height=(ctx.screen_rect().height()-180.).clamp(160.,520.)
                    .min((ui.available_height()-footer_height).max(0.));
                egui::ScrollArea::vertical().id_salt("kickthemap-login-body")
                    .max_height(body_height).min_scrolled_height(body_height.min(64.))
                    .show(ui, |ui| {
                        theme::card(12,14,12).show(ui, |ui| {
                            if connected {
                                ui.heading("Je bent verbonden");
                                ui.label("Open je jobs om proefsleuven en bestanden op te halen.");
                                if let Some(email)=account_owner.as_ref().and_then(|owner|owner.email()) {
                                    ui.small(email);
                                }else{ui.small("Aangemeld via de KickTheMap-pagina");}
                                ui.small("Uitloggen verwijdert ook de bewaarde aanmelding op dit apparaat.");
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
                                if !busy&&needs_verification&&(email.changed()||password.changed()) {
                                    needs_verification=false;
                                    self.browser_ui.status="Aanmeldgegevens aangepast. Klik op Inloggen om opnieuw te proberen.".into();
                                    self.message=self.browser_ui.status.clone();
                                }
                                form_has_focus=email.has_focus()||email.lost_focus()||password.has_focus()||password.lost_focus();
                                ui.add_enabled(!busy,egui::Checkbox::new(&mut self.browser_ui.show_password,"Toon wachtwoord"));
                                ui.add_space(8.);
                                ui.add_enabled(!busy,egui::Checkbox::new(&mut self.shell.login_remember,
                                    "Inloggegevens onthouden en automatisch aanmelden"));
                                ui.small(if self.shell.login_remember {
                                    "Na een geslaagde aanmelding wordt deze versleuteld bewaard. SleufBase probeert je bij de volgende start automatisch aan te melden."
                                }else{
                                    "Na een geslaagde aanmelding gebruik je KickTheMap alleen voor deze sessie. Eerder bewaarde inloggegevens worden dan verwijderd."
                                });
                                ui.small("Uitloggen zet automatisch aanmelden uit en verwijdert de bewaarde aanmelding.");
                                if saved && self.shell.login_password.is_empty() {
                                    ui.small("Een bewaard wachtwoord werkt alleen met hetzelfde e-mailadres. Vul bij een ander account het bijbehorende wachtwoord in.");
                                }
                            }
                        });
                        if busy {
                            ui.add_space(8.);
                            ui.horizontal_wrapped(|ui| {
                                ui.spinner();
                                ui.label(if self.live_refresh.is_some() { "Je jobs worden vernieuwd…" }
                                    else if self.live_download_rx.is_some() { "Je KickTheMap-bestanden worden opgehaald…" }
                                    else if self.browser_ui.restoring_login { "Je bewaarde aanmelding herstellen…" }
                                    else if connecting { "Rond je aanmelding af in SleufBase…" }
                                    else if signing_in { "Aanmelden en jobs ophalen…" }
                                    else { "KickTheMap is bezig…" });
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
                    close=ui.button(if signing_in { "Annuleren" } else { "Sluiten" }).clicked();
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

#[cfg(test)]
mod refresh_lifecycle_tests {
    use super::*;

    fn session()->KickTheMapLiveClient {
        let cookie=sleufbase_rust::kickthemap_browser::BrowserCookie{
            name:"fixture".into(),value:"local-only".into(),domain:"my.kickthemap.com".into(),
            path:"/".into(),secure:true,
        };
        KickTheMapLiveClient::from_authenticated_browser(&[cookie],"fixture-csrf").unwrap()
    }

    fn job(id:u64)->Job {
        Job{job_id:id,title:format!("PS{id}"),address:String::new(),municipality:String::new(),
            coordinates:String::new(),client_date:String::new(),status:9,downloadable:true,prefix:String::new()}
    }

    fn app()->SleufBaseApp {
        let session=session();
        let mut app=SleufBaseApp::default();
        app.live_jobs_owner=Some(session.snapshot_owner().clone());
        app.live_session=Some(session);app.live_jobs=vec![job(42)];
        app.browser_ui.selected.insert(42);app.save_live_cache();
        app
    }

    fn start_reply(app:&mut SleufBaseApp)->(KickTheMapLiveClient,mpsc::Sender<LiveJobsRefreshResult>) {
        let session=app.live_session.take().unwrap();let (tx,rx)=mpsc::channel();
        app.live_refresh=Some(LiveJobsRefresh{owner:session.snapshot_owner().clone(),events:rx});
        app.live_busy=true;(session,tx)
    }

    #[test]
    fn temporary_refresh_error_keeps_session_jobs_selection_and_cache() {
        let mut app=app();let owner=app.live_jobs_owner.clone();
        let allocation=app.live_jobs.as_ptr();
        let cache=app.jobs_cache.as_ref().unwrap().source_path.clone();
        let cached=std::fs::read(&cache).unwrap();
        let (session,tx)=start_reply(&mut app);
        assert!(app.live_operation_busy());
        tx.send(LiveJobsRefreshResult{session,jobs:Err("Tijdelijke verbindingstoring".into())}).ok().unwrap();
        app.poll_live_jobs();
        assert_eq!(app.live_session.as_ref().map(|session|session.snapshot_owner()),owner.as_ref());
        assert_eq!(app.live_jobs_owner,owner);assert_eq!(app.live_jobs.as_ptr(),allocation);
        assert_eq!(app.live_jobs[0].job_id,42);assert!(app.browser_ui.selected.contains(&42));
        assert_eq!(std::fs::read(&cache).unwrap(),cached);
        assert!(!app.live_operation_busy()&&!app.shell.login_open);
        assert!(app.browser_ui.status.contains("Jobs vernieuwen mislukt"));
        assert!(app.browser_ui.status.contains("Probeer Vernieuwen opnieuw"));
    }

    #[test]
    fn successful_refresh_replaces_jobs_and_cache_for_the_same_owner() {
        let mut app=app();let owner=app.live_jobs_owner.clone();let (session,tx)=start_reply(&mut app);
        tx.send(LiveJobsRefreshResult{session,jobs:Ok(vec![job(42),job(43)])}).ok().unwrap();
        app.poll_live_jobs();
        assert_eq!(app.live_session.as_ref().map(|session|session.snapshot_owner()),owner.as_ref());
        assert_eq!(app.live_jobs.iter().map(|job|job.job_id).collect::<Vec<_>>(),vec![42,43]);
        assert_eq!(app.jobs_cache.as_ref().unwrap().jobs.len(),2);
        assert!(!app.live_operation_busy()&&!app.shell.login_open);
        assert!(app.browser_ui.status.contains("2 KickTheMap jobs vernieuwd"));
    }

    #[test]
    fn expired_refresh_still_requires_login_when_browser_fallback_also_fails() {
        for error in ["KickTheMap-sessie verlopen (HTTP 401)",sleufbase_rust::kickthemap_live::INTERACTIVE_LOGIN_REQUIRED] {
            let mut app=app();let cache=app.jobs_cache.as_ref().unwrap().source_path.clone();
            let cached=std::fs::read(&cache).unwrap();let (session,tx)=start_reply(&mut app);
            let error=live_refresh_fallback_error(error.into(),"Browser kon job-API niet opvragen".into());
            tx.send(LiveJobsRefreshResult{session,jobs:Err(error.clone())}).ok().unwrap();
            app.poll_live_jobs();
            assert!(app.live_session.is_none()&&app.shell.login_open);
            assert!(!app.live_operation_busy());assert_eq!(app.browser_ui.status,error);
            assert!(app.live_rx.is_none()&&!app.shell.login_is_native);
            assert_eq!(app.browser_ui.launch_requests,0);
            assert_eq!(app.live_jobs[0].job_id,42);assert!(app.browser_ui.selected.contains(&42));
            assert_eq!(std::fs::read(&cache).unwrap(),cached);
        }
    }

    #[test]
    fn disconnected_refresh_returns_to_login_without_clearing_previous_jobs() {
        let mut app=app();let (session,tx)=start_reply(&mut app);drop(session);drop(tx);
        app.poll_live_jobs();
        assert!(app.live_session.is_none()&&app.shell.login_open);
        assert!(!app.live_operation_busy());assert_eq!(app.live_jobs[0].job_id,42);
        assert!(app.browser_ui.status.contains("vernieuwen is onderbroken"));
    }

    #[test]
    fn late_refresh_cannot_restore_a_signed_out_or_replaced_account() {
        for state in ["signed-out","replaced","newer-download","same-owner-download","same-owner-browser-login"] {
        for disconnected in [false,true] {
            let mut previous=app();let (session,tx)=start_reply(&mut previous);
            let refresh=previous.live_refresh.take().unwrap();previous.live_busy=false;
            previous.logout_live_jobs();
            let mut current=if state=="signed-out"{previous}
                else if state.starts_with("same-owner-") {
                    // Native account identity is stable across sign-ins. A new
                    // worker for that owner must still reject an older refresh.
                    previous.live_jobs_owner=Some(refresh.owner.clone());
                    previous.live_jobs=vec![job(42)];
                    previous.browser_ui.status="Nieuw bestand downloaden…".into();
                    previous
                }else{app()};
            let (download_tx,download_rx)=mpsc::channel();
            let (browser_tx,browser_rx)=mpsc::channel();
            let downloading=matches!(state,"newer-download"|"same-owner-download");
            let browser_login=state=="same-owner-browser-login";
            let newer_session=if downloading {
                current.live_download_rx=Some(download_rx);current.live_busy=true;
                current.live_session.take()
            }else{None};
            if browser_login {
                current.browser_ui.capturing=Some(browser_rx);current.shell.login_open=true;
                current.browser_ui.status="Nieuwe browseraanmelding…".into();
            }
            let expected_owner=current.live_jobs_owner.clone();
            let expected_session=current.live_session.as_ref().map(|session|session.snapshot_owner().clone());
            let expected_ids=current.live_jobs.iter().map(|job|job.job_id).collect::<Vec<_>>();
            let expected_status=current.browser_ui.status.clone();
            current.live_refresh=Some(refresh);
            if disconnected{drop(session);drop(tx);}
            else{tx.send(LiveJobsRefreshResult{session,jobs:Ok(vec![job(999)])}).ok().unwrap();}
            current.poll_live_jobs();
            assert_eq!(current.live_jobs_owner,expected_owner);
            assert_eq!(current.live_session.as_ref().map(|session|session.snapshot_owner()),expected_session.as_ref());
            assert_eq!(current.live_jobs.iter().map(|job|job.job_id).collect::<Vec<_>>(),expected_ids);
            assert_eq!(current.browser_ui.status,expected_status);
            assert_eq!(current.shell.login_open,browser_login);
            assert_eq!(current.browser_ui.capturing.is_some(),browser_login);
            assert!(current.live_refresh.is_none());
            assert_eq!(current.live_busy,downloading);
            assert_eq!(current.live_operation_busy(),downloading||browser_login);
            drop((download_tx,browser_tx,newer_session));
        }
        }
    }
}

#[cfg(test)]
mod edited_login_tests {
    use super::*;

    fn frame(app:&mut SleufBaseApp,ctx:&egui::Context,events:Vec<egui::Event>)->egui::FullOutput {
        ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(Pos2::ZERO,egui::vec2(1200.,760.))),events,..Default::default()},|ctx|app.show_kickthemap_login(ctx))
    }

    fn click_login(app:&mut SleufBaseApp,ctx:&egui::Context) {
        let out=frame(app,ctx,vec![]);
        let pos=out.shapes.iter().find_map(|shape|match &shape.shape{
            egui::Shape::Text(text) if text.galley.text()=="Inloggen"=>Some(text.pos+text.galley.rect.size()/2.),_=>None,
        }).unwrap();
        for pressed in [true,false] {
            frame(app,ctx,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{
                pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default(),
            }]);
        }
    }

    #[test]
    fn editing_email_or_password_retries_native_login_after_an_old_challenge() {
        for password in [false,true] {
            let mut app=SleufBaseApp::default();let ctx=egui::Context::default();theme::install(&ctx);
            app.shell.login_email="fixture@example.test".into();app.shell.login_password="test-only".into();
            app.browser_ui.status=sleufbase_rust::kickthemap_live::INTERACTIVE_LOGIN_REQUIRED.into();
            app.open_kickthemap_login();for _ in 0..4{frame(&mut app,&ctx,vec![]);}
            if password {
                frame(&mut app,&ctx,vec![egui::Event::Key{key:egui::Key::Tab,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
                frame(&mut app,&ctx,vec![egui::Event::Key{key:egui::Key::Tab,physical_key:None,pressed:false,repeat:false,modifiers:Default::default()}]);
            }
            frame(&mut app,&ctx,vec![egui::Event::Text("x".into())]);
            if password{assert_ne!(app.shell.login_password,"test-only");}
            else{assert_ne!(app.shell.login_email,"fixture@example.test");}
            assert_ne!(app.browser_ui.status,sleufbase_rust::kickthemap_live::INTERACTIVE_LOGIN_REQUIRED);
            click_login(&mut app,&ctx);
            assert!(app.shell.login_is_native&&app.live_rx.is_some());
            assert_eq!(app.browser_ui.launch_requests,0);
            assert!(app.shell.login_password.is_empty());
        }
    }

    #[test]
    fn unchanged_challenge_keeps_the_explicit_browser_route() {
        let mut app=SleufBaseApp::default();let ctx=egui::Context::default();theme::install(&ctx);
        app.shell.login_email="fixture@example.test".into();
        app.browser_ui.status=sleufbase_rust::kickthemap_live::INTERACTIVE_LOGIN_REQUIRED.into();
        app.open_kickthemap_login();for _ in 0..4{frame(&mut app,&ctx,vec![]);}
        click_login(&mut app,&ctx);
        assert_eq!(app.browser_ui.launch_requests,1);
        assert!(app.live_rx.is_none()&&!app.shell.login_is_native);
    }
}

#[cfg(test)]
mod account_options_ui_tests {
    use super::*;

    fn session()->KickTheMapLiveClient {
        let cookie=sleufbase_rust::kickthemap_browser::BrowserCookie{
            name:"fixture".into(),value:"local-only".into(),domain:"my.kickthemap.com".into(),
            path:"/".into(),secure:true,
        };
        KickTheMapLiveClient::from_authenticated_browser(&[cookie],"fixture-csrf").unwrap()
    }

    fn frame(app:&mut SleufBaseApp,ctx:&egui::Context,size:[f32;2],events:Vec<egui::Event>)->egui::FullOutput {
        ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(Pos2::ZERO,egui::vec2(size[0],size[1]))),events,..Default::default()},|ctx|app.show_kickthemap_login(ctx))
    }

    fn settled(app:&mut SleufBaseApp,ctx:&egui::Context,size:[f32;2])->egui::FullOutput {
        for _ in 0..4{frame(app,ctx,size,vec![]);}
        frame(app,ctx,size,vec![])
    }

    fn contains(output:&egui::FullOutput,text:&str)->bool {
        output.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(value) if value.galley.text().contains(text)))
    }

    fn click(app:&mut SleufBaseApp,ctx:&egui::Context,size:[f32;2],label:&str)->egui::FullOutput {
        let output=frame(app,ctx,size,vec![]);
        let position=output.shapes.iter().find_map(|shape|match &shape.shape{
            egui::Shape::Text(value) if value.galley.text()==label=>{
                let rect=value.galley.rect.translate(value.pos.to_vec2());
                assert!(shape.clip_rect.contains_rect(rect),"{label} must stay visible");Some(rect.center())
            },_=>None,
        }).unwrap_or_else(||panic!("Missing action {label}"));
        for pressed in [true,false] {
            frame(app,ctx,size,vec![egui::Event::PointerMoved(position),egui::Event::PointerButton{
                pos:position,button:egui::PointerButton::Primary,pressed,modifiers:Default::default(),
            }]);
        }
        frame(app,ctx,size,vec![])
    }

    #[test]
    fn account_card_remains_connected_while_owned_jobs_run_without_an_unverified_email() {
        for size in [[680.,420.],[1200.,760.]] {
        for operation in ["connected","refresh","download"] {
            let mut app=SleufBaseApp::default();let session=session();
            let owner=session.snapshot_owner().clone();app.live_jobs_owner=Some(owner.clone());
            app.shell.login_email="not-verified@example.test".into();
            let (refresh_tx,refresh_rx)=mpsc::channel();
            let (download_tx,download_rx)=mpsc::channel();
            match operation {
                "refresh"=>{app.live_refresh=Some(LiveJobsRefresh{owner,events:refresh_rx});app.live_busy=true;},
                "download"=>{app.live_download_rx=Some(download_rx);app.live_busy=true;},
                _=>app.live_session=Some(session),
            }
            app.open_kickthemap_login();let ctx=egui::Context::default();theme::install(&ctx);
            let output=settled(&mut app,&ctx,size);
            assert!(contains(&output,"Je bent verbonden"),"{operation}");
            assert!(contains(&output,"Aangemeld via de KickTheMap-pagina"));
            assert!(!contains(&output,"not-verified@example.test")&&!contains(&output,"E-mailadres"));
            if operation!="connected" {
                assert!(contains(&output,if operation=="refresh"{"Je jobs worden vernieuwd…"}else{"Je KickTheMap-bestanden worden opgehaald…"}));
                click(&mut app,&ctx,size,"Open jobs");
                assert!(app.shell.login_open&&!app.browser_ui.open,"busy account action must stay disabled");
                click(&mut app,&ctx,size,"Sluiten");
                assert!(!app.shell.login_open&&app.live_operation_busy(),"closing the account window must not cancel its worker");
            }
            drop((refresh_tx,download_tx));
        }
        }
    }

    #[test]
    fn account_card_needs_a_verified_owner_and_never_masks_a_replacement_login() {
        for operation in ["native-login","browser-login","different-refresh-owner","unowned-download"] {
            let mut app=SleufBaseApp::default();let live_session=session();
            let owner=live_session.snapshot_owner().clone();app.live_jobs_owner=Some(owner);
            let (login_tx,login_rx)=mpsc::channel();let (browser_tx,browser_rx)=mpsc::channel();
            let (refresh_tx,refresh_rx)=mpsc::channel();let (download_tx,download_rx)=mpsc::channel();
            app.live_busy=true;
            match operation {
                "native-login"=>{app.live_session=Some(live_session);app.live_rx=Some(login_rx);app.shell.login_is_native=true;},
                "browser-login"=>{app.live_session=Some(live_session);app.browser_ui.capturing=Some(browser_rx);},
                "different-refresh-owner"=>{let other=session();app.live_refresh=Some(LiveJobsRefresh{owner:other.snapshot_owner().clone(),events:refresh_rx});},
                _=>{app.live_jobs_owner=None;app.live_download_rx=Some(download_rx);},
            }
            app.open_kickthemap_login();let ctx=egui::Context::default();theme::install(&ctx);
            let output=settled(&mut app,&ctx,[1200.,760.]);
            assert!(contains(&output,"Je KickTheMap-account"),"{operation}");
            assert!(!contains(&output,"Je bent verbonden"),"{operation}");
            drop((login_tx,browser_tx,refresh_tx,download_tx));
        }
    }

    #[test]
    fn remember_explanation_follows_the_choice_and_reopening_keeps_session_only() {
        let mut app=SleufBaseApp::default();let ctx=egui::Context::default();theme::install(&ctx);
        assert!(app.shell.login_remember,"keep the existing initial automatic-login choice");
        app.open_kickthemap_login();let size=[1200.,760.];let output=settled(&mut app,&ctx,size);
        assert!(contains(&output,"probeert je bij de volgende start automatisch aan te melden"));
        let output=click(&mut app,&ctx,size,"Inloggegevens onthouden en automatisch aanmelden");
        assert!(!app.shell.login_remember);
        assert!(contains(&output,"alleen voor deze sessie"));
        assert!(!contains(&output,"probeert je bij de volgende start automatisch aan te melden"));
        click(&mut app,&ctx,size,"Sluiten");app.open_kickthemap_login();
        let output=settled(&mut app,&ctx,size);
        assert!(!app.shell.login_remember&&contains(&output,"alleen voor deze sessie"));
        let output=click(&mut app,&ctx,size,"Inloggegevens onthouden en automatisch aanmelden");
        assert!(app.shell.login_remember&&contains(&output,"probeert je bij de volgende start automatisch aan te melden"));
        assert!(app.live_rx.is_none()&&app.live_refresh.is_none()&&app.browser_ui.capturing.is_none());
    }
}
