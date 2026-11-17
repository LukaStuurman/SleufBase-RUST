//! Responsive overview and cable entry for the staged trench editor.
use super::*;

#[cfg(test)]
#[path = "ui_trench_workspace_tests.rs"]
mod tests;

impl Editor {
    pub(super) fn show_entry_actions(&mut self, ui: &mut egui::Ui) {
        let can_store = self.prepared_entry.is_none() || self.entry_changed();
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(can_store, egui::Button::new(if self.selected.is_some() {
                "Wijzig kabel/leiding"
            } else {
                "Kabel/leiding toevoegen"
            })).on_hover_text("Bewaar deze invoer in de proefsleuf. Met Opslaan in project bewaar je daarna de hele proefsleuf.").clicked() {
                match self.add_or_replace() {
                    Ok(()) => {
                        self.error.clear();
                        self.notice = "Kabel/leiding toegevoegd aan het concept. Voeg er nog een toe of klik op Opslaan in project.".into();
                    }
                    Err(error) => self.error = error,
                }
            }
            if ui.add_enabled(can_store, egui::Button::new(if self.selected.is_some() {
                "Bewaren en herhalen"
            } else {
                "Toevoegen en herhalen"
            })).on_hover_text("Bewaar deze kabel of leiding en zet dezelfde eigenschappen klaar voor de volgende. Kies daarna een nieuwe positie.").clicked() {
                if let Err(error) = self.add_and_repeat() { self.error = error; }
            }
            let can_reset = self.entry_changed() || self.prepared_entry.is_some();
            if ui.add_enabled(can_reset, egui::Button::new(if self.selected.is_some() {
                "Wijzigingen herstellen"
            } else {
                "Invoer wissen"
            })).on_hover_text("Herstel alleen de huidige invoer. Eerder toegevoegde kabels en leidingen blijven in het concept.").clicked() {
                self.reset_entry();
            }
        });
    }

    pub(super) fn show_objects_workspace(
        &mut self,
        ui: &mut egui::Ui,
        rules: &[sleufbase_rust::Rule],
        materials: &[String],
        extras: &[String],
    ) -> bool {
        let mut pick = false;
        if ui.available_width() >= 820. {
            ui.columns(2, |columns| {
                self.show_object_overview(&mut columns[0], true, rules);
                pick = self.show_entry_panel(&mut columns[1], rules, materials, extras);
            });
        } else {
            self.show_object_overview(ui, false, rules);
            ui.add_space(6.);
            pick = self.show_entry_panel(ui, rules, materials, extras);
            if !self.draft.objects.is_empty() {
                ui.add_space(6.);
                theme::card(10, 10, 10).show(ui, |ui| {
                    ui.strong(format!("Kabels en leidingen ({})", self.draft.objects.len()));
                    self.show_object_list(ui, rules);
                });
            }
        }
        pick
    }

    fn show_object_overview(&mut self, ui: &mut egui::Ui, show_list: bool, rules: &[sleufbase_rust::Rule]) {
        theme::card(10, 10, 10).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.strong("1. Plaats in de doorsnede");
            self.show_section(ui, rules);
            if !show_list { return; }
            ui.add_space(6.);
            ui.horizontal_wrapped(|ui| {
                ui.strong("Kabels en leidingen");
                ui.weak(format!("{} in deze proefsleuf", self.draft.objects.len()));
            });
            if self.draft.objects.is_empty() {
                ui.weak("Nog leeg. Klik een plek in de doorsnede en kies de eigenschappen van de eerste kabel of leiding.");
            } else {
                self.show_object_list(ui, rules);
            }
        });
    }

    fn show_entry_panel(
        &mut self,
        ui: &mut egui::Ui,
        rules: &[sleufbase_rust::Rule],
        materials: &[String],
        extras: &[String],
    ) -> bool {
        theme::card(10, 12, 10).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.strong("2. Kies de eigenschappen");
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(if self.selected.is_some() {
                    "Kabel of leiding bewerken"
                } else {
                    "Nieuwe kabel of leiding"
                }).strong().color(DARK));
                if let Some(index) = self.selected {
                    ui.weak(format!("Nr. {}", index + 1));
                } else if self.prepared_entry.is_some() && !self.entry_changed() {
                    ui.weak("Kies nog een positie");
                }
            });
            ui.horizontal_wrapped(|ui| {
                if self.selected.is_some() && ui.button("Nieuwe kabel/leiding").clicked() {
                    match self.finish_entry() {
                        Ok(()) => self.error.clear(),
                        Err(error) => self.error = error,
                    }
                }
                let can_repeat = self.selected.is_some() || self.entry_changed() || self.last_entry.is_some();
                if ui.add_enabled(can_repeat, egui::Button::new("Zelfde type toevoegen"))
                    .on_hover_text("Bewaar de huidige invoer en neem type, materiaal, diameter, aantal en kruishoek over voor een volgende kabel. Kies daarna een nieuwe plek.")
                    .clicked()
                {
                    if let Err(error) = self.prepare_similar_entry() {
                        self.error = error;
                    }
                }
            });
            ui.separator();
            object_ui::show(self, ui, rules, materials, extras)
        }).inner
    }

    fn show_object_list(&mut self, ui: &mut egui::Ui, rules: &[sleufbase_rust::Rule]) {
        let mut remove = None;
        let mut select = None;
        egui::ScrollArea::vertical()
            .id_salt("trench-object-list")
            .max_height(164.)
            .auto_shrink([false, false])
            .show_rows(ui, 54., self.draft.objects.len(), |ui, rows| {
                for index in rows {
                    let object = &self.draft.objects[index];
                    let (along, _) = edit::coordinates(&self.draft, object.position);
                    let depth = object.metadata.get("sleufbase_depth_meters")
                        .and_then(serde_json::Value::as_f64)
                        .or_else(|| self.start.z.zip(object.position.z).map(|(ground, z)| ground - z));
                    let diameter = object_diameter(object);
                    let diameter = if diameter > 0. { format!("Ø{diameter} mm") } else { "Ø onbekend".into() };
                    let title = format!("{}  {} · {} · {}×", index + 1, object.object_name, diameter, object_quantity(object));
                    let detail = format!("{along:.2} m vanaf begin · diepte {}", depth.map(|d| format!("{d:.2} m")).unwrap_or_else(|| "onbekend".into())).replace('.', ",");
                    ui.push_id(index, |ui| {
                        ui.horizontal(|ui| {
                            let color = utility_color(&object.source_name,
                                sleufbase_rust::virtual_raster::display_rgb(&object.metadata), rules);
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(10., 20.), egui::Sense::hover());
                            ui.painter().circle_filled(rect.center(), 4., color);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("×").on_hover_text("Verwijder deze kabel/leiding uit het concept").clicked() {
                                    remove = Some(index);
                                }
                                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                                    if ui.selectable_label(self.selected == Some(index), &title).on_hover_text(&title).clicked() {
                                        select = Some(index);
                                    }
                                });
                            });
                        });
                        ui.add(egui::Label::new(egui::RichText::new(detail).small().weak()).truncate());
                    });
                }
            });
        if let Some(index) = select {
            self.select_object(index);
        }
        if let Some(index) = remove {
            match edit::remove_object(&mut self.draft, index) {
                Ok(()) if self.selected == Some(index) => {
                    self.selected = None;
                    self.object = ObjectForm::default();
                    self.prepared_entry = None;
                    self.entry_started = false;
                    self.section_drag = None;
                }
                Ok(()) => {
                    self.selected = self.selected.map(|selected| selected - usize::from(index < selected));
                }
                Err(error) => self.error = error,
            }
        }
    }
}
