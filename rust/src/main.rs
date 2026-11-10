#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

mod ui;
mod rule_editor;
mod export_dialog;
mod template_dialog;
mod ui_preferences;

use sleufbase_rust::{
    load_job_features, write_xlsx,
    marxact, project::{NativeProject,save_project_new},
    settings_compat,
};
use std::path::PathBuf;

fn print_help() {
    println!("SleufBase Rust {} — lokale KickTheMap-objecten", env!("CARGO_PKG_VERSION"));
    println!("sleufbase-rust.exe                               Open Windows desktopapp");
    println!("sleufbase-rust.exe project.json [kaart.tif ...]    Open project of kaartbestanden");
    println!("sleufbase-rust.exe --xlsx uitvoer.xlsx jobFeatures.json [extra.json ...]");
    println!("sleufbase-rust.exe --dxf uitvoer.dxf jobFeatures.json [extra.json ...]");
    println!("sleufbase-rust.exe --template-dxf uitvoer.dxf voorbereide-export.json");
    println!("sleufbase-rust.exe --marxact uitvoer.json meting.dxf [extra.dxf ...]");
    println!("sleufbase-rust.exe --marxact-dxf uitvoer.dxf meting.dxf [extra.dxf ...]");
    println!("sleufbase-rust.exe --import-python uitvoer.json project.klicproject");
    println!("sleufbase-rust.exe --export-python nieuw.klicproject Rust-project.json");
    println!("Optioneel vóór de opdracht: --settings Rust-instellingen.json");
    println!("Licenties van overgenomen onderdelen: --licenses");
    println!("Bestaande uitvoerbestanden worden niet overschreven.");
    println!("Sjabloonexport en een ingebouwd KickTheMap-inlogvenster zijn beschikbaar; volledige praktijkacceptatie loopt nog.");
}

fn run_cli(args: &[String]) -> Result<(), String> {
    let (settings_path,args)=if args.first().is_some_and(|s|s=="--settings") {
        if args.len()<5 {return Err("Gebruik --settings instellingen.json --dxf/--xlsx uitvoer invoer".into());}
        (PathBuf::from(&args[1]),&args[2..])
    } else {(sleufbase_rust::project::app_data_directory().join("settings.json"),args)};
    if args.len() < 3 {
        return Err("Gebruik --xlsx/--dxf met jobFeatures.json of --marxact/--marxact-dxf met MarXact-DXF".into());
    }
    let output = PathBuf::from(&args[1]);
    if args[0]=="--template-dxf" {
        if args.len()!=3 { return Err("Gebruik --template-dxf uitvoer.dxf voorbereide-export.json".into()); }
        if output.exists() { return Err("Uitvoerbestand bestaat al; overschrijven is niet toegestaan".into()); }
        let control=sleufbase_rust::export_control::Control::default();
        let request=sleufbase_rust::template_export::read_request(&PathBuf::from(&args[2]),&control)?;
        let exported=sleufbase_rust::template_export::export_with_progress(&output,&request,&control,|stage|println!("{stage}"))?;
        println!("{} vakken en {} profielen opgeslagen naar {}",exported.filled_slots,exported.profiles,exported.published.drawing.display());
        if let Some(path)=exported.companion {println!("Discipline-Excel opgeslagen naar {}",path.display());}
        for warning in exported.warnings {eprintln!("Melding: {warning}");}
        return Ok(());
    }
    if args[0]=="--import-python" {
        if args.len()!=3 {return Err("Gebruik --import-python uitvoer.json project.klicproject".into());}
        let project=sleufbase_rust::legacy_project::read_legacy_project(&PathBuf::from(&args[2]))?;
        save_project_new(&output,&project)?;
        println!("Python-project geïmporteerd naar {}",output.display());
        for notice in &project.legacy_archive.as_ref().unwrap().notices {println!("Melding: {notice}");}
        return Ok(());
    }
    if args[0]=="--export-python" {
        if args.len()!=3 {return Err("Gebruik --export-python nieuw.klicproject Rust-project.json".into());}
        let project=sleufbase_rust::project::read_project(&PathBuf::from(&args[2]))?;
        let ui=sleufbase_rust::legacy_project::ProjectUiState::from_project(&project)?.unwrap_or_default();
        sleufbase_rust::legacy_project::write_legacy_project(&project,&output,&ui,&std::sync::atomic::AtomicBool::new(false))?;
        println!("Nieuw Python-project opgeslagen naar {}",output.display());
        return Ok(());
    }
    if matches!(args[0].as_str(),"--marxact"|"--marxact-dxf") {
        if output.exists(){
            return Err("Uitvoerbestand bestaat al; overschrijven is niet toegestaan".into());
        }
        let settings=settings_compat::read_rust_settings(&settings_path)?;
        let mut trenches=Vec::new();
        for source in &args[2..] {
            let result=marxact::import_dxf(
                &PathBuf::from(source),&settings.marxact_name_mappings)?;
            trenches.extend(result.trenches);
        }
        if trenches.is_empty(){return Err("Geen MarXact-proefsleuven gevonden".into());}
        if args[0]=="--marxact" {
            let project=NativeProject{
                name:"MarXact import".into(),
                virtual_trenches:trenches,
                ..NativeProject::default()
            };
            save_project_new(&output,&project)?;
        } else {
            sleufbase_rust::object_dxf::write_with_layers(&output,&[],&trenches,None,&[],&settings.object_rules,settings.object_dxf_options())?;
        }
        println!("MarXact-conversie opgeslagen naar {}",output.display());
        return Ok(());
    }
    let mut datasets = Vec::new();
    let settings=settings_compat::read_rust_settings(&settings_path)?;
    for input in &args[2..] {
        datasets.push(load_job_features(&PathBuf::from(input))?);
    }
    match args[0].as_str() {
        "--xlsx" => write_xlsx(&output, &datasets, &settings.object_rules)?,
        "--dxf" => sleufbase_rust::object_dxf::write_with_layers(&output,&datasets,&[],None,&[],&settings.object_rules,settings.object_dxf_options())?,
        _ => return Err(format!("Onbekende opdracht: {}", args[0]))
    }
    println!("{} job(s) geëxporteerd naar {}", datasets.len(), output.display());
    Ok(())
}

#[cfg(windows)]
fn attach_cli_console(){
    use windows_sys::Win32::{
        Foundation::INVALID_HANDLE_VALUE,
        System::Console::{AttachConsole,GetStdHandle,ATTACH_PARENT_PROCESS,STD_ERROR_HANDLE,STD_OUTPUT_HANDLE},
    };
    // The desktop uses the GUI subsystem. CLI calls may still use existing
    // redirected handles or the console of the invoking terminal.
    unsafe {
        let redirected=[STD_OUTPUT_HANDLE,STD_ERROR_HANDLE].into_iter().any(|stream|{
            let handle=GetStdHandle(stream);
            !handle.is_null()&&handle!=INVALID_HANDLE_VALUE
        });
        if !redirected{AttachConsole(ATTACH_PARENT_PROCESS);}
    }
}

fn main() -> eframe::Result<()> {
    let raw_args: Vec<_> = std::env::args_os().skip(1).collect();
    if raw_args.first().is_some_and(|s|s=="--kickthemap-webview") {
        let result=if raw_args.len()==2 {sleufbase_rust::kickthemap_webview::run(&PathBuf::from(&raw_args[1]))}else{Err("Ongeldige interne browseropdracht".into())};
        if let Err(error)=result {eprintln!("{error}");std::process::exit(2);}
        return Ok(());
    }
    let command=raw_args.first().is_some_and(|s|s.to_string_lossy().starts_with('-')||s=="help");
    if command {
        #[cfg(windows)]
        attach_cli_console();
        let args:Vec<String>=match raw_args.iter().map(|s|s.clone().into_string()).collect(){Ok(args)=>args,Err(_)=>{eprintln!("Fout: ongeldige tekst in opdrachtregel");std::process::exit(2);}};
        if args[0]=="--licenses" {println!("{}",include_str!("../third_party/RustCrypto-NOTICES.txt"));println!("Wry 0.57.0 — ingebouwde WebView2-weergave\n{}",include_str!("../third_party/wry-LICENSE.txt"));println!("Pillow 11.3.0 — affine rasterrotatie, bilineaire kaartresampling en alphacompositie\n{}\nezdxf 1.4.4 — SHP/SHX/LFF-protocol, fontmetingen en vector-tekstuitlijning (native Rust-port met correcties)\n{}\nproj4rs 0.2.1 — native GPS/RD-navigatie\n{}",include_str!("../third_party/Pillow-LICENSE.txt"),include_str!("../third_party/ezdxf-LICENSE.txt"),include_str!("../third_party/proj4rs-NOTICES.txt"));return Ok(());}
        if ["-h", "--help", "help"].contains(&args[0].as_str()) {
            print_help();
            return Ok(());
        }
        if let Err(error) = run_cli(&args) {
            eprintln!("Fout: {error}");
            print_help();
            std::process::exit(2);
        }
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: ui::viewport(),
        ..Default::default()
    };
    let paths=raw_args.into_iter().map(PathBuf::from).collect();
    eframe::run_native("SleufBase", options,
        Box::new(move |_cc| Ok(Box::new(ui::SleufBaseApp::with_input_paths(paths)))))
}
