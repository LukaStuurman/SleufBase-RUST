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
        let expected: Vec<_> = menu["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["label"].as_str().unwrap_or(""))
            .collect();
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
    let ctx = egui::Context::default();
    let mut textures = HashMap::new();
    let mut result = None;
    for index in 0..3 {
        let mut raw = input(size);
        raw.time = Some(index as f64 * 0.2);
        raw.predicted_dt = 0.2;
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
            "AI",
            "Instellingen",
            "marXact",
            "Weergave",
            "GeoTIFF Proefsleuven",
            "KLIC Overlays",
            "Kabelinformatie",
            "Kaart",
            "Maaiveld invullen",
            "Referentiepunt",
            "Beginpunt",
            "RD: -",
            "Verwijder geselecteerde KLIC",
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
            .rev()
            .find(|(label, _)| label == "Kaart")
            .unwrap();
        let expected_sidebar = (size[0] as f32 * 0.255).clamp(360., 400.);
        assert!(
            (map_title.1.left() - (expected_sidebar + 12.)).abs() < 2.,
            "original split moved: {:?}",
            map_title.1
        );
        assert!(!labels.iter().any(|(label, _)| label == "KickTheMap online"
            || label == "Migratie-preview"
            || label == "StreetSmart"));
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
        assert_eq!(image.get_pixel(40, 300).0, [251, 252, 254, 255]);
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
fn physical_raster_and_klic_shortcuts_preserve_sources_and_ignore_duplicate_paths() {
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
            assert_eq!(remaining, if duplicate {2} else {0});
            assert_eq!(std::fs::read(source).unwrap(), bytes);
        }
    }
}
