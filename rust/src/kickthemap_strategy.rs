//! Original manual download repair contract (kickthemap.py:286..469).
//! Stores request templates only; never cookies, credentials or signed URLs.
use super::*;
use serde::{Serialize,Deserialize};
use std::collections::BTreeMap;
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="lowercase")]
pub enum Encoding {Multipart,Form,Json}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Strategy {pub version:u32,pub request_mode:Encoding,pub request_method:String,pub endpoint_path:String,pub fields:BTreeMap<String,String>,pub response_url_path:Option<String>}
fn endpoint(raw:&str)->Result<String,String>{
    let raw=crate::python_values::trim(raw);
    let path=if raw.starts_with("https://") {let url=reqwest::Url::parse(raw).map_err(|e|e.to_string())?;
        if !crate::kickthemap_browser::is_kickthemap_origin(&url)||!url.username().is_empty()||url.password().is_some()||url.query().is_some()||url.fragment().is_some(){return Err("Downloadaanvraag verwijst niet naar de vaste KickTheMap-site".into());}url.path().to_owned()
    }else{raw.to_owned()};
    if !path.starts_with('/')||path.starts_with("//")||path.contains(['?','#','\\','\r','\n'])||path.len()>1024{return Err("Onveilige KickTheMap-downloadendpoint".into());}Ok(path)
}
fn sensitive(key:&str)->bool{let key=key.to_ascii_lowercase();["token","password","cookie","secret","authorization","credential"].iter().any(|p|key.contains(p))}
fn template(value:&str,key:&str,job:&Job,prefix:&str)->String{
    let normalized=key.chars().filter(|c|c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
    if matches!(normalized.as_str(),"projectid"|"project"|"jobid")&&value==job.job_id.to_string(){return "{job_id}".into();}
    if matches!(normalized.as_str(),"filename"|"remotefilename")&&[".tif",".tiff"].iter().any(|s|value.to_ascii_lowercase().ends_with(s)){return "{storage_prefix}.tiff".into();}
    if matches!(normalized.as_str(),"exportname"|"exportfilename")&&[".tif",".tiff"].iter().any(|s|value.to_ascii_lowercase().ends_with(s)){return "{export_name}".into();}
    for (candidate,replacement) in [(format!("{prefix}.tiff"),"{storage_prefix}.tiff"),(format!("{}.tiff",safe_file_stem(&job.title,job.job_id)),"{export_name}"),(format!("{}.tiff",job.title),"{export_name}"),(job.job_id.to_string(),"{job_id}"),(prefix.into(),"{storage_prefix}"),(job.prefix.clone(),"{storage_prefix}"),(safe_file_stem(&job.title,job.job_id),"{safe_stem}"),(job.title.clone(),"{title}")]{if !candidate.is_empty()&&value==candidate{return replacement.into();}}value.into()
}
impl Strategy {
    pub fn from_capture(payload:&str,job:&Job,storage_prefix:&str)->Result<Self,String>{
        if payload.len()>256*1024{return Err("Handmatige downloadregistratie is te groot".into());}
        let capture:Value=serde_json::from_str(payload).map_err(|e|format!("Downloadregistratie: {e}"))?;
        if capture["status"]!="captured"{return Err("Leg eerst een handmatige GeoTIFF-download vast in de KickTheMap-browser".into());}
        let request=&capture["request"];let raw=request["fields"].as_object().ok_or("Downloadregistratie mist velden")?;let mut fields=BTreeMap::new();
        for (key,value)in raw {if !matches!(value,Value::String(_)|Value::Number(_)|Value::Bool(_)){continue;}
            let text=crate::python_values::string(Some(value)).unwrap_or_default();fields.insert(key.clone(),template(&text,key,job,storage_prefix));}
        let strategy=Self{version:1,request_mode:serde_json::from_value(Value::String(crate::python_values::trim(request["request_mode"].as_str().unwrap_or("")).to_ascii_lowercase())).map_err(|_|"Onbekende downloadcodering")?,request_method:request.get("method").and_then(Value::as_str).map(crate::python_values::trim).filter(|s|!s.is_empty()).unwrap_or("POST").to_ascii_uppercase(),endpoint_path:endpoint(request["endpoint"].as_str().ok_or("Downloadendpoint ontbreekt")?)?,fields,response_url_path:request.get("response_url_path").and_then(Value::as_str).map(crate::python_values::trim).filter(|s|!s.is_empty()).map(str::to_owned)};
        strategy.validate()?;Ok(strategy)
    }
    pub fn validate(&self)->Result<(),String>{
        if self.version!=1||!matches!(self.request_method.as_str(),"GET"|"POST")||self.fields.is_empty()||self.fields.len()>64{return Err("Ongeldige KickTheMap-downloadstrategie".into());}endpoint(&self.endpoint_path)?;
        if self.fields.iter().any(|(k,v)|k.is_empty()||k.len()>128||sensitive(k)||v.len()>8192||v.contains(['\r','\n'])){return Err("Downloadregistratie bevat geheime of ongeldige velden".into());}
        if self.response_url_path.as_ref().is_some_and(|p|p.len()>256||p.split('.').any(|s|s.is_empty()||!s.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_'))){return Err("Ongeldig antwoordpad in downloadstrategie".into());}Ok(())
    }
    pub(super) fn render(&self,fields:&[(&str,String)],title:&str)->Result<BTreeMap<String,String>,String>{
        self.validate()?;let get=|key|fields.iter().find(|(k,_)|*k==key).map(|(_,v)|v.as_str()).unwrap_or("");
        let prefix=get("fileName").strip_suffix(".tiff").unwrap_or(get("fileName"));let export=get("exportName");let stem=export.strip_suffix(".tiff").unwrap_or(export);
        Ok(self.fields.iter().map(|(k,v)|{let mut out=v.clone();for(key,value)in [("job_id",fields.iter().find(|(key,_)|*key=="job_id").map(|(_,value)|value.as_str()).unwrap_or_else(||get("projectId"))),("storage_prefix",prefix),("export_name",export),("safe_stem",stem),("title",title)]{out=out.replace(&format!("{{{key}}}"),value);} (k.clone(),out)}).collect())
    }
    pub(super) fn signed_url(&self,text:&str)->Result<reqwest::Url,String>{
        if let Some(path)=&self.response_url_path{let payload:Value=serde_json::from_str(text).map_err(|e|e.to_string())?;if !as_bool(payload.get("status")){return Err("KickTheMap heeft de herstel-download geweigerd".into());}
            let value=path.split('.').try_fold(&payload,|value,key|value.get(key));
            if let Some(value)=value.filter(|v|!v.is_null()){return validate_signed_url(value.as_str().ok_or("Vastgelegd URL-pad bevat geen tekst")?);}}
        extract_signed_url(text)
    }
}

#[cfg(test)]mod tests{
    use super::*;
    fn job(value:&Value)->Job{Job{job_id:value["job_id"].as_u64().unwrap(),title:value["title"].as_str().unwrap().into(),prefix:value["prefix"].as_str().unwrap().into(),address:String::new(),municipality:String::new(),coordinates:String::new(),client_date:String::new(),status:0,downloadable:true}}
    pub(super) fn capture(fields:Value)->String{serde_json::json!({"status":"captured","request":{"endpoint":"/jobs/get-file-url","method":"POST","request_mode":"form","fields":fields,"response_url_path":"data.url"}}).to_string()}
    #[test]fn original_capture_templates_and_rendered_fields_match_source_oracle(){
        let fixture:Value=serde_json::from_str(include_str!("../tests/fixtures/kickthemap-strategy.json")).unwrap();
        for case in fixture["cases"].as_array().unwrap(){let job=job(&case["job"]);let strategy=Strategy::from_capture(&capture(case["fields"].clone()),&job,&job.prefix).unwrap();
            assert_eq!(serde_json::to_value(&strategy.fields).unwrap(),case["templates"]);assert_eq!(safe_file_stem(&job.title,job.job_id),case["safe_stem"]);
            let fields=[("projectId",job.job_id.to_string()),("fileName",format!("{}.tiff",job.prefix)),("exportName",format!("{}.tiff",safe_file_stem(&job.title,job.job_id)))];
            assert_eq!(serde_json::to_value(strategy.render(&fields,&job.title).unwrap()).unwrap(),case["rendered"]);
        }
    }
    #[test]fn captures_reject_secrets_foreign_endpoints_and_incomplete_records(){
        let fixture:Value=serde_json::from_str(include_str!("../tests/fixtures/kickthemap-strategy.json")).unwrap();let job=job(&fixture["cases"][0]["job"]);
        assert!(Strategy::from_capture(&capture(serde_json::json!({"token":"never retain"})),&job,&job.prefix).is_err());
        let mut raw:Value=serde_json::from_str(&capture(serde_json::json!({"projectId":"42"}))).unwrap();raw["request"]["endpoint"]="https://evil.test/jobs/get-file-url".into();assert!(Strategy::from_capture(&raw.to_string(),&job,&job.prefix).is_err());
        raw["request"]["endpoint"]="//evil.test/jobs/get-file-url".into();assert!(Strategy::from_capture(&raw.to_string(),&job,&job.prefix).is_err());
        raw["status"]="waiting".into();assert!(Strategy::from_capture(&raw.to_string(),&job,&job.prefix).is_err());
    }
    #[test]fn original_custom_response_path_falls_back_only_for_missing_or_null_values(){
        let fixture:Value=serde_json::from_str(include_str!("../tests/fixtures/kickthemap-strategy.json")).unwrap();
        let job=job(&fixture["cases"][0]["job"]);
        let mut raw:Value=serde_json::from_str(&capture(serde_json::json!({"projectId":"42"}))).unwrap();
        raw["request"]["response_url_path"]=" payload.asset.link ".into();
        raw["request"]["request_mode"]=" FORM ".into();raw["request"]["method"]=" post ".into();
        let strategy=Strategy::from_capture(&raw.to_string(),&job,&job.prefix).unwrap();
        for row in fixture["responses"].as_array().unwrap(){
            let got=strategy.signed_url(&row["payload"].to_string());
            if let Some(expected)=row["expected"].as_str(){assert_eq!(got.unwrap().as_str(),expected);}else{assert!(got.is_err());}
        }
    }
}
