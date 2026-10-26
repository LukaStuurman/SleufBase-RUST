#[path="../tests/support/template_server.rs"] mod support;
use sleufbase_rust::{template_server::{self,Mode},template_export,export_control::Control};
use std::path::PathBuf;
fn main()->Result<(),String>{
    let output=PathBuf::from(std::env::args().nth(1).ok_or("fresh output directory required")?);if output.exists(){return Err("Output already exists".into());}std::fs::create_dir_all(&output).map_err(|e|e.to_string())?;
    let repository=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    for case in support::cases().as_array().unwrap(){let name=case["name"].as_str().unwrap();let mut request=support::request(case);let mut script=support::Script::new(case);
        template_server::refresh(&mut request,Mode::Fresh,true,true,&mut script,&Control::default())?;
        if script.cursor!=script.calls.len(){return Err("Incomplete service plan".into());}
        let overview=request.overview.as_mut().unwrap();for layer in &mut overview.layers{layer.path=repository.join("rust/tests/fixtures/cadastral-overview").join(layer.path.file_name().unwrap());}
        let summary=serde_json::json!({"linework":overview.linework,"labels":overview.labels,"roads":request.roads,"terrain":request.terrain_boundaries,"surfaces":request.surfaces});
        std::fs::write(output.join(format!("{name}-services.json")),serde_json::to_vec_pretty(&summary).unwrap()).map_err(|e|e.to_string())?;
        request.template=repository.join("assets/cadastral_template.dxf");request.settings.paired=true;
        for slot in request.slots.iter_mut().flatten(){slot.profile=None;slot.tiff_source.as_mut().unwrap().path=repository.join("rust/tests/fixtures/template-raster/orientation-1.tiff");slot.map_png=Default::default();slot.map_source=Some(serde_json::from_value(serde_json::json!({"settings":{"dpi":40,"scale":225.},"background":{"kind":"solid","color":[245,245,245,255]}})).unwrap());}
        template_export::export(&output.join(format!("template-{name}.dxf")),&request,&Control::default())?;
    }Ok(())
}
