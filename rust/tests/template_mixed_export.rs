use sleufbase_rust::{
    export_control::Control,
    template_export,
    template_project::{self, Options},
    template_server::Mode,
    virtual_trench::VirtualTrench,
    Vertex,
};
#[test]
fn builtin_template_exports_both_a_loaded_tiff_and_a_virtual_trench_offline() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters/gray16.tiff");
    let rasters = vec![sleufbase_rust::geotiff::read_geotiff_metadata(&path).unwrap()];
    let trenches = vec![VirtualTrench::new(
        "PS12".into(),
        Vertex {
            x: 155000.,
            y: 463000.,
            z: Some(2.),
        },
        Vertex {
            x: 155010.,
            y: 463000.,
            z: Some(2.),
        },
        0.6,
    )
    .unwrap()];
    let options = Options {
        server_mode: Mode::Offline,
        choices: template_project::suggest_rasters(&rasters, &trenches),
        paired: false,
        dpi: 72,
        include_bgt: false,
        include_cadastral: false,
        include_cad: false,
        ..Default::default()
    };
    let control = Control::default();
    let request = template_project::build(
        &options,
        &[],
        &rasters,
        &trenches,
        &[],
        None,
        sleufbase_rust::disciplines::RULES,
        &control,
    )
    .unwrap();
    assert_eq!(request.slots.len(), 2);
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("mixed.dxf");
    let exported =
        template_export::export_with_progress(&output, &request, &control, |_| {}).unwrap();
    assert_eq!(exported.filled_slots, 2);
    let doc = sleufbase_rust::dxf_document::Document::load(
        &output,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    assert!(doc.records().filter(|r| r.kind() == "IMAGE").count() >= 2);
    let images = std::fs::read_dir(&exported.published.assets)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "png"))
        .count();
    assert!(images >= 2);
    assert_eq!(rasters.len(), 1);
    assert_eq!(trenches[0].label, "PS12");
    let original = std::fs::read(&output).unwrap();
    assert!(template_export::export_with_progress(&output, &request, &control, |_| {}).is_err());
    assert_eq!(std::fs::read(&output).unwrap(), original);
}
