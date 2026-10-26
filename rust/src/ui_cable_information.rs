//! Cable picking shares the canvas's prepared CAD geometry and visibility.
//! Original reference: _selection_tolerance_meters and renderer.pick_feature;
//! ties sort by display layer, filename, then source handle, all stably.
use super::*;
use sleufbase_rust::cad_scene::DrawRef;

fn tolerance_pixels(scale: f64) -> Option<f64> {
    if !scale.is_finite() || scale <= 0. {
        return None;
    }
    // Original default min_meters_per_pixel is 0.001; cap at 3 world metres.
    Some((6. / scale).clamp(0.001, 3.) * scale)
}
fn pixel(view: &raster_point_ui::View, point: Vertex) -> [f64; 2] {
    [
        f64::from(view.center.x) + (point.x - view.origin[0]) * view.scale,
        f64::from(view.center.y) - (point.y - view.origin[1]) * view.scale,
    ]
}
fn clip_segment(a: [f64; 2], b: [f64; 2], rect: [f64; 4]) -> Option<[[f64; 2]; 2]> {
    if a.iter().chain(b.iter()).any(|v| !v.is_finite()) {
        return None;
    }
    let d = [b[0] - a[0], b[1] - a[1]];
    if d.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let (mut lo, mut hi) = (0f64, 1f64);
    for (p, q) in [
        (-d[0], a[0] - rect[0]),
        (d[0], rect[2] - a[0]),
        (-d[1], a[1] - rect[1]),
        (d[1], rect[3] - a[1]),
    ] {
        if p == 0. {
            if q < 0. {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0. {
                lo = lo.max(t);
            } else {
                hi = hi.min(t);
            }
            if lo > hi {
                return None;
            }
        }
    }
    Some([
        [a[0] + lo * d[0], a[1] + lo * d[1]],
        [a[0] + hi * d[0], a[1] + hi * d[1]],
    ])
}
fn segment_distance(pointer: [f64; 2], [a, b]: [[f64; 2]; 2]) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let len = d[0] * d[0] + d[1] * d[1];
    let t = if len > 0. {
        (((pointer[0] - a[0]) * d[0] + (pointer[1] - a[1]) * d[1]) / len).clamp(0., 1.)
    } else {
        0.
    };
    (pointer[0] - a[0] - t * d[0]).hypot(pointer[1] - a[1] - t * d[1])
}
fn feature_distance(
    feature: &DrawRef<'_>,
    view: &raster_point_ui::View,
    pointer: [f64; 2],
    clip: [f64; 4],
) -> f64 {
    let mut best = f64::INFINITY;
    // Only POINT and the expanded fallback INSERT marker are drawn as dots.
    // A lone control point of an unsupported entity is not visible geometry.
    if matches!(feature.source.entity_type.as_str(), "POINT" | "INSERT") {
        if let Some(point) = feature.points.first() {
            let p = pixel(view, *point);
            if p[0] >= clip[0] && p[0] <= clip[2] && p[1] >= clip[1] && p[1] <= clip[3] {
                best = (pointer[0] - p[0]).hypot(pointer[1] - p[1]);
            }
        }
    }
    // These paths are exactly those painted by canvas(), including cached
    // curve/spline tessellation, block transforms and text contour breaks.
    for path in feature.paths() {
        for pair in path.windows(2) {
            if let Some(line) = clip_segment(pixel(view, pair[0]), pixel(view, pair[1]), clip) {
                best = best.min(segment_distance(pointer, line));
            }
        }
    }
    best
}
fn pick_all(
    overlays: &[DxfOverlay],
    view: &raster_point_ui::View,
    pointer: Pos2,
) -> Vec<(usize, usize)> {
    let Some(tolerance) = tolerance_pixels(view.scale) else {return Vec::new()};
    let rect = view.rect.intersect(view.painter.clip_rect());
    if !rect.is_positive() || !rect.contains(pointer) {
        return Vec::new();
    }
    let clip = [
        f64::from(rect.min.x),
        f64::from(rect.min.y),
        f64::from(rect.max.x),
        f64::from(rect.max.y),
    ];
    let pointer = [f64::from(pointer.x), f64::from(pointer.y)];
    let mut found:Vec<(f64, String, String, String, usize, usize)> = Vec::new();
    for (overlay_index, overlay) in overlays.iter().enumerate().filter(|(_, o)| o.visible) {
        for index in 0..overlay.features.len() + overlay.expanded.features.len() {
            // rendered_features deliberately omits raw INSERT origins, drawing
            // only their transformed block leaves or explicit fallback marker.
            if index < overlay.features.len() && overlay.features[index].entity_type == "INSERT" {
                continue;
            }
            let Some(feature) = overlay.inspect_feature(index) else {
                continue;
            };
            if !feature.visible {
                continue;
            }
            let distance = feature_distance(&feature, view, pointer, clip);
            if distance > tolerance || !distance.is_finite() {
                continue;
            }
            let display = if feature.layer.is_empty() {
                feature.source.entity_type.as_str()
            } else {
                feature.layer
            };
            let key = (
                distance,
                display.to_lowercase(),
                overlay
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase(),
                feature.source.handle.clone(),
                overlay_index,
                index,
            );
            found.push(key);
        }
    }
    found.sort_by(|a,b|a.0.total_cmp(&b.0)
        .then_with(||a.1.cmp(&b.1)).then_with(||a.2.cmp(&b.2))
        .then_with(||a.3.cmp(&b.3)).then_with(||a.4.cmp(&b.4))
        .then_with(||a.5.cmp(&b.5)));
    found.into_iter().map(|(_,_,_,_,overlay,feature)|(overlay,feature)).collect()
}
fn pick(overlays:&[DxfOverlay],view:&raster_point_ui::View,pointer:Pos2)->Option<(usize,usize)>{
    pick_all(overlays,view,pointer).into_iter().next()
}
impl SleufBaseApp {
    pub(super) fn show_found_layers_popup(&mut self,ctx:&egui::Context){
        if !self.found_layers_open{return;}
        let mut open=true;
        let mut request_close=false;
        let mut selected=None;
        egui::Window::new(format!("Gevonden lagen ({})",self.found_layers.len()))
            .default_width(380.).default_height(260.)
            .default_pos(self.found_layers_pos.unwrap_or(egui::pos2(180.,160.))+egui::vec2(10.,10.))
            .open(&mut open).show(ctx,|ui|{
                ui.small("Klik op een van de gevonden objecten voor de kabelinformatie.");
                egui::ScrollArea::vertical().max_height(260.).show(ui,|ui|{
                    for &(overlay_index,feature_index) in &self.found_layers{
                        let Some(layer)=self.overlays.get(overlay_index)else{continue};
                        let Some(feature)=layer.inspect_feature(feature_index)else{continue};
                        let filename=layer.path.file_name().unwrap_or_default().to_string_lossy();
                        let title=format!("{} — {} ({})",feature.layer,filename,feature.source.entity_type);
                        if ui.selectable_label(self.dxf_selection.as_ref().is_some_and(|s|
                            s.overlay==overlay_index&&s.feature==feature_index),title).clicked(){
                            selected=Some((overlay_index,feature_index));
                        }
                    }
                });
                if ui.button("Sluiten").clicked(){request_close=true;}
            });
        if let Some((overlay,feature))=selected{
            self.select_dxf(overlay,feature);
        }
        self.found_layers_open=open&&!request_close;
    }
    pub(super) fn handle_cable_information(&mut self, view: &raster_point_ui::View) {
        if self.project_sources_owned()
            || self.virtual_map.captures()
            || self.raster_rotation.active()
            || self.raster_points.mode.is_some()
            || self.comment_tools.captures()
            || self.maaiveld_ui.captures()
            || self.trench_editor.is_some()
            || self.tiff_comment_editor.is_some()
            || self.settings_dialog.is_some()
            || self.pending_project_action.is_some()
            || self.navigation.pointer_tool
        {
            return;
        }
        let Some(pointer) = view.clicked else {
            return;
        };
        self.found_layers=pick_all(&self.overlays,view,pointer);
        self.found_layers_pos=Some(pointer);
        self.found_layers_open=!self.found_layers.is_empty();
        if let Some((overlay, feature)) = self.found_layers.first().copied() {
            self.select_dxf(overlay, feature);
            if let Some(selection) = &self.dxf_selection {
                let layer = selection
                    .details
                    .iter()
                    .find(|(key, _)| key == "Laag")
                    .map_or("", |(_, value)| value.as_str());
                self.message = format!("Kabel/object geselecteerd: {layer}");
            }
        } else {
            self.dxf_selection = None;
            self.message = "Geen kabel geselecteerd op deze locatie.".into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sleufbase_rust::{cad, dxf_read::DxfFeature};
    fn line(handle: &str, layer: &str, points: &[[f64; 2]]) -> DxfFeature {
        DxfFeature {
            entity_type: "LINE".into(),
            handle: handle.into(),
            layer: layer.into(),
            color_aci: 7,
            points: points
                .iter()
                .map(|p| Vertex {
                    x: p[0],
                    y: p[1],
                    z: None,
                })
                .collect(),
            raw_text: String::new(),
            block_name: String::new(),
            closed: Some(false),
            cad: None,
        }
    }
    fn overlay(path: &str, features: Vec<DxfFeature>) -> DxfOverlay {
        DxfOverlay {
            path: path.into(),
            visible: true,
            features,
            ..Default::default()
        }
    }
    fn with_view(run: impl FnOnce(&raster_point_ui::View)) {
        let ctx = egui::Context::default();
        let mut run = Some(run);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let rect = egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(100., 100.));
                let response = ui.interact(rect, egui::Id::new("pick-test"), egui::Sense::click());
                let view = raster_point_ui::View {
                    painter: ui.painter().with_clip_rect(rect),
                    response,
                    rect,
                    center: Pos2::ZERO,
                    origin: [0., 0.],
                    scale: 1.,
                    clicked: None,
                };
                run.take().unwrap()(&view);
            });
        });
    }
    #[test]
    fn original_tolerance_has_six_pixel_radius_and_world_caps() {
        assert_eq!(tolerance_pixels(10.), Some(6.));
        assert_eq!(tolerance_pixels(1.), Some(3.));
        assert_eq!(tolerance_pixels(10000.), Some(10.));
        assert!(tolerance_pixels(0.).is_none());
    }
    #[test]
    fn closest_visible_geometry_and_original_label_file_handle_ties() {
        with_view(|view| {
            let view = raster_point_ui::View {
                scale: 10.,
                response: view.response.clone(),
                painter: view.painter.clone(),
                rect: view.rect,
                center: view.center,
                origin: view.origin,
                clicked: None,
            };
            let a = overlay("z.dxf", vec![line("2", "b", &[[0., -5.], [10., -5.]])]);
            let b = overlay("a.dxf", vec![line("1", "A", &[[0., -5.], [10., -5.]])]);
            assert_eq!(
                pick(&[a.clone(), b.clone()], &view, Pos2::new(50., 50.)),
                Some((1, 0))
            );
            assert_eq!(
                pick(std::slice::from_ref(&a), &view, Pos2::new(50., 56.)),
                Some((0, 0))
            );
            assert_eq!(
                pick(std::slice::from_ref(&a), &view, Pos2::new(50., 56.1)),
                None
            );
            let mut hidden = b.clone();
            hidden.visible = false;
            assert_eq!(
                pick(&[hidden, a.clone()], &view, Pos2::new(50., 50.)),
                Some((1, 0))
            );
            let mut layer_hidden = a;
            layer_hidden.layers.insert(
                "b".into(),
                cad::Layer {
                    visible: false,
                    ..Default::default()
                },
            );
            assert_eq!(pick(&[layer_hidden], &view, Pos2::new(50., 50.)), None);
        });
    }
    #[test]
    fn clip_and_path_breaks_prevent_invisible_connections() {
        with_view(|view| {
            let outside = overlay("off.dxf", vec![line("1", "a", &[[-2., 0.], [-2., -100.]])]);
            assert_eq!(pick(&[outside], view, Pos2::new(0., 50.)), None);
            let mut broken = line(
                "2",
                "a",
                &[[0., 0.], [10., 0.], [90., -100.], [100., -100.]],
            );
            broken.cad = Some(Box::new(cad::Properties {
                path_breaks: vec![2],
                ..Default::default()
            }));
            assert_eq!(
                pick(
                    &[overlay("broken.dxf", vec![broken])],
                    view,
                    Pos2::new(50., 50.)
                ),
                None
            );
            let crossing = overlay(
                "cross.dxf",
                vec![line("3", "a", &[[-1e9, -50.], [1e9, -50.]])],
            );
            assert_eq!(pick(&[crossing], view, Pos2::new(50., 50.)), Some((0, 0)));
        });
    }
    #[test]
    fn cached_curves_follow_drawn_arc_instead_of_endpoint_chord() {
        with_view(|view| {
            use sleufbase_rust::cad_geometry::{Affine, ArcCurve, CurvePath, Segment};
            let curve = CurvePath {
                segments: vec![Segment::Arc(ArcCurve {
                    origin: [70., -50., 0.],
                    u: [20., 0., 0.],
                    v: [0., 20., 0.],
                    sweep: std::f64::consts::FRAC_PI_2,
                })],
            };
            let mut feature = line("arc", "curve", &[]);
            feature.entity_type = "ARC".into();
            feature.points = curve.flatten(Affine::IDENTITY, 0.25, 10000).unwrap();
            feature.cad = Some(Box::new(cad::Properties {
                curve: Some(Box::new(curve)),
                ..Default::default()
            }));
            let overlay = overlay("arc.dxf", vec![feature]);
            assert_eq!(
                pick(
                    std::slice::from_ref(&overlay),
                    view,
                    Pos2::new(64.14, 35.86)
                ),
                Some((0, 0))
            );
            assert_eq!(pick(&[overlay], view, Pos2::new(60., 40.)), None);
        });
    }
    #[test]
    fn block_hits_use_expanded_geometry_indices_and_parent_visibility() {
        with_view(|view| {
            use sleufbase_rust::{cad_geometry::Placement, cad_scene::Block};
            let mut insert = line("parent", "parent-layer", &[[20., -20.]]);
            insert.entity_type = "INSERT".into();
            insert.block_name = "cable".into();
            insert.cad = Some(Box::new(cad::Properties {
                placement: Some(Box::new(Placement {
                    insert: [20., -20., 0.],
                    scale: [1.; 3],
                    rotation: 0.,
                    normal: [0., 0., 1.],
                    columns: 1,
                    rows: 1,
                    column_spacing: 0.,
                    row_spacing: 0.,
                })),
                ..Default::default()
            }));
            let mut o = overlay("block.dxf", vec![insert]);
            o.blocks.insert(
                "cable".into(),
                std::sync::Arc::new(Block {
                    name: "cable".into(),
                    base: [0.; 3],
                    features: vec![line("leaf", "0", &[[0., -20.], [20., -20.]])],
                }),
            );
            o.rebuild_block_cache().unwrap();
            assert_eq!(
                pick(std::slice::from_ref(&o), view, Pos2::new(30., 40.)),
                Some((0, 1))
            );
            assert_eq!(
                pick(std::slice::from_ref(&o), view, Pos2::new(20., 20.)),
                None
            );
            o.layers.insert(
                "parent-layer".into(),
                cad::Layer {
                    visible: false,
                    ..Default::default()
                },
            );
            o.refresh_block_visibility();
            assert_eq!(pick(&[o], view, Pos2::new(30., 40.)), None);
        });
    }
    #[test]
    fn modelspace_filtering_matches_canvas_import() {
        with_view(|view| {
            let source="0\nSECTION\n2\nENTITIES\n0\nLINE\n5\nMODEL\n8\nmodel\n67\n0\n10\n10\n20\n-50\n11\n90\n21\n-50\n0\nLINE\n5\nPAPER\n8\npaper\n67\n1\n10\n10\n20\n-20\n11\n90\n21\n-20\n0\nENDSEC\n0\nEOF\n";
            let o = sleufbase_rust::dxf_read::parse_dxf(source, std::path::Path::new("space.dxf"))
                .unwrap();
            assert_eq!(
                pick(std::slice::from_ref(&o), view, Pos2::new(50., 50.)),
                Some((0, 0))
            );
            assert_eq!(pick(&[o], view, Pos2::new(50., 20.)), None);
        });
    }
    #[test]
    fn selected_details_are_existing_cad_details_and_tools_prevent_changes() {
        with_view(|view| {
            let mut app = SleufBaseApp {
                overlays: vec![overlay(
                    "cables.dxf",
                    vec![line("ABC", "Water", &[[0., -50.], [100., -50.]])],
                )],
                ..Default::default()
            };
            let view = raster_point_ui::View {
                clicked: Some(Pos2::new(50., 50.)),
                response: view.response.clone(),
                painter: view.painter.clone(),
                rect: view.rect,
                center: view.center,
                origin: view.origin,
                scale: view.scale,
            };
            app.raster_points.mode = Some(raster_point_ui::Mode::Reference);
            app.handle_cable_information(&view);
            assert!(app.dxf_selection.is_none());
            app.raster_points.mode = None;
            app.handle_cable_information(&view);
            let selection = app.dxf_selection.as_ref().unwrap();
            assert_eq!((selection.overlay, selection.feature), (0, 0));
            assert!(selection.details.contains(&("Handle".into(), "ABC".into())));
            assert!(selection.details.contains(&("Laag".into(), "Water".into())));
        });
    }
}
