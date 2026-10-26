use sleufbase_rust::{Bounds,template_server::{Paths,Provider},export_control::Control,cadastral::CadLabel,bgt_features::SurfaceFeature};
use serde_json::Value;
pub struct Script {pub calls:Vec<Value>,pub cursor:usize,pub fail_at:Option<usize>,pub cancel_at:Option<usize>}
impl Script {
    pub fn new(case:&Value)->Self{Self{calls:case["calls"].as_array().unwrap().clone(),cursor:0,fail_at:None,cancel_at:None}}
    fn take(&mut self,kind:&str,b:Bounds,c:&Control)->Result<Value,String>{
        c.check()?;
        if self.fail_at==Some(self.cursor){return Err("scripted late service failure".into());}
        if self.cancel_at==Some(self.cursor){c.cancel();c.check()?;}
        let row=self.calls.get(self.cursor).ok_or("unexpected service request")?;
        if row["kind"]!=kind||row["bounds"]!=serde_json::to_value(b).unwrap(){return Err(format!("service plan mismatch at {}: {kind} {b:?}, expected {row}",self.cursor));}
        self.cursor+=1;Ok(row["result"].clone())
    }
}
impl Provider for Script {
    fn outlines(&mut self,b:Bounds,c:&Control)->Result<Paths,String>{serde_json::from_value(self.take("outlines",b,c)?).map_err(|e|e.to_string())}
    fn parcels(&mut self,b:Bounds,c:&Control)->Result<Paths,String>{serde_json::from_value(self.take("parcels",b,c)?).map_err(|e|e.to_string())}
    fn roads(&mut self,b:Bounds,c:&Control)->Result<Paths,String>{serde_json::from_value(self.take("roads",b,c)?).map_err(|e|e.to_string())}
    fn terrain(&mut self,b:Bounds,c:&Control)->Result<Paths,String>{serde_json::from_value(self.take("terrain",b,c)?).map_err(|e|e.to_string())}
    fn labels(&mut self,b:Bounds,c:&Control)->Result<Vec<CadLabel>,String>{serde_json::from_value(self.take("labels",b,c)?).map_err(|e|e.to_string())}
    fn surfaces(&mut self,b:Bounds,c:&Control)->Result<Vec<SurfaceFeature>,String>{
        self.take("surfaces",b,c)?.as_array().ok_or("invalid surfaces")?.iter().map(|f|Ok(SurfaceFeature{layer_name:f["layer_name"].as_str().ok_or("missing layer")?.into(),feature_id:f["feature_id"].as_str().ok_or("missing identity")?.into(),surface:sleufbase_rust::bgt_surface::Feature{physical_appearance:f["physical_appearance"].as_str().ok_or("missing appearance")?.into(),geometry:serde_json::from_value(f["geometry"].clone()).map_err(|e|e.to_string())?}})).collect()
    }
}
pub fn cases()->Value{serde_json::from_str(include_str!("../fixtures/template-server/reference.json")).unwrap()}
pub fn request(case:&Value)->sleufbase_rust::template_export::Request{
    let fixture:Value=serde_json::from_str(include_str!("../fixtures/template-export/reference.json")).unwrap();
    let mut value=fixture.as_array().unwrap().iter().find(|c|c["name"]=="native-tiff").unwrap()["request"].clone();
    let slot=value["slots"].as_array().unwrap().iter().find(|s|!s.is_null()).unwrap().clone();
    value["slots"]=Value::Array(case["bounds"].as_array().unwrap().iter().enumerate().map(|(i,b)|{let mut s=slot.clone();s["bounds"]=b.clone();s["label"]=format!("PS{:03}",i+1).into();if case["terrain"]==true{if s["profile"].is_null(){s["profile"]=serde_json::json!({"dataset":0});}s["profile"]["draw"]=case["surfaces"].clone();s["profile"]["auto_fill_bgt"]=case["surfaces"].clone();}else{s["profile"]=Value::Null;}s}).collect());
    value["overview"]=case["overview"].clone();serde_json::from_value(value).unwrap()
}
pub fn snapshot(r:&sleufbase_rust::template_export::Request)->Value{
    serde_json::json!({"overview":r.overview,"roads":r.roads,"terrain":r.terrain_boundaries,"surfaces":r.surfaces,"slots":r.slots.iter().map(|s|s.as_ref().map(|s|serde_json::json!({"surfaces":s.surfaces,"constructed":s.surfaces_constructed,"label":s.label,"bounds":s.bounds}))).collect::<Vec<_>>()})
}
