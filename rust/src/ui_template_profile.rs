//! Per-slot profile scale, independent from the A4 location-map scale.
use eframe::egui;
pub(super) fn scale(ui: &mut egui::Ui, scale: &mut Option<i64>) {
    ui.horizontal(|ui| {
        ui.label("Profielschaal");
        let mut auto = scale.is_none();
        if ui.checkbox(&mut auto, "Automatisch").changed() {
            *scale = if auto { None } else { Some(200) };
        }
        if let Some(value) = scale {
            ui.label("1:");
            ui.add(egui::DragValue::new(value).range(20..=5000).speed(5.));
        }
    });
}
