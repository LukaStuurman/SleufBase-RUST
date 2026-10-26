//! Original Python project settings and slot serialization. Unsupported active
//! native choices fail explicitly; no ignored extension payload claims parity.
use super::*;
fn unsupported(field:&str)->String{format!("Python-project kan de actieve sjabloonvoorkeur '{field}' niet verliesvrij toepassen")}
fn standard_techbase(path:&Path)->bool{
    std::fs::read(path).is_ok_and(|bytes|bytes==include_bytes!("../../assets/template_logos/techbase_full.png"))
}
fn merge_rules(active:&[crate::disciplines::Rule],old:Option<&Value>)->Result<Value,String>{
    let originals=old.and_then(Value::as_array).cloned().unwrap_or_default();let mut used=HashSet::new();let mut output=Vec::new();
    let extra=|v:&Value|v.as_array().is_some_and(|a|a.len()>4)||v.as_object().is_some_and(|m|m.keys().any(|key|!matches!(key.as_str(),"keywords"|"target_layer"|"color"|"profile_label")));
    for rule in active{
        if rule.keywords.iter().any(|word|word.contains([',',';','\n'])){return Err(unsupported("objectcodering bevat Python-scheidingsteken"));}
        let hits=originals.iter().enumerate().filter(|(i,v)|!used.contains(i)&&v.get("target_layer").or_else(||v.get(1)).and_then(Value::as_str)==Some(rule.target_layer.as_ref())).map(|(i,_)|i).collect::<Vec<_>>();
        if hits.len()>1&&hits.iter().any(|i|extra(&originals[*i])){return Err(unsupported("dubbelzinnige onbekende objectregelvelden"));}
        let previous=hits.first().map(|i|{used.insert(*i);originals[*i].clone()});
        let words:Value=rule.keywords.iter().map(|w|w.as_ref()).collect::<Vec<_>>().join(", ").into();
        if let Some(Value::Array(mut raw))=previous{raw.resize(raw.len().max(4),Value::Null);raw[0]=words;raw[1]=rule.target_layer.to_string().into();raw[2]=rule.color_aci.into();raw[3]=rule.label.to_string().into();output.push(raw.into());}else{
            let mut raw=previous.as_ref().map(object).unwrap_or_default();raw.insert("keywords".into(),words);raw.insert("target_layer".into(),rule.target_layer.to_string().into());raw.insert("color".into(),rule.color_aci.into());raw.insert("profile_label".into(),rule.label.to_string().into());output.push(Value::Object(raw));
        }
    }
    if originals.iter().enumerate().any(|(i,v)|!used.contains(&i)&&extra(v)){return Err(unsupported("gewijzigde objectregel met onbekende velden"));}Ok(output.into())
}
fn settings(active:&crate::settings_compat::MigratedSettings,raw:&mut Map<String,Value>)->Result<(),String>{
    let defaults=crate::settings_compat::MigratedSettings::default();
    let a=serde_json::to_value(active).map_err(|e|e.to_string())?;let d=serde_json::to_value(&defaults).map_err(|e|e.to_string())?;
    for key in ["template_auto_fill_bgt_fysiek_voorkomen","template_auto_export_discipline_excel","kickthemap_material_choices","kickthemap_profile_extra_choices","marxact_name_mappings"]{if a[key]!=d[key]{return Err(unsupported(key));}}
    if active.map_export.dpi!=defaults.map_export.dpi||active.map_export.scale!=defaults.map_export.scale{return Err(unsupported("kaart-DPI/schaal"));}
    if !active.export_preferences.cadastral_template_path.is_empty(){return Err(unsupported("DXF-sjabloonpad"));}
    if !matches!(active.map_export.background.as_str(),"pdok"|"osm"|"cadastral"){return Err(unsupported("kaartachtergrond"));}
    let preferences=serde_json::to_value(&active.export_preferences).map_err(|e|e.to_string())?;
    for(key,value)in preferences.as_object().unwrap(){if key!="cadastral_template_path"{raw.insert(key.clone(),value.clone());}}
    if !active.export_preferences.template_client_logo_path.is_empty(){raw.insert("template_client_logo_path".into(),path_value(Path::new(&active.export_preferences.template_client_logo_path)));}
    raw.insert("map_export_background_source".into(),active.map_export.background.clone().into());raw.insert("include_klic_in_map_exports".into(),active.map_export.include_cad.into());raw.insert("dxf_label_gap".into(),json!(active.dxf_label_gap));
    raw.insert("kickthemap_object_layer_rules".into(),merge_rules(&active.object_rules,raw.get("kickthemap_object_layer_rules"))?);Ok(())
}
fn layer_key(layer:&Value)->Result<String,String>{
    if let Some(id)=layer["metadata"]["kickthemap_job_id"].as_u64().filter(|id|*id>0){return Ok(format!("job:{id}"));}
    if layer["type"]=="virtual_trench"{let name=layer["name"].as_str().ok_or("Sleufnaam ontbreekt")?;return Ok(format!("virtual:{}",crate::python_values::casefold(name)));}
    let path=layer["path"].as_str().ok_or("Rasterpad ontbreekt")?;Ok(format!("path:{}",crate::python_values::casefold(path)))
}
fn overview(active:&crate::cadastral_overview::Settings,raw:&mut Map<String,Value>){
    raw.insert("dxf_trench_export_mode".into(),match active.trench_mode{crate::cadastral_overview::TrenchMode::Centerline=>"centerline",crate::cadastral_overview::TrenchMode::Polygon=>"polygon",crate::cadastral_overview::TrenchMode::None=>"none"}.into());raw.insert("include_tiffs_in_dxf_export".into(),active.include_tiff_images.into());
    for(key,rgb)in [("dxf_centerline_color",active.centerline_color),("dxf_label_color",active.label_color)]{raw.insert(key.into(),format!("#{:02X}{:02X}{:02X}",rgb[0],rgb[1],rgb[2]).into());}
}
fn state(project:&NativeProject,state:&crate::template_order::State,raw:&mut Map<String,Value>,output:&mut Map<String,Value>,tiffs:&mut [Value],source_targets:&std::collections::HashMap<usize,usize>)->Result<(),String>{
    state.validate()?;
    if state.server_mode==crate::template_server::Mode::Offline{return Err(unsupported("offline servergegevens"));}
    for(field,bad)in [("ongepaarde export",!state.paired),("omgekeerde basisrichting",state.reverse),("DXF-sjabloonpad",state.template.as_ref().is_some_and(|p|!p.as_os_str().is_empty())),("lettertypemappen",!state.font_directories.is_empty()),("getekend door",!state.drawn_by.is_empty()),("afwijkende sjabloonprojectnaam",!state.project.is_empty()&&state.project!=project.name),("geen luchtfoto",!state.aerial),("automatisch BGT uit",!state.auto_bgt),("eigen Techbase-logo",state.logos.techbase.as_ref().is_some_and(|path|!standard_techbase(path))) ]{if bad{return Err(unsupported(field));}}
    if !(0.005..=0.2).contains(&state.marker_diameter){return Err(unsupported("markerdiameter buiten Python-bereik"));}
    overview(&state.overview_settings,raw);
    for(key,value)in [("template_include_cross_sections",state.profiles),("template_show_profile_direction",state.show_reference),("template_enable_maaiveld_edit_points",state.use_custom_maaiveld),("template_avoid_multileader_collisions",state.avoid_collisions),("template_clip_cross_section_markers_to_profile",state.clip_markers)]{raw.insert(key.into(),value.into());}
    raw.insert("template_cross_section_marker_diameter".into(),json!(state.marker_diameter));raw.insert("template_cross_section_diameter_text".into(),state.diameter_prefix.clone().into());raw.insert("dxf_label_gap".into(),json!(state.label_gap));
    if let Some(path)=&state.logos.client{raw.insert("template_client_logo_preset".into(),"custom".into());raw.insert("template_client_logo_path".into(),path_value(path));}else{raw.insert("template_client_logo_preset".into(),"".into());raw.insert("template_client_logo_path".into(),"".into());}
    let originals=output.get("template_export_slots").and_then(Value::as_array).cloned().unwrap_or_default();let mut used=HashSet::new();let mut rows=Vec::<Value>::new();let labels=state.labels()?;
    for(index,(row,label))in state.rows.iter().zip(labels).enumerate(){
        if row.dataset.is_some()||row.without_profile||!row.comments.is_empty()||row.address.is_some(){return Err(unsupported("profielkoppeling/opmerking/adres per slot"));}
        if row.scale.is_some_and(|s|!(20..=5000).contains(&s)){return Err(unsupported("profielschaal buiten Python-bereik"));}
        let target=if let Some(target)=source_targets.get(&index){Some(*target)}else if let Some(path)=&row.raster{let path=path_value(path);let hits=tiffs.iter().enumerate().filter(|(_,layer)|layer["path"]==path).map(|(i,_)|i).collect::<Vec<_>>();if hits.len()!=1{return Err(unsupported("ontbrekende/dubbelzinnige raster-slotbron"));}Some(hits[0])}else{None};
        let key=target.map(|i|layer_key(&tiffs[i])).transpose()?;
        let hits=if let Some(i)=row.legacy_slot{
            let original=originals.get(i).ok_or_else(||unsupported("oorspronkelijke slotidentiteit ontbreekt"))?;
            let original_key=original.get("layer_key").filter(|v|!v.is_null()).map(|v|crate::python_values::string(Some(v))).transpose()?.map(|v|crate::python_values::trim(&v).to_owned()).filter(|v|!v.is_empty());
            if used.contains(&i)||original_key.as_deref()!=key.as_deref(){return Err(unsupported("oorspronkelijke slotidentiteit is gewijzigd of dubbel gebruikt"));}
            vec![i]
        }else{originals.iter().enumerate().filter(|(i,v)|!used.contains(i)&&v["layer_key"].as_str()==key.as_deref()).map(|(i,_)|i).collect::<Vec<_>>()};
        let extras=|v:&Value|v.as_object().is_some_and(|m|m.keys().any(|key|!matches!(key.as_str(),"layer_key"|"included"|"scale"|"ps_number"|"variant"|"variant_code")));
        if hits.len()>1&&hits.iter().any(|i|extras(&originals[*i])){return Err(unsupported("dubbelzinnige onbekende slotvelden"));}
        let mut entry=hits.first().map(|i|{used.insert(*i);object(&originals[*i])}).unwrap_or_default();
        entry.insert("layer_key".into(),key.map(Value::String).unwrap_or(Value::Null));entry.insert("included".into(),target.is_none().then_some(true).unwrap_or(row.included).into());entry.insert("scale".into(),if target.is_some(){json!(row.scale)}else{Value::Null});entry.insert("ps_number".into(),if target.is_some(){crate::template_order::number(&row.number).unwrap_or_default()}else{String::new()}.into());let variant=crate::template_order::variant(&json!(row.variant));entry.insert("variant".into(),(!variant.is_empty()).into());entry.insert("variant_code".into(),variant.clone().into());
        if let Some(i)=target{
            let metadata=tiffs[i]["metadata"].as_object_mut().ok_or("Rastermetadata ontbreekt")?;
            if let Some(values)=row.metadata.as_object(){for(key,value)in values{metadata.insert(key.clone(),value.clone());}}
            metadata.insert("template_export_order".into(),index.into());metadata.insert("template_export_include".into(),row.included.into());metadata.insert("template_export_ps_number".into(),crate::template_order::number(&row.number).unwrap_or_default().into());metadata.insert("template_export_variant".into(),(!variant.is_empty()).into());if variant.is_empty(){metadata.remove("template_export_variant_code");}else{metadata.insert("template_export_variant_code".into(),variant.into());}
            if let Some(scale)=row.scale{metadata.insert("template_cross_section_scale_denominator".into(),scale.into());}else{metadata.remove("template_cross_section_scale_denominator");}
            if let Some(label)=label{metadata.insert("template_proefsleuf_label".into(),label.into());}else{metadata.remove("template_proefsleuf_label");}
        }
        rows.push(entry.into());
    }
    if originals.iter().enumerate().any(|(i,v)|!used.contains(&i)&&v.as_object().is_some_and(|m|m.keys().any(|key|!matches!(key.as_str(),"layer_key"|"included"|"scale"|"ps_number"|"variant"|"variant_code")))){return Err(unsupported("verwijderd/hernoemd slot met onbekende velden"));}
    output.insert("template_export_slots".into(),rows.into());Ok(())
}
fn preferences(project:&NativeProject,prefs:&crate::template_prefs::TemplatePreferences,raw:&mut Map<String,Value>,output:&mut Map<String,Value>,tiffs:&mut [Value])->Result<(),String>{
    use crate::template_project::{BackgroundKind,Source};
    for(field,bad)in [("DXF-sjabloonpad",!prefs.template.as_os_str().is_empty()),("kaart-DPI/schaal",prefs.dpi!=150||prefs.scale!=225.),("lege kaartachtergrond",prefs.background!=BackgroundKind::Pdok),("uitgeschakelde BGT/Kadaster/CAD-lagen",!prefs.include_bgt||!prefs.include_cadastral||!prefs.include_cad)]{if bad{return Err(unsupported(field));}}
    let options=prefs.restore(&project.datasets,&project.rasters,&project.virtual_trenches);let style=&prefs.profile_style;
    let mut active=crate::template_order::State{server_mode:prefs.server_mode,overview_settings:prefs.overview_settings.clone(),label_gap:prefs.overview_settings.label_gap,logos:prefs.logos.clone(),template:None,paired:prefs.paired,project:if prefs.legacy_layout_only{project.name.clone()}else{prefs.project.clone()},drawn_by:prefs.drawn_by.clone(),font_directories:prefs.font_directories.clone(),auto_bgt:style.auto_fill_bgt,show_reference:style.show_reference,avoid_collisions:style.avoid_collisions,marker_diameter:style.marker_diameter,use_custom_maaiveld:style.use_custom_maaiveld,clip_markers:style.clip_markers,diameter_prefix:style.diameter_prefix.clone(),..Default::default()};
    let flags=options.choices.iter().filter(|c|!matches!(c.source,Source::Empty(_))).map(|c|c.profile).collect::<HashSet<_>>();if flags.len()>1{return Err(unsupported("verschillende profielopties per slot"));}active.profiles=flags.iter().next().copied().unwrap_or(true);
    let expression=regex::Regex::new(r"(?i)^PS\s*(\d+)([A-Z]*)$").map_err(|e|e.to_string())?;let mut targets=std::collections::HashMap::new();
    for choice in options.choices{
        if choice.reverse||!choice.photos.is_empty()||!choice.comments.is_empty(){return Err(unsupported("omgekeerde richting/foto/opmerking per slot"));}
        let raster=match choice.source{
            Source::Empty(_)=>{if !choice.include{continue;}active.rows.push(crate::template_order::Row{legacy_slot:choice.legacy_slot,..Default::default()});continue;},
            Source::Virtual(index)=>{if choice.raster.is_some(){return Err(unsupported("virtuele profielbron gekoppeld aan afzonderlijke TIFF"));}targets.insert(active.rows.len(),index);Some(format!("native-virtual-{index}").into())},
            Source::Raster(index)=>{
                if choice.raster!=Some(index){return Err(unsupported("gewijzigde TIFF-slotkoppeling"));}
                let raster=project.rasters.get(index).ok_or_else(||unsupported("ontbrekende TIFF-slotbron"))?;
                // Exchange groups virtual layers first, followed by physical
                // layers in native order. The restored choice already resolved
                // its durable origin; a path may belong to multiple layers.
                targets.insert(active.rows.len(),project.virtual_trenches.len()+index);
                Some(raster.path.clone())
            },
            Source::Job(_)=>return Err(unsupported("losse profieljob")),
        };
        let caps=expression.captures(choice.label.as_deref().unwrap_or(&choice.title)).ok_or_else(||unsupported("slotnaam buiten PS-nummer/variant"))?;let number=crate::template_order::number(&caps[1]).ok_or_else(||unsupported("PS-nummer"))?;let variant=caps[2].to_ascii_uppercase();if variant=="AUTO"&&choice.label.is_none(){return Err(unsupported("letterlijke AUTO-variantnaam"));}
        active.rows.push(crate::template_order::Row{legacy_slot:choice.legacy_slot,raster,included:choice.include,scale:choice.manual_scale,number,variant,..Default::default()});
    }
    let unchanged_settings=prefs.legacy_layout_only.then(||raw.clone());
    let result=state(project,&active,raw,output,tiffs,&targets);
    if let Some(unchanged)=unchanged_settings{*raw=unchanged;}
    result
}
pub(super) fn merge(project:&NativeProject,output:&mut Map<String,Value>,tiffs:&mut [Value])->Result<(),String>{
    let mut raw=object(output.get("template_export_settings").unwrap_or(&Value::Null));
    if let Some(active)=&project.project_settings{settings(active,&mut raw)?;}
    if project.template_preferences.is_some()&&project.template_export.is_some(){return Err(unsupported("twee onafhankelijke sjabloonvolgordes"));}
    if let Some(active)=&project.template_preferences{preferences(project,active,&mut raw,output,tiffs)?;}
    if let Some(active)=&project.template_export{state(project,active,&mut raw,output,tiffs,&std::collections::HashMap::new())?;}
    if project.project_settings.is_some()||project.template_export.is_some()||project.template_preferences.is_some(){output.insert("template_export_settings".into(),raw.into());}Ok(())
}

#[cfg(test)]mod slot_identity_tests{
    use super::*;
    #[test]fn two_empty_slots_keep_separate_unknown_fields_after_reorder_and_native_save(){
        let mut project=crate::legacy_project::import_value(json!({"version":1,"future_project":{"keep":true},"template_export_slots":[{"layer_key":null,"custom":{"name":"first"}},{"layer_key":null,"custom":{"name":"second"}}]}),Path::new("source.klicproject")).unwrap();
        project.template_preferences=None;
        project.template_export=Some(crate::template_order::State{rows:vec![crate::template_order::Row{legacy_slot:Some(1),..Default::default()},crate::template_order::Row{legacy_slot:Some(0),..Default::default()}],..Default::default()});
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("saved.json");crate::project::save_project(&path,&project).unwrap();let saved=crate::project::read_project(&path).unwrap();
        let payload=crate::legacy_project::legacy_payload(&saved,&crate::legacy_project::ProjectUiState::default()).unwrap();
        assert_eq!(payload["template_export_slots"][0]["custom"]["name"],"second");assert_eq!(payload["template_export_slots"][1]["custom"]["name"],"first");assert_eq!(payload["future_project"],json!({"keep":true}));
        let again=crate::legacy_project::import_value(payload,Path::new("new.klicproject")).unwrap();let layout=crate::legacy_project::template_import::restore(&again).unwrap().unwrap();assert_eq!(layout.slots.len(),2);assert_eq!(layout.slots[0].original["custom"]["name"],"second");
        project.template_export.as_mut().unwrap().rows[1].legacy_slot=Some(1);assert!(crate::legacy_project::legacy_payload(&project,&crate::legacy_project::ProjectUiState::default()).is_err());
    }
}
