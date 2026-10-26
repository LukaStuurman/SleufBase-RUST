use sleufbase_rust::{dxf_document::Document,template_pages};
use std::{path::PathBuf,sync::atomic::AtomicBool};
fn root()->PathBuf{PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/template-columns")}
fn bytes(d:&Document)->Vec<u8>{let mut b=Vec::new();d.write_to(&mut b,&AtomicBool::new(false)).unwrap();b}
#[test]
fn linked_and_embedded_columns_survive_repeated_sheet_extension(){
    for version in ["R2007","R2010","R2013","R2018"]{let cancel=AtomicBool::new(false);let mut d=Document::load(&root().join(format!("{version}.dxf")),&cancel).unwrap();for required in [9,17]{template_pages::extend(&mut d,required,&cancel).unwrap();let mut references=std::collections::HashSet::new();for r in d.records().filter(|r|r.kind()=="MTEXT"){let mut columns=false;for(c,v)in r.tags(){if c==1000&&v=="ACAD_MTEXT_COLUMNS_BEGIN"{columns=true;}else if c==1000&&v=="ACAD_MTEXT_COLUMNS_END"{columns=false;}else if columns&&c==1005{assert!(references.insert(v.to_owned()),"shared column {version}: {v}");assert_eq!(d.by_handle(v).unwrap().unwrap().kind(),"MTEXT");}}}}}
}
#[test]
fn missing_or_cyclic_linked_column_and_cancellation_leave_drawing_unchanged(){
    let source=std::fs::read_to_string(root().join("R2007.dxf")).unwrap();
    for cyclic in [false,true]{let mut lines=source.lines().map(str::to_owned).collect::<Vec<_>>();let mut main=String::new();let mut mtext=false;let mut columns=false;
        for pair in lines.chunks_exact_mut(2){if pair[0].trim()=="0"{mtext=pair[1]=="MTEXT";columns=false;main.clear();}if mtext&&pair[0].trim()=="5"{main=pair[1].clone();}if mtext&&pair[0].trim()=="1000"&&pair[1]=="ACAD_MTEXT_COLUMNS_BEGIN"{columns=true;}if columns&&pair[0].trim()=="1005"{pair[1]=if cyclic{main.clone()}else{"FFFFFF0".into()};break;}}
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("invalid.dxf");std::fs::write(&path,lines.join("\n")+"\n").unwrap();let mut d=Document::load(&path,&AtomicBool::new(false)).unwrap();let before=bytes(&d);assert!(template_pages::extend(&mut d,9,&AtomicBool::new(false)).is_err());assert_eq!(bytes(&d),before);assert!(template_pages::extend(&mut d,9,&AtomicBool::new(true)).is_err());assert_eq!(bytes(&d),before);
    }
}
