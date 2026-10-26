use super::*;
use crate::{template_export, virtual_trench::VirtualTrench, Vertex};
use std::{collections::BTreeMap, fs};

fn request(name: &str) -> Request {
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/template-export/reference.json"
    ))
    .unwrap();
    let mut r: Request = serde_json::from_value(
        cases
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap()["request"]
            .clone(),
    )
    .unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    r.template = root.join(&r.template);
    r.settings.export_date = Some([2026, 10, 26]);
    for slot in r.slots.iter_mut().flatten() {
        if !slot.tiff_png.as_os_str().is_empty() {
            slot.tiff_png = root.join(&slot.tiff_png);
        }
        if !slot.map_png.as_os_str().is_empty() {
            slot.map_png = root.join(&slot.map_png);
        }
        if let Some(source) = &mut slot.tiff_source {
            source.path = root.join(&source.path);
        }
    }
    r
}
fn policy(workers: usize) -> ResourcePolicy {
    let mut policy = crate::resource_policy::get().clone();
    policy.light_map_workers = workers;
    policy.heavy_raster_workers = workers;
    policy
}
fn assets(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(root)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (
                path.file_name().unwrap().to_str().unwrap().to_owned(),
                fs::read(path).unwrap(),
            )
        })
        .collect()
}
fn normalized_dxf(output: &Path, assets: &Path) -> String {
    let text = fs::read_to_string(output)
        .unwrap()
        .replace(assets.to_str().unwrap(), "OWNED_ASSETS");
    // AutoCAD requires fresh dynamic-block GUIDs per export. Normalize only
    // that XData identity, preserving order, distinctness and all other tags.
    let mut lines = text.lines().map(str::to_owned).collect::<Vec<_>>();
    let mut identities = BTreeMap::new();
    for index in 2..lines.len() {
        if lines[index - 2] == "AcDbDynamicBlockGUID" && lines[index - 1].trim() == "1000" {
            let next = identities.len();
            let identity = *identities.entry(lines[index].clone()).or_insert(next);
            lines[index] = format!("DYNAMIC_GUID_{identity}");
        }
    }
    lines.join("\n")
}
#[test]
fn serial_and_parallel_exports_have_identical_dxf_and_png_bytes() {
    // Prepared PNGs, duplicate labels, blank slots, profiles, real TIFFs,
    // virtual rasters, normal/reverse, shared maps and rotated photos all use
    // the production transaction and deterministic DXF assembly.
    for case in ["prepared", "tiff", "virtual", "virtual-reference"] {
        let mut r = request(if case == "prepared" {
            "mixed"
        } else {
            "native-tiff"
        });
        if case != "prepared" {
            r.settings.paired = true;
            for slot in r.slots.iter_mut().flatten() {
                slot.map_png = PathBuf::new();
                slot.map_source = Some(map_export::Source {
                    settings: crate::map_page::Settings {
                        dpi: 30,
                        scale: 225.,
                    },
                    background: map_export::Background::Solid { color: [245; 4] },
                    ..Default::default()
                });
                if case.starts_with("virtual") {
                    slot.tiff_source = None;
                    slot.tiff_png = PathBuf::new();
                    slot.virtual_source = Some(virtual_raster::Source {
                        trench: VirtualTrench::new(
                            slot.label.clone(),
                            Vertex {
                                x: slot.bounds.min_x + 2.,
                                y: slot.bounds.min_y + 3.,
                                z: Some(1.),
                            },
                            Vertex {
                                x: slot.bounds.max_x - 2.,
                                y: slot.bounds.max_y - 3.,
                                z: Some(1.),
                            },
                            0.6,
                        )
                        .unwrap(),
                        quality: 0.2,
                        reverse: false,
                    });
                    if let Some(p) = &mut slot.profile {
                        p.show_reference = case == "virtual-reference";
                    }
                }
            }
        }
        let photo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/template-export/tiff.png");
        for slot in r.slots.iter_mut().flatten() {
            slot.photos = vec![template_photo::Photo {
                path: photo.clone(),
                turns: 1,
                area: template_photo::Area::Tiff,
            }];
        }
        let dir = tempfile::tempdir().unwrap();
        let serial_path = dir.path().join("serial/export.dxf");
        let parallel_path = dir.path().join("parallel/export.dxf");
        let serial = template_export::export_with_asset_policy(
            &serial_path,
            &r,
            &r.datasets,
            &Control::default(),
            |_| {},
            &policy(1),
        )
        .unwrap();
        let parallel = template_export::export_with_asset_policy(
            &parallel_path,
            &r,
            &r.datasets,
            &Control::default(),
            |_| {},
            &policy(4),
        )
        .unwrap();
        assert_eq!(serial.filled_slots, parallel.filled_slots, "{case}");
        assert_eq!(serial.profiles, parallel.profiles, "{case}");
        assert_eq!(serial.added_pages, parallel.added_pages, "{case}");
        let serial_assets = assets(&serial.published.assets);
        let parallel_assets = assets(&parallel.published.assets);
        assert_eq!(
            serial_assets.keys().collect::<Vec<_>>(),
            parallel_assets.keys().collect::<Vec<_>>(),
            "{case}"
        );
        for (name, expected) in serial_assets {
            assert!(
                parallel_assets[&name] == expected,
                "PNG bytes differ: {case}/{name}"
            );
        }
        let serial_dxf = normalized_dxf(&serial_path, &serial.published.assets);
        let parallel_dxf = normalized_dxf(&parallel_path, &parallel.published.assets);
        assert!(
            serial_dxf == parallel_dxf,
            "DXF differs: {case}, first differing line: {:?}",
            serial_dxf
                .lines()
                .zip(parallel_dxf.lines())
                .enumerate()
                .find(|(_, (a, b))| a != b)
        );
    }
}
#[test]
fn quota_counts_aliases_exactly_and_asset_failure_does_not_cancel_the_caller() {
    let r = request("mixed");
    let source = &r.slots[0].as_ref().unwrap().tiff_png;
    let size = fs::metadata(source).unwrap().len();
    for remaining in [size * 2, size * 2 - 1] {
        let dir = tempfile::tempdir().unwrap();
        let mut batch = Batch::default();
        let index = batch.push(Kind::Copy(source), dir.path().join("normal.png"));
        batch.alias(index, dir.path().join("reverse.png"));
        let control = Control::default();
        let result = batch.run(
            &r,
            &control,
            BUNDLE_LIMIT - remaining,
            &policy(4),
            &mut |_| {},
        );
        assert!(control.check().is_ok());
        if remaining == size * 2 {
            assert_eq!(result.unwrap(), BUNDLE_LIMIT);
            assert_eq!(
                fs::read(dir.path().join("normal.png")).unwrap(),
                fs::read(dir.path().join("reverse.png")).unwrap()
            );
        } else {
            assert!(result.is_err());
        }
    }
}
#[test]
fn cancellation_during_parallel_assets_preserves_previous_published_files_and_allows_retry() {
    let r = request("mixed");
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("export.dxf");
    let first = template_export::export(&output, &r, &Control::default()).unwrap();
    let previous_dxf = fs::read(&output).unwrap();
    let previous_assets = assets(&first.published.assets);
    let control = Control::default();
    let mut reached = false;
    let result = template_export::export_with_asset_policy(
        &output,
        &r,
        &r.datasets,
        &control,
        |stage| {
            if stage.starts_with("Afbeelding 1 van") {
                reached = true;
                control.cancel();
            }
        },
        &policy(4),
    );
    assert!(result.is_err());
    assert!(reached);
    assert_eq!(fs::read(&output).unwrap(), previous_dxf);
    assert_eq!(assets(&first.published.assets), previous_assets);
    assert_eq!(
        fs::read_dir(first.published.assets.parent().unwrap())
            .unwrap()
            .count(),
        1
    );
    template_export::export(&output, &r, &Control::default()).unwrap();
}
#[test]
fn callback_panic_during_asset_batch_joins_workers_and_removes_attempt() {
    let r = request("mixed");
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("export.dxf");
    fs::write(&output, b"previous drawing").unwrap();
    let control = Control::default();
    let result = template_export::export_with_asset_policy(
        &output,
        &r,
        &r.datasets,
        &control,
        |stage| {
            if stage.starts_with("Afbeelding 1 van") {
                panic!("injected progress failure");
            }
        },
        &policy(4),
    );
    assert!(result.is_err());
    assert!(control.check().is_ok());
    assert_eq!(fs::read(output).unwrap(), b"previous drawing");
    assert_eq!(
        fs::read_dir(dir.path().join("export_assets"))
            .unwrap()
            .count(),
        0
    );
}
#[test]
fn full_resolution_backgrounds_and_high_dpi_maps_use_the_heavy_lane() {
    let bounds = Bounds {
        min_x: 0.,
        min_y: 0.,
        max_x: 10.,
        max_y: 10.,
    };
    let mut source = map_export::Source {
        include_tiff: false,
        background: map_export::Background::Solid { color: [255; 4] },
        ..Default::default()
    };
    assert!(!Kind::Map {
        source: &source,
        bounds,
        fallback: None,
        reference: None
    }
    .heavy());
    source.settings.dpi = 600;
    assert!(Kind::Map {
        source: &source,
        bounds,
        fallback: None,
        reference: None
    }
    .heavy());
    source.settings.dpi = 300;
    source.background = map_export::Background::Png {
        path: "full.png".into(),
    };
    assert!(Kind::Map {
        source: &source,
        bounds,
        fallback: None,
        reference: None
    }
    .heavy());
}
