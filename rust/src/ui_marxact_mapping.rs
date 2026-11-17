//! Review imported measurement names before publishing any project changes.
use super::*;
use sleufbase_rust::{file_loader::{FileContent, FileOutcome}, marxact_names as names};
use std::collections::BTreeMap;

pub(super) struct Dialog {
    generation: u64,
    outcomes: Vec<FileOutcome>,
    choices: Vec<names::Choice>,
    rows: Vec<names::NameRow>,
    remember: bool,
    error: String,
    trench_count: usize,
    failed_files: usize,
}

impl SleufBaseApp {
    pub(super) fn marxact_import_settings(&self) -> Result<MigratedSettings, String> {
        let mut settings = self.project_export_settings()?;
        // Remembered aliases are shared between imports, including projects
        // with an older settings snapshot. Project rules still define choices.
        names::merge(&mut settings.marxact_name_mappings,
            &self.migrated_settings.marxact_name_mappings);
        Ok(settings)
    }

    pub(super) fn prepare_marxact_mapping(&mut self, outcomes: Vec<FileOutcome>, ctx: &egui::Context) {
        let settings = match self.marxact_import_settings() {
            Ok(settings) => settings,
            Err(error) => { self.message = format!("marXact niet geïmporteerd: {error}"); return; }
        };
        let choices = names::choices(&settings);
        let trenches = outcomes.iter().filter_map(|outcome| match &outcome.result {
            Ok(FileContent::MarXact(prepared)) => Some(prepared.result.trenches.as_slice()),
            _ => None,
        }).flatten().collect::<Vec<_>>();
        let trench_count = trenches.len();
        let rows = names::collect(trenches.iter().flat_map(|trench| trench.objects.iter()), &settings, &choices);
        // Every successful survey import is reviewed, even when all names
        // are remembered or the file contains only measured trench borders.
        if trench_count == 0 {
            self.apply_file_outcomes(file_loading::LoadPurpose::MarXact, outcomes, ctx);
            return;
        }
        let failed_files = outcomes.iter().filter(|outcome| outcome.result.is_err()).count();
        self.marxact_mapping = Some(Dialog {
            generation: self.project_generation, outcomes, choices, rows,
            remember: true, error: String::new(), trench_count, failed_files,
        });
        self.message = "Koppel de marXact-namen voordat je de metingen importeert.".into();
        ctx.request_repaint();
    }

    fn confirm_marxact_mapping_with(&mut self, ctx: &egui::Context,
        save: &mut impl FnMut(&MigratedSettings, &AutosaveSettings) -> Result<(), String>) -> bool {
        let Some(mut dialog) = self.marxact_mapping.take() else { return false; };
        if dialog.generation != self.project_generation {
            self.message = "Project gewijzigd; de vorige marXact-import is geannuleerd.".into();
            return false;
        }
        let mut mappings = BTreeMap::new();
        for row in &dialog.rows {
            let Some(target) = row.target.as_ref().filter(|target|
                target.as_str() == names::IGNORE || dialog.choices.iter().any(|choice| &choice.code == *target)) else {
                dialog.error = "Kies voor iedere naam een kabel-/leidingtype of Negeren.".into();
                self.marxact_mapping = Some(dialog);
                return false;
            };
            mappings.insert(row.key.clone(), target.clone());
        }
        let remember = dialog.remember && !mappings.is_empty();
        if remember {
            let mut settings = self.migrated_settings.clone();
            names::merge(&mut settings.marxact_name_mappings, &mappings);
            if let Err(error) = settings.validate().and_then(|()| save(&settings, &self.backup_settings)) {
                dialog.error = format!("Koppelingen niet opgeslagen: {error}. Je keuzes blijven staan. Zet onthouden uit om alleen deze import uit te voeren.");
                self.marxact_mapping = Some(dialog);
                return false;
            }
            self.migrated_settings = settings;
            if let Some(settings) = self.project_settings.as_mut() {
                names::merge(&mut settings.marxact_name_mappings, &mappings);
            }
        }
        let mut ignored = 0;
        for outcome in &mut dialog.outcomes {
            if let Ok(FileContent::MarXact(prepared)) = &mut outcome.result {
                let before = prepared.result.trenches.iter().map(|trench| trench.objects.len()).sum::<usize>();
                names::apply(&mut prepared.result.trenches, &mappings);
                let after = prepared.result.trenches.iter().map(|trench| trench.objects.len()).sum::<usize>();
                ignored += before - after;
                // Marker indices refer to the retained objects. Keep the
                // measured boundaries/axis from the unfiltered import intact.
                prepared.markers = prepared.result.trenches.iter()
                    .map(sleufbase_rust::cross_section::markers).collect();
            }
        }
        self.apply_file_outcomes(file_loading::LoadPurpose::MarXact, dialog.outcomes, ctx);
        if ignored > 0 { self.message.push_str(&format!(" {ignored} meetobjecten genegeerd.")); }
        if remember { self.message.push_str(" Naamkoppelingen onthouden voor volgende imports."); }
        true
    }

    pub(super) fn show_marxact_mapping(&mut self, ctx: &egui::Context) {
        if self.marxact_mapping.as_ref().is_some_and(|dialog| dialog.generation != self.project_generation) {
            self.marxact_mapping = None;
            self.message = "Project gewijzigd; de vorige marXact-import is geannuleerd.".into();
            return;
        }
        self.paint_busy_backdrop(ctx);
        let Some(dialog) = self.marxact_mapping.as_mut() else { return; };
        let mut open = true;
        let mut confirm = false;
        let mut cancel = ctx.input(|input| input.key_pressed(egui::Key::Escape));
        let pending = dialog.rows.iter().filter(|row| row.target.is_none()).count();
        egui::Window::new("marXact: kabels en leidingen koppelen")
            .id(egui::Id::new("marxact-name-mapping")).open(&mut open)
            .collapsible(false).resizable(true)
            .default_width((ctx.screen_rect().width() - 48.).clamp(340., 760.))
            .default_height((ctx.screen_rect().height() - 64.).clamp(260., 620.))
            .min_width(320.).min_height(240.).constrain_to(ctx.screen_rect().shrink(12.))
            .show(ctx, |ui| {
                settings_ui::control_style(ui);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                ui.spacing_mut().item_spacing = egui::vec2(8., 6.);
                egui::TopBottomPanel::bottom("marxact-name-actions").show_inside(ui, |ui| {
                    ui.checkbox(&mut dialog.remember, "Deze koppelingen onthouden voor volgende imports");
                    if !dialog.error.is_empty() { ui.colored_label(Color32::from_rgb(180, 45, 45), &dialog.error); }
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_enabled(pending == 0, egui::Button::new(
                            egui::RichText::new("Importeren").strong().color(Color32::WHITE))
                            .fill(ORANGE).corner_radius(8).min_size(egui::vec2(105., 32.))).clicked() { confirm = true; }
                        if ui.button("Annuleren").clicked() { cancel = true; }
                        if pending > 0 { ui.label(format!("Nog {pending} naam/namen kiezen")); }
                    });
                });
                ui.label(format!("{} proefsleuven · {} meetobjecten · {} verschillende namen",
                    dialog.trench_count, dialog.rows.iter().map(|row| row.count).sum::<usize>(), dialog.rows.len()));
                ui.label("Kies welk bekend kabel- of leidingtype bij elke marXact-naam hoort. Opgeslagen keuzes staan al ingevuld. Kies Negeren voor meetpunten die geen kabel of leiding zijn.");
                if dialog.rows.is_empty() {
                    ui.label("Er zijn geen kabel- of leidingnamen gevonden. Je kunt de gemeten sleufgrenzen wel importeren.");
                }
                if dialog.failed_files > 0 {
                    ui.label(format!("{} bestand(en) konden niet worden gelezen. De overige metingen kun je wel importeren.", dialog.failed_files));
                }
                ui.add_space(4.);
                let narrow = ui.available_width() < 540.;
                egui::ScrollArea::vertical().id_salt("marxact-name-rows").auto_shrink([false, false])
                    .show(ui, |ui| {
                        for row in &mut dialog.rows {
                            let key = row.key.clone();
                            ui.push_id(key, |ui| {
                                theme::card(10, 10, 8).show(ui, |ui| {
                                    let choice = |ui: &mut egui::Ui, row: &mut names::NameRow| {
                                        let selected = row.target.as_deref().map(|target| {
                                            if target == names::IGNORE { "Negeren" }
                                            else { dialog.choices.iter().find(|choice| choice.code == target)
                                                .map_or(target, |choice| choice.label.as_str()) }
                                        }).unwrap_or("Kies kabel / leiding…").to_owned();
                                        egui::ComboBox::from_id_salt("target").width(ui.available_width().max(150.))
                                            .selected_text(selected).show_ui(ui, |ui| {
                                                ui.selectable_value(&mut row.target, Some(names::IGNORE.into()), "Negeren");
                                                ui.separator();
                                                for choice in &dialog.choices {
                                                    ui.selectable_value(&mut row.target, Some(choice.code.clone()), &choice.label)
                                                        .on_hover_text(&choice.code);
                                                }
                                            });
                                    };
                                    if narrow {
                                        ui.horizontal(|ui| { ui.strong(&row.source); ui.small(format!("{}×", row.count)); });
                                        choice(ui, row);
                                    } else {
                                        ui.columns(2, |columns| {
                                            columns[0].horizontal_wrapped(|ui| {
                                                ui.strong(&row.source); ui.small(format!("{}×", row.count));
                                                if row.remembered { ui.small("onthouden"); }
                                            });
                                            choice(&mut columns[1], row);
                                        });
                                    }
                                });
                            });
                        }
                    });
            });
        if cancel || !open {
            self.marxact_mapping = None;
            self.message = "marXact-import geannuleerd; project en naamkoppelingen zijn behouden.".into();
        } else if confirm {
            self.confirm_marxact_mapping_with(ctx, &mut |settings, autosave|
                settings_store::save_pair(&project::app_data_directory(), settings, autosave));
        }
    }

    #[cfg(test)]
    pub(super) fn accept_test_marxact_mapping(&mut self, ctx: &egui::Context) {
        if let Some(dialog) = self.marxact_mapping.as_mut() {
            dialog.remember = false;
            for row in &mut dialog.rows {
                if row.target.is_none() { row.target = Some(names::IGNORE.into()); }
            }
            assert!(self.confirm_marxact_mapping_with(ctx, &mut |_, _| panic!("test import must not write user settings")));
        }
    }
}

#[cfg(test)]
#[path = "ui_marxact_mapping_tests.rs"]
mod tests;
