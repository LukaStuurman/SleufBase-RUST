use sleufbase_rust::{
    export_control::Control,
    geotiff::GeoTiffInfo,
    template_export,
    template_project::{self, Choice, Options, Source},
};
use std::path::PathBuf;
fn main() -> Result<(), String> {
    let output = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("fresh output directory required")?,
    );
    if output.exists() {
        return Err("Output already exists".into());
    }
    std::fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/template-export/reference.json"
    ))
    .map_err(|e| e.to_string())?;
    let base: template_export::Request = serde_json::from_value(
        cases
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "native-tiff")
            .unwrap()["request"]
            .clone(),
    )
    .map_err(|e| e.to_string())?;
    let slot = base.slots.iter().flatten().next().unwrap();
    let source = slot.tiff_source.as_ref().unwrap();
    let raster = GeoTiffInfo {
        path: root.join(&source.path),
        width: 100,
        height: 100,
        transform: source.transform,
        bounds: slot.bounds,
        epsg: Some(28992),
        kickthemap_job_id: Some(base.datasets[0].job_id),
        kickthemap_job_title: String::new(),
        kickthemap_job_sidecar: None,
        metadata: Default::default(),
    };
    for scale in [None, Some(40), Some(200), Some(5000)] {
        let mut options = Options {
            server_mode: sleufbase_rust::template_server::Mode::Offline,
            template: root.join(&base.template),
            paired: false,
            dpi: 40,
            include_bgt: false,
            include_cadastral: false,
            include_cad: false,
            choices: vec![Choice {label:None,legacy_slot:None,
                manual_scale: scale,
                source: Source::Job(0),
                raster: Some(0),
                include: true,
                title: "Schaalproef".into(),
                profile: true,
                reverse: false,
                comments: String::new(),
                photos: Vec::new(),
            }],
            ..Default::default()
        };
        options.profile_style.marker_diameter = 0.1;
        options.profile_style.auto_fill_bgt = false;
        let request = template_project::build(
            &options,
            &base.datasets,
            std::slice::from_ref(&raster),
            &[],
            &[],
            None,
            &base.rules,
            &Control::default(),
        )?;
        let name = scale.map_or("auto".into(), |s| s.to_string());
        template_export::export(
            &output.join(format!("scale-{name}.dxf")),
            &request,
            &Control::default(),
        )?;
    }
    Ok(())
}
