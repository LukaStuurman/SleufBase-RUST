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
    // A new egui context needs the production theme even when reusing an app.
    app.shell.installed=false;
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
fn login_menu_starts_embedded_browser_without_password_or_direct_http(){
    let ctx=egui::Context::default();let mut app=SleufBaseApp::default();
    frame(&mut app,&ctx,vec![]);let out=frame(&mut app,&ctx,vec![]);
    let out=click(&mut app,&ctx,label_position(&out,"KickTheMap"));

    let out=click(&mut app,&ctx,label_position(&out,"Inloggen..."));
    assert!(app.shell.login_open);
    label_position(&out,"Sessie overnemen");
    assert!(!out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="Wachtwoord")));
    let out=click(&mut app,&ctx,label_position(&out,"Inloggen"));
    assert_eq!(app.browser_ui.launch_requests,1);assert!(app.live_rx.is_none());
    assert!(!app.message.contains("reCAPTCHA. Kies"));

    click(&mut app,&ctx,label_position(&out,"Sluiten"));assert!(!app.shell.login_open);
}
#[test]
fn login_form_renders_at_minimum_size_without_contacting_accounts(){
    let mut app=SleufBaseApp::default();app.open_kickthemap_login();
    let(labels,image)=capture(&mut app,[1200,760]);
    for text in ["KickTheMap inloggen","Inloggen","Sessie overnemen","Sluiten"]{
        let(_,rect)=labels.iter().find(|(s,_)|s==text).unwrap();
        assert!(rect.min.x>=0.&&rect.max.x<=1200.&&rect.min.y>=0.&&rect.max.y<=760.);
    }
    if let Some(dir)=std::env::var_os("SLEUFBASE_UI_CAPTURE_DIR"){image.save(PathBuf::from(dir).join("kickthemap-login.png")).unwrap();}
}
#[test]
fn embedded_login_primary_action_accepts_keyboard_and_never_sends_credentials(){
    let ctx=egui::Context::default();let mut app=SleufBaseApp::default();
    app.open_kickthemap_login();for _ in 0..3{frame(&mut app,&ctx,vec![]);}
    frame(&mut app,&ctx,vec![shortcut(egui::Key::Space,egui::Modifiers::NONE)]);
    frame(&mut app,&ctx,vec![egui::Event::Key{key:egui::Key::Space,physical_key:None,pressed:false,repeat:false,modifiers:Default::default()}]);
    assert_eq!(app.browser_ui.launch_requests,1);assert!(app.live_rx.is_none());
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
    let mut app=SleufBaseApp::default();let (tx,rx)=mpsc::channel();
    app.live_busy=true;app.live_rx=Some(rx);
    tx.send(Err(sleufbase_rust::kickthemap_live::INTERACTIVE_LOGIN_REQUIRED.into())).unwrap();
    app.poll_live_jobs();assert!(app.shell.login_open);assert!(!app.live_busy);
    let ctx=egui::Context::default();frame(&mut app,&ctx,vec![]);
    let out=frame(&mut app,&ctx,vec![]);
    label_position(&out,"Inloggen");
    label_position(&out,"Sessie overnemen");
    assert!(app.browser_ui.browser.is_none());assert!(!app.live_operation_busy());
    assert!(!app.message.contains("nog in Rust"));
}
#[test]
fn polish_visual_review_captures_virtual_and_template_dialogs(){
    let Some(dir)=std::env::var_os("SLEUFBASE_UI_CAPTURE_DIR") else{return};
    let dir=PathBuf::from(dir);std::fs::create_dir_all(&dir).unwrap();
    for size in [[680,420],[1200,760]] {
        let mut app=SleufBaseApp::default();
        app.virtual_trenches.push(sleufbase_rust::virtual_trench::VirtualTrench::new("PS12".into(),sleufbase_rust::Vertex{x:155000.,y:463000.,z:Some(2.)},sleufbase_rust::Vertex{x:155010.,y:463000.,z:Some(2.)},0.6).unwrap());
        app.open_virtual_editor(0);
        let (_,image)=capture(&mut app,size);image.save(dir.join(format!("virtual-{}x{}.png",size[0],size[1]))).unwrap();
        app.trench_editor=None;app.open_project_template_dialog();
        let (_,image)=capture(&mut app,size);image.save(dir.join(format!("template-{}x{}.png",size[0],size[1]))).unwrap();
        app.project_template_dialog=None;
        let path=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters/gray16.tiff");
        app.geotiffs.push(sleufbase_rust::geotiff::read_geotiff_metadata(&path).unwrap());
        app.open_template_export();
        let (_,image)=capture(&mut app,size);image.save(dir.join(format!("template-mixed-{}x{}.png",size[0],size[1]))).unwrap();
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
                app.open_template_export();app.template_order_dialog.as_mut().unwrap().state.drawn_by="Tekenaar-veld-controle".into();
            }else{
                app.virtual_trenches.push(VirtualTrench::new("PS12".into(),Vertex{x:155000.,y:463000.,z:Some(2.)},Vertex{x:155010.,y:463000.,z:Some(2.)},0.6).unwrap());
                app.open_project_template_dialog();app.project_template_dialog.as_mut().unwrap().options.drawn_by="Tekenaar-veld-controle".into();
            }
            let mut out=None;for _ in 0..4{out=Some(ctx.run(input(size),|ctx|app.render(ctx)));}
            let out=out.unwrap();let label=label_position(&out,"Tekenaar");let value=label_position(&out,"Tekenaar-veld-controle");
            assert!(label.x<value.x);
            for text in ["Projectnaam","Tekenaar"]{assert!(out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()==text&&t.galley.rows.len()==1)),"identity label must stay on one line: {text}");}
            assert!((label.y-value.y).abs()<3.,"{physical} {size:?}: {label:?} {value:?}");
            label_position(&out,"Sjabloon: ingebouwd SleufBase-sjabloon");
            assert!(!out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="Eigen DXF-sjabloon kiezen…")));
            assert!(app.template_job.is_none());
        }
    }
}

#[test]
fn jobs_workspace_keeps_native_download_actions_visible_at_small_and_normal_sizes(){
    for size in [[680,420],[1200,760]]{
        let cookie=sleufbase_rust::kickthemap_browser::BrowserCookie{name:"fixture".into(),value:"local-only".into(),domain:"my.kickthemap.com".into(),path:"/".into(),secure:true};
        let session=KickTheMapLiveClient::from_authenticated_browser(&[cookie],"fixture-csrf").unwrap();let owner=session.snapshot_owner().clone();
        let mut app=SleufBaseApp{live_session:Some(session),live_jobs_owner:Some(owner),..Default::default()};app.browser_ui.open=true;app.browser_ui.selected.insert(42);
        app.live_jobs=vec![sleufbase_rust::jobs_cache::Job{job_id:42,title:"PS42".into(),address:"Voorbeeldstraat 1".into(),municipality:"Voorbeeldplaats".into(),coordinates:String::new(),client_date:"2026-10-28".into(),status:9,downloadable:true,prefix:"fixture@example.test_2026-10-28_12-00-00".into()}];
        let(labels,image)=capture(&mut app,size);
        for text in ["Laad geselecteerde jobs","GeoTIFF downloaden + laden","Objectpunten downloaden + laden"]{let rect=labels.iter().find(|(t,_)|t==text).expect(text).1;assert!(rect.min.x>=0.&&rect.min.y>=0.&&rect.max.x<=size[0] as f32&&rect.max.y<=size[1] as f32,"{size:?} {text}: {rect:?}");}
        assert!(!app.live_operation_busy());assert_eq!(app.browser_ui.launch_requests,0);
        if let Some(dir)=std::env::var_os("SLEUFBASE_UI_CAPTURE_DIR"){let dir=PathBuf::from(dir);std::fs::create_dir_all(&dir).unwrap();image.save(dir.join(format!("jobs-{}x{}.png",size[0],size[1]))).unwrap();}
        app.live_busy=true;app.browser_ui.batch_mode=true;app.browser_ui.status="Bestanden worden gecontroleerd en geladen. ".repeat(20);
        let(labels,image)=capture(&mut app,size);let rect=labels.iter().find(|(text,_)|text=="Download annuleren").unwrap().1;assert!(rect.max.y<=size[1] as f32,"cancellation must stay visible: {rect:?}");
        if let Some(dir)=std::env::var_os("SLEUFBASE_UI_CAPTURE_DIR"){image.save(PathBuf::from(dir).join(format!("jobs-busy-{}x{}.png",size[0],size[1]))).unwrap();}
    }
}
