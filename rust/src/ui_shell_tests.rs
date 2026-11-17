use super::*;
use std::collections::HashMap;

#[test]
fn original_menu_order_matches_validated_launcher_snapshot() {
    let original: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/ui/original-shell.json")).unwrap();
    for (name, actual) in [("Acties", ACTIONS), ("KickTheMap", KICKTHEMAP)] {
        let menu = original["visible_modern_menu_snapshot"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["label"] == name)
            .unwrap();
        let mut expected: Vec<_> = menu["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["label"].as_str().unwrap_or(""))
            .collect();
        if name == "Acties" {
            // The captured launcher predates the terrain-loading action. Keep
            // its original order and verify this known addition at its exact spot.
            let starts = expected.iter().position(|label| *label == "Laad alle beginpunten").unwrap();
            expected.insert(starts + 1, "Laad alle maaiveldgegevens");
        }
        assert_eq!(actual.iter().map(|row| row.0).collect::<Vec<_>>(), expected);
    }
}

fn input(size: [u32; 2]) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            Pos2::ZERO,
            egui::vec2(size[0] as f32, size[1] as f32),
        )),
        ..Default::default()
    }
}

// A CPU raster of the actual egui meshes. This does not start an app, read user
// settings, log in, or require a GPU/display. It is a visual review aid, not an
// assertion of pixel equivalence with Tk. Font rendering follows egui's atlas.
fn raster(
    ctx: &egui::Context,
    output: egui::FullOutput,
    textures: &HashMap<egui::TextureId, egui::ColorImage>,
    size: [u32; 2],
) -> image::RgbaImage {
    let mut pixels =
        image::RgbaImage::from_pixel(size[0], size[1], image::Rgba([245, 247, 251, 255]));
    for primitive in ctx.tessellate(output.shapes, 1.) {
        let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive else {
            panic!("Unexpected GPU callback");
        };
        let texture = textures.get(&mesh.texture_id).expect("mesh texture");
        for indices in mesh.indices.as_chunks::<3>().0 {
            let v = indices
                .iter()
                .map(|&i| &mesh.vertices[i as usize])
                .collect::<Vec<_>>();
            let edge =
                |a: Pos2, b: Pos2, p: Pos2| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
            let area = edge(v[0].pos, v[1].pos, v[2].pos);
            if area.abs() < 0.00001 {
                continue;
            }
            let min_x = v
                .iter()
                .map(|v| v.pos.x)
                .fold(f32::INFINITY, f32::min)
                .max(primitive.clip_rect.left())
                .max(0.)
                .floor() as u32;
            let min_y = v
                .iter()
                .map(|v| v.pos.y)
                .fold(f32::INFINITY, f32::min)
                .max(primitive.clip_rect.top())
                .max(0.)
                .floor() as u32;
            let max_x = v
                .iter()
                .map(|v| v.pos.x)
                .fold(f32::NEG_INFINITY, f32::max)
                .min(primitive.clip_rect.right())
                .min(size[0] as f32)
                .ceil()
                .max(0.) as u32;
            let max_y = v
                .iter()
                .map(|v| v.pos.y)
                .fold(f32::NEG_INFINITY, f32::max)
                .min(primitive.clip_rect.bottom())
                .min(size[1] as f32)
                .ceil()
                .max(0.) as u32;
            for y in min_y..max_y {
                for x in min_x..max_x {
                    let p = Pos2::new(x as f32 + 0.5, y as f32 + 0.5);
                    if !primitive.clip_rect.contains(p) {
                        continue;
                    }
                    let w = [
                        edge(v[1].pos, v[2].pos, p) / area,
                        edge(v[2].pos, v[0].pos, p) / area,
                        edge(v[0].pos, v[1].pos, p) / area,
                    ];
                    if w.iter().any(|w| *w < -0.00001) {
                        continue;
                    }
                    let uv = v
                        .iter()
                        .zip(w)
                        .fold(egui::Vec2::ZERO, |uv, (v, w)| uv + v.uv.to_vec2() * w);
                    let tx = (uv.x * texture.size[0] as f32)
                        .floor()
                        .clamp(0., texture.size[0] as f32 - 1.)
                        as usize;
                    let ty = (uv.y * texture.size[1] as f32)
                        .floor()
                        .clamp(0., texture.size[1] as f32 - 1.)
                        as usize;
                    let texel = texture.pixels[ty * texture.size[0] + tx].to_array();
                    let tint: [f32; 4] = std::array::from_fn(|c| {
                        v.iter().zip(w).map(|(v, w)| v.color[c] as f32 * w).sum()
                    });
                    let color: [f32; 4] = std::array::from_fn(|c| tint[c] * texel[c] as f32 / 255.);
                    let pixel = pixels.get_pixel_mut(x, y);
                    for c in 0..3 {
                        pixel[c] = (color[c] + pixel[c] as f32 * (1. - color[3] / 255.))
                            .round()
                            .clamp(0., 255.) as u8;
                    }
                }
            }
        }
    }
    pixels
}

fn capture(
    app: &mut SleufBaseApp,
    size: [u32; 2],
) -> (Vec<(String, egui::Rect)>, image::RgbaImage) {
    capture_open(app, size, &[])
}

fn capture_open(
    app: &mut SleufBaseApp,
    size: [u32; 2],
    open_labels: &[&str],
) -> (Vec<(String, egui::Rect)>, image::RgbaImage) {
    let ctx = egui::Context::default();
    // A new egui context needs the production theme even when reusing an app.
    app.shell.installed=false;
    let mut textures = HashMap::new();
    let mut result: Option<egui::FullOutput> = None;
    let mut pointer = Pos2::ZERO;
    for index in 0..3 + open_labels.len() * 3 {
        let mut raw = input(size);
        raw.time = Some(index as f64 * 0.2);
        raw.predicted_dt = 0.2;
        if index >= 3 {
            let phase = (index - 3) % 3;
            if phase == 0 {
                pointer = label_position(result.as_ref().unwrap(), open_labels[(index - 3) / 3]);
            }
            if phase < 2 {
                raw.events = vec![egui::Event::PointerMoved(pointer), egui::Event::PointerButton {
                    pos: pointer, button: egui::PointerButton::Primary,
                    pressed: phase == 0, modifiers: egui::Modifiers::NONE,
                }];
            }
        }
        let output = ctx.run(raw, |ctx| app.render(ctx));
        for (id, delta) in &output.textures_delta.set {
            let image = match &delta.image {
                egui::ImageData::Color(image) => (**image).clone(),
                egui::ImageData::Font(image) => egui::ColorImage {
                    size: image.size,
                    pixels: image.srgba_pixels(None).collect(),
                },
            };
            if let Some([x, y]) = delta.pos {
                let texture = textures.get_mut(id).expect("existing texture");
                let texture: &mut egui::ColorImage = texture;
                for row in 0..image.size[1] {
                    let start = (y + row) * texture.size[0] + x;
                    texture.pixels[start..start + image.size[0]].copy_from_slice(
                        &image.pixels[row * image.size[0]..(row + 1) * image.size[0]],
                    );
                }
            } else {
                textures.insert(*id, image);
            }
        }
        result = Some(output);
    }
    let output = result.unwrap();
    let labels = output
        .shapes
        .iter()
        .filter_map(|shape| {
            if let egui::Shape::Text(t) = &shape.shape {
                Some((
                    t.galley.job.text.clone(),
                    t.galley.rect.translate(t.pos.to_vec2()),
                ))
            } else {
                None
            }
        })
        .collect();
    (labels, raster(&ctx, output, &textures, size))
}

#[test]
fn original_shell_renders_without_clipped_primary_controls_at_both_window_sizes() {
    for size in [[1500, 930], [1200, 760]] {
        let (labels, image) = capture(&mut SleufBaseApp::default(), size);
        for text in [
            "Locatie",
            "Acties",
            "KickTheMap",
            "Instellingen",
            "marXact",
            "Proefsleuven",
            "Overlays",
            "Kabelinformatie",
            "Kaart",
            "TIFF",
            "Maaiveld invullen",
            "Referentiepunt",
            "Beginpunt",
            "RD: -",
            "Verwijder selectie",
        ] {
            let (_, rect) = labels
                .iter()
                .find(|(label, _)| label == text)
                .unwrap_or_else(|| panic!("missing {text} at {size:?}"));
            assert!(
                rect.min.x >= 0.
                    && rect.min.y >= 0.
                    && rect.max.x <= size[0] as f32
                    && rect.max.y <= size[1] as f32,
                "{text}: {rect:?} at {size:?}"
            );
        }
        let map_title = labels
            .iter()
            .find(|(label, _)| label == "Kaart")
            .unwrap();
        let expected_sidebar = (size[0] as f32 * 0.255).clamp(360., 400.);
        assert!(
            (map_title.1.left() - (expected_sidebar + 12.)).abs() < 2.,
            "original split moved: {:?}",
            map_title.1
        );
        let tiff_control = labels.iter().find(|(label, _)| label == "TIFF").unwrap().1;
        assert!(tiff_control.left() >= map_title.1.left() && tiff_control.top() > map_title.1.bottom(),
            "display controls must sit below the map heading: {tiff_control:?}");
        assert!(!labels.iter().any(|(label, _)| label == "KickTheMap online"
            || label == "Migratie-preview"
            || label == "StreetSmart"
            || label == "Weergave"));
        let status = labels
            .iter()
            .find(|(label, _)| label.starts_with("Laad GeoTIFF-bestanden"))
            .unwrap();
        assert!(
            status.1.left() < map_title.1.left() + 40.,
            "status must stay left-aligned"
        );
        // Empty list bodies and the information panel must retain their original
        // pane heights, rather than collapsing to the footer/content height.
        let trench_title = labels.iter().find(|(label, _)| label == "Proefsleuven").unwrap().1;
        let trench_footer = labels.iter().find(|(label, _)| label == "0 geselecteerd").unwrap().1;
        assert!(trench_footer.top() > trench_title.bottom() + 80., "empty trench list must retain its body height");
        assert_eq!(image.get_pixel(40, (trench_title.bottom() + 60.).round() as u32).0, [255, 255, 255, 255]);
        assert_eq!(image.get_pixel(24, size[1] - 50).0, [255, 255, 255, 255]);
        if let Some(dir) = std::env::var_os("SLEUFBASE_UI_CAPTURE_DIR") {
            let dir = PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            image
                .save(dir.join(format!("shell-{}x{}.png", size[0], size[1])))
                .unwrap();
        }
    }
}

#[test]
fn loaded_job_objects_are_listed_in_klic_overlays_without_duplicate_dxf_files(){
    let jobs=["PS10","PS2"];
    let mut app=SleufBaseApp::default();
    for (index,title) in jobs.iter().enumerate(){
        let mut ds=sleufbase_rust::parse_job_features(
            include_str!("../tests/fixtures/jobFeatures.json"),(index+1) as u64,title
        ).unwrap();
        ds.source_path=PathBuf::from(format!("{title}_jobFeatures.json"));
        app.datasets.push(ds);
    }
    app.selected=Some(0);
    let (labels,_)=capture(&mut app,[1500,930]);
    for title in jobs {
        assert!(labels.iter().any(|(text,_)|text.starts_with(&format!("Job: {title} ("))));
    }
    assert!(app.overlays.is_empty(),"job objects must not be duplicated as DXF files");
    assert_eq!(app.project_view.opacity_percent,100.);
    let hidden=app.datasets[0].source_path.clone();
    app.project_view.hidden_object_layers.push(hidden.clone());
    let (labels,_)=capture(&mut app,[1500,930]);
    assert!(labels.iter().any(|(text,_)|text.starts_with("Job: PS10 (")));
    assert_eq!(app.navigation.hidden_object_layers,vec![hidden.clone()]);
    assert_eq!(app.snapshot().view.unwrap().hidden_object_layers,vec![hidden]);
    assert_eq!(app.datasets.len(),2);
}

#[test]
fn removal_keeps_raster_indices_and_virtual_selection_consistent() {
    let mut app = SleufBaseApp::default();
    app.virtual_trenches.push(
        VirtualTrench::new(
            "PS1".into(),
            Vertex {
                x: 1.,
                y: 2.,
                z: None,
            },
            Vertex {
                x: 3.,
                y: 2.,
                z: None,
            },
            1.,
        )
        .unwrap(),
    );
    app.virtual_map.selected = Some(0);
    app.remove_shell_tiff();
    assert!(app.virtual_trenches.is_empty());
    assert!(app.virtual_map.selected.is_none());
    assert!(app.dirty);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters/gray16.tiff");
    app.geotiffs
        .push(sleufbase_rust::geotiff::read_geotiff_metadata(&path).unwrap());
    app.raster_points.selected = Some(path);
    app.raster_textures.push(None);
    app.raster_errors.push(None);
    app.remove_shell_tiff();
    assert!(app.geotiffs.is_empty());
    assert!(app.raster_textures.is_empty());
    assert!(app.raster_errors.is_empty());
}

fn frame(
    app: &mut SleufBaseApp,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut raw = input([1500, 930]);
    raw.events = events;
    ctx.run(raw, |ctx| app.render(ctx))
}
fn label_position(output: &egui::FullOutput, text: &str) -> Pos2 {
    output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == text => {
                Some(t.galley.rect.translate(t.pos.to_vec2()).center())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("Missing interactive label {text}"))
}
fn click(app: &mut SleufBaseApp, ctx: &egui::Context, position: Pos2) -> egui::FullOutput {
    for pressed in [true, false] {
        frame(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    frame(app, ctx, vec![])
}

#[test]
fn original_menu_and_map_buttons_execute_actions_through_real_pointer_events() {
    let ctx = egui::Context::default();
    let mut app = SleufBaseApp::default();
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    let out = click(&mut app, &ctx, label_position(&out, "Locatie"));
    assert!(app.location.open);
    assert!(out
        .shapes
        .iter()
        .any(|s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="x")));
    app.location.reset();
    app.comments.push(Comment {
        x: 1.,
        y: 2.,
        text: "Opmerking".into(),
    });
    let out = frame(&mut app, &ctx, vec![]);
    let out = click(&mut app, &ctx, label_position(&out, "Acties"));
    let pos = label_position(&out, "Wis opmerkingen");
    click(&mut app, &ctx, pos);
    assert!(app.comments.is_empty());
    assert!(app.dirty);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters/gray16.tiff");
    app.geotiffs
        .push(sleufbase_rust::geotiff::read_geotiff_metadata(&path).unwrap());
    app.raster_textures.push(None);
    app.raster_errors.push(None);
    let out = frame(&mut app, &ctx, vec![]);
    let out = click(&mut app, &ctx, label_position(&out, "Referentiepunt"));
    assert!(app.raster_points.mode == Some(raster_point_ui::Mode::Reference));
    click(&mut app, &ctx, label_position(&out, "Referentie sluiten"));
    assert!(app.raster_points.mode.is_none());
    app.virtual_trenches.push(
        VirtualTrench::new(
            "PS1".into(),
            Vertex {
                x: 1.,
                y: 2.,
                z: None,
            },
            Vertex {
                x: 3.,
                y: 2.,
                z: None,
            },
            0.6,
        )
        .unwrap(),
    );
    let out = frame(&mut app, &ctx, vec![]);
    let out = click(&mut app, &ctx, label_position(&out, "PS1"));
    assert_eq!(app.virtual_map.selected, Some(0));
    assert!(app.raster_points.selected.is_none());
    let out=click(&mut app, &ctx, label_position(&out, "gray16.tiff"));
    assert!(app.virtual_map.selected.is_none());
    assert!(app.raster_points.selected.is_some());
    click(&mut app,&ctx,label_position(&out,"Virtuele proefsleuf"));
    assert!(app.virtual_map.captures());
    frame(&mut app,&ctx,vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:egui::Modifiers::NONE}]);
    assert!(!app.virtual_map.captures());
}

#[test]
fn large_cad_values_are_bounded_in_the_visible_information_panel() {
    let mut app = SleufBaseApp {
        // A real visible source keeps the selection valid across capture frames;
        // the map correctly clears stale selections that have no matching overlay.
        overlays: vec![DxfOverlay {
            path: "huge-cad-value.dxf".into(),
            visible: true,
            features: vec![sleufbase_rust::dxf_read::DxfFeature {
                entity_type: "LINE".into(),
                handle: "1".into(),
                layer: "Testkabel".into(),
                color_aci: 7,
                points: vec![
                    Vertex { x: 0., y: 0., z: None },
                    Vertex { x: 1., y: 1., z: None },
                ],
                raw_text: String::new(),
                block_name: String::new(),
                closed: Some(false),
                cad: None,
            }],
            ..Default::default()
        }],
        dxf_selection: Some(DxfSelection {
            overlay: 0,
            feature: 0,
            details: vec![("Data".into(), "x".repeat(200000))],
        }),
        ..Default::default()
    };
    let (labels, _) = capture(&mut app, [1200, 760]);
    assert!(labels.iter().all(|(label, _)| label.len() < 16384));
    assert!(labels
        .iter()
        .any(|(label, _)| label.len() == 600 && label.starts_with("xxxxxxxx")));
}

#[test]
fn initial_viewport_matches_original_size_minimum_and_brand_icon() {
    let original: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/ui/original-shell.json")).unwrap();
    let viewport = theme::viewport();
    let size = |key: &str| {
        egui::vec2(
            original["window"][key][0].as_f64().unwrap() as f32,
            original["window"][key][1].as_f64().unwrap() as f32,
        )
    };
    assert_eq!(viewport.inner_size, Some(size("initial_inner_size")));
    assert_eq!(viewport.min_inner_size, Some(size("minimum_inner_size")));
    assert!(viewport.icon.is_some());
}

#[test]
fn settings_use_original_tab_order_and_keep_the_footer_visible() {
    for (category, name) in [
        (settings_ui::Category::Maps, "settings-maps"),
        (settings_ui::Category::Dxf, "settings-dxf"),
        (settings_ui::Category::Trenches, "settings-trenches"),
        (settings_ui::Category::KickTheMap, "settings-kickthemap"),
        (settings_ui::Category::Backups, "settings-backups"),
    ] {
        let mut app = SleufBaseApp::default();
        app.open_settings();
        app.settings_dialog.as_mut().unwrap().category = category;
        let (labels, image) = capture(&mut app, [1500, 930]);
        let mut previous = 0.;
        for name in [
            "Kaarten",
            "DXF-export",
            "Proefsleuven",
            "KickTheMap",
            "Back-ups",
        ] {
            let (_, rect) = labels
                .iter()
                .rev()
                .find(|(label, _)| label == name)
                .unwrap();
            assert!(rect.left() > previous);
            previous = rect.right();
        }
        for label in ["Annuleren", "Opslaan"] {
            let (_, rect) = labels.iter().find(|(text, _)| text == label).unwrap();
            assert!(rect.bottom() < 930. && rect.top() > 600.);
        }
        if let Some(dir) = std::env::var_os("SLEUFBASE_UI_CAPTURE_DIR") {
            image
                .save(PathBuf::from(dir).join(format!("{name}.png")))
                .unwrap();
        }
    }
    if let Some(dir)=std::env::var_os("SLEUFBASE_UI_CAPTURE_DIR") {
        let mut app=SleufBaseApp::default();app.open_settings();
        app.settings_dialog.as_mut().unwrap().words=true;
        let (_,image)=capture(&mut app,[1500,930]);
        image.save(PathBuf::from(dir).join("settings-words.png")).unwrap();
    }

}

fn shortcut(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }
}
fn layer_app() -> SleufBaseApp {
    let mut app = SleufBaseApp::default();
    for label in ["PS1", "PS2"] {
        app.virtual_trenches.push(VirtualTrench::new(label.into(),
            Vertex {x:1., y:2., z:None}, Vertex {x:3., y:2., z:None}, 1.).unwrap());
    }
    app
}
#[test]
fn list_delete_and_backspace_remove_only_the_focused_selected_row() {
    for key in [egui::Key::Delete, egui::Key::Backspace] {
        let ctx = egui::Context::default();
        let mut app = layer_app();
        frame(&mut app, &ctx, vec![]);
        let out = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, label_position(&out, "PS1"));
        frame(&mut app, &ctx, vec![shortcut(key, egui::Modifiers::NONE)]);
        assert_eq!(app.virtual_trenches.len(), 1);
        assert_eq!(app.virtual_trenches[0].label, "PS2");
        assert!(app.virtual_map.selected.is_none());
        assert!(app.dirty);
        frame(&mut app, &ctx, vec![shortcut(key, egui::Modifiers::NONE)]);
        assert_eq!(app.virtual_trenches.len(), 1, "repeat must not remove the next row");
    }
}
#[test]
fn layer_shortcuts_ignore_dialogs_map_focus_and_modified_keys() {
    for blocked in ["settings", "map", "ctrl", "unfocused"] {
        let ctx = egui::Context::default();
        let mut app = layer_app();
        frame(&mut app, &ctx, vec![]);
        let out = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, label_position(&out, "PS1"));
        match blocked {
            "settings" => {app.open_settings(); frame(&mut app, &ctx, vec![]);},
            "map" => {click(&mut app, &ctx, Pos2::new(1100., 500.));},
            "unfocused" => {ctx.memory_mut(|m| {if let Some(id)=m.focused(){m.surrender_focus(id);}});},
            _ => {},
        }
        let modifiers = if blocked == "ctrl" {egui::Modifiers::CTRL} else {egui::Modifiers::NONE};
        frame(&mut app, &ctx, vec![shortcut(egui::Key::Delete, modifiers)]);
        assert_eq!(app.virtual_trenches.len(), 2, "{blocked}");
    }
}

#[test]
fn backspace_edits_location_text_without_removing_the_selected_layer() {
    let ctx = egui::Context::default();
    let mut app = layer_app();
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, label_position(&out, "PS1"));
    app.open_location();
    frame(&mut app, &ctx, vec![]);
    frame(&mut app, &ctx, vec![egui::Event::Text("abc".into())]);
    let out = frame(&mut app, &ctx, vec![shortcut(egui::Key::Backspace, egui::Modifiers::NONE)]);
    assert!(out.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "ab")));
    assert_eq!(app.virtual_trenches.len(), 2);
}
#[test]
fn physical_raster_and_overlay_shortcuts_preserve_sources_and_remove_exact_duplicate_row() {
    for overlay in [false, true] {
        for duplicate in [false, true] {
            let ctx = egui::Context::default();
            let mut app = SleufBaseApp::default();
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
            let label;
            if overlay {
                let path = root.join("cad-space/R2018-normal-ascii.dxf");
                label = path.file_name().unwrap().to_str().unwrap().to_owned();
                app.overlays.push(load_dxf(&path).unwrap());
                if duplicate {app.overlays.push(app.overlays[0].clone());}
            } else {
                let path = root.join("rasters/gray16.tiff");
                label = "gray16.tiff".to_owned();
                app.geotiffs.push(sleufbase_rust::geotiff::read_geotiff_metadata(&path).unwrap());
                if duplicate {app.geotiffs.push(app.geotiffs[0].clone());}
                app.raster_textures = vec![None;app.geotiffs.len()];
                app.raster_errors = vec![None;app.geotiffs.len()];
            }
            let source = if overlay {app.overlays[0].path.clone()} else {app.geotiffs[0].path.clone()};
            let bytes = std::fs::read(&source).unwrap();
            frame(&mut app, &ctx, vec![]);
            let out = frame(&mut app, &ctx, vec![]);
            click(&mut app, &ctx, label_position(&out, &label));
            frame(&mut app, &ctx, vec![shortcut(egui::Key::Delete, egui::Modifiers::NONE)]);
            let remaining = if overlay {app.overlays.len()} else {app.geotiffs.len()};
            assert_eq!(remaining, if duplicate {1} else {0});
            assert_eq!(std::fs::read(source).unwrap(), bytes);
        }
    }
}

#[test]
fn double_clicking_a_physical_or_virtual_trench_fits_only_that_trench() {
    for physical in [false,true] {
        for size in [[1200,760],[1500,930]] {
            let ctx=egui::Context::default();let mut app=layer_app();
            app.comments.push(Comment{x:100000.,y:100000.,text:"far away".into()});
            let target;let label;
            if physical {
                let path=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters/gray16.tiff");
                let mut raster=sleufbase_rust::geotiff::read_geotiff_metadata(&path).unwrap();
                sleufbase_rust::geotiff_rotation::set(&mut raster,37.).unwrap();
                target=raster.bounds;label="gray16.tiff";
                app.geotiffs.push(raster);app.raster_textures.push(None);app.raster_errors.push(None);
            } else {
                let mut bounds=None;
                for (x,y) in app.virtual_trenches[0].measured_boundary() {
                    bounds=union_bounds(bounds,Some(Bounds{min_x:x,min_y:y,max_x:x,max_y:y}));
                }
                target=bounds.unwrap();label="PS1";
            }
            let frame=|app:&mut SleufBaseApp,events| {
                let mut raw=input(size);raw.events=events;
                ctx.run(raw,|ctx|app.render(ctx))
            };
            frame(&mut app,vec![]);let out=frame(&mut app,vec![]);
            let before=app.project_view.camera.clone().unwrap();
            let pos=label_position(&out,label);
            for click in 0..2 {
                for pressed in [true,false] {
                    frame(&mut app,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{
                        pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);
                }
                frame(&mut app,vec![]);
                if click==0 {assert_eq!(app.project_view.camera.as_ref(),Some(&before));}
            }
            let after=app.project_view.camera.as_ref().unwrap();
            assert!((after.center[0]-(target.min_x+target.max_x)/2.).abs()<1e-6);
            assert!((after.center[1]-(target.min_y+target.max_y)/2.).abs()<1e-6);
            assert!(after.meters_per_pixel<before.meters_per_pixel/100.);
            let visible=app.navigation.viewport.unwrap();
            assert!(visible.min_x<=target.min_x&&visible.max_x>=target.max_x);
            assert!(visible.min_y<=target.min_y&&visible.max_y>=target.max_y);
            assert_eq!(app.comments.len(),1);assert_eq!(app.virtual_trenches.len(),2);
            app.navigation.fit_bounds=Some(target);
            app.apply_project(NativeProject::default(),None,false,Vec::new(),&ctx);
            assert!(app.navigation.fit_bounds.is_none(),"project switch must discard a pending trench fit");
        }
    }
}

#[test]
fn double_clicking_job_and_drawing_overlays_fits_the_clicked_extent_without_changing_visibility() {
    for drawing in [false,true] {
        for visible in [false,true] {
            let ctx=egui::Context::default();
            let mut app=layer_app();
            app.datasets.push(Dataset {
                job_id:1,title:"Far away".into(),source_path:"far-away.json".into(),
                features:vec![sleufbase_rust::Feature {
                    object_name:"Reference".into(),source_name:"Reference".into(),
                    geometry:Geometry::Point(Vertex{x:100000.,y:100000.,z:None}),
                    attributes:Default::default(),
                }],
            });
            let target;
            let label;
            let path;
            if drawing {
                path=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cad-space/R2018-normal-ascii.dxf");
                let mut overlay=load_dxf(&path).unwrap();
                overlay.visible=visible;
                target=overlay.bounds().unwrap();
                label=path.file_name().unwrap().to_str().unwrap().to_owned();
                app.overlays.push(overlay);
            }else{
                path=PathBuf::from("zoom-job.json");
                let job=Dataset {
                    job_id:2,title:"Zoomjob".into(),source_path:path.clone(),
                    features:vec![sleufbase_rust::Feature {
                        object_name:"Kabel".into(),source_name:"Kabel".into(),
                        geometry:Geometry::Polyline(vec![
                            Vertex{x:100.,y:200.,z:None},Vertex{x:120.,y:240.,z:None},
                        ]),attributes:Default::default(),
                    }],
                };
                target=Bounds::from_datasets(std::slice::from_ref(&job)).unwrap();
                label="Job: Zoomjob (1 objecten)".to_owned();
                app.datasets.push(job);
                if !visible{app.project_view.hidden_object_layers.push(path.clone());}
            }
            frame(&mut app,&ctx,vec![]);
            let out=frame(&mut app,&ctx,vec![]);
            let before=app.project_view.camera.clone().unwrap();
            let pos=label_position(&out,&label);
            click(&mut app,&ctx,pos);
            assert_eq!(app.project_view.camera.as_ref(),Some(&before),"single click must only select the overlay");
            click(&mut app,&ctx,pos);
            let after=app.project_view.camera.as_ref().unwrap();
            assert!((after.center[0]-(target.min_x+target.max_x)/2.).abs()<1e-6);
            assert!((after.center[1]-(target.min_y+target.max_y)/2.).abs()<1e-6);
            assert!(after.meters_per_pixel<before.meters_per_pixel);
            let viewport=app.navigation.viewport.unwrap();
            assert!(viewport.min_x<=target.min_x&&viewport.max_x>=target.max_x);
            assert!(viewport.min_y<=target.min_y&&viewport.max_y>=target.max_y);
            assert_eq!(app.shell.overlay_selection.indices(),vec![1]);
            if drawing {
                assert_eq!(app.project_view.selected_dxf.as_ref(),Some(&path));
                assert_eq!(app.overlays[0].visible,visible);
            }else{
                assert_eq!(app.selected,Some(1));
                assert_eq!(!app.project_view.hidden_object_layers.contains(&path),visible);
            }
            assert_eq!(app.virtual_trenches.len(),2);
        }
    }
}

#[test]
fn login_menu_opens_native_form_and_never_starts_browser() {
    let ctx=egui::Context::default();
    let mut app=SleufBaseApp::default();
    app.shell.login_email="fixture@example.test".into();
    app.shell.login_password="example-secret-023".into();
    frame(&mut app,&ctx,vec![]);
    let out=frame(&mut app,&ctx,vec![]);
    let out=click(&mut app,&ctx,label_position(&out,"KickTheMap"));
    let out=click(&mut app,&ctx,label_position(&out,"Inloggen..."));
    assert!(app.shell.login_open);
    label_position(&out,"Inloggegevens onthouden en automatisch aanmelden");
    label_position(&out,"E-mailadres");
    label_position(&out,"Wachtwoord");
    assert!(!out.shapes.iter().any(|s|matches!(&s.shape,
        egui::Shape::Text(t) if t.galley.text().contains("example-secret-023"))));
    let out=click(&mut app,&ctx,label_position(&out,"Inloggen"));
    assert_eq!(app.browser_ui.launch_requests,0);
    assert!(app.shell.login_password.is_empty());
    assert!(app.live_session.is_none());
    let busy = frame(&mut app,&ctx,vec![]);
    let close_label=if app.live_busy { "Annuleren" } else { "Sluiten" };
    click(&mut app,&ctx,label_position(&busy,close_label));
    assert!(!app.shell.login_open);
    assert!(app.live_rx.is_none());
    assert!(!app.live_busy);
}
#[test]
fn login_form_renders_at_minimum_size_without_contacting_accounts(){
    let mut app=SleufBaseApp::default();
    app.open_kickthemap_login();
    let(labels,image)=capture(&mut app,[1200,760]);
    for text in ["KickTheMap inloggen","Inloggen","E-mailadres","Wachtwoord",
        "Inloggegevens onthouden en automatisch aanmelden","Sluiten"] {
        let(_,rect)=labels.iter().find(|(s,_)|s==text).unwrap();
        assert!(rect.min.x>=0.&&rect.max.x<=1200.&&rect.min.y>=0.&&rect.max.y<=760.);
    }
    if let Some(dir)=std::env::var_os("SLEUFBASE_UI_CAPTURE_DIR"){
        image.save(PathBuf::from(dir).join("kickthemap-login.png")).unwrap();
    }
}
#[test]
fn pressing_enter_starts_native_login_without_opening_browser(){
    let ctx=egui::Context::default();let mut app=SleufBaseApp::default();
    app.open_kickthemap_login();
    app.shell.login_email="fixture@example.test".into();
    app.shell.login_password="example-secret".into();
    for _ in 0..3 {frame(&mut app,&ctx,vec![]);}
    frame(&mut app,&ctx,vec![shortcut(egui::Key::Enter,egui::Modifiers::NONE)]);
    frame(&mut app,&ctx,vec![egui::Event::Key{key:egui::Key::Enter,
        physical_key:None,pressed:false,repeat:false,modifiers:Default::default()}]);
    assert_eq!(app.browser_ui.launch_requests,0);
    assert!(app.shell.login_password.is_empty());
}
#[test]
fn menu_hover_paints_a_clear_background_for_top_and_submenu_buttons(){
    let ctx=egui::Context::default();let mut app=SleufBaseApp::default();
    frame(&mut app,&ctx,vec![]);let out=frame(&mut app,&ctx,vec![]);
    let pos=label_position(&out,"Acties");
    let out=frame(&mut app,&ctx,vec![egui::Event::PointerMoved(pos)]);
    let orange=Color32::from_rgb(255,224,199);
    assert!(out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Rect(r) if r.rect.contains(pos)&&r.fill==orange)));
    let out=click(&mut app,&ctx,pos);let pos=label_position(&out,"Project opslaan");
    let out=frame(&mut app,&ctx,vec![egui::Event::PointerMoved(pos)]);
    assert!(out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Rect(r) if r.rect.contains(pos)&&r.fill==orange)));
}
#[test]
fn interactive_login_error_reaches_the_browser_controls_without_starting_a_browser(){
    for size in [[680,420],[1200,760],[1500,930]] {
    for font_scale in [1.,1.5] {
        let mut app=SleufBaseApp::default();let (tx,rx)=mpsc::channel();
        app.live_busy=true;app.live_rx=Some(rx);
        tx.send(Err(sleufbase_rust::kickthemap_live::INTERACTIVE_LOGIN_REQUIRED.into())).unwrap();
        app.poll_live_jobs();assert!(app.shell.login_open);assert!(!app.live_busy);
        let ctx=egui::Context::default();
        let render=|app:&mut SleufBaseApp,events| {
            let mut raw=input(size);raw.events=events;
            ctx.run(raw,|ctx| {
                // Install inside the first render, like production: Windows
                // fonts become active on the following frame. Larger metrics
                // also exercise that height change on other platforms.
                if !app.shell.installed {
                    theme::install(ctx);app.shell.installed=true;
                    ctx.style_mut(|style|for font in style.text_styles.values_mut(){font.size*=font_scale;});
                }
                app.render(ctx)
            })
        };
        // egui deliberately hides a new window's first sizing frame. Its very
        // next frame must already show the actions, without waiting for growth.
        render(&mut app,vec![]);
        let out=render(&mut app,vec![]);
        let screen=egui::Rect::from_min_size(Pos2::ZERO,egui::vec2(size[0] as f32,size[1] as f32));
        for label in ["Inloggen","Sluiten"] {
            let (rect,clip)=out.shapes.iter().find_map(|shape|match &shape.shape{
                egui::Shape::Text(text) if text.galley.text()==label=>
                    Some((text.galley.rect.translate(text.pos.to_vec2()),shape.clip_rect)),
                _=>None,
            }).unwrap_or_else(||panic!("Missing {label} at {size:?}, font scale {font_scale}"));
            assert!(screen.contains_rect(rect)&&clip.contains_rect(rect),
                "{label} must be visible immediately at {size:?}, font scale {font_scale}: {rect:?}, clip {clip:?}");
        }
        if size[1]>=760&&font_scale==1. {
            label_position(&out,"Inloggegevens onthouden en automatisch aanmelden");
        }
        assert!(app.browser_ui.browser.is_none());assert!(!app.live_operation_busy());
        assert_eq!(app.browser_ui.launch_requests,0,"an interactive challenge must not launch automatically");
        assert!(!app.message.contains("nog in Rust"));
        let pos=label_position(&out,"Inloggen");
        for pressed in [true,false] {
            render(&mut app,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{
                pos,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::NONE,
            }]);
        }
        assert_eq!(app.browser_ui.launch_requests,1,"the visible action must accept an explicit click");
        assert!(app.browser_ui.browser.is_none(),"tests never start a real browser");
    }
    }
}
#[test]
fn polish_visual_review_captures_virtual_and_template_dialogs(){
    let Some(dir)=std::env::var_os("SLEUFBASE_UI_CAPTURE_DIR") else{return};
    let dir=PathBuf::from(dir);std::fs::create_dir_all(&dir).unwrap();
    for size in [[680,420],[1200,760],[1500,930]] {
        let mut app=SleufBaseApp::default();
        let mut trench=VirtualTrench::new("PS12".into(),Vertex{x:155000.,y:463000.,z:Some(2.)},Vertex{x:155010.,y:463000.,z:Some(2.)},0.6).unwrap();
        for (name,chainage,depth,diameter,rgb,angle) in [
            ("LS",1.4,0.7,55.,[214,48,49],0.),
            ("KPN",3.2,0.55,40.,[76,150,58],0.),
            ("Water",5.4,0.9,110.,[49,119,206],-20.),
            ("Gas",7.8,0.8,90.,[225,169,28],15.),
        ] {
            trench.objects.push(sleufbase_rust::virtual_trench::TrenchPoint{
                role:"kabel".into(),object_name:name.into(),source_name:name.into(),
                attributes:[name.into(),format!("{diameter} mm"),String::new()],
                position:Vertex{x:trench.start.x+chainage,y:trench.start.y,z:Some(2.-depth)},
                metadata:serde_json::json!({"surface_mode":"start","surface_z":2.,"sleufbase_depth_meters":depth,"display_rgb":rgb,"crossing_angle_degrees":angle}),
            });
        }
        app.virtual_trenches.push(trench);
        app.open_virtual_editor(0);
        let (_,image)=capture(&mut app,size);image.save(dir.join(format!("virtual-{}x{}.png",size[0],size[1]))).unwrap();
        if size == [1200,760] {
            let (_,image)=capture_open(&mut app,size,&["Toevoegen en herhalen"]);
            image.save(dir.join("virtual-repeat-1200x760.png")).unwrap();
        }
        app.trench_editor=None;app.open_project_template_dialog();
        let (_,image)=capture(&mut app,size);image.save(dir.join(format!("template-{}x{}.png",size[0],size[1]))).unwrap();
        app.project_template_dialog=None;
        let path=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters/gray16.tiff");
        app.geotiffs.push(sleufbase_rust::geotiff::read_geotiff_metadata(&path).unwrap());
        app.open_template_export();
        let (_,image)=capture(&mut app,size);image.save(dir.join(format!("template-mixed-{}x{}.png",size[0],size[1]))).unwrap();
        if size == [1500,930] {
            let (_,image)=capture_open(&mut app,size,&["Meer exportinstellingen"]);
            image.save(dir.join("template-advanced-1500x930.png")).unwrap();
        }
        app.project_template_dialog.as_mut().unwrap().error=format!("PS12: werkfoto ontbreekt: C:/Projecten/{}/sleuf-foto.jpg", "Lange projectmap/".repeat(18));
        let (_,image)=capture(&mut app,size);image.save(dir.join(format!("template-error-{}x{}.png",size[0],size[1]))).unwrap();
    }
}

#[test]
fn template_defaults_to_builtin_and_drawer_label_precedes_its_field_on_the_same_row(){
    for physical in [true,false]{
        for size in [[680,420],[1200,760],[1500,930]]{
            let ctx=egui::Context::default();let mut app=SleufBaseApp::default();
            if physical{
                let path=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters/gray16.tiff");
                app.geotiffs.push(sleufbase_rust::geotiff::read_geotiff_metadata(&path).unwrap());
                app.open_template_export();app.project_template_dialog.as_mut().unwrap().options.drawn_by="Tekenaar-veld-controle".into();
            }else{
                app.virtual_trenches.push(VirtualTrench::new("PS12".into(),Vertex{x:155000.,y:463000.,z:Some(2.)},Vertex{x:155010.,y:463000.,z:Some(2.)},0.6).unwrap());
                app.open_project_template_dialog();app.project_template_dialog.as_mut().unwrap().options.drawn_by="Tekenaar-veld-controle".into();
            }
            assert!(app.project_template_dialog.as_ref().unwrap().options.template.as_os_str().is_empty(),
                "a fresh export must use the built-in template");
            let mut tick = 0;
            let mut render = |app: &mut SleufBaseApp, events: Vec<egui::Event>| {
                let mut raw = input(size);
                raw.time = Some(f64::from(tick) * 0.2);
                raw.predicted_dt = 0.2;
                raw.events = events;
                tick += 1;
                ctx.run(raw, |ctx| app.render(ctx))
            };
            let mut out=None;for _ in 0..4{out=Some(render(&mut app, vec![]));}
            let out=out.unwrap();let label=label_position(&out,"Tekenaar");let value=label_position(&out,"Tekenaar-veld-controle");
            assert!(label.x<value.x);
            for text in ["Projectnaam","Tekenaar"]{assert!(out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()==text&&t.galley.rows.len()==1)),"identity label must stay on one line: {text}");}
            assert!((label.y-value.y).abs()<3.,"{physical} {size:?}: {label:?} {value:?}");
            // The drawing-stamp fields were added after this compact-layout
            // fixture and start expanded. Collapse them through the real control
            // before checking the compact form and first selectable trench.
            let position = label_position(&out, "Tekeningstempel");
            for pressed in [true, false] {
                render(&mut app, vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
            }
            let mut out = render(&mut app, vec![]);
            for _ in 0..3 { out = render(&mut app, vec![]); }
            label_position(&out,"Sjabloon: ingebouwd SleufBase-sjabloon");
            assert!(!out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="Eigen DXF-sjabloon kiezen…")));
            if size[0] >= 1200 {
                let row = out.shapes.iter().find(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="Meenemen")).expect("first trench choice");
                let egui::Shape::Text(text)=&row.shape else{unreachable!()};
                let bounds=text.galley.rect.translate(text.pos.to_vec2());
                assert!(row.clip_rect.contains_rect(bounds),"the first trench must be selectable without scrolling at {size:?}: {bounds:?}, clip {:?}",row.clip_rect);
            }
            assert!(app.template_job.is_none());
        }
    }
}

#[test]
fn removing_job_object_layer_preserves_its_source_file_and_other_jobs(){
    let dir=tempfile::tempdir().unwrap();
    let path=dir.path().join("PS42_jobFeatures.json");
    std::fs::write(&path,b"fixture").unwrap();
    let mut app=SleufBaseApp::default();
    for (job_id,source_path) in [(42,path.clone()),(43,dir.path().join("PS43.json"))]{
        app.datasets.push(sleufbase_rust::Dataset{job_id,title:format!("PS{job_id}"),source_path,features:Vec::new()});
    }
    app.selected=Some(1);
    app.project_view.hidden_object_layers.push(path.clone());
    app.remove_job_overlay(0);
    assert_eq!(std::fs::read(&path).unwrap(),b"fixture");
    assert_eq!(app.datasets.len(),1);
    assert_eq!(app.datasets[0].job_id,43);
    assert_eq!(app.selected,Some(0));
    assert!(app.project_view.hidden_object_layers.is_empty());
    assert!(app.dirty);
}
#[test]
fn jobs_workspace_keeps_native_download_actions_visible_at_small_and_normal_sizes(){
    for size in [[680,420],[1200,760]]{
        let cookie=sleufbase_rust::kickthemap_browser::BrowserCookie{name:"fixture".into(),value:"local-only".into(),domain:"my.kickthemap.com".into(),path:"/".into(),secure:true};
        let session=KickTheMapLiveClient::from_authenticated_browser(&[cookie],"fixture-csrf").unwrap();let owner=session.snapshot_owner().clone();
        let mut app=SleufBaseApp{live_session:Some(session),live_jobs_owner:Some(owner),..Default::default()};app.browser_ui.open=true;app.browser_ui.selected.insert(42);
        app.live_jobs=vec![sleufbase_rust::jobs_cache::Job{job_id:42,title:"PS42".into(),address:"Voorbeeldstraat 1".into(),municipality:"Voorbeeldplaats".into(),coordinates:String::new(),client_date:"2026-10-28".into(),status:9,downloadable:true,prefix:"fixture@example.test_2026-10-28_12-00-00".into()}];
        let(labels,image)=capture(&mut app,size);
        for text in ["Laad geselecteerde jobs","Open in KickTheMap-browser","Meer"]{let rect=labels.iter().find(|(t,_)|t==text).expect(text).1;assert!(rect.min.x>=0.&&rect.min.y>=0.&&rect.max.x<=size[0] as f32&&rect.max.y<=size[1] as f32,"{size:?} {text}: {rect:?}");}
        assert!(!app.live_operation_busy());assert_eq!(app.browser_ui.launch_requests,0);
        if let Some(dir)=std::env::var_os("SLEUFBASE_UI_CAPTURE_DIR"){let dir=PathBuf::from(dir);std::fs::create_dir_all(&dir).unwrap();image.save(dir.join(format!("jobs-{}x{}.png",size[0],size[1]))).unwrap();}
        // The responsive Jobs footer groups the individual download actions in
        // Meer. Open that real menu before asserting both actions are reachable.
        let(labels,image)=capture_open(&mut app,size,&["Meer"]);
        for text in ["Alleen GeoTIFF laden","Alleen objectpunten laden"]{let rect=labels.iter().find(|(t,_)|t==text).expect(text).1;assert!(rect.min.x>=0.&&rect.min.y>=0.&&rect.max.x<=size[0] as f32&&rect.max.y<=size[1] as f32,"{size:?} {text}: {rect:?}");}
        assert!(!app.live_operation_busy());assert_eq!(app.browser_ui.launch_requests,0);
        if let Some(dir)=std::env::var_os("SLEUFBASE_UI_CAPTURE_DIR"){image.save(PathBuf::from(dir).join(format!("jobs-more-{}x{}.png",size[0],size[1]))).unwrap();}
        app.live_busy=true;app.browser_ui.batch_mode=true;app.browser_ui.status="Bestanden worden gecontroleerd en geladen. ".repeat(20);
        let(labels,image)=capture(&mut app,size);let rect=labels.iter().find(|(text,_)|text=="Download annuleren").unwrap().1;assert!(rect.max.y<=size[1] as f32,"cancellation must stay visible: {rect:?}");
        if let Some(dir)=std::env::var_os("SLEUFBASE_UI_CAPTURE_DIR"){image.save(PathBuf::from(dir).join(format!("jobs-busy-{}x{}.png",size[0],size[1]))).unwrap();}
    }
}


#[test]
fn mixed_trench_selection_deletes_exact_rows_and_keeps_surviving_raster_state_aligned() {
    let mut app = layer_app();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters/gray16.tiff");
    let source = std::fs::read(&path).unwrap();
    let raster = sleufbase_rust::geotiff::read_geotiff_metadata(&path).unwrap();
    app.geotiffs = vec![raster.clone(), raster];
    app.raster_textures = vec![None, None];
    app.raster_errors = vec![None, None];
    app.sync_trench_selection();
    app.shell.trench_selection.click(0, false, false);
    app.shell.trench_selection.click(3, true, false);
    app.apply_trench_list_selection();
    app.remove_shell_tiff();
    assert_eq!(app.geotiffs.len(), 1);
    assert_eq!(app.raster_textures.len(), 1);
    assert_eq!(app.raster_errors.len(), 1);
    assert_eq!(app.virtual_trenches.len(), 1);
    assert_eq!(app.virtual_trenches[0].label, "PS1");
    assert!(app.virtual_map.selected.is_none());
    assert!(app.raster_points.selected.is_none());
    assert!(app.project_view.selected_tiff.is_none());
    assert_eq!(app.shell.trench_selection.len(), 0);
    assert_eq!(std::fs::read(path).unwrap(), source);
}

#[test]
fn mixed_overlay_selection_deletes_jobs_and_drawings_without_deleting_duplicate_sources() {
    let mut app = SleufBaseApp::default();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cad-space/R2018-normal-ascii.dxf");
    let source = std::fs::read(&path).unwrap();
    let drawing = load_dxf(&path).unwrap();
    app.overlays = vec![drawing.clone(), drawing];
    for id in [42, 43] {
        app.datasets.push(sleufbase_rust::Dataset {
            job_id: id, title: format!("PS{id}"), source_path: PathBuf::from(format!("PS{id}.json")), features: Vec::new(),
        });
    }
    app.project_view.hidden_object_layers.push(PathBuf::from("PS42.json"));
    app.sync_overlay_selection();
    app.shell.overlay_selection.click(0, false, false);
    app.shell.overlay_selection.click(3, true, false);
    app.apply_overlay_list_selection();
    app.remove_shell_overlay();
    assert_eq!(app.datasets.len(), 1);
    assert_eq!(app.datasets[0].job_id, 43);
    assert_eq!(app.overlays.len(), 1);
    assert!(app.project_view.hidden_object_layers.is_empty());
    assert!(app.project_view.selected_dxf.is_none());
    assert!(app.selected.is_none());
    assert_eq!(app.shell.overlay_selection.len(), 0);
    assert_eq!(std::fs::read(path).unwrap(), source);
}

#[test]
fn batch_removal_is_blocked_before_any_selected_trench_is_removed() {
    let mut app = layer_app();
    app.sync_trench_selection();
    app.shell.trench_selection.click(0, false, false);
    app.shell.trench_selection.click(1, true, false);
    app.apply_trench_list_selection();
    app.open_virtual_editor(1);
    app.remove_shell_tiff();
    assert_eq!(app.virtual_trenches.len(), 2);
    assert_eq!(app.shell.trench_selection.len(), 2);
    assert!(app.message.contains("Sluit eerst"));
}
