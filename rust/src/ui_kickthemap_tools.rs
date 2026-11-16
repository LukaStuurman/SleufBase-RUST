//! Native cable/pipe properties beside the owned KickTheMap WebView.
//! Drawing and final saving remain explicit actions in the provider's editor.
use super::*;
use sleufbase_rust::kickthemap_form::{BridgeReply, FormSnapshot, FormValues};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

#[derive(Clone, Copy, PartialEq)]
enum Operation { Inspect, Read, Apply, Reload }

struct Pending {
    operation: Operation,
    cancel: Arc<AtomicBool>,
    events: Receiver<Result<BridgeReply, String>>,
}

#[derive(Default)]
pub(super) struct State {
    pub(super) job: Option<Job>,
    pub(super) expected_url: String,
    snapshot: Option<FormSnapshot>,
    draft: FormValues,
    dirty: bool,
    previous: Option<FormValues>,
    pending: Option<Pending>,
    last_read: Option<Instant>,
    message: String,
}

impl State {
    pub(super) fn set_job(&mut self, job: Job, url: String) {
        self.cancel();
        *self = Self { job: Some(job), expected_url: url, ..Default::default() };
    }

    pub(super) fn cancel(&mut self) {
        if let Some(pending) = self.pending.take() {
            pending.cancel.store(true, Ordering::SeqCst);
        }
    }

    pub(super) fn writing(&self) -> bool {
        self.pending.as_ref().is_some_and(|task|
            matches!(task.operation, Operation::Apply | Operation::Reload))
    }

    fn accept(&mut self, operation: Operation, reply: Result<BridgeReply, String>) {
        match reply {
            Ok(reply) => {
                let other_job = reply.snapshot.as_ref().is_some_and(|snapshot|
                    !same_job_page(&snapshot.url, &self.expected_url));
                let snapshot = reply.snapshot.filter(|snapshot|
                    same_job_page(&snapshot.url, &self.expected_url));
                if let Some(snapshot) = snapshot {
                    if operation == Operation::Inspect && self.dirty {
                        // The user may have started typing after this inspection
                        // was launched. Never retarget that draft to a new object.
                        if self.snapshot.as_ref() != Some(&snapshot) {
                            self.snapshot = None;
                            self.message = "Het geselecteerde object is gewijzigd. Je invoer is bewaard; lees eerst het huidige object opnieuw.".into();
                        }
                        return;
                    }
                    if operation == Operation::Apply {
                        self.previous = Some(self.draft.clone());
                    }
                    let preserve_bundle = snapshot.values.cover_band && self.draft.cover_band
                        && (operation == Operation::Apply || (operation == Operation::Inspect
                            && self.snapshot.as_ref().is_some_and(|old| old.token == snapshot.token)));
                    let bundle = self.draft.bundle.clone();
                    self.draft = snapshot.values.clone();
                    if preserve_bundle { self.draft.bundle = bundle; }
                    self.dirty = false;
                    self.snapshot = Some(snapshot);
                    self.message = if operation == Operation::Apply {
                        "Gegevens ingevuld. Klik op Opslaan in KickTheMap om het object te bewaren.".into()
                    } else { reply.message };
                } else {
                    self.snapshot = None;
                    self.message = if other_job {
                        "De website toont een andere job. Open de gewenste job opnieuw vanuit het jobsvenster.".into()
                    } else if operation == Operation::Apply {
                        format!("Niet ingevuld: {} Lees eerst het huidige object opnieuw.", reply.message)
                    } else if reply.message.is_empty() {
                        "Open het eigenschappenpaneel van een punt of lijn in deze job.".into()
                    } else { reply.message };
                }
            }
            Err(error) => {
                self.snapshot = None;
                self.message = error;
            }
        }
    }
}

fn same_job_page(actual: &str, expected: &str) -> bool {
    let (Ok(actual), Ok(expected)) = (reqwest::Url::parse(actual), reqwest::Url::parse(expected)) else {
        return false;
    };
    actual.scheme() == "https" && actual.username().is_empty() && actual.password().is_none()
        && actual.query().is_none() && actual.fragment().is_none() && actual.port_or_known_default() == Some(443)
        && matches!(actual.host_str(), Some("www.my.kickthemap.com" | "my.kickthemap.com"))
        && actual == expected && actual.path().starts_with("/viewJob/") && actual.path().len() > "/viewJob/".len()
        && actual.path().trim_start_matches("/viewJob/").split('/').count() == 1
}

#[derive(Clone, Copy)]
enum Action { Read, Apply, Reload, Jobs, Close, RefreshObjects, CancelRefresh }

impl SleufBaseApp {
    fn start_kickthemap_form_operation(&mut self, operation: Operation) {
        if self.browser_ui.tools.pending.is_some() || self.browser_ui.is_opening_job()
            || self.browser_ui.capturing.is_some() || self.browser_ui.tools.job.is_none() {
            return;
        }
        let Some(channel) = self.browser_ui.browser.as_ref().map(|browser| browser.channel()) else { return; };
        let state = &mut self.browser_ui.tools;
        let snapshot = state.snapshot.clone();
        let draft = state.draft.clone();
        if operation == Operation::Apply {
            if snapshot.is_none() { return; }
            if let Err(error) = draft.validate() { state.message = error; return; }
        }
        let expected_url = state.expected_url.clone();
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        state.pending = Some(Pending { operation, cancel: cancel.clone(), events: rx });
        state.last_read = Some(Instant::now());
        #[cfg(not(test))]
        std::thread::spawn(move || {
            let result = match operation {
                Operation::Apply => channel.apply_form(&snapshot.unwrap(), &draft, &cancel),
                Operation::Reload => channel.reload_job(&expected_url, &cancel).map(|()| BridgeReply {
                    snapshot: None, message: "Jobpagina opnieuw geladen. Selecteer een object om verder te gaan.".into(),
                }),
                Operation::Read | Operation::Inspect => channel.form_snapshot(&cancel),
            };
            let _ = tx.send(result);
        });
        #[cfg(test)]
        {
            let _ = (channel, snapshot, draft, expected_url, cancel);
            let _ = tx.send(Err("Test: browseropdracht niet uitgevoerd".into()));
        }
    }

    pub(super) fn poll_kickthemap_form(&mut self, ctx: &egui::Context) {
        let result = self.browser_ui.tools.pending.as_ref().map(|pending| pending.events.try_recv());
        let result = match result {
            Some(Ok(result)) => Some(result),
            Some(Err(TryRecvError::Disconnected)) => Some(Err("Browseropdracht onderbroken; lees het object opnieuw.".into())),
            _ => None,
        };
        if let Some(result) = result {
            if let Some(pending) = self.browser_ui.tools.pending.take() {
                if !pending.cancel.load(Ordering::SeqCst) {
                    self.browser_ui.tools.accept(pending.operation, result);
                }
            }
        }
        let tools = &self.browser_ui.tools;
        if self.browser_ui.web_visible && tools.job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(150));
            // Preserve a typed draft. A later apply independently checks that the
            // same object and original values are still selected on the website.
            if !tools.dirty && tools.pending.is_none() && !self.live_operation_busy()
                && tools.last_read.is_none_or(|last| last.elapsed() >= Duration::from_millis(900)) {
                self.start_kickthemap_form_operation(Operation::Inspect);
            }
        }
    }

    pub(super) fn show_kickthemap_job_browser(&mut self, ctx: &egui::Context) {
        let busy = self.live_operation_busy();
        let mut action = None;
        let settings = self.project_settings.as_ref().unwrap_or(&self.migrated_settings).clone();
        egui::CentralPanel::default().show(ctx, |ui| {
            settings_ui::control_style(ui);
            ui.horizontal_wrapped(|ui| {
                ui.heading("KickTheMap-browser");
                if let Some(job) = &self.browser_ui.tools.job {
                    ui.label(egui::RichText::new(&job.title).strong());
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!busy, egui::Button::new("Terug naar jobs")).clicked() { action = Some(Action::Jobs); }
                if ui.add_enabled(!busy, egui::Button::new("Pagina vernieuwen")).clicked() { action = Some(Action::Reload); }
                if ui.add_enabled(!busy, egui::Button::new("Objecten opnieuw laden")).on_hover_text(
                    "Sla je wijzigingen eerst op in KickTheMap. Dit haalt de actuele objectpunten op en vervangt de bijbehorende lokale joblaag."
                ).clicked() { action = Some(Action::RefreshObjects); }
                if ui.button("Terug naar kaart").clicked() { action = Some(Action::Close); }
            });
            if !self.browser_ui.status.is_empty() {
                ui.small(&self.browser_ui.status);
            }
            if self.browser_ui.reloading.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner(); ui.label("Actuele objecten ophalen…");
                    if ui.button("Ophalen annuleren").clicked() { action = Some(Action::CancelRefresh); }
                });
            }
            ui.separator();
            let area = ui.available_rect_before_wrap();
            // The native WebView must never cover the egui fields. In compact
            // windows put them above the page; all fields remain scrollable.
            let side_by_side = area.width() >= 690.;
            let (form_rect, web_rect) = if side_by_side {
                let width = 280_f32.min(area.width() * 0.4);
                (egui::Rect::from_min_max(area.min, egui::pos2(area.left() + width, area.bottom())),
                 egui::Rect::from_min_max(egui::pos2(area.left() + width + 10., area.top()), area.max))
            } else {
                let height = (area.height() * 0.43).clamp(80., 250.).min(area.height().max(0.));
                (egui::Rect::from_min_max(area.min, egui::pos2(area.right(), area.top() + height)),
                 egui::Rect::from_min_max(egui::pos2(area.left(), (area.top() + height + 8.).min(area.bottom())), area.max))
            };
            let mut form_ui = ui.new_child(egui::UiBuilder::new().id_salt("kickthemap-native-fields")
                .max_rect(form_rect).layout(egui::Layout::top_down(egui::Align::Min)));
            form_ui.set_clip_rect(form_rect);
            egui::ScrollArea::vertical().id_salt("kickthemap-native-fields-scroll").auto_shrink([false, false])
                .show(&mut form_ui, |ui| {
                    if let Some(choice) = properties_panel(ui, &mut self.browser_ui.tools, &settings, !busy) {
                        action = Some(choice);
                    }
                });
            self.set_kickthemap_web_bounds(ctx, web_rect);
        });
        match action {
            Some(Action::Read) => self.start_kickthemap_form_operation(Operation::Read),
            Some(Action::Apply) => self.start_kickthemap_form_operation(Operation::Apply),
            Some(Action::Reload) => {
                self.browser_ui.tools.cancel();
                self.browser_ui.tools.snapshot = None;
                self.start_kickthemap_form_operation(Operation::Reload);
            }
            Some(Action::Jobs) => {
                self.hide_kickthemap_pane();
                self.open_kickthemap_workspace();
            }
            Some(Action::Close) => self.hide_kickthemap_pane(),
            Some(Action::CancelRefresh) => {
                if let Some(reload) = &self.browser_ui.reloading { reload.cancel(); }
            }
            Some(Action::RefreshObjects) => {
                if let Some(id) = self.browser_ui.tools.job.as_ref().map(|job| job.job_id) {
                    self.start_kickthemap_object_reload(id);
                }
            }
            None => {},
        }
    }

    pub(super) fn set_kickthemap_web_bounds(&self, ctx: &egui::Context, rect: egui::Rect) {
        if let Some(dock) = &self.browser_ui.dock {
            // A native child window otherwise paints over egui dialogs. Menus,
            // settings and other floating windows must remain usable above it.
            let covered = ctx.memory(|memory| memory.any_popup_open()
                || memory.areas().visible_layer_ids().iter().any(|layer|
                    layer.order != egui::Order::Background
                        && memory.area_rect(layer.id).is_some_and(|area| area.intersects(rect))));
            let scale = ctx.pixels_per_point();
            let bounds = (!covered && rect.width() >= 1. && rect.height() >= 1.).then(|| [
                (rect.left() * scale).round() as i32, (rect.top() * scale).round() as i32,
                (rect.width() * scale).round() as i32, (rect.height() * scale).round() as i32,
            ]);
            let _ = dock.set_bounds(bounds);
        }
    }
}

fn properties_panel(ui: &mut egui::Ui, state: &mut State, settings: &MigratedSettings, enabled: bool) -> Option<Action> {
    let mut action = None;
    ui.strong("Kabel / leiding toevoegen");
    ui.small("Teken een punt of lijn in de job en open de eigenschappen. Vul hier de gegevens in en klik daarna op Opslaan in KickTheMap.");
    ui.add_space(5.);
    let pending = state.pending.is_some();
    if ui.add_enabled(!pending && enabled, egui::Button::new("Lees geselecteerd object")).on_hover_text(
        "Neem de huidige websitevelden over. Dit vervangt alleen de invoer in dit paneel."
    ).clicked() { action = Some(Action::Read); }
    if !state.message.is_empty() { ui.small(&state.message); }
    ui.separator();
    let can_edit = enabled && state.snapshot.is_some()
        && state.pending.as_ref().is_none_or(|pending| pending.operation == Operation::Inspect);
    ui.add_enabled_ui(can_edit, |ui| {
        let before = state.draft.clone();
        ui.label("Type kabel / leiding");
        let rules = &settings.object_rules;
        let label = rules.iter().find(|rule| rule.keywords.iter().any(|key|
            key.eq_ignore_ascii_case(&state.draft.name)) || rule.display_label().eq_ignore_ascii_case(&state.draft.name))
            .map(|rule| rule.display_label()).unwrap_or(&state.draft.name);
        egui::ComboBox::from_id_salt("kickthemap-object-type").width(ui.available_width().max(80.) - 10.)
            .selected_text(if label.is_empty() { "Kies een type" } else { label }).show_ui(ui, |ui| {
                let mut seen = std::collections::BTreeSet::new();
                for rule in rules {
                    if let Some(code) = rule.keywords.iter().map(|word| word.trim()).find(|word| !word.is_empty()) {
                        if !seen.insert(code.to_lowercase()) { continue; }
                        if ui.selectable_label(state.draft.name.eq_ignore_ascii_case(code), rule.display_label()).clicked() {
                            state.draft.name = code.to_string();
                        }
                    }
                }
                for value in settings_compat::profile_extra_choices(&settings.kickthemap_profile_extra_choices, rules) {
                    ui.selectable_value(&mut state.draft.name, value.to_owned(), value);
                }
            });
        ui.add(egui::TextEdit::singleline(&mut state.draft.name).hint_text("Afkorting of eigen type").desired_width(f32::INFINITY));
        ui.label("Materiaal");
        ui.add(egui::TextEdit::singleline(&mut state.draft.material).hint_text("Bijv. PVC, PE of koper").desired_width(f32::INFINITY));
        if !settings.kickthemap_material_choices.is_empty() {
            egui::ComboBox::from_id_salt("kickthemap-material-choice").selected_text("Kies materiaal uit instellingen")
                .show_ui(ui, |ui| {
                    for value in &settings.kickthemap_material_choices {
                        ui.selectable_value(&mut state.draft.material, value.clone(), value);
                    }
                });
        }
        ui.label("Diameter");
        ui.add(egui::TextEdit::singleline(&mut state.draft.diameter).hint_text("Bijv. 110").desired_width(f32::INFINITY));
        ui.label("Aantal in bundel");
        ui.add_enabled(!state.draft.cover_band, egui::TextEdit::singleline(&mut state.draft.bundle)
            .hint_text("Bijv. 3").desired_width(f32::INFINITY));
        ui.checkbox(&mut state.draft.cover_band, "Dekband");
        if state.draft.cover_band { ui.small("KickTheMap bewaart dekband in hetzelfde veld als het bundelaantal."); }
        if state.draft != before { state.dirty = true; }
    });
    ui.add_space(7.);
    let can_apply = enabled && !pending && state.snapshot.is_some() && state.draft.validate().is_ok();
    if ui.add_enabled(can_apply, egui::Button::new(egui::RichText::new("Vul gegevens in").strong().color(Color32::WHITE)).fill(ORANGE)).clicked() {
        action = Some(Action::Apply);
    }
    if let Some(previous) = &state.previous {
        if ui.add_enabled(enabled && !pending && state.snapshot.is_some(), egui::Button::new("Gebruik vorige invoer")).clicked() {
            state.draft = previous.clone(); state.dirty = true;
        }
    }
    if state.snapshot.is_none() {
        ui.small("De invoer wordt pas toegepast wanneer één geldig eigenschappenformulier van deze job openstaat.");
    }
    if pending { ui.spinner(); }
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(name: &str) -> FormSnapshot {
        FormSnapshot { token: "form-1".into(), url: "https://www.my.kickthemap.com/viewJob/owner_2026".into(),
            values: FormValues { name: name.into(), material: "PE100".into(), diameter: "2 × 40".into(), bundle: "3".into(), cover_band: false } }
    }

    #[test]
    fn edited_draft_survives_inspection_and_stale_apply_rejection() {
        let original = snapshot("eigen-type");
        let mut state = State { expected_url: original.url.clone(), ..Default::default() };
        state.accept(Operation::Read, Ok(BridgeReply { snapshot: Some(original), message: String::new() }));
        state.draft.material = "Aangepast".into(); state.dirty = true;
        state.accept(Operation::Inspect, Ok(BridgeReply { snapshot: Some(snapshot("andere-kabel")), message: String::new() }));
        assert_eq!(state.draft.material, "Aangepast");
        assert!(state.snapshot.is_none(), "an in-flight inspection must not retarget a typed draft");
        state.accept(Operation::Apply, Ok(BridgeReply { snapshot: None, message: "Selectie gewijzigd".into() }));
        assert_eq!(state.draft.material, "Aangepast");
        assert!(state.snapshot.is_none() && state.dirty);
    }

    #[test]
    fn dekband_keeps_the_draft_bundle_for_unchecking_and_polling() {
        let mut value = snapshot("water"); value.values.cover_band = true; value.values.bundle.clear();
        let mut draft = value.values.clone(); draft.bundle = "3".into();
        let mut state = State { expected_url: value.url.clone(), draft, dirty: true, ..Default::default() };
        state.accept(Operation::Apply, Ok(BridgeReply { snapshot: Some(value.clone()), message: String::new() }));
        assert_eq!(state.draft.bundle, "3");
        state.accept(Operation::Inspect, Ok(BridgeReply { snapshot: Some(value), message: String::new() }));
        assert_eq!(state.draft.bundle, "3");
        state.draft.cover_band = false;
        assert_eq!(state.draft.attribute3(), "3");
    }

    #[test]
    fn form_of_another_job_or_origin_cannot_become_apply_target() {
        let expected = snapshot("water").url;
        assert!(!same_job_page("https://www.my.kickthemap.com/viewJob/other", &expected));
        assert!(!same_job_page("https://evil.test/viewJob/owner_2026", &expected));
        assert!(!same_job_page("http://www.my.kickthemap.com/viewJob/owner_2026", &expected));
        assert!(!same_job_page(&(expected.clone() + "#view"), &expected));
        assert!(!same_job_page(&(expected.clone() + "?mode=edit"), &expected));
        assert!(same_job_page(&expected, &expected));
        let mut state = State { expected_url: expected, ..Default::default() };
        let mut wrong = snapshot("water"); wrong.url = "https://www.my.kickthemap.com/viewJob/other".into();
        state.accept(Operation::Read, Ok(BridgeReply { snapshot: Some(wrong), message: String::new() }));
        assert!(state.snapshot.is_none());
    }

    #[test]
    fn successful_apply_keeps_a_repeat_preset_and_explicit_read_preserves_custom_values() {
        let value = snapshot("eigen-water");
        let mut state = State { expected_url: value.url.clone(), draft: value.values.clone(), dirty: true, ..Default::default() };
        state.accept(Operation::Apply, Ok(BridgeReply { snapshot: Some(value.clone()), message: String::new() }));
        assert_eq!(state.previous.as_ref(), Some(&value.values));
        assert!(!state.dirty);
        assert_eq!(state.draft.diameter, "2 × 40");
        assert!(state.message.contains("Opslaan in KickTheMap"));
    }
}
