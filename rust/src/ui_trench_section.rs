//! Interactive longitudinal section. Horizontal position is chainage; the
//! crossing angle describes direction in plan, independently of depth.
use super::*;

const SURFACE: Color32 = Color32::from_rgb(119, 89, 59);

#[derive(Clone, Copy)]
struct Section {
    plot: egui::Rect,
    length: f64,
    max_depth: f64,
}

impl Section {
    fn point(self, chainage: f64, depth: Option<f64>) -> Pos2 {
        let x =
            self.plot.left() + (chainage / self.length).clamp(0., 1.) as f32 * self.plot.width();
        let y = match depth {
            Some(depth) => {
                self.plot.top() + (depth / self.max_depth).clamp(0., 1.) as f32 * self.plot.height()
            }
            None => self.plot.top() - 16.,
        };
        egui::pos2(x, y)
    }

    fn coordinates(self, point: Pos2) -> (f64, f64) {
        let along = f64::from((point.x - self.plot.left()) / self.plot.width()).clamp(0., 1.);
        let depth = f64::from((point.y - self.plot.top()) / self.plot.height()).clamp(0., 1.);
        (along * self.length, depth * self.max_depth)
    }

    fn radius(self, diameter_mm: f64) -> f32 {
        // Keep small cables selectable and very large pipes inside the panel.
        (diameter_mm as f32 / 2000. * self.plot.height() / self.max_depth as f32).clamp(4., 32.)
    }
}

pub(super) struct Drag {
    section: Section,
    start: Pos2,
    grab: Vec2,
    initial_chainage: f64,
    initial_use_depth: bool,
    initial_depth: f64,
}

impl Drag {
    fn new(section: Section, start: Pos2, grab: Vec2, form: &ObjectForm) -> Self {
        Self {
            section,
            start,
            grab,
            initial_chainage: form.chainage,
            initial_use_depth: form.use_depth,
            initial_depth: form.depth,
        }
    }

    fn apply(&self, pointer: Pos2, form: &mut ObjectForm) {
        let (along, depth) = self.section.coordinates(pointer - self.grab);
        form.chainage = if (pointer.x - self.start.x).abs() > 0.5 {
            along
        } else {
            self.initial_chainage
        };
        if (pointer.y - self.start.y).abs() > 0.5 {
            form.use_depth = true;
            form.depth = depth;
        } else {
            form.use_depth = self.initial_use_depth;
            form.depth = self.initial_depth;
        }
    }
}

fn measured_depth(
    metadata: &serde_json::Value,
    ground: Option<f64>,
    z: Option<f64>,
) -> Option<f64> {
    metadata
        .get("sleufbase_depth_meters")
        .and_then(serde_json::Value::as_f64)
        .filter(|depth| depth.is_finite() && *depth >= 0.)
        .or_else(|| {
            ground
                .zip(z)
                .map(|(ground, z)| ground - z)
                .filter(|depth| depth.is_finite())
        })
}

impl Editor {
    pub(super) fn show_section(&mut self, ui: &mut egui::Ui, rules: &[sleufbase_rust::Rule]) {
        ui.small(if self.selected.is_some() {
            "Sleep een object om de afstand en diepte te wijzigen. Kies Nieuwe kabel/leiding om er een bij te plaatsen."
        } else {
            "Klik in de doorsnede om een nieuwe kabel of leiding te plaatsen. Sleep bestaande objecten om ze te verplaatsen."
        });
        let height = (ui.ctx().screen_rect().height() * 0.28).clamp(105., 240.);
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().max(100.), height),
            egui::Sense::click_and_drag(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 6., Color32::from_rgb(248, 250, 252));
        let length = self.draft.length();
        if length <= 1e-9 {
            return;
        }

        let ground = self.start.z;
        let active_depth = if self.object.use_depth {
            Some(self.object.depth)
        } else {
            let z = self.object.has_elevation.then_some(self.object.elevation);
            let original = self.selected.map(|i| &self.draft.objects[i]);
            original
                .filter(|_| !self.entry_changed())
                .and_then(|object| measured_depth(&object.metadata, ground, z))
                .or_else(|| ground.zip(z).map(|(a, b)| a - b))
        };
        let mut max_depth = 1.5_f64.max(active_depth.unwrap_or(0.) * 1.25);
        let stride = self.draft.objects.len().div_ceil(2048).max(1);
        let objects: Vec<_> = self
            .draft
            .objects
            .iter()
            .enumerate()
            .step_by(stride)
            .filter(|(index, _)| self.selected != Some(*index))
            .map(|(index, object)| {
                let (along, _) = edit::coordinates(&self.draft, object.position);
                let depth = measured_depth(&object.metadata, ground, object.position.z);
                max_depth = max_depth.max(depth.unwrap_or(0.) * 1.25);
                (index, along, depth, object)
            })
            .collect();
        let section = self
            .section_drag
            .as_ref()
            .map(|drag| drag.section)
            .unwrap_or(Section {
                plot: egui::Rect::from_min_max(
                    rect.min + egui::vec2(76., 42.),
                    rect.max - egui::vec2(25., 22.),
                ),
                length,
                max_depth: max_depth.clamp(1.5, 1000.),
            });
        let plot = section.plot;
        if plot.width() <= 0. || plot.height() <= 0. {
            return;
        }
        painter.rect_filled(plot, 0., Color32::from_rgb(239, 230, 218));
        let font = egui::FontId::proportional(11.);
        for (depth, label) in [
            (None, "Onbekend".into()),
            (Some(0.), "Maaiveld".into()),
            (
                Some(section.max_depth / 2.),
                format!("{:.2} m", section.max_depth / 2.),
            ),
            (
                Some(section.max_depth),
                format!("{:.2} m", section.max_depth),
            ),
        ] {
            let y = section.point(0., depth).y;
            painter.line_segment(
                [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
                Stroke::new(if depth == Some(0.) { 2.0_f32 } else { 0.6 }, SURFACE),
            );
            painter.text(
                egui::pos2(plot.left() - 8., y),
                egui::Align2::RIGHT_CENTER,
                label,
                font.clone(),
                INK,
            );
        }
        // Choose a 1/2/5 interval with enough room for labels at this width.
        let desired = (plot.width() / 90.).floor().max(2.) as f64;
        let rough = length / desired;
        let unit = 10_f64.powf(rough.log10().floor());
        let step = [1., 2., 5., 10.]
            .into_iter()
            .map(|factor| factor * unit)
            .find(|step| *step >= rough)
            .unwrap_or(length);
        let decimals = (-step.log10().floor()).clamp(0., 6.) as usize;
        let grid = Color32::from_rgb(202, 185, 163);
        for index in 1..=(length / step).floor().min(40.) as usize {
            let meters = index as f64 * step;
            let x = section.point(meters, Some(0.)).x;
            painter.vline(x, plot.y_range(), Stroke::new(0.7_f32, grid));
            if plot.right() - x > 54. && x - plot.left() > 38. {
                painter.text(
                    egui::pos2(x, plot.bottom() + 4.),
                    egui::Align2::CENTER_TOP,
                    format!("{meters:.decimals$}").replace('.', ","),
                    font.clone(),
                    INK,
                );
            }
        }
        painter.text(
            plot.left_bottom() + egui::vec2(0., 4.),
            egui::Align2::LEFT_TOP,
            "Begin · 0 m",
            font.clone(),
            INK,
        );
        painter.text(
            plot.right_bottom() + egui::vec2(0., 4.),
            egui::Align2::RIGHT_TOP,
            format!("Eind · {length:.2} m").replace('.', ","),
            font.clone(),
            INK,
        );

        let active = self.selected.is_some() || self.entry_changed();
        // The actual depth is never invented in the stored measurement.
        // Until the user supplies a depth, show a clearly labelled VISUAL
        // diameter example within the plot rather than hiding it above the
        // "Onbekend" annotation at plot.top() - 16.
        let illustrative = active && active_depth.is_none();
        let display_depth = if illustrative {
            Some(self.object.depth.clamp(0.1, section.max_depth * 0.75))
        } else { active_depth };
        let display_chainage = if illustrative && self.selected.is_none()
            && !self.entry_started && self.object.chainage == 0.
        { length * 0.5 } else { self.object.chainage };
        let position = if active {
            Some((display_chainage, display_depth))
        } else {
            response
                .hover_pos()
                .filter(|point| plot.contains(*point))
                .map(|point| {
                    let (along, depth) = section.coordinates(point);
                    (along, Some(depth))
                })
        };
        let readout = if let Some((along, depth)) = position {
            let x = section.point(along, Some(0.)).x;
            painter.vline(x, plot.y_range(), Stroke::new(1.5_f32, ORANGE));
            let depth = depth
                .map(|depth| format!("{depth:.2} m").replace('.', ","))
                .unwrap_or_else(|| "onbekend".into());
            if illustrative {
                format!("Diameter voorbeeld: Ø {} mm · diepte nog niet opgegeven",
                    format!("{:.1}", self.object.diameter).replace('.', ","))
            } else {
                format!(
                    "{} m vanaf begin   ·   Diepte {depth}",
                    format!("{along:.2}").replace('.', ",")
                )
            }
        } else if stride > 1 {
            "Afstand vanaf begin · alle objecten staan in de objectenlijst".into()
        } else {
            "Afstand vanaf beginpunt (m)".into()
        };
        painter.text(
            rect.left_top() + egui::vec2(10., 6.),
            egui::Align2::LEFT_TOP,
            readout,
            egui::FontId::proportional(12.),
            INK,
        );

        let origin = ui.input(|input| input.pointer.press_origin());
        let pointer = response.interact_pointer_pos();
        let hit_position = if response.drag_started() {
            origin
        } else {
            pointer
        };
        let mut hit: Option<(f32, Option<usize>, Pos2)> = None;
        for (index, along, depth, object) in objects {
            let point = section.point(along, depth);
            let color = utility_color(&object.source_name,
                sleufbase_rust::virtual_raster::display_rgb(&object.metadata), rules);
            let radius = section.radius(object_diameter(object));
            painter.circle_filled(point, radius, color);
            let label_beside = depth.is_none() || point.y - radius - 18. < plot.top();
            let label_left = point.x > plot.center().x;
            let number_rect = (self.draft.objects.len() <= 24).then(|| painter.text(
                if label_beside { point + egui::vec2(if label_left { -radius - 4. } else { radius + 4. }, 0.) }
                else { point - egui::vec2(0., radius + 3.) },
                if label_beside && label_left { egui::Align2::RIGHT_CENTER }
                else if label_beside { egui::Align2::LEFT_CENTER }
                else { egui::Align2::CENTER_BOTTOM },
                (index + 1).to_string(),
                font.clone(),
                INK,
            ));
            if let Some(pos) = hit_position {
                // A number belongs to its cable: clicking its label must never
                // start an unrelated new entry in the empty space above it.
                let distance = if number_rect.is_some_and(|rect| rect.expand(2.).contains(pos)) {
                    0.
                } else { pos.distance(point) };
                if distance <= radius.max(9.)
                    && hit.is_none_or(|(nearest, _, _)| distance <= nearest)
                {
                    hit = Some((distance, Some(index), point));
                }
            }
        }

        if active {
            let point = section.point(display_chainage, display_depth);
            let radius = section.radius(self.object.diameter);
            painter.circle_filled(
                point,
                radius,
                utility_color(&self.object.source, self.object.rgb, rules),
            );
            painter.circle_stroke(point, radius + 2., Stroke::new(2.0_f32, ORANGE));
            let diameter = if self.object.diameter > 0. {
                format!("Ø {} mm", self.object.diameter)
            } else {
                "Ø onbekend".into()
            };
            let left = point.x > plot.center().x;
            let label = if illustrative {
                format!("Voorbeeld · {diameter}")
            } else {
                match self.selected {
                    Some(index) => format!("{} · {diameter}", index + 1),
                    None => format!("Nieuw · {diameter}"),
                }
            };
            let label_beside = illustrative || point.y - radius - 18. < plot.top();
            let label_rect = painter.text(
                point + egui::vec2(if left { -radius - 5. } else { radius + 5. },
                    if label_beside { 0. } else { -radius - 2. }),
                if label_beside && left { egui::Align2::RIGHT_CENTER }
                else if label_beside { egui::Align2::LEFT_CENTER }
                else if left {
                    egui::Align2::RIGHT_BOTTOM
                } else {
                    egui::Align2::LEFT_BOTTOM
                },
                label,
                font,
                INK,
            );
            if let Some(pos) = hit_position {
                let distance = if label_rect.expand(2.).contains(pos) { 0. } else { pos.distance(point) };
                if distance <= radius.max(9.)
                    && hit.is_none_or(|(nearest, _, _)| distance <= nearest)
                {
                    hit = Some((distance, self.selected, point));
                }
            }
        }

        if response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(((_, index, point), start)) = hit.zip(origin) {
                if index.is_none_or(|index| self.select_object(index)) {
                    self.section_drag = Some(Drag::new(section, start, start - point, &self.object));
                }
            }
        }
        if let Some((drag, pointer)) = self.section_drag.as_ref().zip(pointer) {
            drag.apply(pointer, &mut self.object);
        }
        if response.drag_stopped() || !ui.input(|input| input.pointer.primary_down()) {
            self.section_drag = None;
        }
        if response.clicked_by(egui::PointerButton::Primary) {
            if let Some((_, index, _)) = hit {
                if let Some(index) = index {
                    self.select_object(index);
                }
            } else if let Some(pos) =
                pointer.filter(|pos| self.selected.is_none() && plot.contains(*pos))
            {
                let (along, depth) = section.coordinates(pos);
                self.object.chainage = along;
                self.object.use_depth = true;
                self.object.depth = depth;
                self.entry_started = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dragging_uses_meters_and_retains_existing_fields() {
        let section = Section {
            plot: egui::Rect::from_min_size(egui::pos2(10., 50.), egui::vec2(400., 160.)),
            length: 8.,
            max_depth: 2.,
        };
        let start = section.point(2., Some(0.5));
        let mut form = ObjectForm {
            chainage: 2.,
            angle: -35.,
            diameter: 125.,
            offset: 0.2,
            ..Default::default()
        };
        let drag = Drag::new(section, start, Vec2::ZERO, &form);
        drag.apply(section.point(5., Some(0.5)), &mut form);
        assert_eq!(form.chainage, 5.);
        assert!(!form.use_depth);
        drag.apply(section.point(5., Some(1.25)), &mut form);
        assert!(form.use_depth);
        assert_eq!(
            (form.depth, form.angle, form.diameter, form.offset),
            (1.25, -35., 125., 0.2)
        );
    }

    #[test]
    fn returning_each_axis_restores_its_initial_value_and_height_mode() {
        let section = Section {
            plot: egui::Rect::from_min_size(egui::pos2(10., 50.), egui::vec2(400., 160.)),
            length: 8.,
            max_depth: 2.,
        };
        let original = ObjectForm {
            chainage: 2.,
            depth: 0.7,
            has_elevation: true,
            elevation: 1.5,
            offset: 0.2,
            angle: -35.,
            ..Default::default()
        };
        let mut form = original.clone();
        let grab = egui::vec2(2., 3.);
        let start = section.point(2., Some(0.5)) + grab;
        let drag = Drag::new(section, start, grab, &form);
        drag.apply(section.point(5., Some(1.25)) + grab, &mut form);
        assert_eq!(form.chainage, 5.);
        assert!(form.use_depth);
        assert_eq!(form.depth, 1.25);
        // Return vertically while retaining the deliberate horizontal move.
        drag.apply(section.point(5., Some(0.5)) + grab, &mut form);
        assert_eq!(form.chainage, 5.);
        assert!(!form.use_depth);
        assert_eq!(form.depth, original.depth);
        assert_eq!(form.elevation, original.elevation);
        // Each axis returns independently, including the small dead zone.
        drag.apply(section.point(2., Some(1.25)) + grab, &mut form);
        assert_eq!(form.chainage, original.chainage);
        assert!(form.use_depth);
        drag.apply(start + egui::vec2(0.25, -0.25), &mut form);
        assert!(form == original);
    }

    #[test]
    fn returning_to_clipped_point_preserves_exact_out_of_plot_values() {
        let section = Section {
            plot: egui::Rect::from_min_size(egui::pos2(10., 50.), egui::vec2(400., 160.)),
            length: 8.,
            max_depth: 2.,
        };
        for (chainage, use_depth, displayed_depth) in [(10., true, 3.), (-2., false, -3.)] {
            let original = ObjectForm {
                chainage,
                use_depth,
                depth: if use_depth { displayed_depth } else { 0.7 },
                has_elevation: true,
                elevation: 2. - displayed_depth,
                ..Default::default()
            };
            let mut form = original.clone();
            let start = section.point(chainage, Some(displayed_depth));
            let drag = Drag::new(section, start, Vec2::ZERO, &form);
            drag.apply(section.point(3., Some(1.)), &mut form);
            assert_eq!(form.chainage, 3.);
            assert!(form.use_depth);
            drag.apply(start, &mut form);
            assert!(form == original, "drawing bounds must not normalize stored values");
        }
    }

    #[test]
    fn round_trip_drag_preserves_manual_measurement_when_saved() {
        let mut trench = VirtualTrench::new(
            "PS12".into(),
            Vertex { x: 155000., y: 463000., z: Some(2.) },
            Vertex { x: 155008., y: 463000., z: Some(2.) },
            0.6,
        ).unwrap();
        trench.objects.push(TrenchPoint {
            role: "kabel".into(),
            object_name: "Gemeten LS".into(),
            source_name: "ls".into(),
            position: Vertex { x: 155002., y: 463000.2, z: Some(1.5) },
            attributes: ["Al".into(), "50".into(), String::new()],
            metadata: serde_json::json!({"point_id":"measurement-17","surface_mode":"manual"}),
        });
        let original = serde_json::to_value(&trench.objects[0]).unwrap();
        let mut editor = Editor::new(0, &trench);
        assert!(editor.select_object(0));
        let section = Section {
            plot: egui::Rect::from_min_size(egui::pos2(10., 50.), egui::vec2(400., 160.)),
            length: 8.,
            max_depth: 2.,
        };
        let start = section.point(2., Some(0.5));
        let drag = Drag::new(section, start, Vec2::ZERO, &editor.object);
        drag.apply(section.point(5., Some(1.25)), &mut editor.object);
        assert!(editor.entry_changed());
        drag.apply(start, &mut editor.object);
        assert!(!editor.entry_changed());
        let saved = editor.completed_draft().unwrap();
        assert_eq!(saved.objects.len(), 1);
        assert_eq!(serde_json::to_value(&saved.objects[0]).unwrap(), original);
    }
}

#[cfg(test)]
mod diameter_display_tests {
    use super::*;
    #[test]
    fn larger_diameter_has_larger_screen_radius_until_plot_cap() {
        let section=Section { plot: egui::Rect::from_min_size(
            egui::pos2(0., 0.), egui::vec2(550., 180.)),
            length:7.,max_depth:1.5 };
        assert!(section.radius(200.) > section.radius(50.));
        assert!(section.radius(400.) > section.radius(200.));
        assert_eq!(section.radius(10000.), 32.);
    }
}
