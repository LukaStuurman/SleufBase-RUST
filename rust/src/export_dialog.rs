use eframe::egui;
use sleufbase_rust::{object_selection::Selection, virtual_trench::VirtualTrench, Dataset};
pub(crate) struct Dialog {
    pub selection: Selection,
    pub extra_layers: bool,
    pub label_gap: f64,
    point_length: f64,
    generation: u64,
    query: String,
    pub error: String,
}
#[derive(Clone, Copy)]
pub(crate) struct Action {
    pub overlay: bool,
    pub open_jobs: bool,
}
impl Dialog {
    pub fn new(datasets: usize, trenches: usize, label_gap: f64, generation: u64) -> Self {
        Self {
            selection: Selection::all(datasets, trenches),
            extra_layers: false,
            label_gap,
            point_length: 1.,
            generation,
            query: String::new(),
            error: String::new(),
        }
    }
    pub fn options(&self) -> sleufbase_rust::object_dxf::Options {
        sleufbase_rust::object_dxf::Options {
            label_gap: self.label_gap,
            point_half_length: self.point_length * 0.5,
            ..Default::default()
        }
    }
}
pub(crate) fn show(
    ctx: &egui::Context,
    dialog: &mut Option<Dialog>,
    datasets: &[Dataset],
    trenches: &[VirtualTrench],
    generation: u64,
) -> Option<Action> {
    let state = dialog.as_mut()?;
    if state.generation != generation {
        *dialog = None;
        return None;
    }
    // Appended sources start unselected; a project replacement closes the dialog.
    state.selection.datasets.resize(datasets.len(), false);
    state.selection.trenches.resize(trenches.len(), false);
    let mut open = true;
    let mut action = None;
    let mut cancel = false;
    egui::Window::new("KickTheMap DXF-export")
        .open(&mut open)
        .default_width(1160.)
        .default_height(700.)
        .min_width(850.)
        .show(ctx, |ui| {
            ui.label("Kies de bronnen voor DXF-export. Geladen jobs en virtuele proefsleuven kunnen direct worden geëxporteerd.");
            if ui.button("KickTheMap Jobs openen...").clicked() {
                action=Some(Action{overlay:false,open_jobs:true});
            }
            ui.horizontal(|ui| {
                ui.label("Zoeken");
                ui.text_edit_singleline(&mut state.query);
            });
            let query = state.query.trim().to_lowercase();
            ui.horizontal(|ui| {
                if ui.button("Alle bronnen selecteren").clicked() {
                    state.selection.datasets.fill(true);
                    state.selection.trenches.fill(true);
                }
                if ui.button("Selectie wissen").clicked() {
                    state.selection.datasets.fill(false);
                    state.selection.trenches.fill(false);
                }
            });
            egui::ScrollArea::vertical()
                .max_height((ctx.screen_rect().height() - 340.).clamp(100., 360.))
                .show(ui, |ui| {
                    egui::Grid::new("export-sources")
                        .striped(true)
                        .show(ui, |ui| {
                            for name in ["Bron", "Type", "Datum", "Details", "Status"]{
                                ui.strong(name);
                            }
                            ui.end_row();
                            for (i, job) in datasets.iter().enumerate() {
                                if !query.is_empty()
                                    && !format!(
                                        "{} {} {}",
                                        job.title,
                                        job.job_id,
                                        job.source_path.display()
                                    )
                                    .to_lowercase()
                                    .contains(&query)
                                {
                                    continue;
                                }
                                ui.checkbox(&mut state.selection.datasets[i], &job.title);
                                ui.label("JobFeatures");
                                ui.label("—");
                                ui.label(format!("{} objecten · ID {}", job.features.len(),job.job_id));
                                ui.label("Geladen");
                                ui.end_row();
                            }
                            for (i, trench) in trenches.iter().enumerate() {
                                if !query.is_empty()
                                    && !trench.label.to_lowercase().contains(&query)
                                {
                                    continue;
                                }
                                ui.checkbox(&mut state.selection.trenches[i], &trench.label);
                                ui.label("Virtuele proefsleuf");
                                ui.label("—");
                                ui.label(format!(
                                    "{} punten",
                                    trench.objects.len()
                                        + if state.selection.include_endpoints {
                                            2
                                        } else {
                                            0
                                        }
                                ));
                                ui.label("Bewerkbaar");
                                ui.end_row();
                            }
                        });
                });
            ui.separator();
            ui.checkbox(
                &mut state.selection.include_endpoints,
                "Begin- en eindpunten van virtuele proefsleuven meenemen",
            );
            ui.checkbox(
                &mut state.extra_layers,
                "Ook alle sleufgrenzen, dwarsmarkeringen en zichtbare kaartlagen meenemen",
            );
            ui.horizontal(|ui| {
                ui.label("Labelafstand (m)");
                ui.add(
                    egui::DragValue::new(&mut state.label_gap)
                        .range(0.0..=100.)
                        .speed(0.1),
                );
                ui.label("Puntlijnlengte (m)");
                ui.add(
                    egui::DragValue::new(&mut state.point_length)
                        .range(0.02..=200.)
                        .speed(0.1),
                );
            });
            let selected = state
                .selection
                .datasets
                .iter()
                .chain(&state.selection.trenches)
                .filter(|&&v| v)
                .count();
            ui.label(format!(
                "{selected} bronnen geselecteerd. Onbekende coderingen blijven op CAD-laag 0."
            ));
            if !state.error.is_empty() {
                ui.colored_label(egui::Color32::RED, &state.error);
            }
            ui.horizontal(|ui| {
                ui.add_enabled_ui(selected > 0 || state.extra_layers, |ui| {
                    if ui.button("Exporteren…").clicked() {
                        action = Some(Action { overlay: false,open_jobs:false });
                    }
                    if ui.button("Exporteren en laden als overlay…").clicked() {
                        action = Some(Action { overlay: true,open_jobs:false });
                    }
                });
                if ui.button("Annuleren").clicked() {
                    cancel = true;
                }
            });
        });
    if !open || cancel {
        *dialog = None;
        return None;
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_changed_project_discards_the_export_selection() {
        let mut dialog = Some(Dialog::new(2, 1, 3.5, 42));
        assert!(show(&egui::Context::default(), &mut dialog, &[], &[], 43).is_none());
        assert!(dialog.is_none());
    }
}
