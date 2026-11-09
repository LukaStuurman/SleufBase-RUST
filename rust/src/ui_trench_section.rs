//! Interactive longitudinal section. Horizontal position is chainage; the
//! crossing angle describes direction in plan, independently of depth.
use super::*;

const SURFACE: Color32 = Color32::from_rgb(119, 89, 59);
const CABLE: Color32 = Color32::from_rgb(37, 99, 235);

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
        (diameter_mm as f32 / 2000. * self.plot.height() / self.max_depth as f32).clamp(4., 24.)
    }
}

pub(super) struct Drag {
    section: Section,
    start: Pos2,
    grab: Vec2,
}

impl Drag {
    fn apply(&self, pointer: Pos2, form: &mut ObjectForm) {
        let (along, depth) = self.section.coordinates(pointer - self.grab);
        if (pointer.x - self.start.x).abs() > 0.5 {
            form.chainage = along;
        }
        if (pointer.y - self.start.y).abs() > 0.5 {
            form.use_depth = true;
            form.depth = depth;
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
    pub(super) fn show_section(&mut self, ui: &mut egui::Ui) {
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
        let position = if active {
            Some((self.object.chainage, active_depth))
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
            format!(
                "{} m vanaf begin   ·   Diepte {depth}",
                format!("{along:.2}").replace('.', ",")
            )
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
            let rgb = sleufbase_rust::virtual_raster::display_rgb(&object.metadata);
            let color = rgb
                .map(|[r, g, b]| Color32::from_rgb(r, g, b))
                .unwrap_or(CABLE);
            let radius = section.radius(object_diameter(object));
            painter.circle_filled(point, radius, color);
            if let Some(pos) = hit_position {
                let distance = pos.distance(point);
                if distance <= radius.max(9.)
                    && hit.is_none_or(|(nearest, _, _)| distance <= nearest)
                {
                    hit = Some((distance, Some(index), point));
                }
            }
        }

        if active {
            let point = section.point(self.object.chainage, active_depth);
            let radius = section.radius(self.object.diameter);
            let rgb = self.object.rgb;
            painter.circle_filled(
                point,
                radius,
                rgb.map(|[r, g, b]| Color32::from_rgb(r, g, b))
                    .unwrap_or(CABLE),
            );
            painter.circle_stroke(point, radius + 2., Stroke::new(2.0_f32, ORANGE));
            let diameter = if self.object.diameter > 0. {
                format!("Ø {} mm", self.object.diameter)
            } else {
                "Ø onbekend".into()
            };
            let left = point.x > plot.center().x;
            painter.text(
                point + egui::vec2(if left { -radius - 5. } else { radius + 5. }, -radius - 2.),
                if left {
                    egui::Align2::RIGHT_BOTTOM
                } else {
                    egui::Align2::LEFT_BOTTOM
                },
                diameter,
                font,
                INK,
            );
            if let Some(pos) = hit_position {
                let distance = pos.distance(point);
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
                    self.section_drag = Some(Drag {
                        section,
                        start,
                        grab: start - point,
                    });
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
        let drag = Drag {
            section,
            start,
            grab: Vec2::ZERO,
        };
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
}
