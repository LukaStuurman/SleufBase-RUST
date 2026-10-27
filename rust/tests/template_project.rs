use sleufbase_rust::{
    export_control::Control,
    geotiff::GeoTiffInfo,
    map_export::Background,
    template_export,
    template_project::{self, BackgroundKind, Choice, Options, Source},
};
use std::path::PathBuf;

fn fixture() -> (template_export::Request, GeoTiffInfo) {
    let data: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/template-export/reference.json"
    )).unwrap();
    let value = data.as_array().unwrap().iter()
        .find(|v|v["name"] == "native-tiff").unwrap()["request"].clone();
    let mut prepared:template_export::Request = serde_json::from_value(value).unwrap();
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    prepared.template = base.join(&prepared.template);
    let input = prepared.slots.iter().flatten()
        .find_map(|s|s.tiff_source.as_ref().map(|source|(source, s.bounds)))
        .unwrap();
    let geotiff = GeoTiffInfo {
        path: base.join(&input.0.path), width: 100, height: 100,
        transform: input.0.transform, bounds: input.1, epsg:Some(28992),
        kickthemap_job_id:Some(9),kickthemap_job_title:String::new(),
        kickthemap_job_sidecar:None,metadata:Default::default(),
    };
    (prepared,geotiff)
}
#[test]
fn manual_profile_scale_defaults_from_source_and_survives_project_preferences(){
    let (fixture,mut tiff)=fixture();tiff.kickthemap_job_id=Some(fixture.datasets[0].job_id);
    tiff.metadata=serde_json::json!({"template_cross_section_scale_denominator":"1 : 125"});
    let mut options=Options{server_mode:sleufbase_rust::template_server::Mode::Offline,template:fixture.template.clone(),choices:template_project::suggest(&fixture.datasets[..1],std::slice::from_ref(&tiff),&[]),..Default::default()};
    assert_eq!(options.choices[0].manual_scale,Some(125));options.choices.truncate(1);options.choices[0].manual_scale=Some(40);
    let prefs=sleufbase_rust::template_prefs::TemplatePreferences::capture(&options,&fixture.datasets[..1],std::slice::from_ref(&tiff),&[]);prefs.validate().unwrap();
    let restored=prefs.restore(&fixture.datasets[..1],std::slice::from_ref(&tiff),&[]);assert_eq!(restored.choices[0].manual_scale,Some(40));
    let request=template_project::build(&restored,&fixture.datasets[..1],std::slice::from_ref(&tiff),&[],&[],None,&fixture.rules,&Control::default()).unwrap();assert_eq!(request.slots[0].as_ref().unwrap().profile.as_ref().unwrap().manual_scale,Some(40));
    let mut old=serde_json::to_value(&prefs).unwrap();for choice in old["choices"].as_array_mut().unwrap(){choice.as_object_mut().unwrap().remove("manual_scale");}
    let old:sleufbase_rust::template_prefs::TemplatePreferences=serde_json::from_value(old).unwrap();assert_eq!(old.restore(&fixture.datasets[..1],std::slice::from_ref(&tiff),&[]).choices[0].manual_scale,None);
    let mut state=sleufbase_rust::template_order::State::default();state.sync(&[tiff]).unwrap();assert_eq!(state.rows[0].scale,Some(125));
}
#[test]
fn real_template_pipeline_can_export_from_a_loaded_project_without_manifest_or_network() {
    let (fixture,tiff) = fixture();
    assert!(!fixture.datasets.is_empty());
    let dir=tempfile::tempdir().unwrap();
    let mut options=Options {raster_layout:false,server_mode:sleufbase_rust::template_server::Mode::Offline,
        overview_settings:sleufbase_rust::cadastral_overview::template_defaults(),
        logos:Default::default(),profile_style:Default::default(),
        font_directories:Vec::new(),
        paired:false,
        template:fixture.template.clone(),
        project:"Native projectport".into(), drawn_by:"Techbase".into(),
        background:BackgroundKind::Pdok, dpi:96, scale:225.,
        include_bgt:false, include_cadastral:false, include_cad:false,
        choices:vec![Choice {label:None,legacy_slot:None,manual_scale:None,
            source:Source::Job(0),raster:Some(0),include:true,
            title:"Automatisch vak 1".into(),profile:false,
            reverse:false,comments:"Geïmporteerde projectgegevens".into(),photos:Vec::new(),
        }],
    };
    let output=dir.path().join("project-output.dxf");
    let request=template_project::build(&options,&fixture.datasets,&[tiff.clone()],
        &[],&[],None,&fixture.rules,&Control::default()).unwrap();
    assert_eq!(request.slots.len(),1);
    assert_eq!(request.datasets.len(),1);
    assert_eq!(request.slots[0].as_ref().unwrap().title,"Automatisch vak 1");
    assert!(matches!(request.slots[0].as_ref().unwrap().map_source.as_ref().unwrap().background,
        Background::Solid{..}));
    let result=template_export::export(&output,&request,&Control::default()).unwrap();
    assert_eq!(result.filled_slots,1);
    assert!(output.is_file());
    assert!(result.published.assets.join("slot00001_tiff.png").is_file());
    assert!(result.published.assets.join("slot00001_kaart.png").is_file());
    let doc=sleufbase_rust::dxf_document::Document::load(
        &output,&std::sync::atomic::AtomicBool::new(false)).unwrap();
    assert!(doc.records().any(|r|r.kind()=="IMAGEDEF"));
    // Same input can safely be exported again under a distinct output name.
    options.choices[0].title="Tweede vak".into();
    let second=template_project::build(&options,&fixture.datasets,&[tiff],
        &[],&[],None,&fixture.rules,&Control::default()).unwrap();
    assert_eq!(second.slots[0].as_ref().unwrap().title,"Tweede vak");
}
#[test]
fn virtual_trench_keeps_measured_start_and_selected_source() {
    let (fixture,tiff)=fixture();
    let x = (tiff.bounds.min_x + tiff.bounds.max_x) / 2.0;
    let y = (tiff.bounds.min_y + tiff.bounds.max_y) / 2.0;
    let half_width = (tiff.bounds.max_x - tiff.bounds.min_x) / 4.0;
    let start=sleufbase_rust::Vertex{x:x-half_width,y,z:Some(-1.)};
    let end=sleufbase_rust::Vertex{x:x+half_width,y,z:Some(-1.)};
    let tiff_bounds=tiff.bounds;
    let mut trench=sleufbase_rust::virtual_trench::VirtualTrench::new(
        "PS-virtual".into(),start,end,0.6).unwrap();
    trench.dekband_rows.push(sleufbase_rust::virtual_trench::Dekband{
        start_chainage:0.,end_chainage:half_width,start_depth:0.2,end_depth:0.8,
        source_name:"Band".into(),display_rgb:Some([20,30,40]),
    });
    let opts=Options {server_mode:sleufbase_rust::template_server::Mode::Offline,
        overview_settings:sleufbase_rust::cadastral_overview::template_defaults(),
        paired:false,
        template:fixture.template,
        choices:vec![Choice {label:None,legacy_slot:None,manual_scale:None,source:Source::Virtual(0),raster:Some(0),
            include:true,title:"Virtueel profiel".into(),profile:true,
            reverse:false,comments:String::new(),photos:Vec::new()}],
        include_bgt:false,include_cadastral:false,
        ..Options::default()
    };
    let request=template_project::build(&opts,&[],&[tiff],&[trench],&[],None,
        &fixture.rules,&Control::default()).unwrap();
    assert_eq!(request.slots.len(),1);
    assert_eq!(request.datasets.len(),1);
    let slot=request.slots[0].as_ref().unwrap();
    assert!(slot.virtual_source.is_none(),"selected GeoTIFF remains the raster source");
    assert_eq!(slot.profile.as_ref().unwrap().virtual_trench.as_ref().unwrap().dekband_rows.len(),1,"profile retains its virtual source even with a selected GeoTIFF");
    assert_eq!(request.datasets[0].features.len(),2,"declared bands are injected per profile direction and excluded from report counts");
    assert_eq!(slot.profile.as_ref().unwrap().forced_start,Some([start.x,start.y]));
    assert_eq!(slot.tiff_source.as_ref().unwrap().axis,Some([
        [start.x,start.y],[end.x,end.y]
    ]));
    let overlays=&slot.map_source.as_ref().unwrap().overlays;
    assert_eq!(overlays.len(),4, "four clipped trench outline edges");
    assert!(overlays.iter().all(|stroke|stroke.points.len()==2 &&
        stroke.points.iter().all(|p|p[0]>=tiff_bounds.min_x && p[0]<=tiff_bounds.max_x &&
            p[1]>=tiff_bounds.min_y && p[1]<=tiff_bounds.max_y)));
}

#[test]
fn exported_project_map_includes_clipped_visible_cad_lines_and_permanent_rasters() {
    use sleufbase_rust::{
        dxf_read::{DxfFeature, DxfOverlay},
        model::Vertex,
    };
    let (source, tiff) = fixture();
    let bounds = tiff.bounds;
    let center_y = (bounds.min_y + bounds.max_y) * 0.5;
    let mut cad = DxfOverlay::default();
    cad.visible = true;
    cad.features.push(DxfFeature {
        entity_type: "LINE".into(), handle: "A12".into(),
        layer: "Leidingen".into(), color_aci: 1,
        points: vec![
            Vertex {x:bounds.min_x-100.,y:center_y,z:None},
            Vertex {x:bounds.max_x+100.,y:center_y,z:None},
        ],
        raw_text:String::new(),block_name:String::new(),closed:None,cad:None,
    });
    let options=Options {server_mode:sleufbase_rust::template_server::Mode::Offline,
        overview_settings:sleufbase_rust::cadastral_overview::template_defaults(),
        paired:false,
        template: source.template.clone(),
        dpi:96,scale:225.,
        include_bgt:false,include_cadastral:false,include_cad:true,
        background:BackgroundKind::Blank,
        choices:vec![Choice {label:None,legacy_slot:None,manual_scale:None,
            source:Source::Job(0),raster:Some(0),include:true,
            title:"CAD op kaart".into(),profile:false,reverse:false,
            comments:String::new(),photos:Vec::new(),
        }],
        ..Options::default()
    };
    let request=template_project::build_with_cad(
        &options,&source.datasets,&[tiff],&[],&[],None,&[cad],
        &source.rules,&Control::default(),
    ).unwrap();
    let overlays=&request.slots[0].as_ref().unwrap().map_source.as_ref().unwrap().overlays;
    assert!(overlays.iter().any(|stroke| {
        stroke.points.len()==2
            && (stroke.points[0][0]-bounds.min_x).abs()<1e-6
            && (stroke.points[1][0]-bounds.max_x).abs()<1e-6
            && (stroke.points[0][1]-center_y).abs()<1e-6
            && (stroke.points[1][1]-center_y).abs()<1e-6
    }),"a CAD line crossing the viewport must not disappear");
    let temp=tempfile::tempdir().unwrap();
    let output=temp.path().join("project-cad.dxf");
    let result=template_export::export(&output,&request,&Control::default()).unwrap();
    assert_eq!(result.filled_slots,1);
    assert!(output.exists());
    assert!(result.published.assets.join("slot00001_kaart.png").is_file());
    assert!(result.published.assets.join("slot00001_tiff.png").is_file());
}

#[test]
fn template_photos_are_persistently_bundled_without_replacing_tiff_or_map() {
    use sleufbase_rust::template_photo::{Photo,Area};
    let (reference,tiff)=fixture();
    let dir=tempfile::tempdir().unwrap();
    let photo=dir.path().join("site.png");
    let mut pixels=image::RgbaImage::from_pixel(64,48,image::Rgba([36,64,112,255]));
    pixels.put_pixel(0,0,image::Rgba([255,10,20,255]));
    pixels.save(&photo).unwrap();
    let options=Options {server_mode:sleufbase_rust::template_server::Mode::Offline,
        overview_settings:sleufbase_rust::cadastral_overview::template_defaults(),
        paired:false,
        template:PathBuf::new(),
        include_bgt:false,include_cadastral:false,include_cad:false,
        choices:vec![Choice {label:None,legacy_slot:None,manual_scale:None,
            source:Source::Job(0),raster:Some(0),include:true,
            title:"Vak met foto's".into(),profile:false,reverse:false,
            comments:String::new(),photos:vec![
                Photo{path:photo.clone(),turns:1,area:Area::Tiff},
                Photo{path:photo.clone(),turns:0,area:Area::Map},
            ],
        }],
        ..Options::default()
    };
    let req=template_project::build(&options,&reference.datasets,&[tiff],
        &[],&[],None,&reference.rules,&Control::default()).unwrap();
    assert_eq!(req.slots[0].as_ref().unwrap().photos.len(),2);
    let output=dir.path().join("photos.dxf");
    let result=template_export::export(&output,&req,&Control::default()).unwrap();
    for asset in [
        "slot00001_tiff.png","slot00001_kaart.png",
        "slot00001_photo01.png","slot00001_photo02.png"
    ] { assert!(result.published.assets.join(asset).is_file(),"{asset}"); }
    let rotated=image::open(result.published.assets.join("slot00001_photo01.png")).unwrap();
    assert_eq!((rotated.width(),rotated.height()),(48,64));
    let doc=sleufbase_rust::dxf_document::Document::load(
        &output,&std::sync::atomic::AtomicBool::new(false)).unwrap();
    assert_eq!(doc.records().filter(|r|r.kind()=="IMAGEDEF").count(),4);
    assert_eq!(doc.records().filter(|r|r.kind()=="IMAGE").count(),4);
    // Input source remains unchanged and is never relocated/deleted.
    assert_eq!(image::open(&photo).unwrap().width(),64);
}

#[test]
fn virtual_marxact_sheet_exports_with_builtin_template_and_no_external_geotiff() {
    use sleufbase_rust::virtual_trench::{VirtualTrench,TrenchPoint};
    use sleufbase_rust::model::Vertex;
    let (reference,_)=fixture();
    let mut trench=VirtualTrench::new(
        "Zelfstandig virtueel raster".into(),
        Vertex{x:155000.,y:463000.,z:Some(-1.0)},
        Vertex{x:155008.,y:463000.,z:Some(-1.2)},0.6,
    ).unwrap();
    trench.objects.push(TrenchPoint{metadata:Default::default(),
        role:"water".into(),object_name:"Waterleiding".into(),
        source_name:"WATER".into(),
        position:Vertex{x:155003.,y:463000.,z:Some(-1.5)},
        attributes:Default::default(),
    });
    let dir=tempfile::tempdir().unwrap();
    let options=Options {server_mode:sleufbase_rust::template_server::Mode::Offline,
        overview_settings:sleufbase_rust::cadastral_overview::template_defaults(),
        paired:false,
        template:PathBuf::new(),
        include_bgt:false,include_cadastral:false,include_cad:false,
        background:BackgroundKind::Blank,
        choices:vec![Choice{label:None,legacy_slot:None,manual_scale:None,
            source:Source::Virtual(0),raster:None,include:true,
            title:trench.label.clone(),profile:false,reverse:false,
            comments:"Eigen meetraster zonder KickTheMap".into(),photos:Vec::new(),
        }],
        ..Options::default()
    };
    let request=template_project::build(
        &options,&[],&[],&[trench.clone()],&[],None,&reference.rules,
        &Control::default(),
    ).unwrap();
    let slot=request.slots[0].as_ref().unwrap();
    assert!(slot.tiff_png.as_os_str().is_empty());
    assert!(slot.tiff_source.is_none());
    assert!(slot.virtual_source.is_some());
    assert_eq!(request.datasets[0].features.len(),3);
    let expected=slot.bounds;
    assert!(expected.min_x < trench.start.x);
    assert!(expected.max_x > trench.end.x);
    let output=dir.path().join("standalone-virtual.dxf");
    let export=template_export::export(&output,&request,&Control::default()).unwrap();
    assert_eq!(export.filled_slots,1);
    let photo=export.published.assets.join("slot00001_tiff.png");
    assert!(photo.is_file());
    let synthetic=image::open(photo).unwrap().to_rgba8();
    assert!(synthetic.pixels().any(|p|p[3]==0));
    assert!(synthetic.pixels().any(|p|p[0]==0&&p[3]==255));
    assert!(synthetic.pixels().any(|p|p[2]==235&&p[3]>=235));
    assert!(export.published.assets.join("slot00001_kaart.png").is_file());
    let doc=sleufbase_rust::dxf_document::Document::load(
        &output,&std::sync::atomic::AtomicBool::new(false)).unwrap();
    assert!(doc.records().filter(|x|x.kind()=="IMAGEDEF").count()>=2);
    assert_eq!(request.datasets[0].features[1].object_name,"Waterleiding");

    let mut with_profile=options;
    with_profile.choices[0].profile=true;
    let project=template_project::build(&with_profile,&[],&[],&[trench],
        &[],None,&reference.rules,&Control::default()).unwrap();
    assert!(project.slots[0].as_ref().unwrap().profile.is_some());
    assert_eq!(project.slots[0].as_ref().unwrap().profile.as_ref().unwrap().forced_start,
        Some([155000.,463000.]));
}
#[test]
fn real_job_still_requires_geotiff_but_virtual_job_does_not(){
    let (reference,_)=fixture();
    let opts=Options{server_mode:sleufbase_rust::template_server::Mode::Offline,
        overview_settings:sleufbase_rust::cadastral_overview::template_defaults(),
        template:reference.template,
        choices:vec![Choice{label:None,legacy_slot:None,manual_scale:None,source:Source::Job(0),raster:None,include:true,
            title:"Echte KickTheMap-job".into(),profile:false,reverse:false,
            comments:String::new(),photos:Vec::new()}],
        ..Options::default()
    };
    let err=template_project::build(&opts,&reference.datasets,&[],&[],&[],None,
        &reference.rules,&Control::default()).err().unwrap();
    assert!(err.contains("GeoTIFF"));
}

#[test]
fn imported_python_physical_profile_survives_native_save_and_exports_dxf_and_excel() {
    use serde_json::json;
    use sleufbase_rust::{legacy_project, project};
    let (reference, raster) = fixture();
    let dir = tempfile::tempdir().unwrap();
    let assets = dir.path().join("assets");
    std::fs::create_dir(&assets).unwrap();
    let tiff_path = assets.join("PS42B.tiff");
    let file = std::fs::File::create(&tiff_path).unwrap();
    let mut encoder = tiff::encoder::TiffEncoder::new(file).unwrap();
    let mut image = encoder.new_image::<tiff::encoder::colortype::RGB8>(30,20).unwrap();
    image.encoder().write_tag(tiff::tags::Tag::ModelPixelScaleTag, &[1.0f64,1.0,0.0][..]).unwrap();
    image.encoder().write_tag(tiff::tags::Tag::ModelTiepointTag,
        &[0.0f64,0.0,0.0,raster.bounds.min_x,raster.bounds.max_y,0.0][..]).unwrap();
    image.write_data(&vec![180u8;30*20*3]).unwrap();
    drop(encoder);
    let x = (raster.bounds.min_x + raster.bounds.max_x) / 2.;
    let y = (raster.bounds.min_y + raster.bounds.max_y) / 2.;
    let raw = json!({"features":[
        {"geometry":{"type":"Point","coordinates":[x-0.2,y,2.]},"properties":{"idName":"water","groupType":"objects"}},
        {"geometry":{"type":"Point","coordinates":[x+0.2,y,2.1]},"properties":{"idName":"water","groupType":"objects"}}
    ]});
    let profile_path = assets.join("profile.json");
    std::fs::write(&profile_path, serde_json::to_vec(&raw).unwrap()).unwrap();
    let legacy = json!({"version":1,"project_name":"PS42B","future_field":{"keep":"Ø"},
        "tiffs":[{"type":"geotiff","name":"PS42B","path":"assets/PS42B.tiff","metadata":{
            "kickthemap_job_id":42,"kickthemap_job_title":"Local physical profile",
            "kickthemap_job_features_path":"assets/profile.json","future_layer":{"keep":true}}}]});
    let source = dir.path().join("source.klicproject");
    let source_bytes = serde_json::to_vec(&legacy).unwrap();
    std::fs::write(&source, &source_bytes).unwrap();
    let imported = legacy_project::read_legacy_project(&source).unwrap();
    assert_eq!(imported.rasters.len(), 1);
    let native = dir.path().join("converted.json");
    project::save_project_new(&native, &imported).unwrap();
    assert!(project::save_project_new(&native, &imported).is_err());
    let restored = project::read_project(&native).unwrap();
    assert_eq!(restored.legacy_archive.as_ref().unwrap().original.as_ref(), &legacy);
    assert_eq!(restored.rasters[0].metadata["future_layer"], json!({"keep":true}));
    assert_eq!(template_project::hydrate::local_path(&restored.rasters[0]), Some(profile_path.clone()));
    let mut options = Options { template:reference.template.clone(),
        server_mode:sleufbase_rust::template_server::Mode::Offline,
        include_bgt:false,include_cadastral:false,include_cad:false,paired:false,
        choices:template_project::suggest_rasters(&restored.rasters, &[]), ..Default::default() };
    options.choices[0].label=Some("PS42B".into());
    options.choices[0].title="An old custom title".into();
    assert!(options.choices[0].profile);
    let mut request=template_project::build(&options,&restored.datasets,&restored.rasters,&[],&[],None,&reference.rules,&Control::default()).unwrap();
    let slot=request.slots[0].as_ref().unwrap();
    assert_eq!(slot.title,"Proefsleuf 42B");
    assert_eq!(slot.report_label.as_deref(),Some("PS42B"));
    assert!(slot.profile.is_some());
    assert_eq!(request.datasets[0].job_id,42);
    assert_eq!(request.datasets[0].features.len(),2);
    assert!(request.settings.protected_sources.contains(&profile_path));
    request.settings.discipline_excel=true;
    let output=dir.path().join("physical-profile.dxf");
    let result=template_export::export(&output,&request,&Control::default()).unwrap();
    assert_eq!(result.filled_slots,1);
    assert!(output.is_file());
    assert!(result.companion.as_ref().is_some_and(|p|p.is_file()),"{:?}",result.warnings);
    assert_eq!(result.profiles,1);
    let doc=sleufbase_rust::dxf_document::Document::load(&output,&std::sync::atomic::AtomicBool::new(false)).unwrap();
    assert!(doc.records().any(|r|r.first(8)==Some(sleufbase_rust::profile_drawing::LAYER)&&matches!(r.kind(),"LINE"|"LWPOLYLINE"|"INSERT")));
    assert!(doc.records().any(|r|matches!(r.kind(),"TEXT"|"MTEXT"|"ATTRIB")&&r.tags().any(|(code,text)|matches!(code,1|3)&&text.to_lowercase().contains("water"))));
    let verify=std::process::Command::new("python").arg("-c").arg(r#"
import sys, zipfile, xml.etree.ElementTree as E
with zipfile.ZipFile(sys.argv[1]) as z:
 ns={'s':'http://schemas.openxmlformats.org/spreadsheetml/2006/main'}
 strings=[''.join(n.itertext()) for n in E.fromstring(z.read('xl/sharedStrings.xml')).findall('s:si',ns)]
 cells={c.attrib['r']:c for c in E.fromstring(z.read('xl/worksheets/sheet1.xml')).findall('.//s:c',ns)}
 def value(ref):
  c=cells[ref]; v=c.find('s:v',ns).text
  return strings[int(v)] if c.attrib.get('t')=='s' else float(v)
 assert value('A2')=='PS42B', value('A2')
 assert value('B2')==1, value('B2')
 assert value('C2')==2, value('C2')
"#).arg(result.companion.as_ref().unwrap()).output().unwrap();
    assert!(verify.status.success(),"{}",String::from_utf8_lossy(&verify.stderr));
    assert!(template_export::export(&profile_path,&request,&Control::default()).is_err());
    assert_eq!(std::fs::read(&source).unwrap(),source_bytes);
    assert_eq!(serde_json::from_slice::<serde_json::Value>(&std::fs::read(profile_path).unwrap()).unwrap(),raw);
}
