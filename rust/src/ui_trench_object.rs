//! Plain-language cable entry; detailed source fields and crossing angles stay optional.
use super::*;
pub(super) fn parse_decimal(text: &str) -> Option<f64> {
    text.trim()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
}

pub(super) fn show(
    editor: &mut Editor,
    ui: &mut egui::Ui,
    rules: &[sleufbase_rust::Rule],
    materials: &[String],
) -> bool {
    let form = &mut editor.object;
    let length = (editor.end.x - editor.start.x).hypot(editor.end.y - editor.start.y);
    let mut pick = false;
    ui.group(|ui|{
        ui.strong("Type en afmetingen");
        let current=rules.iter().find(|r|r.keywords.iter().any(|k|k.eq_ignore_ascii_case(&form.source))).map(|r|r.display_label()).unwrap_or(&form.name).to_owned();
        egui::ComboBox::from_id_salt("cable-type").selected_text(current).width(280.).show_ui(ui,|ui|{
            for rule in rules {
                if ui.selectable_label(rule.keywords.iter().any(|k|k.eq_ignore_ascii_case(&form.source)),rule.display_label()).clicked(){
                    form.name=rule.display_label().to_owned();form.source=rule.keywords.first().map(|k|k.to_string()).unwrap_or_else(||form.name.clone());
                    form.role=if form.name.to_lowercase().contains("leiding"){"leiding"}else{"kabel"}.into();
                    form.rgb=Some(if rule.color_aci==7{[17,24,39]}else{sleufbase_rust::cad::aci_rgb(i32::from(rule.color_aci))});
                }
            }
        });
        ui.horizontal_wrapped(|ui|{
            ui.label("Diameter");if ui.add(egui::DragValue::new(&mut form.diameter).custom_parser(parse_decimal).range(0.0..=10000.).speed(1.).suffix(" mm")).changed(){form.attributes[1]=if form.diameter>0.{form.diameter.to_string()}else{String::new()};}
            ui.label("Aantal");if ui.add(egui::DragValue::new(&mut form.quantity).custom_parser(|s|s.trim().parse::<u32>().ok().map(f64::from)).range(1..=1000)).changed(){form.update_quantity_attribute();}
        });

        ui.horizontal_wrapped(|ui|{
            ui.label("Materiaal");egui::ComboBox::from_id_salt("cable-material").selected_text(if form.attributes[0].is_empty(){"Onbekend"}else{&form.attributes[0]}).show_ui(ui,|ui|{
                ui.selectable_value(&mut form.attributes[0],String::new(),"Onbekend");
                for value in materials{ui.selectable_value(&mut form.attributes[0],value.clone(),value);}
            });
            ui.checkbox(&mut form.use_depth,"Diepte invullen");
            ui.add_enabled(form.use_depth,egui::DragValue::new(&mut form.depth).custom_parser(parse_decimal).speed(0.05).range(0.0..=1000.).suffix(" m"));
        });
        if form.use_depth{ui.small("Diepte onder het beginmaaiveld. Zonder maaiveldhoogte blijft de hoogte voorlopig onbekend.");}
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Kruishoek");
        ui.add(
            egui::DragValue::new(&mut form.angle)
                .custom_parser(parse_decimal)
                .range(-80.0..=80.)
                .speed(1.)
                .suffix("°"),
        )
        .on_hover_text("0° = haaks. Positief wijst richting het eindpunt.");
        let (rect, _) = ui.allocate_exact_size(egui::vec2(48., 27.), egui::Sense::hover());
        let center = rect.center();
        let painter = ui.painter_at(rect);
        painter.line_segment(
            [rect.left_center(), rect.right_center()],
            Stroke::new(1., INK),
        );
        let (sin, cos) = form.angle.to_radians().sin_cos();
        let direction = egui::vec2(sin as f32, -cos as f32) * 12.;
        painter.line_segment(
            [center - direction, center + direction],
            Stroke::new(2., ORANGE),
        );
        ui.small("Bovenaanzicht");
    });
    ui.collapsing("Exacte positie of kaartpunt", |ui| {
        pick = ui.button("Kies punt op de kaart").clicked();
        ui.horizontal_wrapped(|ui| {
            ui.label("Of afstand vanaf");
            ui.radio_value(&mut form.from_end, false, "Beginpunt");
            ui.radio_value(&mut form.from_end, true, "Eindpunt");
            let mut distance = form.distance(length);
            if ui
                .add(
                    egui::DragValue::new(&mut distance)
                        .custom_parser(parse_decimal)
                        .speed(0.05)
                        .range(0.0..=length.max(0.))
                        .suffix(" m"),
                )
                .changed()
            {
                let _ = form.set_distance(length, distance);
            }
        });
        ui.small(format!(
            "Positie: {:.2} m vanaf begin / {:.2} m vanaf eind",
            form.chainage,
            length - form.chainage
        ));
    });
    ui.collapsing("Extra invoer en brongegevens", |ui| {
        ui.label("Naam");
        ui.text_edit_singleline(&mut form.name);
        ui.label("Bron/typecode");
        ui.text_edit_singleline(&mut form.source);
        ui.label("Rol");
        ui.text_edit_singleline(&mut form.role);
        ui.horizontal_wrapped(|ui| {
            ui.label("Dwarsafstand");
            ui.add(
                egui::DragValue::new(&mut form.offset)
                    .speed(0.02)
                    .suffix(" m"),
            );
            ui.checkbox(&mut form.has_elevation, "Z-hoogte handmatig");
            ui.add_enabled(
                form.has_elevation && !form.use_depth,
                egui::DragValue::new(&mut form.elevation)
                    .speed(0.05)
                    .suffix(" m NAP"),
            );
        });
        ui.label("Extra attribuut");
        ui.text_edit_singleline(&mut form.attributes[2]);
    });
    pick
}
