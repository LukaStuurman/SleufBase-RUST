//! Synthetic export regression: no uploaded survey/project data is embedded.
use image::{Rgba, RgbaImage};
use sleufbase_rust::{
    export_control::Control,
    map_export::Background,
    template_export,
    template_project::{self, Options},
    template_server::Mode,
    virtual_trench::{TrenchPoint, VirtualTrench},
    Dataset, Feature, Geometry, Rule, Vertex,
};
use std::{fs, path::Path, sync::atomic::AtomicBool};

const OLD_FALLBACK: [u8; 4] = [37, 99, 235, 255];
const CUSTOM: [u8; 3] = [103, 61, 167];

fn vertex(x: f64, y: f64) -> Vertex {
    Vertex { x, y, z: Some(1.4) }
}

fn png(root: &Path, name: &str) -> RgbaImage {
    image::open(root.join(name)).unwrap().into_rgba8()
}

fn has_rgb(image: &RgbaImage, rgb: [u8; 3]) -> bool {
    image.pixels().any(|pixel| pixel[3] > 0 && pixel.0[..3] == rgb)
}

fn assert_utility_colors(image: &RgbaImage) {
    for expected in [[255, 0, 0], [0, 255, 0], [0, 0, 255], CUSTOM] {
        assert!(has_rgb(image, expected), "missing utility color {expected:?}");
    }
    assert!(!has_rgb(image, [37, 99, 235]), "known utilities must not use the generic blue fallback");
}

fn assert_map_colors(image: &RgbaImage) {
    // Raster alpha/compositing changes exact RGB values. Each utility must
    // still have its own strong primary color over the unchanged blue map.
    for channel in 0..3 {
        assert!(image.pixels().any(|p| p[channel] > 200
            && (0..3).filter(|&other| other != channel).all(|other| p[other] < 40)),
            "missing utility channel {channel} on the map");
    }
    assert!(image.pixels().any(|p| p.0 == OLD_FALLBACK),
        "the background's legitimate blue pixels must remain unchanged");
}

#[test]
fn template_assets_use_project_utility_colors_without_recoloring_tiffs_maps_or_photos() {
    let dir = tempfile::tempdir().unwrap();
    let control = Control::default();
    // Canonical MarXact mappings and KickTheMap idNames use the same codes.
    // Deliberately custom colors prove the active rules reach every renderer.
    let rules = vec![
        Rule::new(vec!["ls".into()], "LOW_VOLTAGE".into(), 1, "LS".into()),
        Rule::new(vec!["hd".into()], "GAS".into(), 3, "Gas".into()),
        Rule::new(vec!["water".into()], "WATER".into(), 5, "Water".into()),
    ];
    let mut trench = VirtualTrench::new(
        "Synthetic virtual trench".into(), vertex(155000., 463000.),
        vertex(155010., 463000.), 0.8,
    ).unwrap();
    for (x, name) in [(155002., "ls"), (155004., "hd"), (155006., "water"), (155008., "ls")] {
        trench.objects.push(TrenchPoint {
            // MarXact and older saved trenches do not carry display_rgb.
            metadata: if x == 155008. { serde_json::json!({"display_rgb": CUSTOM}) }
                else { serde_json::Value::Null },
            role: "object".into(), object_name: name.into(), source_name: name.into(),
            position: vertex(x, 463000.), attributes: Default::default(),
        });
    }
    let trenches = vec![trench];
    let original_trenches = serde_json::to_value(&trenches).unwrap();

    // A real TIFF deliberately contains exactly the former fallback blue.
    // Fixing semantic geometry must never replace matching photographic RGB.
    let tiff_path = dir.path().join("physical.tiff");
    let source_pixels = RgbaImage::from_fn(32, 20, |x, _| {
        Rgba(if x < 16 { OLD_FALLBACK } else { [71, 124, 53, 255] })
    });
    let rgb = source_pixels.pixels().flat_map(|p| p.0[..3].iter().copied()).collect::<Vec<_>>();
    let mut encoder = tiff::encoder::TiffEncoder::new(fs::File::create(&tiff_path).unwrap()).unwrap();
    let mut encoded = encoder.new_image::<tiff::encoder::colortype::RGB8>(32, 20).unwrap();
    encoded.encoder().write_tag(tiff::tags::Tag::ModelPixelScaleTag, &[0.5f64, 0.5, 0.][..]).unwrap();
    encoded.encoder().write_tag(tiff::tags::Tag::ModelTiepointTag,
        &[0.0f64, 0., 0., 155050., 463005., 0.][..]).unwrap();
    encoded.write_data(&rgb).unwrap();
    drop(encoder);
    let original_tiff = fs::read(&tiff_path).unwrap();
    let mut info = sleufbase_rust::geotiff::read_geotiff_metadata(&tiff_path).unwrap();
    info.epsg = Some(28992);
    info.kickthemap_job_id = Some(42);
    let rasters = vec![info];
    let jobs = vec![Dataset {
        job_id: 42, title: "Synthetic KickTheMap job".into(), source_path: Default::default(),
        features: [(155053., "ls"), (155057., "hd"), (155061., "water")].into_iter()
            .map(|(x, name)| Feature {
                object_name: name.into(), source_name: name.into(), attributes: Default::default(),
                geometry: Geometry::Polyline(vec![vertex(x, 462999.), vertex(x, 463001.)]),
            }).collect(),
    }];
    let photo_path = dir.path().join("photo.png");
    source_pixels.save(&photo_path).unwrap();
    let original_photo = fs::read(&photo_path).unwrap();

    let mut options = Options {
        server_mode: Mode::Offline, paired: true, dpi: 96, scale: 75.,
        include_bgt: false, include_cadastral: false, include_cad: false,
        choices: template_project::suggest(&jobs, &rasters, &trenches),
        ..Default::default()
    };
    options.overview_settings.include_tiff_images = true;
    assert_eq!(options.choices.len(), 2);
    for choice in &mut options.choices { choice.profile = false; }
    assert!(options.choices[1].raster.is_none(), "the virtual source must render its own geometry");
    options.choices[1].photos.push(sleufbase_rust::template_photo::Photo {
        path: photo_path.clone(), turns: 0, area: Default::default(),
    });
    let mut request = template_project::build(
        &options, &jobs, &rasters, &trenches, &[], None, &rules, &control,
    ).unwrap();
    request.settings.export_date = Some([2026, 11, 17]);
    let font = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/map-export/Hack-Regular.ttf");
    for slot in request.slots.iter_mut().flatten() {
        let map = slot.map_source.as_mut().unwrap();
        map.background = Background::Solid { color: OLD_FALLBACK };
        map.fonts = Some(sleufbase_rust::map_text::Paths { regular: font.clone(), bold: font.clone() });
    }
    let exported = template_export::export(&dir.path().join("colors.dxf"), &request, &control).unwrap();
    let assets = &exported.published.assets;
    for direction in ["normaal", "reverse"] {
        assert_utility_colors(&png(assets, &format!("slot00002_tiff_{direction}.png")));
        assert_map_colors(&png(assets, &format!("slot00002_kaart_{direction}.png")));
        assert_map_colors(&png(assets, &format!("slot00001_kaart_{direction}.png")));
        assert_eq!(png(assets, &format!("slot00001_tiff_{direction}.png")), source_pixels,
            "the physical GeoTIFF pixels must retain their source colors");
        assert_eq!(png(assets, &format!("slot00002_photo01_{direction}.png")), source_pixels,
            "the attached photo must retain its source colors");
    }
    // Overview images are published as TIFFs, separately from the detail PNGs.
    let overview = fs::read_dir(assets).unwrap().filter_map(Result::ok)
        .find(|entry| entry.file_name().to_string_lossy().ends_with("__slot002.tiff"))
        .expect("virtual overview TIFF").path();
    let raster = sleufbase_rust::raster::decode_full_with_cancel(&overview, &AtomicBool::new(false)).unwrap();
    let image = RgbaImage::from_raw(raster.width, raster.height, raster.rgba).unwrap();
    assert_utility_colors(&image);
    assert_eq!(serde_json::to_value(&trenches).unwrap(), original_trenches);
    assert_eq!(fs::read(tiff_path).unwrap(), original_tiff);
    assert_eq!(fs::read(photo_path).unwrap(), original_photo);
}
