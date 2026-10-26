use serde_json::{json, Value};
use sleufbase_rust::{
    legacy_project::{import_value, legacy_payload, ProjectUiState},
    project::{read_project, save_project},
    template_prefs::TemplatePreferences,
    template_project::Source,
};
use std::path::Path;

fn write_tiff(path: &Path) {
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = tiff::encoder::TiffEncoder::new(file).unwrap();
    let mut image = encoder
        .new_image::<tiff::encoder::colortype::RGB8>(4, 2)
        .unwrap();
    image
        .encoder()
        .write_tag(tiff::tags::Tag::ModelPixelScaleTag, &[1.0f64, 1.0, 0.0][..])
        .unwrap();
    image
        .encoder()
        .write_tag(
            tiff::tags::Tag::ModelTiepointTag,
            &[0.0f64, 0.0, 0.0, 168000.0, 401002.0, 0.0][..],
        )
        .unwrap();
    image.write_data(&[100u8; 24]).unwrap();
}

#[test]
fn duplicate_physical_paths_preserve_layer_and_slot_identity_through_native_and_python_roundtrips()
{
    let dir = tempfile::tempdir().unwrap();
    let tiff = dir.path().join("same.tif");
    write_tiff(&tiff);
    let key = format!("path:{}", tiff.to_string_lossy().to_lowercase());
    let raw = json!({"version":1,"project_name":"Physical identity","tiffs":[
        {"type":"geotiff","path":tiff,"future_layer":{"name":"A"},"metadata":{"custom":{"name":"A"},"template_export_order":1}},
        {"type":"geotiff","path":tiff,"future_layer":{"name":"B"},"metadata":{"custom":{"name":"B"},"template_export_order":0}}
    ],"template_export_slots":[
        {"layer_key":key,"ps_number":"12","scale":100,"future_slot":{"name":"B"}},
        {"layer_key":key,"ps_number":"11","scale":200,"future_slot":{"name":"A"}}
    ]});
    let mut project = import_value(raw, &dir.path().join("source.klicproject")).unwrap();
    assert_eq!(project.rasters.len(), 2);
    assert_eq!(
        project.rasters[0].metadata["_sleufbase_python_tiff_index"],
        0
    );
    assert_eq!(
        project.rasters[1].metadata["_sleufbase_python_tiff_index"],
        1
    );
    let prefs = project.template_preferences.as_ref().unwrap();
    let original = prefs.restore(&[], &project.rasters, &[]);
    assert_eq!(original.choices[0].source, Source::Raster(1));
    assert_eq!(original.choices[1].source, Source::Raster(0));

    project.rasters.swap(0, 1);
    let mut options = prefs.restore(&[], &project.rasters, &[]);
    assert_eq!(options.choices[0].source, Source::Raster(0));
    assert_eq!(options.choices[0].legacy_slot, Some(0));
    options.choices[0].label = Some("PS22B".into());
    options.choices[0].manual_scale = Some(250);
    options.choices[0].include = false;
    options.choices[1].label = Some("PS31A".into());
    project.template_preferences = Some(TemplatePreferences::capture(
        &options,
        &[],
        &project.rasters,
        &[],
    ));
    let native = dir.path().join("saved.json");
    save_project(&native, &project).unwrap();
    let saved = read_project(&native).unwrap();
    let exported = legacy_payload(&saved, &ProjectUiState::default()).unwrap();
    for (i, name) in ["B", "A"].iter().enumerate() {
        assert_eq!(exported["tiffs"][i]["future_layer"]["name"], *name);
        assert_eq!(exported["tiffs"][i]["metadata"]["custom"]["name"], *name);
        assert_eq!(
            exported["template_export_slots"][i]["future_slot"]["name"],
            *name
        );
        assert!(exported["tiffs"][i]["metadata"]
            .get("_sleufbase_python_tiff_index")
            .is_none());
    }
    assert_eq!(
        exported["tiffs"][0]["metadata"]["template_export_ps_number"],
        "22"
    );
    assert_eq!(
        exported["tiffs"][0]["metadata"]["template_export_variant_code"],
        "B"
    );
    assert_eq!(
        exported["tiffs"][0]["metadata"]["template_cross_section_scale_denominator"],
        250
    );
    assert_eq!(
        exported["tiffs"][0]["metadata"]["template_export_include"],
        false
    );
    assert_eq!(
        exported["tiffs"][1]["metadata"]["template_export_ps_number"],
        "31"
    );
    let again = import_value(exported, &dir.path().join("roundtrip.klicproject")).unwrap();
    let restored = again
        .template_preferences
        .as_ref()
        .unwrap()
        .restore(&[], &again.rasters, &[]);
    assert_eq!(restored.choices[0].source, Source::Raster(0));
    assert_eq!(restored.choices[0].label.as_deref(), Some("PS22B"));
    assert!(!restored.choices[0].include);
    assert_eq!(restored.choices[1].label.as_deref(), Some("PS31A"));
    let second = legacy_payload(&again, &ProjectUiState::default()).unwrap();
    assert_eq!(second["tiffs"][0]["future_layer"]["name"], "B");
    assert_eq!(
        second["template_export_slots"][0]["future_slot"]["name"],
        "B"
    );

    let mut duplicate_origin = saved.clone();
    duplicate_origin.rasters[1].metadata["_sleufbase_python_tiff_index"] = json!(1);
    assert!(legacy_payload(&duplicate_origin, &ProjectUiState::default()).is_err());
}

#[test]
fn physical_metadata_null_and_missing_get_origins_but_scalar_metadata_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let tiff = dir.path().join("plain.tif");
    write_tiff(&tiff);
    for metadata in [Value::Null, json!({"unknown":[1,"Ø"]})] {
        let project = import_value(
            json!({"version":1,"tiffs":[{"type":"geotiff","path":tiff,"metadata":metadata}]}),
            &dir.path().join("source.klicproject"),
        )
        .unwrap();
        assert_eq!(
            project.rasters[0].metadata["_sleufbase_python_tiff_index"],
            0
        );
        if metadata.is_object() {
            assert_eq!(project.rasters[0].metadata["unknown"], metadata["unknown"]);
        }
    }
    let project = import_value(
        json!({"version":1,"tiffs":[{"type":"geotiff","path":tiff}]}),
        &dir.path().join("source.klicproject"),
    )
    .unwrap();
    assert_eq!(
        project.rasters[0].metadata["_sleufbase_python_tiff_index"],
        0
    );
    for metadata in [json!("unexpected"), json!([1, 2]), json!(false)] {
        let error = import_value(
            json!({"version":1,"tiffs":[{"type":"geotiff","path":tiff,"metadata":metadata}]}),
            &dir.path().join("source.klicproject"),
        )
        .unwrap_err();
        assert!(error.contains("ongeldige metadata"));
    }
}
