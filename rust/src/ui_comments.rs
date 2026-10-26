//! Transactional free-map comments. Drag previews never alter saved project data.
use super::*;
use sleufbase_rust::map_comments;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Mode {
    Place,
    Move,
}
#[derive(Default)]
pub(super) struct State {
    mode: Option<Mode>,
    editor: Option<Editor>,
    drag: Option<Drag>,
}
struct Target {
    index: usize,
    generation: u64,
    original: Comment,
}
impl Target {
    fn valid(&self, app: &SleufBaseApp) -> bool {
        self.generation == app.project_generation
            && app.comments.get(self.index).is_some_and(|c| {
                c.x == self.original.x && c.y == self.original.y && c.text == self.original.text
            })
    }
}
struct Editor {
    target: Option<Target>,
    generation: u64,
    point: [f64; 2],
    text: String,
    error: String,
}
struct Drag {
    target: Target,
    offset: [f64; 2],
    point: [f64; 2],
}
impl State {
    pub(super) fn captures(&self) -> bool {
        self.mode.is_some() || self.drag.is_some()
    }
}

impl SleufBaseApp {
    fn comment_target(&self, index: usize) -> Option<Target> {
        self.comments.get(index).map(|c| Target {
            index,
            generation: self.project_generation,
            original: c.clone(),
        })
    }
    pub(super) fn start_comment_mode(&mut self, mode: Mode) {
        let next = (self.comment_tools.mode != Some(mode)).then_some(mode);
        self.cancel_virtual_map();
        self.stop_raster_rotation();
        self.raster_points.mode = None;
        self.comment_tools = State {
            mode: next,
            ..Default::default()
        };
        if next.is_some() && self.navigation.viewport.is_none() && self.navigation.focus.is_none() {
            self.navigation.focus = Some(location_ui::Focus {
                center: [155000., 463000.],
                meters_per_pixel: 1.,
            });
        }
        self.message = match next {
            Some(Mode::Place) => {
                "Klik op de kaart voor een nieuwe opmerking. Escape sluit de kaartactie."
            }
            Some(Mode::Move) => {
                "Sleep de stip van een opmerking. Escape annuleert het verplaatsen."
            }
            None => "Opmerkingen bewerken op de kaart afgesloten.",
        }
        .into();
    }
    pub(super) fn open_map_comment(&mut self, index: Option<usize>, point: [f64; 2]) {
        let target = index.and_then(|i| self.comment_target(i));
        if index.is_some() && target.is_none() {
            return;
        }
        let text = target
            .as_ref()
            .map_or_else(String::new, |t| t.original.text.clone());
        self.comment_tools.mode = None;
        self.comment_tools.drag = None;
        self.comment_tools.editor = Some(Editor {
            target,
            generation: self.project_generation,
            point,
            text,
            error: String::new(),
        });
    }
    pub(super) fn remove_map_comment(&mut self, index: usize) {
        if index < self.comments.len() {
            self.comments.remove(index);
            self.comment_tools = State::default();
            self.dirty = true;
        }
    }
    fn submit_map_comment(&mut self) -> bool {
        let Some(mut editor) = self.comment_tools.editor.take() else {
            return false;
        };
        if editor.generation != self.project_generation
            || editor.target.as_ref().is_some_and(|t| !t.valid(self))
        {
            self.message = "De opmerking is gewijzigd; open de bewerking opnieuw.".into();
            return false;
        }
        match map_comments::edit(
            &mut self.comments,
            editor.target.as_ref().map(|t| t.index),
            editor.point,
            &editor.text,
        ) {
            Ok(changed) => {
                self.dirty |= changed;
                self.message = "Kaartopmerking opgeslagen.".into();
                true
            }
            Err(error) => {
                editor.error = error;
                self.comment_tools.editor = Some(editor);
                false
            }
        }
    }
    pub(super) fn show_map_comment_editor(&mut self, ctx: &egui::Context) {
        let Some(editor) = self.comment_tools.editor.as_mut() else {
            return;
        };
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        egui::Window::new("Kaartopmerking")
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("RD X");
                    ui.add(egui::DragValue::new(&mut editor.point[0]).speed(0.1));
                    ui.label("RD Y");
                    ui.add(egui::DragValue::new(&mut editor.point[1]).speed(0.1));
                });
                ui.add(
                    egui::TextEdit::multiline(&mut editor.text)
                        .desired_rows(5)
                        .desired_width(400.),
                );
                if !editor.error.is_empty() {
                    ui.colored_label(Color32::RED, &editor.error);
                }
                ui.horizontal(|ui| {
                    save = ui.button("Opslaan").clicked();
                    cancel = ui.button("Annuleren").clicked();
                });
            });
        if !open || cancel || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.comment_tools.editor = None;
        } else if save {
            self.submit_map_comment();
        }
    }
    pub(super) fn show_map_comment_controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("Kaartopmerkingen");
        if ui
            .selectable_label(
                self.comment_tools.mode == Some(Mode::Move),
                "Opmerkingen verplaatsen",
            )
            .clicked()
        {
            self.start_comment_mode(Mode::Move);
        }
        let mut edit = None;
        let mut remove = None;
        for (i, note) in self.comments.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(
                        false,
                        format!(
                            "{} ({:.1}, {:.1})",
                            note.text.chars().take(100).collect::<String>(),
                            note.x,
                            note.y
                        ),
                    )
                    .clicked()
                {
                    edit = Some(i);
                }
                if ui.small_button("×").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.remove_map_comment(i);
        } else if let Some(i) = edit {
            let c = &self.comments[i];
            self.open_map_comment(Some(i), [c.x, c.y]);
        }
    }
    pub(super) fn handle_map_comments(&mut self, view: &raster_point_ui::View) {
        let ctx = view.painter.ctx();
        // Other map tools may have been started since the comment tool was opened.
        if self.virtual_map.captures()
            || self.raster_rotation.active()
            || self.raster_points.mode.is_some()
        {
            self.comment_tools.mode = None;
            self.comment_tools.drag = None;
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) || view.response.secondary_clicked() {
            self.comment_tools.mode = None;
            self.comment_tools.drag = None;
            return;
        }
        if self.comment_tools.editor.is_some() {
            return;
        }
        if self.comment_tools.mode == Some(Mode::Place) {
            if let Some(p) = view.clicked {
                self.open_map_comment(None, view.world(p));
            }
            return;
        }
        if view.response.double_clicked() {
            if let Some(p) = view.response.interact_pointer_pos() {
                if let Some(i) = map_comments::pick(&self.comments, view.world(p), 1. / view.scale)
                {
                    let note = &self.comments[i];
                    self.open_map_comment(Some(i), [note.x, note.y]);
                    return;
                }
            }
        }
        if self.comment_tools.mode != Some(Mode::Move) {
            return;
        }
        if view.response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(p) = ctx.input(|i| i.pointer.press_origin()) {
                let world = view.world(p);
                if let Some(target) = map_comments::pick(&self.comments, world, 1. / view.scale)
                    .and_then(|i| self.comment_target(i))
                {
                    let point = [target.original.x, target.original.y];
                    self.comment_tools.drag = Some(Drag {
                        target,
                        offset: [point[0] - world[0], point[1] - world[1]],
                        point,
                    });
                }
            }
        }
        if let Some(mut drag) = self.comment_tools.drag.take() {
            if !drag.target.valid(self) {
                self.message = "De opmerking is gewijzigd; kies deze opnieuw.".into();
                return;
            }
            if let Some(p) = ctx.input(|i| i.pointer.interact_pos()) {
                let world = view.world(p);
                drag.point = [world[0] + drag.offset[0], world[1] + drag.offset[1]];
            }
            let preview = view.project(drag.point);
            view.painter
                .circle_stroke(preview, 9., Stroke::new(2.0_f32, ORANGE));
            view.painter.text(
                preview + egui::vec2(12., -12.),
                egui::Align2::LEFT_BOTTOM,
                &drag.target.original.text,
                egui::FontId::proportional(13.),
                INK,
            );
            if ctx.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
                if ctx
                    .input(|i| i.pointer.interact_pos())
                    .is_some_and(|p| view.rect.contains(p))
                {
                    match map_comments::edit(
                        &mut self.comments,
                        Some(drag.target.index),
                        drag.point,
                        &drag.target.original.text,
                    ) {
                        Ok(changed) => self.dirty |= changed,
                        Err(e) => self.message = e,
                    }
                }
            } else if ctx.input(|i| i.pointer.primary_down()) {
                self.comment_tools.drag = Some(drag);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> SleufBaseApp {
        SleufBaseApp {
            comments: vec![Comment {
                x: 155000.,
                y: 463000.,
                text: "Eerste\nTweede".into(),
            }],
            navigation: location_ui::Navigation {
                focus: Some(location_ui::Focus {
                    center: [155000., 463000.],
                    meters_per_pixel: 0.1,
                }),
                ..Default::default()
            },
            ..Default::default()
        }
    }
    fn frame(
        app: &mut SleufBaseApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> raster_point_ui::View {
        let mut result = None;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(900., 700.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    app.navigation.pointer_tool = app.comment_tools.captures();
                    let view = canvas(
                        ui,
                        &[],
                        &[],
                        &[],
                        &[],
                        &[],
                        &[],
                        &[],
                        None,
                        &app.comments,
                        None,
                        None,
                        false,
                        &mut app.zoom,
                        &mut app.pan,
                        &app.migrated_settings.object_rules,
                        Some(&mut app.navigation),
                    )
                    .unwrap();
                    app.handle_map_comments(&view);
                    result = Some(view);
                });
            },
        );
        result.unwrap()
    }
    fn button(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }
    #[test]
    fn actual_pointer_drag_previews_without_panning_then_commits_only_on_release() {
        let mut app = app();
        let ctx = egui::Context::default();
        app.start_comment_mode(Mode::Move);
        let view = frame(&mut app, &ctx, vec![]);
        let p = view.project([155000., 463000.]);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(p), button(p, true)],
        );
        let q = p + egui::vec2(45., -30.);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(q)]);
        assert!(app.comment_tools.drag.is_some());
        assert_eq!([app.comments[0].x, app.comments[0].y], [155000., 463000.]);
        assert!(!app.dirty);
        assert_eq!(app.pan, egui::Vec2::ZERO);
        frame(&mut app, &ctx, vec![button(q, false)]);
        assert_eq!([app.comments[0].x, app.comments[0].y], [155004.5, 463003.]);
        assert!(app.dirty);
        assert_eq!(app.comments[0].text, "Eerste\nTweede");
        assert!(app.comment_tools.drag.is_none());
    }
    #[test]
    fn escape_and_release_outside_canvas_discard_drag_preview() {
        for escape in [false, true] {
            let mut app = app();
            let ctx = egui::Context::default();
            app.start_comment_mode(Mode::Move);
            let view = frame(&mut app, &ctx, vec![]);
            let p = view.project([155000., 463000.]);
            frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerMoved(p), button(p, true)],
            );
            frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerMoved(p + egui::vec2(40., 25.))],
            );
            if escape {
                frame(
                    &mut app,
                    &ctx,
                    vec![egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Default::default(),
                    }],
                );
            }
            frame(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(Pos2::new(-10., -10.)),
                    button(Pos2::new(-10., -10.), false),
                ],
            );
            assert_eq!([app.comments[0].x, app.comments[0].y], [155000., 463000.]);
            assert!(!app.dirty);
            assert!(app.comment_tools.drag.is_none());
        }
    }
    #[test]
    fn actual_map_click_opens_multiline_editor_and_stale_edit_never_replaces_shifted_comment() {
        let mut app = app();
        let ctx = egui::Context::default();
        app.start_comment_mode(Mode::Place);
        let p = frame(&mut app, &ctx, vec![]).project([155002., 463001.]);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(p), button(p, true)],
        );
        frame(&mut app, &ctx, vec![button(p, false)]);
        assert!(app.comment_tools.editor.is_some());
        assert_eq!(app.comments.len(), 1);
        assert!(!app.dirty);
        let editor = app.comment_tools.editor.as_mut().unwrap();
        editor.text = "  Nieuwe\nregel\u{001c}".into();
        assert!(app.submit_map_comment());
        assert_eq!(app.comments.len(), 2);
        assert_eq!(app.comments[1].text, "Nieuwe\nregel");
        app.open_map_comment(Some(1), [155002., 463001.]);
        app.comments.remove(0);
        app.dirty = false;
        assert!(!app.submit_map_comment());
        assert_eq!(app.comments.len(), 1);
        assert_eq!(app.comments[0].text, "Nieuwe\nregel");
        assert!(!app.dirty);
    }
    #[test]
    fn editor_cancel_and_invalid_text_never_change_project_or_identity() {
        let mut app = app();
        let ctx = egui::Context::default();
        let name = app.project_name.clone();
        app.open_map_comment(Some(0), [155000., 463000.]);
        app.comment_tools.editor.as_mut().unwrap().text = " ".into();
        assert!(!app.submit_map_comment());
        assert!(!app.dirty);
        assert!(!app.comment_tools.editor.as_ref().unwrap().error.is_empty());
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }],
                ..Default::default()
            },
            |ctx| app.show_map_comment_editor(ctx),
        );
        assert!(app.comment_tools.editor.is_none());
        assert!(!app.dirty);
        assert_eq!(app.comments[0].text, "Eerste\nTweede");
        assert_eq!(app.project_name, name);
    }
}
