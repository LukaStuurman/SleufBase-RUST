//! Active Python order restoration: validated KlicViewerApp initial order and
//! app.py:_install_template_variant_order_patch. Unknown originals stay attached.
use super::*;
use std::collections::{HashMap,VecDeque};
#[derive(Clone,Debug,PartialEq,Eq)]
pub enum Source {Raster(PathBuf),Virtual{job_id:u64,label:String},Empty}
#[derive(Clone,Debug)]
pub struct Slot {pub original_index:Option<usize>,pub source:Source,pub included:bool,pub scale:Option<i64>,pub number:String,pub variant:String,pub metadata:Value,pub original:Value}
#[derive(Clone,Debug,Default)]
pub struct Layout {pub slots:Vec<Slot>,pub warnings:Vec<String>}
fn key(source:&Source,metadata:&Value)->String{
    let job=metadata.get("kickthemap_job_id").and_then(|v|crate::python_values::string(Some(v)).ok()).and_then(|s|s.parse::<u64>().ok()).filter(|n|*n>0);
    if let Some(id)=job{return format!("job:{id}");}
    match source{Source::Raster(path)=>{let path=if path.is_absolute()||path.to_string_lossy().as_bytes().get(1)==Some(&b':'){path.clone()}else{std::env::current_dir().unwrap_or_default().join(path)};format!("path:{}",crate::python_values::casefold(&path.to_string_lossy()))},Source::Virtual{label,..}=>format!("virtual:{}",crate::python_values::casefold(label)),Source::Empty=>String::new()}
}
fn source_path(source:&Source)->PathBuf{match source{Source::Raster(path)=>path.clone(),Source::Virtual{label,..}=>format!("{label}.tif").into(),Source::Empty=>PathBuf::new()}}
fn included(value:Option<&Value>,slot:bool)->bool{
    match value{Some(Value::String(s))=>!matches!(crate::python_values::casefold(crate::python_values::trim(s)).as_str(),"false"|"off"|"nee"|"no"|"0"),Some(Value::Bool(b))=>*b,Some(v)if slot=>crate::python_values::truth(v),_=>true}
}
fn saved_key(raw:&Value)->Option<String>{raw.get("layer_key").filter(|v|!v.is_null()).and_then(|v|crate::python_values::string(Some(v)).ok()).map(|s|crate::python_values::trim(&s).to_owned()).filter(|s|!s.is_empty())}
fn normalized_number(value:Option<&Value>)->Option<String>{let text=crate::python_values::string(value.filter(|v|crate::python_values::truth(v))).ok()?;crate::template_order::number(&text)}
fn order_value(value:Option<&Value>)->Option<i64>{match value?{Value::String(s)=>{let s=crate::python_values::trim(s);let mapped=s.chars().map(|c|crate::object_layout::decimal(c).map_or(c,|n|char::from(b'0'+n as u8))).collect::<String>();mapped.parse().ok()},Value::Bool(b)=>Some(i64::from(*b)),Value::Number(n)=>n.as_i64().or_else(||n.as_f64().filter(|n|n.is_finite()).map(|n|n as i64)),_=>None}}

fn sort_key(source:&Source,metadata:&Value)->(bool,i64,u8,String,String){
    let order=order_value(metadata.get("template_export_order"));
    let path=source_path(source);let stem=path.file_stem().unwrap_or_default().to_string_lossy();
    let ps=regex::Regex::new(r"(?i)\bps[\s._-]*(\d+)\b").unwrap();let digits=regex::Regex::new(r"\d+").unwrap();
    let (class,number)=if let Some(c)=ps.captures(&stem){(0,crate::template_order::number(&c[1]).unwrap_or_default())}else if let Some(c)=digits.find(&stem){(1,crate::template_order::number(c.as_str()).unwrap_or_default())}else{(2,"0".into())};
    // Positive arbitrary-precision integer ordering using length + digit text.
    (order.is_none(),order.unwrap_or(0),class,format!("{:08}{}",number.len(),number),crate::python_values::casefold(&stem))
}
pub fn restore(project:&NativeProject)->Result<Option<Layout>,String>{
    let Some(archive)=&project.legacy_archive else{return Ok(None);};
    let raw=archive.original.get("template_export_slots").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]);
    let mut sources=project.rasters.iter().map(|r|(Source::Raster(r.path.clone()),r.metadata.clone())).chain(project.virtual_trenches.iter().map(|v|(Source::Virtual{job_id:v.job_id,label:v.label.clone()},v.metadata.clone()))).collect::<Vec<_>>();
    sources.sort_by_cached_key(|(s,m)|sort_key(s,m));
    let mut used=std::collections::HashSet::new();let mut rows=Vec::new();let mut out=Layout::default();
    let mut numbers:HashMap<String,VecDeque<String>>=HashMap::new();let mut variants:HashMap<String,VecDeque<String>>=HashMap::new();
    for entry in raw.iter().filter(|v|v.is_object()) {if let Some(k)=saved_key(entry){if let Some(number)=normalized_number(entry.get("ps_number")){numbers.entry(k.clone()).or_default().push_back(number);}
        variants.entry(k).or_default().push_back(crate::template_order::variant(entry.get("variant_code").or_else(||entry.get("variant")).unwrap_or(&Value::Bool(false))));}}
    for (raw_index,entry) in raw.iter().enumerate().filter(|(_,v)|v.is_object()){
        let Some(k)=saved_key(entry)else{rows.push((None,entry.clone(),Some(raw_index)));continue;};
        if let Some((i,_))=sources.iter().enumerate().find(|(i,(s,m))|!used.contains(i)&&key(s,m)==k){used.insert(i);rows.push((Some(i),entry.clone(),Some(raw_index)));}
        else{out.warnings.push(format!("Opgeslagen sjabloonbron {k} is niet geladen; oorspronkelijk slot blijft in het bronarchief"));}
    }
    for i in 0..sources.len(){if !used.contains(&i){rows.push((Some(i),Value::Null,None));}}
    if rows.len()>5000{return Err("Maximaal 5000 bronnen en lege vakken per sjabloonexport".into());}
    for(index,(source,original,original_index))in rows.into_iter().enumerate(){
        let Some(i)=source else{out.slots.push(Slot{original_index,source:Source::Empty,included:true,scale:None,number:String::new(),variant:String::new(),metadata:Value::Null,original});continue;};
        let (source,metadata)=&sources[i];let k=key(source,metadata);
        let number=numbers.get_mut(&k).and_then(VecDeque::pop_front).or_else(||normalized_number(metadata.get("template_export_ps_number"))).unwrap_or_else(||crate::template_order::default_number(&source_path(source),index+1));
        let variant=variants.get_mut(&k).and_then(VecDeque::pop_front).unwrap_or_else(||{let code=crate::template_order::variant(metadata.get("template_export_variant_code").or_else(||metadata.get("template_export_variant")).unwrap_or(&Value::Bool(false)));if code.is_empty(){crate::template_order::default_variant(&source_path(source))}else{code}});
        out.slots.push(Slot{original_index,source:source.clone(),included:if original.is_object(){included(original.get("included"),true)}else{included(metadata.get("template_export_include"),false)},scale:crate::template_scale::coerce(original.get("scale")).or_else(||crate::template_scale::coerce(metadata.get(crate::template_scale::METADATA_KEY))),number,variant,metadata:metadata.clone(),original});
    }Ok(Some(out))
}

#[cfg(test)]mod tests{
    use super::*;
    fn project(case:&Value)->NativeProject{
        let tiffs=case["layers"].as_array().unwrap().iter().map(|layer|{
            let mut metadata=layer["metadata"].as_object().unwrap().clone();if layer["job"].as_u64().unwrap()>0{metadata.insert("kickthemap_job_id".into(),layer["job"].clone());}
            metadata.insert("virtual_trench_payload".into(),serde_json::json!({"width_meters":0.6,"points":[{"role":"start","x":0.,"y":0.,"z":0.},{"role":"end","x":10.,"y":0.,"z":0.}]}));
            serde_json::json!({"type":"virtual_trench","name":layer["name"],"metadata":metadata})
        }).collect::<Vec<_>>();
        import_value(serde_json::json!({"version":1,"tiffs":tiffs,"template_export_slots":case["raw"]}),Path::new("fixture.klicproject")).unwrap()
    }
    #[test]fn all_imported_order_rows_match_active_original_methods(){
        let fixture:Value=serde_json::from_str(include_str!("../tests/fixtures/python-slot-import.json")).unwrap();
        for case in fixture["cases"].as_array().unwrap(){let project=project(case);let before=project.legacy_archive.as_ref().unwrap().original.clone();let restored=restore(&project).unwrap().unwrap();
            let rows=restored.slots.iter().map(|s|serde_json::json!({"name":match &s.source{Source::Virtual{label,..}=>Some(label.as_str()),Source::Empty=>None,_=>panic!("fixture virtual only")},"included":s.included,"scale":s.scale,"number":s.number,"variant":s.variant})).collect::<Vec<_>>();
            assert_eq!(serde_json::json!(rows),case["expected"],"{}",case["raw"]);
            assert_eq!(&*before,&*project.legacy_archive.as_ref().unwrap().original);
            if let Some(row)=restored.slots.iter().find(|r|r.original.get("unknown").is_some()){assert_eq!(row.original["unknown"],serde_json::json!({"preserve":[1,2]}));}
            // Native save/read does not replace or flatten original slot fields.
            let dir=tempfile::tempdir().unwrap();let path=dir.path().join("roundtrip.json");crate::project::save_project(&path,&project).unwrap();
            let mut saved=crate::project::read_project(&path).unwrap();let again=restore(&saved).unwrap().unwrap();assert_eq!(again.slots.len(),restored.slots.len());
            saved.template_preferences=None;saved.template_export=None;saved.project_settings=None;let payload=crate::legacy_project::legacy_payload(&saved,&crate::legacy_project::ProjectUiState::default()).unwrap();assert_eq!(payload["template_export_slots"],case["raw"]);
        }
    }
}
