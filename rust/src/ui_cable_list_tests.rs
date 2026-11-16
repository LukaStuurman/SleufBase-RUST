use super::*;
use sleufbase_rust::{cad::Properties, dxf_read::DxfFeature, model::Vertex};
use std::collections::BTreeMap;

struct ListHarness {
    ctx: egui::Context,
    state: State,
    overlays: Vec<DxfOverlay>,
    candidates: Vec<Candidate>,
    selected: Option<Candidate>,
    width: f32,
}

impl ListHarness {
    fn new() -> Self {
        let features = (0..96)
            .map(|index| {
                let long = index % 2 == 0;
                DxfFeature {
                    entity_type: "LINE".into(),
                    handle: index.to_string(),
                    layer: if long {
                        format!("Kabel {index:03} middenspanning elektriciteit met een extra lange technische omschrijving en meerdere aftakkingen in dezelfde sleuf")
                    } else {
                        format!("Kabel {index:03}")
                    },
                    color_aci: 7,
                    points: vec![
                        Vertex { x: 0., y: index as f64, z: None },
                        Vertex { x: 10., y: index as f64, z: None },
                    ],
                    raw_text: String::new(),
                    block_name: String::new(),
                    closed: Some(false),
                    cad: Some(Box::new(Properties {
                        xdata: BTreeMap::from([("InfraCAD".into(), vec![Tag {
                            code: 1000,
                            value: if long {
                                "Materiaal koper; diameter 150 mm; bundel 3; aanvullende kabelinformatie met lange omschrijvingen over verbindingen en aftakkingen".into()
                            } else { "PVC 50 mm".into() },
                        }])]),
                        ..Default::default()
                    })),
                }
            })
            .collect();
        Self {
            ctx: egui::Context::default(),
            state: State { open: true, ..Default::default() },
            overlays: vec![DxfOverlay {
                path: "mixed-cable-heights.dxf".into(),
                visible: true,
                features,
                ..Default::default()
            }],
            candidates: (0..96).map(|index| (0, index)).collect(),
            selected: Some((0, 0)),
            width: 1500.,
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> (egui::FullOutput, Option<Candidate>) {
        let mut choice = None;
        let output = self.ctx.run(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO, egui::vec2(self.width, 760.),
            )),
            events,
            ..Default::default()
        }, |ctx| {
            // Preserve the normal shell's left sidebar while exercising the
            // real candidate panel at the application's supported window sizes.
            egui::SidePanel::left("test-sidebar").resizable(false).exact_width(360.)
                .show(ctx, |_| {});
            choice = show(ctx, &mut self.state, 1, &self.overlays, &self.candidates, self.selected);
        });
        (output, choice)
    }

    fn settle(&mut self) -> egui::FullOutput {
        let mut output = self.frame(vec![]).0;
        // A distant reveal first uses estimates, then measures the destination.
        for _ in 0..5 { output = self.frame(vec![]).0; }
        output
    }

    fn label(&self, candidate: Candidate) -> &str {
        &self.overlays[candidate.0].features[candidate.1].layer
    }
}

fn visible_label(output: &egui::FullOutput, label: &str) -> Option<egui::Pos2> {
    output.shapes.iter().find_map(|shape| {
        let egui::Shape::Text(text) = &shape.shape else { return None; };
        if text.galley.text() != label { return None; }
        let rect = text.galley.rect.translate(text.pos.to_vec2());
        shape.clip_rect.contains_rect(rect).then_some(rect.center())
    })
}

#[test]
fn cable_list_distant_selection_and_resize_keep_mixed_height_rows_visible_and_clickable() {
    let mut list = ListHarness::new();
    let target = (0, 80);
    let output = list.settle();
    assert!(visible_label(&output, list.label(target)).is_none());

    list.selected = Some(target);
    let output = list.settle();
    assert!(visible_label(&output, list.label(target)).is_some(),
        "a distant selected cable must be revealed after its height is measured");

    // Visit the preceding rows, as when browsing a long result list. Their
    // measured heights differ from the cheap estimates used after a resize.
    for index in (0..=target.1).step_by(4) {
        list.selected = Some((0, index));
        list.settle();
    }
    assert!(list.state.row_heights[..=target.1].iter().all(Option::is_some));
    let heights: Vec<_> = list.state.row_heights[..=target.1].iter().flatten().copied().collect();
    assert!(heights.iter().copied().fold(f32::NEG_INFINITY, f32::max)
        > heights.iter().copied().fold(f32::INFINITY, f32::min) + 12.,
        "the fixture must exercise both compact and wrapped cable rows");

    let previous_width = list.state.row_layout.unwrap()[0];
    list.width = 1200.;
    let output = list.settle();
    assert!(list.state.row_layout.unwrap()[0] < previous_width);
    assert!(visible_label(&output, list.label(target)).is_some(),
        "discarding measured heights during resize must not lose the selected cable");

    let (candidate, position) = list.candidates.iter().copied()
        .filter(|candidate| *candidate != target)
        .find_map(|candidate| visible_label(&output, list.label(candidate))
            .map(|position| (candidate, position)))
        .expect("a neighboring cable must remain selectable in the resized list");
    let mut chosen = None;
    for pressed in [true, false] {
        chosen = list.frame(vec![
            egui::Event::PointerMoved(position),
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]).1.or(chosen);
    }
    assert_eq!(chosen, Some(candidate), "the visible row must select its own DXF object");
}
