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
    ui.horizontal_wrapped(|ui| {
        ui.strong("Vanaf beginpunt");
        ui.add(
            egui::DragValue::new(&mut form.chainage)
                .custom_parser(parse_decimal)
                .range(0.0..=length.max(0.))
                .clamp_existing_to_range(false)
                .fixed_decimals(2)
                .speed(0.05)
                .suffix(" m"),
        );
        ui.separator();
        ui.checkbox(&mut form.use_depth, "Diepte invullen");
        ui.add_enabled(
            form.use_depth,
            egui::DragValue::new(&mut form.depth)
                .custom_parser(parse_decimal)
                .speed(0.05)
                .range(0.0..=1000.)
                .fixed_decimals(2)
                .suffix(" m"),
        )
        .on_hover_text("Diepte onder het beginmaaiveld");
        ui.small(format!("Sleuflengte {:.2} m", length).replace('.', ","));
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
    });
    ui.group(|ui| {
        ui.horizontal_wrapped(|ui| {
            ui.strong("Type");
            let current = rules
                .iter()
                .find(|r| {
                    r.keywords
                        .iter()
                        .any(|k| k.eq_ignore_ascii_case(&form.source))
                })
                .map(|r| r.display_label())
                .unwrap_or(&form.name)
                .to_owned();
            egui::ComboBox::from_id_salt("cable-type")
                .selected_text(current)
                .width(160.)
                .show_ui(ui, |ui| {
                    for rule in rules {
                        if ui
                            .selectable_label(
                                rule.keywords
                                    .iter()
                                    .any(|k| k.eq_ignore_ascii_case(&form.source)),
                                rule.display_label(),
                            )
                            .clicked()
                        {
                            form.name = rule.display_label().to_owned();
                            form.source = rule
                                .keywords
                                .first()
                                .map(|k| k.to_string())
                                .unwrap_or_else(|| form.name.clone());
                            form.role = if form.name.to_lowercase().contains("leiding") {
                                "leiding"
                            } else {
                                "kabel"
                            }
                            .into();
                            form.rgb = Some(if rule.color_aci == 7 {
                                [17, 24, 39]
                            } else {
                                sleufbase_rust::cad::aci_rgb(i32::from(rule.color_aci))
                            });
                        }
                    }
                });
            ui.label("Materiaal");
            egui::ComboBox::from_id_salt("cable-material")
                .selected_text(if form.attributes[0].is_empty() {
                    "Onbekend"
                } else {
                    &form.attributes[0]
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut form.attributes[0], String::new(), "Onbekend");
                    for value in materials {
                        ui.selectable_value(&mut form.attributes[0], value.clone(), value);
                    }
                });
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Diameter");
            if ui
                .add(
                    egui::DragValue::new(&mut form.diameter)
                        .custom_parser(parse_decimal)
                        .range(0.0..=10000.)
                        .speed(1.)
                        .suffix(" mm"),
                )
                .changed()
            {
                form.attributes[1] = if form.diameter > 0. {
                    form.diameter.to_string()
                } else {
                    String::new()
                };
            }
            ui.label("Aantal");
            if ui
                .add(
                    egui::DragValue::new(&mut form.quantity)
                        .custom_parser(|s| s.trim().parse::<u32>().ok().map(f64::from))
                        .range(1..=1000),
                )
                .changed()
            {
                form.update_quantity_attribute();
            }
            ui.label("Kruishoek");
            ui.add(
                egui::DragValue::new(&mut form.angle)
                    .custom_parser(parse_decimal)
                    .range(-80.0..=80.)
                    .speed(1.)
                    .suffix("°"),
            )
            .on_hover_text("0° = haaks. Positief wijst richting het eindpunt.");
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(108., 54.), egui::Sense::click_and_drag());
            let center = rect.center();
            if response.dragged_by(egui::PointerButton::Primary)
                || response.clicked_by(egui::PointerButton::Primary)
            {
                if let Some(angle) = response
                    .interact_pointer_pos()
                    .and_then(|pointer| crossing_angle_from_pointer(pointer - center))
                {
                    form.angle = angle;
                }
            }
            response
                .on_hover_cursor(egui::CursorIcon::Grab)
                .on_hover_text("Bovenaanzicht · sleep de kabel om de kruishoek aan te passen");
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 5., Color32::from_rgb(242, 245, 248));
            let axis = egui::Rect::from_center_size(center, egui::vec2(rect.width() - 12., 16.));
            painter.rect_filled(axis, 2., Color32::from_rgb(231, 220, 205));
            painter.line_segment(
                [axis.left_center(), axis.right_center()],
                Stroke::new(1.0_f32, INK),
            );
            painter.text(
                axis.right_bottom() + egui::vec2(0., 2.),
                egui::Align2::RIGHT_TOP,
                "Eind",
                egui::FontId::proportional(10.),
                INK,
            );
            let (sin, cos) = form.angle.to_radians().sin_cos();
            let direction = egui::vec2(sin as f32, -cos as f32) * 21.;
            painter.line_segment(
                [center - direction, center + direction],
                Stroke::new(2.5_f32, ORANGE),
            );
            for endpoint in [center - direction, center + direction] {
                painter.circle_filled(endpoint, 4., ORANGE);
            }
        });
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

// A cable is a line, so either end represents the same crossing direction.
fn crossing_angle_from_pointer(mut direction: Vec2) -> Option<f64> {
    if direction.length_sq() < 16. {
        return None;
    }
    if direction.y > 0. {
        direction = -direction;
    }
    Some(
        f64::from(direction.x)
            .atan2(-f64::from(direction.y))
            .to_degrees()
            .clamp(-80., 80.),
    )
}

#[cfg(test)]
#[test]
fn rotation_uses_either_handle_and_respects_crossing_limits() {
    assert_eq!(crossing_angle_from_pointer(egui::vec2(0., -12.)), Some(0.));
    assert_eq!(
        crossing_angle_from_pointer(egui::vec2(12., -12.)),
        Some(45.)
    );
    assert_eq!(
        crossing_angle_from_pointer(egui::vec2(-12., 12.)),
        Some(45.)
    );
    assert_eq!(
        crossing_angle_from_pointer(egui::vec2(-20., 0.)),
        Some(-80.)
    );
    assert_eq!(crossing_angle_from_pointer(Vec2::ZERO), None);
}
