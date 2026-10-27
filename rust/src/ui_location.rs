//! Address/RD/GPS lookup and temporary navigation, separate from project data.
use super::*;
use sleufbase_rust::location_search::{
    self,
    worker::{Ready, Worker},
    Location,
};

#[derive(Default)]
pub(super) struct Search {
    pub open: bool,
    query: String,
    results: Vec<Location>,
    selected: usize,
    result_focus: bool,
    result_ids: Vec<egui::Id>,
    deadline: Option<Instant>,
    worker: Option<Worker>,
    ticket: Option<u64>,
    error: Option<String>,
    focus_entry: bool,
    pub needs_aerial: bool,
}
impl Search {
    pub fn reset(&mut self) {
        if let Some(worker) = &self.worker {
            worker.cancel();
        }
        let worker = self.worker.take();
        *self = Self {
            worker,
            ..Default::default()
        };
    }
    fn cancel(&mut self) {
        if let Some(worker) = &self.worker {
            worker.cancel();
        }
        self.ticket = None;
        self.deadline = None;
        self.result_ids.clear();
    }
    fn accept(&mut self, ready: Ready) {
        if self.ticket != Some(ready.ticket) || !self.open {
            return;
        }
        self.ticket = None;
        match ready.result {
            Ok(mut values) => {
                values.truncate(6);
                self.results = values;
                self.selected = 0;
                self.result_focus = false;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }
    fn start(&mut self) {
        self.cancel();
        self.error = None;
        self.results.clear();
        self.selected = 0;
        self.result_focus = false;
        if let Some(location) = location_search::parse_coordinate(&self.query) {
            self.results.push(location);
            return;
        }
        if self.query.trim().chars().count() < 3 {
            return;
        }
        let result = (|| {
            if self.worker.is_none() {
                self.worker = Some(Worker::new()?);
            }
            self.worker.as_ref().unwrap().search(self.query.clone())
        })();
        match result {
            Ok(ticket) => self.ticket = Some(ticket),
            Err(error) => self.error = Some(error),
        }
    }
}
#[derive(Clone, Copy)]
pub(super) struct Focus {
    pub center: [f64; 2],
    pub meters_per_pixel: f64,
}
#[derive(Default)]
pub(super) struct Navigation {
    pub raster_crops:Vec<Option<raster_viewport_ui::Crop>>,
    pub raster_opacity:Option<f32>,
    pub show_background:Option<bool>,
    pub pointer_tool:bool,
    pub camera_changed:bool,
    pub focus: Option<Focus>,
    pub viewport: Option<Bounds>,
    pub marker: Option<([f64; 2], Instant)>,
}
impl Navigation {
    fn mark(&mut self, location: &Location, jump: bool) -> Result<(), String> {
        if !location.x.is_finite() || !location.y.is_finite() {
            return Err("Ongeldige locatie".into());
        }
        let center = [location.x, location.y];
        if jump {
            self.focus = Some(Focus {
                center,
                meters_per_pixel: location.meters_per_pixel(),
            });
            self.viewport = None;
        }
        self.marker = Some((center, Instant::now()));
        Ok(())
    }
    pub fn bounds(&self) -> Option<Bounds> {
        self.focus.map(|focus| {
            self.viewport.unwrap_or(Bounds {
                min_x: focus.center[0] - 600. * focus.meters_per_pixel,
                max_x: focus.center[0] + 600. * focus.meters_per_pixel,
                min_y: focus.center[1] - 450. * focus.meters_per_pixel,
                max_y: focus.center[1] + 450. * focus.meters_per_pixel,
            })
        })
    }
}
impl SleufBaseApp {
    pub(super) fn open_location(&mut self) {
        self.location.open = true;
        self.location.focus_entry = true;
    }
    pub(super) fn poll_location(&mut self, ctx: &egui::Context) {
        if let Some(ready) = self.location.worker.as_ref().and_then(Worker::poll) {
            self.location.accept(ready);
        }
        if self.location.open
            && self
                .location
                .deadline
                .is_some_and(|at| Instant::now() >= at)
        {
            self.location.start();
        }
        if self.location.ticket.is_some() || self.location.deadline.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if self.location.needs_aerial && !self.pdok_busy && self.navigation.viewport.is_some() {
            self.location.needs_aerial = false;
            self.start_pdok();
        }
    }
    fn choose_location(&mut self, location: Location, jump: bool) -> Result<(), String> {
        if self
            .geotiffs
            .iter()
            .any(|r| r.epsg.is_some_and(|e| e != 28992))
        {
            return Err("Locatiezoeken vereist RD (EPSG:28992) voor geladen kaartlagen".into());
        }
        self.navigation.mark(&location, jump)?;
        if jump {
            self.zoom = 1.;
            self.pan = egui::Vec2::ZERO;
            self.osm_visible = false;
            self.location.needs_aerial = true;
            self.location.open = false;
            self.location.cancel();
            self.message = format!("Gesprongen naar: {}", location.label);
        } else {
            self.message = format!("Locatie gemarkeerd: {}", location.label);
        }
        Ok(())
    }
    /// Compatibility entry point for callers without a canvas rectangle.
    /// The map shell should call show_location_on_map instead.
    #[cfg(test)]
    pub(super) fn show_location(&mut self, ctx: &egui::Context) {
        self.show_location_in_rect(ctx, ctx.available_rect());
    }
    pub(super) fn show_location_on_map(&mut self, ui: &egui::Ui, rect: egui::Rect) {
        self.show_location_in_rect(ui.ctx(), rect);
    }
    fn show_location_in_rect(&mut self, ctx: &egui::Context, map_rect: egui::Rect) {
        if !self.location.open || !map_rect.is_positive() { return; }
        let status = self.location.error.clone().or_else(||
            self.location.ticket.map(|_| "Locaties zoeken…".to_owned()));
        let (bar, suggestions) = inline_geometry(map_rect, self.location.results.len(), status.is_some());
        let bounds = suggestions.map_or(bar, |r| r.union(bar));
        let mut close = false;
        let mut choice = None;
        let mut submit = false;
        egui::Area::new(egui::Id::new("location-search"))
            .order(egui::Order::Foreground).movable(false).constrain(false)
            .fixed_pos(bounds.min).show(ctx, |ui| {
                ui.set_min_size(bounds.size());
                ui.set_clip_rect(map_rect);
                let entry_id = egui::Id::new("location-query");
                let keyboard_active = ui.memory(|m| m.focused()).is_some_and(|id|
                    id == entry_id || self.location.result_ids.contains(&id)) || self.location.focus_entry;
                let down = keyboard_active && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown));
                let up = keyboard_active && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp));
                submit = keyboard_active && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                close = keyboard_active && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                ui.painter().rect_filled(bar, 14., Color32::WHITE);
                ui.painter().rect_stroke(bar, 14., egui::Stroke::new(1_f32, Color32::from_rgb(221,228,238)), egui::StrokeKind::Inside);
                let entry_rect = egui::Rect::from_min_size(bar.min + egui::vec2(12.,7.), egui::vec2(bar.width()-96.,28.));
                let response = ui.put(entry_rect, egui::TextEdit::singleline(&mut self.location.query)
                    .id(entry_id).char_limit(1024).frame(false)
                    .margin(egui::vec2(0.,4.)).text_color(Color32::from_rgb(17,24,39))
                    .hint_text("Adres, plaats, RD of GPS"));
                if self.location.focus_entry { response.request_focus(); self.location.focus_entry=false; }
                if response.has_focus() {
                    ui.memory_mut(|m| m.set_focus_lock_filter(entry_id,egui::EventFilter {
                        horizontal_arrows:true,vertical_arrows:true,escape:true,..Default::default()
                    }));
                }
                if response.changed() {
                    self.location.cancel(); self.location.results.clear(); self.location.result_focus=false;
                    self.location.error=None; self.location.deadline=Some(Instant::now()+Duration::from_millis(250));
                }
                if down && !self.location.results.is_empty() {
                    self.location.selected=if self.location.result_focus {
                        (self.location.selected+1).min(self.location.results.len()-1)
                    } else { 0 };
                    self.location.result_focus=true;
                }
                if up && self.location.result_focus {
                    if self.location.selected==0 { self.location.result_focus=false; }
                    else { self.location.selected-=1; }
                }
                let pulse_rect=egui::Rect::from_min_size(egui::pos2(bar.right()-76.,bar.top()+7.),egui::vec2(30.,28.));
                if ui.put(pulse_rect,egui::Button::new(egui::RichText::new("●")
                    .color(Color32::from_rgb(249,115,22))).fill(Color32::WHITE)
                    .stroke(egui::Stroke::NONE).corner_radius(13.)).on_hover_text("Alleen markeren").clicked() {
                    choice=location_search::parse_coordinate(&self.location.query)
                        .or_else(||self.location.results.get(self.location.selected).cloned()).map(|r|(r,false));
                    if choice.is_none() { self.message="Geen locatie om te markeren.".into(); }
                }
                let close_rect=egui::Rect::from_min_size(egui::pos2(bar.right()-42.,bar.top()+7.),egui::vec2(30.,28.));
                close |= ui.put(close_rect,egui::Button::new("x").fill(Color32::WHITE)
                    .stroke(egui::Stroke::NONE).corner_radius(13.)).on_hover_text("Sluiten").clicked();
                self.location.result_ids.clear();
                if let Some(rect)=suggestions {
                    ui.painter().rect_filled(rect,12.,Color32::WHITE);
                    ui.painter().rect_stroke(rect,12.,egui::Stroke::new(1_f32,Color32::from_rgb(221,228,238)),egui::StrokeKind::Inside);
                    let inner=rect.shrink2(egui::vec2(6.,5.));
                    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(inner), |ui| {
                        if let Some(text)=&status {
                            ui.add_sized([inner.width(),24.],egui::Label::new(egui::RichText::new(text)
                                .color(Color32::from_rgb(102,112,133))).truncate());
                        }
                        let height=self.location.results.len().min(4) as f32*24.;
                        egui::ScrollArea::vertical().id_salt("location-suggestions")
                            .max_height(height).auto_shrink([false,false]).show(ui,|ui| {
                                ui.spacing_mut().item_spacing.y=0.;
                                for (index,result) in self.location.results.iter().enumerate() {
                                    let selected=self.location.result_focus && self.location.selected==index;
                                    let text=egui::RichText::new(&result.label).color(if selected {Color32::WHITE}
                                        else {Color32::from_rgb(17,24,39)});
                                    let response=ui.add_sized([ui.available_width(),24.],egui::Button::new(text)
                                        .fill(if selected {Color32::from_rgb(249,115,22)} else {Color32::WHITE})
                                        .stroke(egui::Stroke::NONE).corner_radius(0.).truncate());
                                    self.location.result_ids.push(response.id);
                                    if response.has_focus() {
                                        ui.memory_mut(|m|m.set_focus_lock_filter(response.id,egui::EventFilter {
                                            vertical_arrows:true,escape:true,..Default::default()
                                        }));
                                    }
                                    if selected && (down||up) { response.scroll_to_me(Some(egui::Align::Center)); }
                                    if response.clicked() {
                                        self.location.selected=index;self.location.result_focus=true;
                                        ui.memory_mut(|m|m.request_focus(entry_id));
                                    }
                                    if response.double_clicked() {choice=Some((result.clone(),true));}
                                }
                            });
                    });
                }
            });
        if close {
            self.location.open=false; self.location.cancel(); self.location.results.clear();
            self.location.result_focus=false;
            return;
        }
        if submit {
            if let Some(location)=location_search::parse_coordinate(&self.location.query) { choice=Some((location,true)); }
            else if let Some(location)=self.location.results.get(self.location.selected) { choice=Some((location.clone(),true)); }
            else { self.location.start(); }
        }
        if let Some((location,jump))=choice {
            if let Err(error)=self.choose_location(location,jump) { self.location.error=Some(error); }
        }
    }

}

/// Pixel geometry from the active original inline-location wrapper (app.py:600).
fn inline_geometry(map: egui::Rect, results: usize, status: bool) -> (egui::Rect, Option<egui::Rect>) {
    let width=(map.width()-40.).clamp(320.,460.);
    let bar=egui::Rect::from_min_size(
        egui::pos2(map.center().x-width/2.,map.top()+(map.height()-60.).max(12.)),egui::vec2(width,42.));
    let rows=results.min(4)+usize::from(status);
    let suggestions=(rows>0).then(|| {
        let height=rows as f32*24.+10.;
        egui::Rect::from_min_size(egui::pos2(bar.left(),(bar.top()-height-6.).max(map.top()+8.)),egui::vec2(width,height))
    });
    (bar,suggestions)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn location() -> Location {
        Location {
            label: "Testlocatie".into(),
            x: 155000.,
            y: 463000.,
            location_type: "rd".into(),
        }
    }
    #[test]
    fn inline_overlay_geometry_matches_original_canvas_offsets_and_four_visible_rows() {
        let map=egui::Rect::from_min_size(egui::pos2(100.,50.),egui::vec2(800.,600.));
        let (bar,suggestions)=inline_geometry(map,6,false);
        assert_eq!(bar.size(),egui::vec2(460.,42.));
        assert_eq!(bar.center().x,map.center().x);
        assert_eq!(map.bottom()-bar.bottom(),18.);
        let suggestions=suggestions.unwrap();
        assert_eq!(suggestions.height(),106.);
        assert_eq!(bar.top()-suggestions.bottom(),6.);
        assert!(map.contains_rect(bar) && map.contains_rect(suggestions));
        let (_,empty)=inline_geometry(map,0,false);
        assert!(empty.is_none());
    }
    #[test]
    fn actual_inline_close_button_cancels_without_starting_a_search_or_moving_the_map() {
        let ctx=egui::Context::default();
        let mut app=SleufBaseApp { location:Search {open:true,query:"unchanged".into(),results:vec![location()],focus_entry:true,..Default::default()},..Default::default()};
        let screen=egui::Rect::from_min_size(Pos2::ZERO,egui::vec2(900.,650.));
        let map=egui::Rect::from_min_size(egui::pos2(100.,50.),egui::vec2(700.,500.));
        let bar=inline_geometry(map,1,false).0;
        let position=egui::pos2(bar.right()-27.,bar.top()+21.);
        let frame=|events:Vec<egui::Event>,app:&mut SleufBaseApp| {
            ctx.run(egui::RawInput {screen_rect:Some(screen),events,..Default::default()},|ctx| {
                egui::CentralPanel::default().show(ctx,|ui|app.show_location_on_map(ui,map));
            })
        };
        for _ in 0..3 {let _=frame(Vec::new(),&mut app);}
        for pressed in [true,false] {
            let _=frame(vec![egui::Event::PointerMoved(position),egui::Event::PointerButton {
                pos:position,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()
            }],&mut app);
        }
        assert!(!app.location.open);
        assert!(app.location.results.is_empty() && app.location.worker.is_none());
        assert!(app.navigation.focus.is_none() && app.navigation.marker.is_none());
    }
    #[test]
    fn stale_or_closed_search_results_never_replace_current_suggestions() {
        let mut search = Search {
            open: true,
            ticket: Some(2),
            ..Default::default()
        };
        search.accept(Ready {
            ticket: 1,
            result: Ok(vec![location()]),
        });
        assert!(search.results.is_empty());
        search.accept(Ready {
            ticket: 2,
            result: Ok(vec![location()]),
        });
        assert_eq!(search.results.len(), 1);
        search.open = false;
        search.ticket = Some(3);
        search.accept(Ready {
            ticket: 3,
            result: Err("stale".into()),
        });
        assert!(search.error.is_none());
        search.reset();
        assert!(search.results.is_empty() && search.ticket.is_none() && !search.open);
    }
    #[test]
    fn a_location_on_an_empty_map_has_the_original_scale_pulse_and_panned_provider_bounds() {
        let ctx = egui::Context::default();
        let mut navigation = Navigation::default();
        navigation.mark(&location(), true).unwrap();
        let mut zoom = 1.;
        let mut pan = egui::vec2(30., -20.);
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(800., 600.),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    canvas(
                        ui,
                        &[],
                        &[],
                        &[],
                        &[],
                        &[],
                        &[],
                        &[],
                        None,
                        &[],
                        None,
                        None,
                        false,
                        &mut zoom,
                        &mut pan,
                        RULES,
                        Some(&mut navigation),
                    );
                });
            },
        );
        assert!(output
            .shapes
            .iter()
            .any(|s| matches!(&s.shape,egui::Shape::Circle(c) if c.fill==ORANGE&&c.radius==4.)));
        let b = navigation.bounds().unwrap();
        assert!(((b.min_x + b.max_x) / 2. - (155000. - 30. * 0.2)).abs() < 1e-8);
        assert!(((b.min_y + b.max_y) / 2. - (463000. - 20. * 0.2)).abs() < 1e-8);
        assert!((b.max_x - b.min_x) < 160. && b.max_x > b.min_x);
    }
    #[test]
    fn navigation_is_not_a_project_edit_and_project_switch_cancels_and_clears_it() {
        let mut app = SleufBaseApp::default();
        let ctx = egui::Context::default();
        app.choose_location(location(), true).unwrap();
        assert!(!app.dirty && app.location.needs_aerial && app.navigation.focus.is_some());
        assert_eq!(app.snapshot().comments.len(), 0);
        app.apply_project(NativeProject::default(), None, false, Vec::new(), &ctx);
        assert!(
            app.navigation.focus.is_none()
                && app.navigation.marker.is_none()
                && !app.location.needs_aerial
        );
        app.choose_location(location(), false).unwrap();
        assert!(app.navigation.focus.is_none() && app.navigation.marker.is_some());
    }
    #[test]
    fn keyboard_enters_the_first_suggestion_then_moves_and_selects_without_a_network_call() {
        let ctx = egui::Context::default();
        let mut app = SleufBaseApp {
            location: Search {
                open: true,
                query: "testplaats".into(),
                results: vec![
                    location(),
                    Location {
                        label: "Tweede".into(),
                        x: 156000.,
                        ..location()
                    },
                ],
                focus_entry: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let frame = |key: Option<egui::Key>, app: &mut SleufBaseApp| {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(900., 650.),
                )),
                ..Default::default()
            };
            if let Some(key) = key {
                input.events.push(egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let _ = ctx.run(input, |ctx| app.show_location(ctx));
        };
        frame(None, &mut app);
        frame(Some(egui::Key::ArrowDown), &mut app);
        assert_eq!(app.location.selected, 0);
        frame(Some(egui::Key::ArrowDown), &mut app);
        assert_eq!(app.location.selected, 1);
        frame(Some(egui::Key::Enter), &mut app);
        assert!(!app.location.open && app.location.worker.is_none());
        assert_eq!(app.navigation.focus.unwrap().center, [156000., 463000.]);
    }
}
