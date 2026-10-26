#[path="support/template_server.rs"] mod support;
use sleufbase_rust::{template_server::{self,Mode},export_control::Control};
#[test]
fn active_original_local_service_bounds_phase_order_deduplication_and_fallback(){
    for case in support::cases().as_array().unwrap(){
        let mut request=support::request(case);let mut script=support::Script::new(case);
        let bounds=request.slots.iter().flatten().map(|s|s.bounds).collect::<Vec<_>>();
        if case["name"]=="wide-merge" {
            let boxes=template_server::fetch_bounds(&bounds).unwrap();
            assert_eq!(boxes.len(),15,"active v3 retains all distant local areas");
            assert!(boxes.iter().all(|b|b.max_x-b.min_x<=1510.));
            for b in boxes {for kind in [sleufbase_rust::profile_context::Kind::Roads,sleufbase_rust::profile_context::Kind::Terrain] {assert!(sleufbase_rust::profile_context::url(kind,b).is_ok());}}
        }
        assert_eq!(serde_json::to_value(template_server::fetch_bounds(&bounds).unwrap()).unwrap(),case["fetch_bounds"]);
        template_server::refresh(&mut request,Mode::Fresh,true,true,&mut script,&Control::default()).unwrap();
        assert_eq!(script.cursor,script.calls.len());
        let overview=request.overview.unwrap();
        assert_eq!(serde_json::to_value(overview.linework).unwrap(),case["expected"]["linework"]);
        assert_eq!(serde_json::to_value(overview.labels).unwrap(),case["expected"]["labels"]);
        assert_eq!(serde_json::to_value(request.roads).unwrap(),case["expected"]["roads"]);
        assert_eq!(serde_json::to_value(request.terrain_boundaries).unwrap(),case["expected"]["terrain"]);
        assert_eq!(request.surfaces.len(),case["expected"]["surfaces"].as_array().unwrap().len());
    }
}
#[test]
fn offline_and_late_failure_or_cancel_never_replace_the_loaded_request(){
    let case=&support::cases()[1];
    for cancel in [false,true]{let mut request=support::request(case);let before=support::snapshot(&request);let mut script=support::Script::new(case);let last=script.calls.len()-1;if cancel{script.cancel_at=Some(last)}else{script.fail_at=Some(last)}
        assert!(template_server::refresh(&mut request,Mode::Fresh,true,true,&mut script,&Control::default()).is_err());assert_eq!(support::snapshot(&request),before);
    }
    let mut request=support::request(case);let before=support::snapshot(&request);let mut script=support::Script::new(case);
    template_server::refresh(&mut request,Mode::Offline,true,true,&mut script,&Control::default()).unwrap();assert_eq!(script.cursor,0);assert_eq!(support::snapshot(&request),before);
}
#[test]
fn both_desktop_planners_fetch_selected_order_and_keep_stale_service_layers_off_slot_maps(){
    use sleufbase_rust::{export_worker::Inputs,geotiff::GeoTiffInfo,geo::GeoTransform,Dataset,template_project,template_workflow,template_order::State};
    let cases=support::cases();let case=&cases[0];let root=std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut inputs=Inputs::default();
    for (i,b) in case["bounds"].as_array().unwrap().iter().enumerate(){let bounds:sleufbase_rust::Bounds=serde_json::from_value(b.clone()).unwrap();let path=root.join("tests/fixtures/cadastral-overview").join(if i==0{"fase_2_ps_00021_66587.tiff"}else{"PS-21_same.tiff"});
        inputs.rasters.push(GeoTiffInfo{path,width:40,height:32,transform:GeoTransform{a:(bounds.max_x-bounds.min_x)/40.,b:0.,c:bounds.min_x,d:0.,e:-(bounds.max_y-bounds.min_y)/32.,f:bounds.max_y},bounds,epsg:Some(28992),kickthemap_job_id:Some(i as u64+1),kickthemap_job_title:String::new(),kickthemap_job_sidecar:None,metadata:Default::default()});
        inputs.datasets.push(Dataset{job_id:i as u64+1,title:format!("PS{}",i+1),source_path:format!("job{i}.json").into(),features:Vec::new()});
    }
    let bounds=inputs.rasters[0].bounds;
    inputs.bgt.push(sleufbase_rust::bgt::BgtLayer{kind:sleufbase_rust::bgt::BgtKind::Wegdeel,bounds,visible:true,paths:vec![vec![(bounds.min_x,bounds.min_y),(bounds.max_x,bounds.max_y)]]});
    inputs.cadastral=Some(sleufbase_rust::cadastral::CadastralLayer{bounds,visible:true,linework:vec![sleufbase_rust::cadastral::CadLinework{layer_name:"STALE_LOADED_LAYER".into(),paths:inputs.bgt[0].paths.clone()}],labels:Vec::new()});
    let opts=template_project::Options{template:root.join("../assets/cadastral_template.dxf"),choices:template_project::suggest(&inputs.datasets,&inputs.rasters,&[]),..Default::default()};
    let mut script=support::Script::new(case);let project=template_project::build_from_inputs_with_provider(&opts,&inputs,&[],&Control::default(),&mut script).unwrap();assert_eq!(script.cursor,script.calls.len());
    assert!(project.slots.iter().flatten().all(|s|s.map_source.as_ref().unwrap().overlays.is_empty()));
    let mut state=State{template:Some(opts.template),aerial:false,..Default::default()};state.sync(&inputs.rasters).unwrap();
    let mut script=support::Script::new(case);let workflow=template_workflow::request_with_provider(&state,&inputs,&[],&Control::default(),&mut script).unwrap();assert_eq!(script.cursor,script.calls.len());
    assert!(workflow.slots.iter().flatten().all(|s|s.map_source.as_ref().unwrap().overlays.is_empty()));
    for request in [project,workflow]{assert_eq!(serde_json::to_value(&request.overview.unwrap().linework).unwrap(),case["expected"]["linework"]);assert_eq!(serde_json::to_value(&request.roads).unwrap(),case["expected"]["roads"]);}
    assert_eq!(inputs.cadastral.unwrap().linework[0].layer_name,"STALE_LOADED_LAYER");
}

#[test]
fn fresh_address_lookup_is_best_effort_staged_and_offline_never_calls_it(){
    use sleufbase_rust::{Bounds,cadastral::CadLabel,bgt_features::SurfaceFeature,template_server::{Provider,Paths}};
    struct Addresses{calls:usize,cancel:bool,fail:bool}
    impl Provider for Addresses{
        fn outlines(&mut self,_:Bounds,_:&Control)->Result<Paths,String>{Ok(Vec::new())}
        fn surfaces(&mut self,_:Bounds,_:&Control)->Result<Vec<SurfaceFeature>,String>{Ok(Vec::new())}
        fn parcels(&mut self,_:Bounds,_:&Control)->Result<Paths,String>{Ok(Vec::new())}
        fn labels(&mut self,_:Bounds,_:&Control)->Result<Vec<CadLabel>,String>{Ok(Vec::new())}
        fn roads(&mut self,_:Bounds,_:&Control)->Result<Paths,String>{Ok(Vec::new())}
        fn terrain(&mut self,_:Bounds,_:&Control)->Result<Paths,String>{Ok(Vec::new())}
        fn address(&mut self,b:Bounds,c:&Control)->Result<Option<String>,String>{self.calls+=1;assert!(b.min_x<b.max_x);if self.cancel{c.cancel();}if self.fail{return Err("address unavailable".into());}Ok(Some("Street,\\PCity".into()))}
    }
    let case=&support::cases()[0];let mut request=support::request(case);request.slots.iter_mut().flatten().next().unwrap().address=Some("Explicit address".into());let count=request.slots.iter().flatten().count();
    let mut provider=Addresses{calls:0,cancel:false,fail:false};template_server::refresh(&mut request,Mode::Fresh,false,false,&mut provider,&Control::default()).unwrap();assert_eq!(provider.calls,count-1);assert_eq!(request.slots.iter().flatten().next().unwrap().address.as_deref(),Some("Explicit address"));assert!(request.slots.iter().flatten().skip(1).all(|s|s.address.as_deref()==Some("Street,\\PCity")));
    // Exercise the complete publication path: the injected lookup must reach
    // the actual DXF address entity, not merely the planner's request.
    let mut exported=request;exported.overview=None;
    let root=std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");exported.template=root.join(&exported.template);
    for slot in exported.slots.iter_mut().flatten(){slot.profile=None;if !slot.tiff_png.as_os_str().is_empty(){slot.tiff_png=root.join(&slot.tiff_png);}if let Some(source)=&mut slot.tiff_source{source.path=root.join(&source.path);}slot.map_png=root.join(&slot.map_png);}
    let dir=tempfile::tempdir().unwrap();let path=dir.path().join("addresses.dxf");sleufbase_rust::template_export::export(&path,&exported,&Control::default()).unwrap();let drawing=std::fs::read_to_string(path).unwrap();assert!(drawing.contains("Explicit address"));assert!(drawing.contains("Street,\\PCity"));
    for offline in [false,true]{let mut request=support::request(case);let mut provider=Addresses{calls:0,cancel:false,fail:true};template_server::refresh(&mut request,if offline{Mode::Offline}else{Mode::Fresh},false,false,&mut provider,&Control::default()).unwrap();assert_eq!(provider.calls,if offline{0}else{count});assert!(request.slots.iter().flatten().all(|s|s.address.is_none()));}
    let mut request=support::request(case);let before=(support::snapshot(&request),request.slots.iter().map(|s|s.as_ref().and_then(|s|s.address.clone())).collect::<Vec<_>>());let mut provider=Addresses{calls:0,cancel:true,fail:false};assert!(template_server::refresh(&mut request,Mode::Fresh,false,false,&mut provider,&Control::default()).is_err());assert_eq!((support::snapshot(&request),request.slots.iter().map(|s|s.as_ref().and_then(|s|s.address.clone())).collect::<Vec<_>>()),before);
}
