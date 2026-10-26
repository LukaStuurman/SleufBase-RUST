use sleufbase_rust::{dxf_document::Document,storage,template_pages};
use std::{path::PathBuf,sync::atomic::AtomicBool};
fn main()->Result<(),String>{
    let output=PathBuf::from(std::env::args().nth(1).ok_or("fresh output directory required")?);if output.exists(){return Err("Output already exists".into());}
    let root=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/template-columns");let cancel=AtomicBool::new(false);
    for version in ["R2007","R2010","R2013","R2018"]{let mut doc=Document::load(&root.join(format!("{version}.dxf")),&cancel)?;for required in [9,17]{template_pages::extend(&mut doc,required,&cancel)?;storage::atomic_write(&output.join(format!("{version}-{required}.dxf")),false,|out|doc.write_to(out,&cancel))?;}}
    Ok(())
}
