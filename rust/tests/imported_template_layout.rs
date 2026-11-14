use serde_json::{json,Value};
use sleufbase_rust::{legacy_project::{self,ProjectUiState},project,template_prefs,template_project::{self,Source},export_control::Control};
use std::path::{Path,PathBuf};

fn original()->Value{
    let layer=|id,name:&str|json!({"type":"virtual_trench","name":name,"unknown_layer":{"keep":id},"metadata":{
        "kickthemap_job_id":id,"virtual_trench_payload":{"width_meters":0.6,"points":[
            {"role":"start","x":155000.,"y":463000.,"z":1.},
            {"role":"end","x":155010.,"y":463000.,"z":1.}]}}});
    json!({"version":1,"project_name":"Herstelde bladen","unknown_project":[1,2],
        "template_export_settings":{"future_setting":{"keep":true},"template_cross_section_marker_diameter":0.075},
        "tiffs":[layer(11,"PS11"),layer(22,"PS22")],
        "template_export_slots":[
            {"layer_key":"job:22","included":true,"scale":40,"ps_number":"0042","variant_code":"C","unknown_slot":"second"},
            {"layer_key":null,"unknown_blank":"first"},
            {"layer_key":"job:11","included":false,"ps_number":"42","variant":true,"unknown_slot":"first"},
            {"layer_key":null,"unknown_blank":"last"}]})
}

#[test]
fn active_order_survives_native_save_edits_and_python_export_with_distinct_blank_metadata(){
    let raw=original();let mut native=legacy_project::import_value(raw.clone(),Path::new("layout.klicproject")).unwrap();
    let prefs=native.template_preferences.as_ref().unwrap();
    assert_eq!((prefs.dpi,prefs.scale),(150,225.));
    let unedited=legacy_project::legacy_payload(&native,&ProjectUiState::default()).unwrap();
    assert_eq!(unedited["template_export_settings"],raw["template_export_settings"]);
    let mut options=prefs.restore(&native.datasets,&native.rasters,&native.virtual_trenches);
    assert_eq!(options.choices.len(),4);assert!(options.raster_layout);
    assert_eq!(options.choices[0].source,Source::Virtual(1));
    assert_eq!(options.choices[0].label.as_deref(),Some("PS42C"));
    assert_eq!(options.choices[0].manual_scale,Some(40));
    assert!(matches!(options.choices[1].source,Source::Empty(_)));
    assert!(!options.choices[2].include);assert_eq!(options.choices[2].label.as_deref(),Some("PS42AUTO"));
    options.choices[2].include=true;options.choices[2].manual_scale=Some(125);
    options.choices.swap(1,3);
    native.template_preferences=Some(template_prefs::TemplatePreferences::capture(&options,&native.datasets,&native.rasters,&native.virtual_trenches));
    let dir=tempfile::tempdir().unwrap();let file=dir.path().join("native.json");project::save_project(&file,&native).unwrap();
    let reopened=project::read_project(&file).unwrap();
    assert_eq!(&*reopened.legacy_archive.as_ref().unwrap().original,&raw);
    let prefs=reopened.template_preferences.as_ref().unwrap();
    assert_eq!((prefs.dpi,prefs.scale),(150,225.));
    let payload=legacy_project::legacy_payload(&reopened,&ProjectUiState::default()).unwrap();
    assert_eq!(payload["unknown_project"],raw["unknown_project"]);
    assert_eq!(payload["template_export_slots"][1]["unknown_blank"],"last");
    assert_eq!(payload["template_export_slots"][3]["unknown_blank"],"first");
    assert_eq!(payload["template_export_slots"][2]["unknown_slot"],"first");
    assert_eq!(payload["template_export_slots"][2]["included"],true);
    assert_eq!(payload["template_export_slots"][2]["scale"],125);
    assert!(!payload.to_string().contains("_sleufbase_python_tiff_index"));
    let again=legacy_project::import_value(payload,Path::new("second.klicproject")).unwrap();
    let mut restored=again.template_preferences.as_ref().unwrap().restore(&again.datasets,&again.rasters,&again.virtual_trenches);
    assert_eq!(restored.choices.len(),4);
    restored.server_mode=sleufbase_rust::template_server::Mode::Offline;
    restored.include_bgt=false;restored.include_cadastral=false;
    restored.template=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/cadastral_template.dxf");
    let request=template_project::build(&restored,&again.datasets,&again.rasters,&again.virtual_trenches,&[],None,&[],&Control::default()).unwrap();
    assert_eq!(request.slots.len(),4);assert!(request.slots[1].is_none()&&request.slots[3].is_none());
    assert_eq!(request.slots[0].as_ref().unwrap().label,"PS42C");
    assert_eq!(request.slots[2].as_ref().unwrap().label,"PS42B");
    assert_eq!(request.slots[2].as_ref().unwrap().profile.as_ref().unwrap().manual_scale,Some(125));
}

#[test]
fn standalone_raster_slot_is_active_without_inventing_a_profile_job(){
    let root=PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path=root.join("tests/fixtures/rasters/gray16.tiff");
    let raw=json!({"version":1,"tiffs":[{"type":"geotiff","path":path,"metadata":{"kickthemap_job_id":73}}],
        "template_export_slots":[{"layer_key":"job:73","ps_number":"7","included":true},{"layer_key":null}]});
    let native=legacy_project::import_value(raw,Path::new("raster.klicproject")).unwrap();
    assert_eq!(native.rasters.len(),1);
    let prefs=native.template_preferences.as_ref().unwrap();
    assert_eq!((prefs.dpi,prefs.scale),(150,225.));
    let mut options=native.template_preferences.as_ref().unwrap().restore(&[],&native.rasters,&[]);
    assert_eq!(options.choices.len(),2);assert_eq!(options.choices[0].source,Source::Raster(0));
    options.server_mode=sleufbase_rust::template_server::Mode::Offline;
    options.template=root.join("../assets/cadastral_template.dxf");options.include_bgt=false;options.include_cadastral=false;
    // The active original freshness hook rejects missing data for known jobs.
    // Explicit image-only export must still work without inventing a dataset.
    assert!(template_project::build(&options,&[],&native.rasters,&[],&[],None,&[],&Control::default()).err().unwrap().contains("objectpunten"));
    options.choices[0].profile=false;
    let request=template_project::build(&options,&[],&native.rasters,&[],&[],None,&[],&Control::default()).unwrap();
    assert_eq!(request.slots[0].as_ref().unwrap().label,"PS7");assert!(request.slots[0].as_ref().unwrap().profile.is_none());
    assert!(request.slots[1].is_none());assert!(request.datasets.is_empty());
    // A standalone TIFF without any job identity has no profile to hydrate.
    let mut standalone=native.rasters[0].clone();standalone.kickthemap_job_id=None;
    standalone.metadata.as_object_mut().unwrap().remove("kickthemap_job_id");
    options.choices[0].profile=true;
    let standalone=template_project::build(&options,&[],&[standalone],&[],&[],None,&[],&Control::default()).unwrap();
    assert!(standalone.datasets.is_empty());assert!(standalone.slots[0].as_ref().unwrap().profile.is_none());
    assert_eq!(legacy_project::legacy_payload(&native,&ProjectUiState::default()).unwrap()["template_export_slots"][0]["layer_key"],"job:73");
}

#[test]
fn duplicate_virtual_names_and_job_ids_keep_distinct_original_slot_identities_after_reordering(){
    let mut raw=original();
    raw["tiffs"][0]["name"]="Duplicate".into();raw["tiffs"][1]["name"]="Duplicate".into();
    raw["tiffs"][1]["metadata"]["kickthemap_job_id"]=11.into();
    raw["template_export_slots"][0]["layer_key"]="job:11".into();
    let mut native=legacy_project::import_value(raw,Path::new("duplicate.klicproject")).unwrap();
    native.virtual_trenches.swap(0,1);
    let options=native.template_preferences.as_ref().unwrap().restore(&[],&[],&native.virtual_trenches);
    assert_eq!(options.choices.len(),4);
    assert_eq!(options.choices[0].source,Source::Virtual(1));assert_eq!(options.choices[0].label.as_deref(),Some("PS42C"));
    assert_eq!(options.choices[2].source,Source::Virtual(0));assert_eq!(options.choices[2].label.as_deref(),Some("PS42AUTO"));
    assert_eq!(options.choices[0].legacy_slot,Some(0));assert_eq!(options.choices[2].legacy_slot,Some(2));
}

#[test]
fn native_high_resolution_preferences_are_preserved_when_python_export_rejects_them(){
    let dir=tempfile::tempdir().unwrap();
    let source=dir.path().join("original.klicproject");
    let original_bytes=serde_json::to_vec(&original()).unwrap();
    std::fs::write(&source,&original_bytes).unwrap();
    let mut native=legacy_project::read_legacy_project(&source).unwrap();
    // New native exports keep their sharp 300 DPI default. Python cannot save
    // this explicit preference, or a different map scale, in its v1 format.
    assert_eq!(template_project::Options::default().dpi,300);
    for (dpi,scale) in [(300,225.),(150,450.)]{
        let mut options=native.template_preferences.as_ref().unwrap()
            .restore(&native.datasets,&native.rasters,&native.virtual_trenches);
        options.dpi=dpi;options.scale=scale;
        native.template_preferences=Some(template_prefs::TemplatePreferences::capture(
            &options,&native.datasets,&native.rasters,&native.virtual_trenches));
        let saved=dir.path().join(format!("native-{dpi}.json"));
        project::save_project(&saved,&native).unwrap();
        let reopened=project::read_project(&saved).unwrap();
        let prefs=reopened.template_preferences.as_ref().unwrap();
        assert_eq!((prefs.dpi,prefs.scale),(dpi,scale));
        let before=serde_json::to_value(&reopened).unwrap();
        let target=dir.path().join(format!("python-{dpi}.klicproject"));
        let error=legacy_project::write_legacy_project(
            &reopened,&target,&ProjectUiState::default(),
            &std::sync::atomic::AtomicBool::new(false),
        ).unwrap_err();
        assert!(error.contains("kaart-DPI/schaal"),"{error}");
        assert!(error.contains("150 DPI"),"{error}");
        assert!(!target.exists());
        assert_eq!(serde_json::to_value(&reopened).unwrap(),before);
        assert_eq!(std::fs::read(&source).unwrap(),original_bytes);
    }
}
