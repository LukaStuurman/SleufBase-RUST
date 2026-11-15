use sleufbase_rust::{
    project::{self, NativeProject},
    template_prefs::TemplatePreferences,
    template_project::Options,
};
#[test]
fn paired_preference_roundtrips_and_older_preferences_enable_the_production_default() {
    let mut options = Options::default();
    assert!(options.paired);
    options.paired = false;
    options.font_directories = vec![std::path::PathBuf::from("CAD Fonts")];
    let saved = TemplatePreferences::capture(&options, &[], &[], &[]);
    let mut data = serde_json::to_value(saved).unwrap();
    let restored: TemplatePreferences = serde_json::from_value(data.clone()).unwrap();
    assert!(!restored.restore(&[], &[], &[]).paired);
    assert_eq!(
        restored.restore(&[], &[], &[]).font_directories,
        options.font_directories
    );
    data.as_object_mut().unwrap().remove("paired");
    data.as_object_mut().unwrap().remove("font_directories");
    let old: TemplatePreferences = serde_json::from_value(data).unwrap();
    assert!(old.restore(&[], &[], &[]).paired);
    assert!(old.restore(&[], &[], &[]).font_directories.is_empty());
    let old: sleufbase_rust::template_order::State = serde_json::from_str("{}").unwrap();
    assert!(old.paired);
}
#[test]
fn native_project_roundtrips_saved_template_preferences_and_old_v3_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("native-project.json");
    let mut project = NativeProject::default();
    project.name = "Persistente DXF-sjabloonlayout".into();
    let mut options = Options::default();
    options.project = "Project 17".into();
    options.drawn_by = "Techbase".into();
    options.font_directories = vec![dir.path().join("CAD Fonts")];
    project.template_export = Some(sleufbase_rust::template_order::State {
        font_directories: options.font_directories.clone(),
        ..Default::default()
    });
    options.include_cad = false;
    options.scale = 500.;
    project.template_preferences = Some(TemplatePreferences::capture(&options, &[], &[], &[]));
    project::save_project_new(&path, &project).unwrap();
    let recovered = project::read_project(&path).unwrap();
    assert_eq!(recovered.name, project.name);
    assert_eq!(
        recovered.template_export.as_ref().unwrap().font_directories,
        options.font_directories
    );
    let preferences = recovered.template_preferences.unwrap();
    assert!(preferences.validate().is_ok());
    let restored = preferences.restore(&[], &[], &[]);
    assert_eq!(restored.project, "Project 17");
    assert_eq!(restored.drawn_by, "Techbase");
    assert_eq!(restored.font_directories, options.font_directories);
    assert_eq!(restored.scale, 500.);
    assert!(!restored.include_cad);
    // A native file created before the preferences field existed must remain
    // readable without requiring an explicit schema conversion.
    let original = std::fs::read_to_string(&path).unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&original).unwrap();
    json.as_object_mut().unwrap().remove("template_preferences");
    let old = dir.path().join("native-old-v3.json");
    std::fs::write(&old, serde_json::to_vec(&json).unwrap()).unwrap();
    let recovered = project::read_project(&old).unwrap();
    assert!(recovered.template_preferences.is_none());
}

#[test]
fn windows_comments_remain_exportable_and_roundtrip_through_project_storage() {
    use sleufbase_rust::{template_project, virtual_trench::VirtualTrench, Vertex};
    let mut project=NativeProject::default();
    project.virtual_trenches.push(VirtualTrench::new("PS12".into(),
        Vertex{x:155000.,y:463000.,z:Some(2.)},Vertex{x:155010.,y:463000.,z:Some(2.)},0.6).unwrap());
    let mut options=Options{choices:template_project::suggest(&[],&[],&project.virtual_trenches),..Default::default()};
    let original="Eerste regel\r\nWater\t63 mm\rVolgende regel\u{b}Nog een regel\u{c}Nieuwe sectie\u{1c}A\u{1d}B\u{1e}C";
    options.choices[0].comments=original.into();
    project.template_preferences=Some(TemplatePreferences::capture(&options,&[],&[],&project.virtual_trenches));
    project.template_preferences.as_ref().unwrap().validate().unwrap();
    let dir=tempfile::tempdir().unwrap();let path=dir.path().join("windows-comments.json");
    project::save_project_new(&path,&project).unwrap();
    let recovered=project::read_project(&path).unwrap();
    let restored=recovered.template_preferences.as_ref().unwrap().restore(&[],&[],&recovered.virtual_trenches);
    assert_eq!(restored.choices[0].comments,original,"saving must preserve exact user text");
    let rendered=sleufbase_rust::template_workflow::comments(
        sleufbase_rust::Bounds{min_x:155000.,min_y:463000.,max_x:155010.,max_y:463001.},&[],&restored.choices[0].comments);
    assert!(rendered.contains("Eerste regel\\PWater\t63 mm\\PVolgende regel"));

    // Other controls are still rejected with a useful source/field diagnostic.
    options.choices[0].comments="Ongeldig\0teken".into();
    options.choices[0].label=Some(String::new()); // Fall back to the usable source title.
    let invalid=TemplatePreferences::capture(&options,&[],&[],&project.virtual_trenches);
    let error=invalid.validate().unwrap_err();
    assert!(error.contains("PS12")&&error.contains("opmerkingen"),"{error}");
}
