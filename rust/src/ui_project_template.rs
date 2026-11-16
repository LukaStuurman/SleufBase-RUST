//! End-user GUI for building template exports directly from project sources.
use super::*;
use sleufbase_rust::template_project::{self, BackgroundKind, Options, Source};
use sleufbase_rust::template_order::{self, SortMode};

fn configure_options(mut options:Options,settings:&MigratedSettings,saved:Option<&sleufbase_rust::template_prefs::TemplatePreferences>,jobs:&[Dataset],rasters:&[GeoTiffInfo],trenches:&[VirtualTrench])->Options{
    let defaults=crate::template_dialog::ExportDefaults::from_settings(settings)
        .expect("validated project export settings");
    let mut configured=if let Some(saved)=saved{
        let mut restored=saved.restore(jobs,rasters,trenches);
        for choice in &mut restored.choices {
            if saved.legacy_layout_only || !saved.contains_source(choice.source,jobs,rasters,trenches) {
                choice.profile=defaults.profiles;
            }
        }
        if saved.legacy_layout_only{
            options.choices=restored.choices;options.raster_layout=restored.raster_layout;
            options
        } else {
            restored
        }
    } else {
        for choice in &mut options.choices { choice.profile=defaults.profiles; }
        options.include_bgt=true;
        options
    };
    // Settings own the general defaults; saved source choices and drawing
    // metadata remain explicit and must survive reopening this dialog.
    defaults.apply_project(&mut configured);
    configured
}

/// Refresh only changed global defaults that the user has not overridden in
/// this dialog. Call before validation/capture, while the baseline still exists.
pub(super) fn refresh_options(options:&mut Options,dialog:Option<&Dialog>,settings:&MigratedSettings)->Result<(),String>{
    let current=crate::template_dialog::ExportDefaults::from_settings(settings)?;
    options.overview_settings=current.overview.clone();
    options.contact=current.contact.clone();
    if let Some(dialog)=dialog {
        current.refresh_project(&dialog.opening_defaults,options);
        if current.profiles!=dialog.opening_defaults.profiles {
            for choice in &mut options.choices {
                if dialog.profile_baseline.get(&choice.source).is_some_and(|opening|choice.profile==*opening) {
                    choice.profile=current.profiles;
                }
            }
        }
    }
    Ok(())
}

fn trench_raster(raster:&GeoTiffInfo)->bool{
    static NAME:std::sync::LazyLock<regex::Regex>=std::sync::LazyLock::new(||
        regex::Regex::new(r"(?i)(?:^|[^a-z0-9])(?:ps|proefsleuf)[ ._-]*[0-9]").unwrap());
    raster.kickthemap_job_id.is_some_and(|id|id>0)
        || raster.metadata.get("template_export_ps_number").is_some_and(|value|!value.is_null())
        || raster.path.file_stem().is_some_and(|name|NAME.is_match(&name.to_string_lossy()))
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ExportSummary {
    trenches: usize,
    profiles: usize,
    empty: usize,
}
fn export_summary(options: &Options) -> ExportSummary {
    let mut summary=ExportSummary::default();
    for choice in options.choices.iter().filter(|choice|choice.include) {
        if matches!(choice.source,Source::Empty(_)) {summary.empty+=1;}
        else {summary.trenches+=1;summary.profiles+=usize::from(choice.profile);}
    }
    summary
}
#[derive(Debug, PartialEq, Eq)]
enum ExportPlacement {
    Excluded,
    Empty(usize),
    Trench { position: usize, label: Option<String> },
}
fn invalid_label_hint(choice:&template_project::Choice)->&'static str {
    match choice.label.as_deref() {
        None=>"Vul een proefsleuflabel in, bijvoorbeeld PS25B.",
        Some(label) if label.trim().is_empty()=>"Vul een proefsleuflabel in, bijvoorbeeld PS25B.",
        Some(label) if label.trim().chars().any(char::is_control)=>"Het proefsleuflabel bevat niet-ondersteunde tekens. Pas het aan.",
        Some(_)=>"Het proefsleuflabel is te lang. Kort het in.",
    }
}
fn placement_caption(ui:&mut egui::Ui,placement:&ExportPlacement,choice:&template_project::Choice) {
    ui.horizontal_wrapped(|ui| {
        let exported_label=match placement {
            ExportPlacement::Excluded=>{ui.weak("Niet in export");None},
            ExportPlacement::Empty(position)=>{
                ui.small(format!("Exportvak {position} · leeg"))
                    .on_hover_text("Dit vak blijft leeg in het sjabloon. Selecteer de regel en kies rechts bij Lege en extra vakken voor Lege vakken verwijderen om de volgende sleuven te laten aansluiten.");
                None
            },
            ExportPlacement::Trench{position,label:Some(label)}=>{
                ui.small(format!("Exportvak {position} · {label}"));
                Some(label.as_str())
            },
            ExportPlacement::Trench{position,label:None}=>{
                ui.colored_label(DARK,format!("Exportvak {position} · {}",invalid_label_hint(choice)));
                None
            },
        };
        if !matches!(choice.source,Source::Empty(_)) && Some(choice.title.as_str())!=exported_label {
            ui.weak(format!("Naam: {}",choice.title));
        }
    });
}
fn effective_map_label(options: &Options) -> &'static str {
    if options.server_mode==sleufbase_rust::template_server::Mode::Offline {
        "Offline · witte kaart, zonder luchtfoto"
    } else {
        match options.background {
            BackgroundKind::Blank=>"Online · witte kaart",
            BackgroundKind::Pdok=>"Online · PDOK-luchtfoto",
            BackgroundKind::Osm=>"Online · OpenStreetMap",
            BackgroundKind::Cadastral=>"Online · Kadaster",
        }
    }
}
/// Run filesystem checks only when starting an export, never for every UI frame.
/// The worker validates again because files can disappear after this check.
pub(super) fn validate_export_sources(options:&Options,jobs:&[Dataset],rasters:&[GeoTiffInfo],trenches:&[VirtualTrench])->Result<(),String>{
    if !(30..=600).contains(&options.dpi){return Err("Kies een kaartresolutie tussen 30 en 600 DPI.".into());}
    if !options.scale.is_finite()||!(1. ..=100_000.).contains(&options.scale){return Err("Kies een kaartschaal tussen 1:1 en 1:100000.".into());}
    sleufbase_rust::dxf_fonts::Catalog::validate_dirs(&options.font_directories)?;
    options.profile_style.validate()?;
    options.title_fields.validate()?;
    let selected=options.choices.iter().filter(|choice|choice.include).collect::<Vec<_>>();
    if !selected.iter().any(|choice|!matches!(choice.source,Source::Empty(_))){return Err("Neem minimaal één proefsleuf mee; lege vakken alleen zijn niet voldoende.".into());}
    if selected.len()>template_project::MAX_SLOTS{return Err(format!("Neem maximaal {} proefsleufvakken mee, inclusief lege vakken.",template_project::MAX_SLOTS));}
    let mut seen=std::collections::HashSet::new();
    for choice in selected {
        if matches!(choice.source,Source::Empty(_)){continue;}
        if !seen.insert(choice.source){return Err(format!("{} is meer dan één keer geselecteerd.",choice.title));}
        let name=choice.label.as_deref().filter(|label|!label.is_empty()).unwrap_or(&choice.title);
        match choice.source {
            Source::Job(index)=>{jobs.get(index).ok_or_else(||format!("{name}: de KickTheMap-bron ontbreekt. Open de export opnieuw."))?;},
            Source::Virtual(index)=>{
                let trench=trenches.get(index).ok_or_else(||format!("{name}: de virtuele sleuf ontbreekt. Open de export opnieuw."))?;
                // Geometry and large point metadata are validated on the export worker.
                if choice.profile&&(trench.start.z.is_none()||trench.end.z.is_none()){
                    return Err(format!("{name}: vul de begin- en eindhoogte in de sleufeditor in, of zet Dwarsprofiel voor deze sleuf uit."));
                }
            },
            Source::Raster(_)=>{},Source::Empty(_)=>unreachable!(),
        }
        let raster_index=choice.raster.or_else(||if let Source::Raster(index)=choice.source{Some(index)}else{None});
        if let Some(index)=raster_index{
            let raster=rasters.get(index).ok_or_else(||format!("{name}: de GeoTIFF-koppeling ontbreekt. Kies de bron opnieuw."))?;
            if !raster.path.is_file(){return Err(format!("{name}: GeoTIFF ontbreekt: {}",raster.path.display()));}
            if raster.epsg.is_some_and(|epsg|epsg!=28992){return Err(format!("{name}: de GeoTIFF moet RD-coördinaten (EPSG:28992) gebruiken."));}
        }else if !matches!(choice.source,Source::Virtual(_)){
            return Err(format!("{name}: koppel een GeoTIFF of zet Meenemen uit."));
        }
        if choice.manual_scale.is_some_and(|scale|!(1..=1_000_000).contains(&scale)){
            return Err(format!("{name}: kies een profielschaal tussen 1:1 en 1:1000000, of Automatisch."));
        }
        sleufbase_rust::template_photo::validate(&choice.photos).map_err(|error|format!("{name}: {error}"))?;
    }
    for (label,path) in [("Techbase-logo",&options.logos.techbase),("Opdrachtgeverlogo",&options.logos.client)] {
        if let Some(path)=path {
            let photo=sleufbase_rust::template_photo::Photo{path:path.clone(),turns:0,area:Default::default()};
            sleufbase_rust::template_photo::validate(&[photo]).map_err(|error|format!("{label}: {error}. Kies het bestand opnieuw of verwijder het bij Meer exportinstellingen."))?;
        }
    }
    Ok(())
}

pub(super) struct Dialog {
    pub options: Options,
    pub output: Option<PathBuf>,
    pub error: String,
    pub automatic: Vec<sleufbase_rust::template_match::Suggestion>,
    pub editor:sleufbase_rust::template_editor::Editor,
    sort_mode: Option<SortMode>,
    reveal_row: Option<usize>,
    opening_defaults: crate::template_dialog::ExportDefaults,
    profile_baseline: std::collections::HashMap<Source,bool>,
    jobs:Vec<(u64,PathBuf)>,
    rasters:Vec<(PathBuf,Option<u64>,Option<u64>)>,
    trenches:Vec<(u64,String)>,
}
impl Dialog {
    fn new(app: &SleufBaseApp,settings:&MigratedSettings) -> Self {
        let options = Options {
            project: app.project_name.clone(),
            raster_layout:app.datasets.is_empty(),
            choices: if app.datasets.is_empty(){template_project::suggest_rasters(&app.geotiffs,&app.virtual_trenches)}else{template_project::suggest(&app.datasets,&app.geotiffs,&app.virtual_trenches)},
            ..Options::default()
        };
        // Retain saved per-source choices and read current general settings.
        let mut options=configure_options(options,settings,app.template_preferences.as_ref(),&app.datasets,&app.geotiffs,&app.virtual_trenches);
        // A raster-only project may use unlabelled TIFFs deliberately. When
        // actual trench sources are present, keep unrelated images available
        // as attachments without turning each one into a new export slot.
        let has_trenches=!app.datasets.is_empty()||!app.virtual_trenches.is_empty()||app.geotiffs.iter().any(trench_raster);
        if has_trenches{
            options.choices.retain(|choice|match choice.source{
                Source::Raster(index)=>trench_raster(&app.geotiffs[index])
                    ||app.template_preferences.as_ref().is_some_and(|saved|saved.contains_raster_slot(&app.geotiffs[index])),
                _=>true,
            });
        }
        let fresh=app.template_preferences.is_none();
        let mut editor=sleufbase_rust::template_editor::Editor::new(&mut options.choices,&app.geotiffs,&app.virtual_trenches);
        // Existing/imported layouts retain their hand-picked sheet order.
        if fresh {editor.sort_choices(&mut options.choices,SortMode::NumberAsc);}
        let automatic=options.choices.iter().map(|choice| match choice.source {
            Source::Job(i) => sleufbase_rust::template_match::for_job(&app.datasets[i], &app.geotiffs),
            Source::Virtual(i) => sleufbase_rust::template_match::for_trench(&app.virtual_trenches[i], &app.geotiffs),
            Source::Raster(i)=>sleufbase_rust::template_match::Suggestion{raster:Some(i),reason:sleufbase_rust::template_match::MatchKind::Missing},
            Source::Empty(_)=>sleufbase_rust::template_match::Suggestion{raster:None,reason:sleufbase_rust::template_match::MatchKind::Missing},
        }).collect();
        let opening_defaults=crate::template_dialog::ExportDefaults::from_settings(settings)
            .expect("validated project export settings");
        let profile_baseline=options.choices.iter()
            .filter(|choice|!matches!(choice.source,Source::Empty(_)))
            .map(|choice|(choice.source,choice.profile)).collect();
        Self { options, output: None, error: String::new(), automatic, editor,
            sort_mode: fresh.then_some(SortMode::NumberAsc),
            reveal_row: None,
            opening_defaults, profile_baseline,
            jobs:app.datasets.iter().map(|d|(d.job_id,d.source_path.clone())).collect(),
            rasters:app.geotiffs.iter().map(|r|(r.path.clone(),r.kickthemap_job_id,r.metadata.get("_sleufbase_python_tiff_index").and_then(serde_json::Value::as_u64))).collect(),
            trenches:app.virtual_trenches.iter().map(|t|(t.job_id,t.label.clone())).collect(),
        }
    }
    fn include_all(&mut self,included:bool) {
        for choice in &mut self.options.choices {
            if !matches!(choice.source,Source::Empty(_)){choice.include=included;}
        }
        self.editor.sync(&self.options.choices);
    }
    fn profiles_for_included(&mut self,enabled:bool) {
        for choice in self.options.choices.iter_mut().filter(|choice|choice.include&&!matches!(choice.source,Source::Empty(_))) {
            choice.profile=enabled;
        }
    }
    fn sort_choices(&mut self, mode: SortMode) {
        self.sort_mode=Some(mode);
        let order=self.editor.sort_choices(&mut self.options.choices,mode);
        template_order::reorder(&mut self.automatic,&order);
        self.reveal_row=self.editor.selected.iter().position(|selected|*selected);
    }
    fn placements(&self)->Vec<ExportPlacement> {
        let (labels,_)=self.editor.preview(&self.options.choices);
        let mut position=0;
        self.options.choices.iter().zip(labels).map(|(choice,label)| {
            if !choice.include{return ExportPlacement::Excluded;}
            position+=1;
            if matches!(choice.source,Source::Empty(_)){ExportPlacement::Empty(position)}
            else{ExportPlacement::Trench{position,label}}
        }).collect()
    }
    /// In-memory readiness only: network paths must not be probed every frame.
    /// Availability and overwrite checks run on the explicit export action.
    fn readiness(&self)->Result<(),String>{
        self.options.title_fields.validate()?;
        let selected=self.options.choices.iter().filter(|c|c.include).collect::<Vec<_>>();
        if !selected.iter().any(|c|!matches!(c.source,Source::Empty(_))){return Err("Neem minimaal één proefsleuf mee; vink Meenemen aan bij de gewenste sleuven.".into());}
        let output=self.output.as_ref().ok_or("Kies eerst een nieuwe uitvoer-DXF.")?;
        if output.extension().is_none_or(|ext|!ext.to_string_lossy().eq_ignore_ascii_case("dxf")){return Err("De uitvoernaam moet eindigen op .dxf.".into());}
        if selected.len()>template_project::MAX_SLOTS{return Err("Er zijn te veel exportvakken. Verminder de selectie.".into());}
        if let Some(&index)=self.editor.preview(&self.options.choices).1.first(){
            return Err(format!("Regel {:02}: {} Of zet Meenemen uit.",index+1,invalid_label_hint(&self.options.choices[index])));
        }
        if let Some(choice)=selected.iter().find(|c|c.raster.is_none()&&!matches!(c.source,Source::Virtual(_)|Source::Empty(_))){return Err(format!("Koppel een GeoTIFF aan {} of zet Meenemen uit.",choice.title));}
        Ok(())
    }
    fn sources_match(&self,app:&SleufBaseApp)->bool {
        self.jobs.len()==app.datasets.len() && self.rasters.len()==app.geotiffs.len()
            && self.trenches.len()==app.virtual_trenches.len()
            && self.jobs.iter().zip(&app.datasets).all(|(key,d)|key.0==d.job_id && key.1==d.source_path)
            && self.rasters.iter().zip(&app.geotiffs).all(|(key,r)|key.0==r.path && key.1==r.kickthemap_job_id && key.2==r.metadata.get("_sleufbase_python_tiff_index").and_then(serde_json::Value::as_u64))
            && self.trenches.iter().zip(&app.virtual_trenches).all(|(key,t)|key.0==t.job_id && key.1==t.label)
    }
}

// On wide windows keep the setup compact so trench selection is visible.
// These cards share the same controls and values in the stacked layout.
fn project_output_card(ui:&mut egui::Ui,dialog:&mut Dialog) {
    theme::card(12,12,10).show(ui,|ui| {
        ui.set_min_width(ui.available_width());
        ui.heading("1. Project en uitvoerbestand");
        let field_width=(ui.available_width()-110.).max(80.);
        egui::Grid::new("project-template-identity").num_columns(2).min_col_width(90.).show(ui,|ui|{
            ui.label("Projectnaam");ui.add(egui::TextEdit::singleline(&mut dialog.options.project).desired_width(field_width));ui.end_row();
            ui.label("Tekenaar");ui.add(egui::TextEdit::singleline(&mut dialog.options.drawn_by).desired_width(field_width));ui.end_row();
        });
        crate::template_dialog::title_fields(ui, &mut dialog.options.title_fields);
        if dialog.options.template.as_os_str().is_empty(){ui.small("Sjabloon: ingebouwd SleufBase-sjabloon");}
        else{ui.small(format!("Eigen sjabloon: {}",dialog.options.template.file_name().unwrap_or_default().to_string_lossy())).on_hover_text(dialog.options.template.display().to_string());}
        if let Some(path)=&dialog.output {
            ui.add(egui::Label::new(path.file_name().unwrap_or_default().to_string_lossy()).truncate())
                .on_hover_text(path.display().to_string());
        }else{ui.weak("Kies onderaan een nieuw uitvoerbestand.");}
    });
}
fn project_map_card(ui:&mut egui::Ui,dialog:&mut Dialog) {
    theme::card(12,12,10).show(ui,|ui| {
        ui.set_min_width(ui.available_width());
        ui.heading("2. Kaarten en profielen");
        ui.horizontal_wrapped(|ui| {
            use sleufbase_rust::template_server::Mode;
            ui.label("Kaartgegevens");
            ui.radio_value(&mut dialog.options.server_mode,Mode::Fresh,"Online")
                .on_hover_text("Actuele kaartgegevens ophalen.");
            ui.radio_value(&mut dialog.options.server_mode,Mode::Offline,"Offline")
                .on_hover_text("Alleen al geladen gegevens gebruiken, met een witte kaart zonder luchtfoto.");
        });
        if dialog.options.server_mode==sleufbase_rust::template_server::Mode::Fresh {
            ui.horizontal_wrapped(|ui| {
                ui.label("Achtergrond");
                ui.radio_value(&mut dialog.options.background,BackgroundKind::Pdok,"Luchtfoto");
                ui.radio_value(&mut dialog.options.background,BackgroundKind::Osm,"OpenStreetMap");
                ui.radio_value(&mut dialog.options.background,BackgroundKind::Cadastral,"Kadaster");
                ui.radio_value(&mut dialog.options.background,BackgroundKind::Blank,"Wit");
            });
        }else{
            ui.small("Witte kaart · geladen gegevens")
                .on_hover_text("Je keuze voor de online achtergrond blijft bewaard.");
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("Kaartkwaliteit");
            let current=match dialog.options.dpi{150=>"150 DPI · kleiner".into(),300=>"300 DPI · scherp".into(),dpi=>format!("{dpi} DPI · aangepast")};
            egui::ComboBox::from_id_salt("project-template-quality").width(160.).selected_text(current).show_ui(ui,|ui|{
                ui.selectable_value(&mut dialog.options.dpi,300,"300 DPI · scherp (standaard)");
                ui.selectable_value(&mut dialog.options.dpi,150,"150 DPI · kleiner bestand");
            });
        });
        ui.checkbox(&mut dialog.options.paired,"Beide kijkrichtingen")
            .on_hover_text("Maakt de normale en de omgekeerde richting. Beide blijven bewaard; kies in AutoCAD met Versie de gewenste richting.");
        ui.collapsing("Meer exportinstellingen",|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Kaartresolutie (DPI)");
                ui.add(egui::DragValue::new(&mut dialog.options.dpi).range(30..=600));
                ui.label("Kaartschaal 1:");
                ui.add(egui::DragValue::new(&mut dialog.options.scale).range(1.0..=100_000.).speed(1.));
            });
            ui.small("Dit zijn instellingen van de locatiekaart. De grote afbeelding van een virtuele sleuf blijft op hoge detailkwaliteit.");
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut dialog.options.include_bgt,"BGT-ondergrond").on_hover_text("Gebouwen, wegen en andere beschikbare topografie meenemen.");
                ui.checkbox(&mut dialog.options.include_cadastral,"Perceelgrenzen en -nummers");
                ui.checkbox(&mut dialog.options.include_cad,"Zichtbare CAD-lagen").on_hover_text("Ook zichtbare lijnen uit geneste DXF-blokken meenemen.");
            });
            if dialog.options.server_mode==sleufbase_rust::template_server::Mode::Offline{ui.small("Offline worden alleen al geladen kaartgegevens gebruikt.");}
            ui.separator();
            if dialog.options.template.as_os_str().is_empty(){ui.label("Sjabloon: ingebouwd SleufBase-sjabloon");}
            else{ui.label(format!("Eigen sjabloon: {}",dialog.options.template.display()));}
            ui.horizontal_wrapped(|ui| {
                if ui.button("Eigen DXF-sjabloon kiezen…").clicked(){
                    if let Some(path)=rfd::FileDialog::new().add_filter("AutoCAD DXF",&["dxf"]).pick_file(){dialog.options.template=path;}
                }
                if !dialog.options.template.as_os_str().is_empty()&&ui.button("Ingebouwd sjabloon gebruiken").clicked(){dialog.options.template=PathBuf::new();}
            });
            crate::template_dialog::logos(ui,&mut dialog.options.logos);
            crate::template_dialog::font_folders(ui,&mut dialog.options.font_directories);
        });
    });
}

fn order_tool_button(ui:&mut egui::Ui,label:&str,enabled:bool)->egui::Response {
    let response=ui.add_enabled(enabled,egui::Button::new(label)
        .min_size(egui::vec2(ui.available_width(),28.)));
    // Source cards were painted before this independent panel. Repaint after
    // an action so their selection, labels and inclusion update immediately.
    if response.clicked() {ui.ctx().request_repaint();}
    response
}

/// The tools have their own scroll area. Row selection and movement stay above
/// it, while source cards and the export footer have independent geometry.
fn project_order_panel(ui:&mut egui::Ui,dialog:&mut Dialog,rasters:&[GeoTiffInfo],trenches:&[VirtualTrench],height:f32) {
    if let Some(direction)=crate::template_dialog::order_controls(ui,&dialog.editor.selected,height) {
        let swaps=dialog.editor.move_selected(&mut dialog.options.choices,direction);
        if !swaps.is_empty() {
            dialog.sort_mode=None;
            for(a,b)in swaps {dialog.automatic.swap(a,b);}
            dialog.reveal_row=if direction<0 {
                dialog.editor.selected.iter().position(|selected|*selected)
            } else {
                dialog.editor.selected.iter().rposition(|selected|*selected)
            };
            ui.ctx().request_repaint();
        }
    }
    ui.separator();
    let tools_height=ui.available_height().max(0.);
    if tools_height<1. { return; }
    egui::ScrollArea::vertical().id_salt("project-template-order-tools")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .max_height(tools_height).auto_shrink([false,false]).show(ui,|ui| {
            ui.set_width(ui.available_width());
            ui.small("Klik links op een regelnummer. Ctrl/Cmd selecteert losse regels; Shift een reeks. Selecteren verandert niet wat wordt geëxporteerd.");

            ui.add_space(8.);
            ui.strong("Sorteren");
            let mut requested_sort=dialog.sort_mode;
            egui::ComboBox::from_id_salt("project-template-sort")
                .width(ui.available_width()).truncate()
                .selected_text(requested_sort.map_or("Handmatig / opgeslagen",SortMode::label))
                .show_ui(ui,|ui| {
                    for mode in SortMode::ALL {
                        ui.selectable_value(&mut requested_sort,Some(mode),mode.label());
                    }
                });
            let sort_again=order_tool_button(ui,"Opnieuw sorteren",!dialog.options.choices.is_empty()).clicked();
            if requested_sort!=dialog.sort_mode || sort_again {
                if let Some(mode)=requested_sort.or_else(||sort_again.then_some(SortMode::NumberAsc)) {
                    dialog.sort_choices(mode);
                    ui.ctx().request_repaint();
                }
            }

            ui.separator();
            ui.strong("Selectie");
            if order_tool_button(ui,"Alle regels selecteren",!dialog.options.choices.is_empty()).clicked() {
                dialog.editor.select_all(&dialog.options.choices,true);
            }
            let has_selection=dialog.editor.selected.iter().any(|selected|*selected);
            if order_tool_button(ui,"Selectie wissen",has_selection).clicked() {
                dialog.editor.select_all(&dialog.options.choices,false);
            }
            let selected_trenches=dialog.options.choices.iter().zip(&dialog.editor.selected)
                .any(|(choice,selected)|*selected&&!matches!(choice.source,Source::Empty(_)));
            for (label,included) in [("Selectie meenemen",true),("Selectie uitsluiten",false)] {
                if order_tool_button(ui,label,selected_trenches).clicked() {
                    dialog.editor.included(&mut dialog.options.choices,included);
                }
            }

            ui.separator();
            ui.strong("Nummering en schaal");
            ui.add_enabled_ui(selected_trenches,|ui| {
                ui.label("PS-nummer");
                ui.add(egui::TextEdit::singleline(&mut dialog.editor.number)
                    .desired_width(ui.available_width()));
                ui.label("Variant");
                egui::ComboBox::from_id_salt("mixed-template-variant")
                    .width(ui.available_width()).truncate()
                    .selected_text(if dialog.editor.variant.is_empty(){"Geen variant"}else{&dialog.editor.variant})
                    .show_ui(ui,|ui| {
                        ui.selectable_value(&mut dialog.editor.variant,String::new(),"Geen variant");
                        ui.selectable_value(&mut dialog.editor.variant,"AUTO".into(),"Automatisch (B/C/D...)");
                        for code in 'B'..='Z' {
                            ui.selectable_value(&mut dialog.editor.variant,code.to_string(),format!("Variant {code}"));
                        }
                    });
                if order_tool_button(ui,"Nummer/variant toepassen",true).clicked() {
                    match dialog.editor.apply_label(&mut dialog.options.choices) {
                        Ok(())=>{
                            dialog.error.clear();
                            if let Some(mode)=dialog.sort_mode { dialog.sort_choices(mode); }
                        },
                        Err(error)=>dialog.error=error,
                    }
                }
                ui.small("Labels gelden voor de geselecteerde, meegenomen sleuven.");
                ui.add_space(4.);
                ui.label("Profielschaal");
                ui.add(egui::TextEdit::singleline(&mut dialog.editor.scale)
                    .desired_width(ui.available_width()).hint_text("Auto of 1:200"));
                if order_tool_button(ui,"Schaal toepassen",true).clicked() {
                    dialog.error=dialog.editor.apply_scale(&mut dialog.options.choices).err().unwrap_or_default();
                }
            });

            ui.separator();
            ui.strong("Lege en extra vakken");
            if order_tool_button(ui,"Leeg vak invoegen",dialog.options.choices.len()<20_000).clicked() {
                match dialog.editor.insert_empty(&mut dialog.options.choices) {
                    Ok(at)=>{
                        dialog.automatic.insert(at,sleufbase_rust::template_match::Suggestion {
                            raster:None,reason:sleufbase_rust::template_match::MatchKind::Missing,
                        });
                        dialog.reveal_row=Some(at);
                    },
                    Err(error)=>dialog.error=error,
                }
            }
            let selected_empty=dialog.options.choices.iter().zip(&dialog.editor.selected)
                .any(|(choice,selected)|*selected&&matches!(choice.source,Source::Empty(_)));
            if order_tool_button(ui,"Lege vakken verwijderen",selected_empty).clicked() {
                for index in dialog.editor.remove_empty(&mut dialog.options.choices) {
                    dialog.automatic.remove(index);
                }
                dialog.reveal_row=dialog.editor.selected.iter().position(|selected|*selected);
            }
            let mut add_raster=None;
            ui.add_enabled_ui(dialog.options.choices.len()<20_000,|ui| {
                ui.menu_button("GeoTIFF als vak toevoegen…",|ui| {
                    egui::ScrollArea::vertical().max_height(230.).show(ui,|ui| {
                        for (index,raster) in rasters.iter().enumerate() {
                            if dialog.options.choices.iter().any(|choice|choice.raster==Some(index)||choice.source==Source::Raster(index)) {continue;}
                            let name=raster.path.file_name().unwrap_or_default().to_string_lossy();
                            if ui.button(name).clicked() {add_raster=Some(index);ui.close_menu();}
                        }
                    });
                });
            });
            if let Some(index)=add_raster {
                let mut choice=template_project::raster_choice(index,&rasters[index]);
                choice.profile=dialog.opening_defaults.profiles;
                dialog.profile_baseline.insert(choice.source,choice.profile);
                dialog.options.choices.push(choice);
                dialog.automatic.push(sleufbase_rust::template_match::Suggestion {
                    raster:Some(index),reason:sleufbase_rust::template_match::MatchKind::Missing,
                });
                dialog.editor=sleufbase_rust::template_editor::Editor::new(
                    &mut dialog.options.choices,rasters,trenches,
                );
                let at=dialog.options.choices.len()-1;
                dialog.editor.select(&dialog.options.choices,at,false,false);
                dialog.reveal_row=Some(at);
                dialog.sort_mode=None;
            }

            ui.separator();
            ui.strong("Alle proefsleuven");
            for (label,included) in [("Alles meenemen",true),("Alles uitsluiten",false)] {
                if order_tool_button(ui,label,!dialog.options.choices.is_empty()).clicked() {
                    dialog.include_all(included);
                }
            }
            ui.label("Dwarsprofielen voor meegenomen sleuven");
            ui.horizontal_wrapped(|ui| {
                if ui.button("Aan").clicked() {dialog.profiles_for_included(true);ui.ctx().request_repaint();}
                if ui.button("Uit").clicked() {dialog.profiles_for_included(false);ui.ctx().request_repaint();}
            });
            if dialog.reveal_row.is_some() {ui.ctx().request_repaint();}
        });
}

impl SleufBaseApp {
    pub(super) fn open_project_template_dialog(&mut self) {
        if self.template_job.is_some() || self.export_intent.is_some() ||
           self.file_intent.is_some() || self.pending_project_action.is_some() {
            self.message = "Rond de lopende taak eerst af".into();
            return;
        }
        if self.datasets.is_empty() && self.virtual_trenches.is_empty() && self.geotiffs.is_empty() {
            self.message = "Laad eerst jobs of maak virtuele proefsleuven".into();
            return;
        }
        self.template_dialog = None;
        self.template_order_dialog = None;
        match self.project_export_settings(){Ok(settings)=>self.project_template_dialog=Some(Dialog::new(self,&settings)),Err(error)=>self.message=error}
    }
    pub(super) fn show_project_template_dialog(&mut self, ctx: &egui::Context) {
        // Choices contain vector indices. Never let removed/reordered sources
        // silently turn a reviewed slot into a different measurement job.
        if self.project_template_dialog.as_ref().is_some_and(|d|!d.sources_match(self)) {
            self.project_template_dialog=None;
            self.message="Projectbronnen gewijzigd. Open de sjabloonexport opnieuw om de koppelingen te controleren.".into();
            return;
        }
        let mut open = true;
        let mut launch = None;
        let mut cancel = false;
        let rasters = &self.geotiffs;
        let raster_previews = &self.raster_textures;
        if let Some(dialog) = &mut self.project_template_dialog {
            let mut clicked_row=None;let mut sync_editor=false;let mut edited_label=None;
            let screen = ctx.screen_rect();
            let maximum = egui::vec2((screen.width()-64.).max(280.), (screen.height()-88.).max(200.));
            egui::Window::new("DXF-sjabloon exporteren")
                // Reserve readable space for the permanent tool panel, including
                // when an older release saved a much narrower window size.
                .id(egui::Id::new("project-template-tools-v2"))
                .open(&mut open)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .default_size(egui::vec2(940.,560.).min(maximum))
                .min_size(egui::vec2(680.,320.).min(maximum))
                .max_size(maximum)
                .constrain_to(screen.shrink(16.))
                .collapsible(false)
                .resizable(true)
                .show(ctx, |ui| {
                    settings_ui::control_style(ui);
                    ui.style_mut().wrap_mode=Some(egui::TextWrapMode::Wrap);
                    let footer_height = if dialog.error.is_empty(){118.}else{142.}
                        + if ui.available_width()<440.{34.}else{0.};
                    let body_height=(ui.available_height()-footer_height).max(64.);
                    let reveal_row=dialog.reveal_row.take();
                    let (mut body_ui,mut controls_ui)=crate::template_dialog::order_columns(ui,body_height);
                    {
                        let ui=&mut body_ui;
                        egui::ScrollArea::vertical().id_salt("project-template-compact-body")
                            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                            .max_height(body_height)
                            .auto_shrink([false,false]).show(ui, |ui| {
                        let card_width=ui.available_width();
                        if ui.available_width()>=600. {
                            ui.columns(2,|columns| {
                                project_output_card(&mut columns[0],dialog);
                                project_map_card(&mut columns[1],dialog);
                            });
                        }else{
                            project_output_card(ui,dialog);
                            ui.add_space(8.);
                            project_map_card(ui,dialog);
                        }
                        ui.separator();
                        let summary=export_summary(&dialog.options);
                        let excluded=dialog.options.choices.iter().filter(|choice|!choice.include&&!matches!(choice.source,Source::Empty(_))).count();
                        ui.horizontal_wrapped(|ui| {
                            ui.heading("3. Proefsleuven kiezen").on_hover_text("Meenemen bepaalt wat wordt geëxporteerd. Uitgesloten sleuven krijgen geen vak; lege vakken blijven behouden.");
                            ui.small(format!("{} mee · {excluded} uit · {} leeg",summary.trenches,summary.empty));
                        });
                        if summary.trenches+summary.empty>template_project::MAX_SLOTS {
                            ui.colored_label(DARK,"Te veel proefsleufvakken voor één export");
                        }
                        let mut placements=dialog.placements();
                        // All cards use the viewport width, not their different
                        // source labels or the length of the current TIFF name.
                        for idx in 0..dialog.options.choices.len() {
                            ui.push_id(idx, |ui| {
                                let frame=theme::card(10,10,8)
                                    .stroke(Stroke::new(1.0_f32,if dialog.editor.selected[idx]{ORANGE}else{theme::BORDER}));
                                let content_width=(card_width-frame.total_margin().sum().x).max(0.);
                                let card=frame.show(ui,|ui| {
                                    ui.set_width(content_width);
                                    // Keep the scroll area's vertical clip: this
                                    // card can grow taller when details expand.
                                    let mut clip=ui.clip_rect();
                                    clip.min.x=clip.min.x.max(ui.max_rect().min.x);
                                    clip.max.x=clip.max.x.min(ui.max_rect().max.x);
                                    ui.set_clip_rect(clip);
                                    ui.spacing_mut().item_spacing.y=8.;
                                    let mut placement_changed=false;
                                    ui.horizontal_wrapped(|ui| {
                                        let slot=&mut dialog.options.choices[idx];
                                        if ui.selectable_label(dialog.editor.selected[idx],format!("Regel {:02}",idx+1))
                                            .on_hover_text("Selecteer voor verplaatsen of samen aanpassen. Ctrl/Cmd: losse regels; Shift: een reeks. Meenemen bepaalt wat wordt geëxporteerd.")
                                            .clicked(){let modifiers=ui.input(|i|i.modifiers);clicked_row=Some((idx,modifiers.ctrl||modifiers.command,modifiers.shift));}
                                        if !matches!(slot.source,Source::Empty(_)){if ui.checkbox(&mut slot.include,"Meenemen").changed(){sync_editor=true;placement_changed=true;}}
                                        if !matches!(slot.source, Source::Empty(_)) {
                                            let label = slot.label.get_or_insert_with(String::new);
                                            if ui.add(egui::TextEdit::singleline(label)
                                                .hint_text("Proefsleuflabel")
                                                .desired_width(150.)
                                                .char_limit(256)).on_hover_text("Bijvoorbeeld PS 25-2 of PS25B. Open Details voor de bron, schaal of werkfoto's.").changed() {
                                                edited_label = Some(idx);
                                                placement_changed=true;
                                            }
                                        }
                                        ui.weak(match slot.source{Source::Job(_)=>"KickTheMap",Source::Virtual(_)=>"Virtueel",Source::Raster(_)=>"GeoTIFF",Source::Empty(_)=>"Leeg vak"});
                                        if !matches!(slot.source,Source::Empty(_)) {
                                            ui.add_enabled(slot.include,egui::Checkbox::new(&mut slot.profile,"Dwarsprofiel"));
                                        }
                                    });
                                    // Refresh after this row's controls so later positions and AUTO
                                    // variants already reflect the changed inclusion in this frame.
                                    if placement_changed{placements=dialog.placements();}
                                    let slot=&mut dialog.options.choices[idx];
                                    placement_caption(ui,&placements[idx],slot);
                                    if matches!(slot.source,Source::Empty(_)){return;}
                                    if slot.include && slot.raster.is_none() && matches!(slot.source,Source::Job(_)|Source::Raster(_)) {
                                        ui.colored_label(DARK, "Kies bij Details een GeoTIFF, of zet Meenemen uit.");
                                    }
                                    ui.collapsing("Details: bron, schaal en werkfoto’s", |ui| {
                                        ui.label("Naam");
                                        ui.add(egui::TextEdit::singleline(&mut slot.title)
                                            .desired_width(ui.available_width()));
                                        ui.label("GeoTIFF:");
                                        let selected_name = slot.raster.and_then(|i| rasters.get(i))
                                            .and_then(|r|r.path.file_name())
                                            .map(|s|s.to_string_lossy().to_string())
                                            .unwrap_or_else(||match slot.source {
                                                Source::Virtual(_)=>"Eigen virtueel raster (geen TIFF nodig)".into(),
                                                Source::Job(_)=>"Ontbreekt — selecteer een TIFF".into(),
                                                Source::Raster(_)=>"Ontbreekt — selecteer een TIFF".into(),
                                                Source::Empty(_)=>"Leeg vak".into(),
                                            });
                                        egui::ComboBox::from_id_salt("raster")
                                            .width(ui.available_width()).truncate()
                                            .selected_text(&selected_name)
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(&mut slot.raster, None,
                                                    if matches!(slot.source,Source::Virtual(_)){
                                                        "Eigen virtueel raster (zonder externe TIFF)"
                                                    }else{"Geen (niet exporteerbaar)"});
                                                for (i, info) in rasters.iter().enumerate() {
                                                    let filename = info.path.file_name()
                                                        .map(|s|s.to_string_lossy().to_string())
                                                        .unwrap_or_else(||"GeoTIFF".into());
                                                    ui.selectable_value(&mut slot.raster, Some(i),
                                                        format!("{} ({})", filename,
                                                            info.kickthemap_job_id.map_or("?".into(),|id|id.to_string())));
                                                }
                                            }).response.on_hover_text(selected_name);
                                        ui.add_enabled(!dialog.options.paired,egui::Checkbox::new(&mut slot.reverse,"Alleen omgekeerde richting"));
                                    ui.add_enabled_ui(slot.profile, |ui| {let previous=slot.manual_scale;template_profile_ui::scale(ui, &mut slot.manual_scale);if previous!=slot.manual_scale{sync_editor=true;}});
                                    if slot.include && slot.raster.is_none() && matches!(slot.source,Source::Job(_)) {
                                        ui.colored_label(DARK, "Koppel eerst een GeoTIFF voordat dit vak geëxporteerd kan worden.");
                                    }
                                    let suggested = dialog.automatic[idx];
                                    if let Some(automatic_idx)=suggested.raster {
                                        if slot.raster==Some(automatic_idx) {
                                            ui.small(match suggested.reason {
                                                sleufbase_rust::template_match::MatchKind::JobId =>
                                                    "Automatische TIFF-koppeling: unieke KickTheMap-job-ID",
                                                sleufbase_rust::template_match::MatchKind::Location =>
                                                    "Automatische TIFF-koppeling: unieke ligging in RD",
                                                _ => "TIFF handmatig beoordelen",
                                            });
                                        } else if ui.small_button("Herstel automatische TIFF-keuze").clicked() {
                                            slot.raster=Some(automatic_idx);
                                        }
                                    } else if suggested.reason == sleufbase_rust::template_match::MatchKind::Ambiguous {
                                        ui.small("Meerdere mogelijke TIFF's; handmatige keuze nodig.");
                                    }
                                    if let Some(raster_index)=slot.raster {
                                        if let Some(info)=rasters.get(raster_index) {
                                            ui.collapsing("GeoTIFF-voorbeeld en kaartgegevens", |ui| {
                                              ui.horizontal_wrapped(|ui| {
                                                if let Some(Some(preview))=raster_previews.get(raster_index) {
                                                    let width=ui.available_width().min(190.).max(1.);
                                                    ui.add(egui::Image::new((preview.id(),egui::vec2(width,width*120./190.))));
                                                }
                                                ui.vertical(|ui| {
                                                    ui.small(format!("RD X {:.1}–{:.1}",info.bounds.min_x,info.bounds.max_x));
                                                    ui.small(format!("RD Y {:.1}–{:.1}",info.bounds.min_y,info.bounds.max_y));
                                                    ui.small(format!("CRS: {}",info.epsg.map_or("onbekend".into(),|id|format!("EPSG:{id}"))));
                                                });
                                              });
                                            });
                                        }
                                    }
                                    ui.collapsing(format!("Werkfoto's voor dit vak ({}/{})",
                                        slot.photos.len(),
                                        sleufbase_rust::template_photo::MAX_PHOTOS_PER_SLOT), |ui| {
                                        ui.small("JPEG/PNG als eigen afbeeldingsbestand op de rechterkant van het gekozen kaart- of TIFF-vak. De oorspronkelijke rasterbron blijft bestaan.");
                                        if ui.add_enabled(slot.photos.len() <
                                            sleufbase_rust::template_photo::MAX_PHOTOS_PER_SLOT,
                                            egui::Button::new("Voeg werkfoto's toe…")).clicked() {
                                            if let Some(selected)=rfd::FileDialog::new()
                                                .add_filter("Foto's", &["jpg","jpeg","png"])
                                                .pick_files() {
                                                for path in selected {
                                                    if slot.photos.len()>=sleufbase_rust::template_photo::MAX_PHOTOS_PER_SLOT {break;}
                                                    if !slot.photos.iter().any(|p|p.path==path) {
                                                        slot.photos.push(sleufbase_rust::template_photo::Photo {
                                                            path,turns:0,
                                                            area:sleufbase_rust::template_photo::Area::Tiff
                                                        });
                                                    }
                                                }
                                            }
                                        }
                                        if !slot.photos.is_empty(){ui.small("De gekozen fotobestanden worden bij het starten gecontroleerd.");}
                                        let mut remove=None;
                                        for (photo_index,photo) in slot.photos.iter_mut().enumerate() {
                                            ui.push_id(photo_index,|ui|{
                                                    ui.add(egui::Label::new(photo.path.file_name()
                                                        .map(|s|s.to_string_lossy().to_string())
                                                        .unwrap_or_else(||"Werkfoto".into())).truncate())
                                                        .on_hover_text(photo.path.display().to_string());
                                                ui.horizontal_wrapped(|ui|{
                                                    egui::ComboBox::from_id_salt("rotate")
                                                        .width(70.).truncate()
                                                        .selected_text(format!("{}°",u32::from(photo.turns)*90))
                                                        .show_ui(ui,|ui|{
                                                            for turn in 0..4 {
                                                                ui.selectable_value(&mut photo.turns,turn,format!("{}°",u32::from(turn)*90));
                                                            }
                                                        });
                                                    egui::ComboBox::from_id_salt("area")
                                                        .width(90.).truncate()
                                                        .selected_text(match photo.area{
                                                            sleufbase_rust::template_photo::Area::Tiff=>"Op TIFF",
                                                            sleufbase_rust::template_photo::Area::Map=>"Op kaart",
                                                        }).show_ui(ui,|ui|{
                                                            ui.selectable_value(&mut photo.area,
                                                                sleufbase_rust::template_photo::Area::Tiff,"Op TIFF");
                                                            ui.selectable_value(&mut photo.area,
                                                                sleufbase_rust::template_photo::Area::Map,"Op kaart");
                                                        });
                                                    if ui.small_button("Verwijder").clicked(){remove=Some(photo_index);}
                                                });
                                            });
                                        }
                                        if let Some(index)=remove {slot.photos.remove(index);}
                                    });
                                    ui.collapsing("Opmerkingen", |ui| {
                                        ui.add(egui::TextEdit::multiline(&mut slot.comments)
                                            .desired_width(ui.available_width()));
                                    });
                                    });
                                });
                                if reveal_row==Some(idx) {
                                    ui.scroll_to_rect(card.response.rect,Some(egui::Align::Min));
                                }
                            });
                            ui.add_space(4.);
                        }
                        });
                    }
                        // Commit this frame's draft edits before moving its rows,
                        // so label caches and automatic TIFF matches move together.
                        if let Some(index)=edited_label.take() {
                            dialog.editor.update_typed_label(&dialog.options.choices,index);
                        }
                        if let Some((index,control,shift))=clicked_row.take() {
                            dialog.editor.select(&dialog.options.choices,index,control,shift);
                            ui.ctx().request_repaint();
                        } else if sync_editor {
                            dialog.editor.sync(&dialog.options.choices);
                        }
                    project_order_panel(&mut controls_ui,dialog,rasters,&self.virtual_trenches,body_height);
                        ui.separator();
                        if !dialog.error.is_empty() {
                            ui.add(egui::Label::new(egui::RichText::new(format!("Laatste poging: {}",dialog.error)).color(DARK)).truncate()).on_hover_text(&dialog.error);
                        }
                        let summary=export_summary(&dialog.options);
                        ui.add(egui::Label::new(egui::RichText::new(format!("{} sleuven · {} profielen · {}",summary.trenches,summary.profiles,
                            if dialog.options.paired{"beide kijkrichtingen"}else{"één kijkrichting"})).strong()).truncate());
                        ui.add(egui::Label::new(egui::RichText::new(format!("{} · {} DPI · kaart 1:{}",effective_map_label(&dialog.options),dialog.options.dpi,dialog.options.scale)).small()).truncate());
                        let readiness=dialog.readiness();
                        let ready=readiness.is_ok();
                        if let Err(reason)=readiness{
                            ui.add(egui::Label::new(egui::RichText::new(&reason).small()).truncate()).on_hover_text(reason);
                        }else{ui.small("Selectie compleet. Bestanden worden bij starten gecontroleerd.");}
                        // These actions are outside the scrolling body and remain
                        // available when advanced options or many slots are open.
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Uitvoerbestand kiezen…").on_hover_text("Kies een nieuwe DXF. De bijbehorende afbeeldingen worden in dezelfde export opgeslagen.").clicked() {
                                if let Some(path)=rfd::FileDialog::new().add_filter("AutoCAD DXF",&["dxf"])
                                    .set_file_name("SleufBase-Project-Sjabloon-Rust.dxf").save_file(){dialog.output=Some(path);}
                            }
                            if ui.add_enabled(ready, egui::Button::new(egui::RichText::new("Export starten").color(Color32::WHITE)).fill(ORANGE))
                                .on_hover_text("Volledige project-sjabloonexport starten").clicked() {
                                launch = dialog.output.clone();
                            }
                            if ui.button("Annuleren").clicked() { cancel = true; }
                        });
                });

        }
        if !open || cancel { self.project_template_dialog = None; return; }
        if let Some(output) = launch {
            let options = self.project_template_dialog.as_ref().unwrap().options.clone();
            self.start_project_template_export(options, output);
            if let Some(dialog)=self.project_template_dialog.as_mut(){dialog.error=self.message.clone();}
        }
    }
}

#[cfg(test)]
mod preference_tests {
    use super::*;
    fn settings()->MigratedSettings {
        let mut settings=MigratedSettings::default();
        let p=&mut settings.export_preferences;
        p.cadastral_template_path="general.dxf".into();
        p.template_client_logo_preset="custom".into();p.template_client_logo_path="general-logo.png".into();
        p.template_include_cross_sections=false;p.template_generate_reverse_cross_section_sheet=true;
        p.template_cross_section_marker_diameter=0.075;p.template_show_profile_direction=false;
        p.template_cross_section_diameter_text="diam".into();settings
    }
    fn jobs()->Vec<Dataset>{vec![Dataset{job_id:42,title:"PS42".into(),source_path:"PS42.json".into(),features:Vec::new()}]}
    #[test]
    fn new_dialog_defaults_to_ps_numeric_order_but_saved_layout_is_untouched(){
        let jobs=vec![
            Dataset{job_id:10,title:"PS10".into(),source_path:"PS10.json".into(),features:Vec::new()},
            Dataset{job_id:2,title:"PS2".into(),source_path:"PS2.json".into(),features:Vec::new()},
            Dataset{job_id:1,title:"PS1".into(),source_path:"PS1.json".into(),features:Vec::new()},
        ];
        let app=SleufBaseApp{datasets:jobs.clone(),..Default::default()};
        let dialog=Dialog::new(&app,&settings());
        assert_eq!(dialog.sort_mode,Some(SortMode::NumberAsc));
        assert_eq!(dialog.options.choices.iter().map(|c|c.title.as_str()).collect::<Vec<_>>(),
                   ["PS1","PS2","PS10"]);
        assert_eq!(dialog.options.choices.iter().map(|c|c.label.as_deref()).collect::<Vec<_>>(),
                   [Some("PS1"),Some("PS2"),Some("PS10")]);
        // A saved layout is intentional, even if it isn't numeric.
        let saved=Options{choices:template_project::suggest(&jobs,&[],&[]),..Default::default()};
        let mut app=app;
        app.template_preferences=Some(sleufbase_rust::template_prefs::TemplatePreferences::capture(&saved,&jobs,&[],&[]));
        let restored=Dialog::new(&app,&settings());
        assert_eq!(restored.sort_mode,None);
        assert_eq!(restored.options.choices.iter().map(|c|c.title.as_str()).collect::<Vec<_>>(),
                   ["PS10","PS2","PS1"]);
    }
    #[test]
    fn project_dialog_applies_global_export_preferences_to_new_choices(){
        let jobs=jobs();let settings=settings();
        let options=Options{choices:template_project::suggest(&jobs,&[],&[]),..Default::default()};
        let configured=configure_options(options,&settings,None,&jobs,&[],&[]);
        assert_eq!(configured.template,PathBuf::from("general.dxf"));
        assert_eq!(configured.logos.client,Some(PathBuf::from("general-logo.png")));
        assert_eq!(configured.profile_style.marker_diameter,0.075);assert!(!configured.profile_style.show_reference);
        assert_eq!(configured.profile_style.diameter_prefix,"diam");
        assert!(!configured.choices[0].profile);assert!(!configured.choices[0].reverse);assert!(configured.paired);
    }
    #[test]
    fn project_dialog_keeps_saved_choices_with_current_general_defaults(){
        let jobs=jobs();let settings=settings();
        let mut saved_options=Options{template:"project.dxf".into(),choices:template_project::suggest(&jobs,&[],&[]),..Default::default()};
        saved_options.profile_style.marker_diameter=0.03;
        saved_options.logos.client=Some("project-logo.png".into());saved_options.include_bgt=false;
        let saved=sleufbase_rust::template_prefs::TemplatePreferences::capture(&saved_options,&jobs,&[],&[]);
        let configured=configure_options(Options::default(),&settings,Some(&saved),&jobs,&[],&[]);
        assert_eq!(configured.template,PathBuf::from("general.dxf"));
        assert_eq!(configured.logos.client,Some(PathBuf::from("general-logo.png")));
        assert_eq!(configured.profile_style.marker_diameter,0.075);assert!(!configured.include_bgt);
        assert!(configured.choices[0].profile);assert!(!configured.choices[0].reverse);
    }
    #[test]
    fn imported_layout_inherits_effective_project_settings_until_first_reviewed_export(){
        let jobs=jobs();let settings=settings();
        let mut imported_options=Options{project:"Archived name".into(),choices:template_project::suggest(&jobs,&[],&[]),..Default::default()};
        imported_options.choices[0].label=Some("PS42C".into());imported_options.choices[0].legacy_slot=Some(3);
        let mut saved=sleufbase_rust::template_prefs::TemplatePreferences::capture(&imported_options,&jobs,&[],&[]);saved.legacy_layout_only=true;
        let configured=configure_options(Options{project:"Current name".into(),..Default::default()},&settings,Some(&saved),&jobs,&[],&[]);
        assert_eq!(configured.project,"Current name");assert_eq!(configured.template,PathBuf::from("general.dxf"));
        assert_eq!(configured.logos.client,Some(PathBuf::from("general-logo.png")));
        assert_eq!(configured.profile_style.marker_diameter,0.075);assert!(!configured.choices[0].profile);
        assert_eq!(configured.choices[0].label.as_deref(),Some("PS42C"));assert_eq!(configured.choices[0].legacy_slot,Some(3));
        let captured=sleufbase_rust::template_prefs::TemplatePreferences::capture(&configured,&jobs,&[],&[]);assert!(!captured.legacy_layout_only);
    }
    #[test]
    fn ordinary_template_menu_opens_imported_mixed_layout_instead_of_resetting_it(){
        let raw=serde_json::json!({"version":1,"project_name":"Bewaard","tiffs":[{"type":"virtual_trench","name":"PS42","metadata":{"kickthemap_job_id":42,"virtual_trench_payload":{"width_meters":0.6,"points":[{"role":"start","x":155000.,"y":463000.,"z":1.},{"role":"end","x":155010.,"y":463000.,"z":1.}]}}}],"template_export_slots":[{"layer_key":null},{"layer_key":"job:42","ps_number":"7","variant_code":"C"}]});
        let project=sleufbase_rust::legacy_project::import_value(raw,std::path::Path::new("fixture.klicproject")).unwrap();
        let mut app=SleufBaseApp{migrated_settings:MigratedSettings::default(),..Default::default()};
        app.apply_project(project,None,true,vec![vec![]],&egui::Context::default());
        app.open_template_export();
        assert!(app.template_order_dialog.is_none());
        let dialog=app.project_template_dialog.as_ref().unwrap();
        assert_eq!(dialog.options.choices.len(),2);assert!(matches!(dialog.options.choices[0].source,Source::Empty(_)));
        assert_eq!(dialog.options.choices[1].label.as_deref(),Some("PS7C"));
        let options=std::mem::take(&mut app.project_template_dialog.as_mut().unwrap().options);
        let allocation=app.virtual_trenches.as_ptr();app.live_busy=true;
        let dir=tempfile::tempdir().unwrap();app.start_project_template_export(options,dir.path().join("deferred.dxf"));
        assert!(app.template_job.is_none());assert_eq!(app.virtual_trenches.as_ptr(),allocation);
        assert!(app.message.contains("downloads"));
    }
}

#[cfg(test)]
mod flow_audit_tests {
    use super::*;
    fn app()->SleufBaseApp{
        let mut app=SleufBaseApp::default();
        app.virtual_trenches.push(VirtualTrench::new("PS12".into(),Vertex{x:155000.,y:463000.,z:Some(2.)},Vertex{x:155010.,y:463000.,z:Some(2.)},0.6).unwrap());app
    }
    #[test]
    fn ordinary_export_suggests_trenches_without_unrelated_raster_rows(){
        let mut app=app();let path=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters/gray16.tiff");
        let plain=sleufbase_rust::geotiff::read_geotiff_metadata(&path).unwrap();
        let mut named=plain.clone();
        named.path="PS2.tiff".into();
        app.geotiffs.extend([plain,named]);
        app.open_template_export();
        let d=app.project_template_dialog.as_ref().expect("mixed project dialog");assert!(app.template_order_dialog.is_none());
        let sources=d.options.choices.iter().map(|choice|choice.source).collect::<Vec<_>>();
        assert_eq!(sources,vec![Source::Raster(1),Source::Virtual(0)]);
        assert_eq!(app.geotiffs.len(),2,"unrelated raster remains available as a source");
    }
    #[test]
    fn mixed_template_source_order_and_legacy_export_state_do_not_hide_virtual_trenches() {
        let mut app=app();
        let path=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters/gray16.tiff");
        let mut raster=sleufbase_rust::geotiff::read_geotiff_metadata(&path).unwrap();
        raster.kickthemap_job_id=Some(88);
        app.geotiffs.push(raster);
        app.template_state=Some(Default::default());
        app.open_template_export();
        assert!(app.template_order_dialog.is_none());
        let dlg=app.project_template_dialog.as_ref().unwrap();
        assert_eq!(dlg.options.background,BackgroundKind::Pdok);
        assert!(dlg.options.choices.iter().any(|c|matches!(c.source,Source::Virtual(0))));
        assert!(dlg.options.choices.iter().any(|c|matches!(c.source,Source::Raster(0))));
        app.project_template_dialog=None;
        app.datasets.push(Dataset{job_id:88,title:"KickTheMap PS88".into(),
            source_path:"PS88.json".into(),features:Vec::new()});
        app.open_template_export();
        let dlg=app.project_template_dialog.as_ref().unwrap();
        assert!(dlg.options.choices.iter().any(|c|matches!(c.source,Source::Job(0))));
        assert!(dlg.options.choices.iter().any(|c|matches!(c.source,Source::Virtual(0))));
        assert!(app.template_order_dialog.is_none());
        app.project_template_dialog=None;
        let previous=template_project::Options{raster_layout:true,
            choices:template_project::suggest_rasters(&app.geotiffs,&[]),..Default::default()};
        app.template_preferences=Some(sleufbase_rust::template_prefs::TemplatePreferences::capture(
            &previous,&[],&app.geotiffs,&[]));
        app.open_template_export();
        let dlg=app.project_template_dialog.as_ref().unwrap();
        assert!(dlg.options.choices.iter().any(|c|matches!(c.source,Source::Job(0))));
        assert!(dlg.options.choices.iter().any(|c|matches!(c.source,Source::Virtual(0))));
    }
    #[test]
    fn preflight_explains_invalid_output_and_busy_launch_keeps_the_reviewed_form(){
        let dir=tempfile::tempdir().unwrap();let mut app=app();app.open_project_template_dialog();
        let d=app.project_template_dialog.as_mut().unwrap();assert!(d.readiness().unwrap_err().contains("uitvoer-DXF"));
        d.output=Some(dir.path().join("wrong.txt"));assert!(d.readiness().unwrap_err().contains(".dxf"));
        d.output=Some(dir.path().join("correct.dxf"));assert!(d.readiness().is_ok());
        for (label,hint) in [("   ".to_owned(),"Vul een proefsleuflabel"),("é".repeat(129),"te lang"),("PS12\u{7}".to_owned(),"niet-ondersteunde tekens")] {
            d.options.choices[0].label=Some(label.clone());
            let error=d.readiness().unwrap_err();
            assert!(error.contains("Regel 01")&&error.contains(hint),"{error}");
            assert_eq!(d.options.choices[0].label.as_deref(),Some(label.as_str()));
        }
        d.options.choices[0].label=Some("PS12".into());assert!(d.readiness().is_ok());
        d.output=Some(dir.path().join("new.dxf"));d.options.project="Behouden keuze".into();app.live_busy=true;
        let ctx=egui::Context::default();
        let frame=|app:&mut SleufBaseApp,events|ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(680.,420.))),events,..Default::default()},|ctx|app.show_project_template_dialog(ctx));
        for _ in 0..4{frame(&mut app,vec![]);}
        let out=frame(&mut app,vec![]);let pos=out.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()=="Export starten"=>Some(t.pos+t.galley.rect.size()/2.),_=>None}).unwrap();assert!(pos.y<420.);
        for pressed in [true,false]{frame(&mut app,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
        let d=app.project_template_dialog.as_ref().expect("rejected launch must retain form");assert_eq!(d.options.project,"Behouden keuze");assert!(d.error.contains("downloads"));assert!(app.template_job.is_none());assert_eq!(app.virtual_trenches.len(),1);
    }
    #[test]
    fn invalid_export_text_keeps_the_form_sources_and_previous_project_preferences(){
        for field in ["opmerkingen","uitgesloten opmerkingen","projectnaam","tekenaar"] {
            let dir=tempfile::tempdir().unwrap();let output=dir.path().join("never.dxf");
            let mut app=app();
            app.virtual_trenches.push(VirtualTrench::new("PS13".into(),Vertex{x:155020.,y:463000.,z:Some(2.)},Vertex{x:155030.,y:463000.,z:Some(2.)},0.6).unwrap());
            app.open_project_template_dialog();
            let initial=&app.project_template_dialog.as_ref().unwrap().options;
            app.template_preferences=Some(sleufbase_rust::template_prefs::TemplatePreferences::capture(initial,&[],&[],&app.virtual_trenches));
            app.template_state=Some(Default::default());app.dirty=false;
            let saved=serde_json::to_value(&app.template_preferences).unwrap();
            let sources=serde_json::to_value(&app.virtual_trenches).unwrap();
            let allocation=app.virtual_trenches.as_ptr();
            let d=app.project_template_dialog.as_mut().unwrap();d.output=Some(output.clone());
            match field {
                "opmerkingen"=>d.options.choices[0].comments="x".repeat(4097),
                "uitgesloten opmerkingen"=>{d.options.choices[1].include=false;d.options.choices[1].comments="x".repeat(4097);},
                "projectnaam"=>d.options.project="x".repeat(2049),
                "tekenaar"=>d.options.drawn_by="x".repeat(2049),
                _=>unreachable!(),
            }
            let options=d.options.clone();
            app.start_project_template_export(options,output.clone());
            assert!(app.template_job.is_none(),"invalid {field} must not start a worker");
            assert!(app.project_template_dialog.is_some(),"the reviewed form must stay available");
            assert_eq!(serde_json::to_value(&app.template_preferences).unwrap(),saved);
            assert_eq!(serde_json::to_value(&app.virtual_trenches).unwrap(),sources);
            assert_eq!(app.virtual_trenches.as_ptr(),allocation);
            assert!(app.template_state.is_some());assert!(!app.dirty);assert!(!output.exists());
            let message=app.message.to_lowercase();
            assert!(message.contains(field.strip_prefix("uitgesloten ").unwrap_or(field)),"{field}: {}",app.message);
            if field.contains("opmerkingen"){assert!(app.message.contains(if field.starts_with("uitgesloten"){"PS13"}else{"PS12"}));}
            // The failed attempt must not poison the next ordinary project save.
            sleufbase_rust::project::save_project_new(&dir.path().join("project.json"),&app.snapshot()).unwrap();
            let draft=&app.project_template_dialog.as_ref().unwrap().options;
            match field {
                "opmerkingen"=>assert_eq!(draft.choices[0].comments.len(),4097),
                "uitgesloten opmerkingen"=>assert_eq!(draft.choices[1].comments.len(),4097),
                "projectnaam"=>assert_eq!(draft.project.len(),2049),
                "tekenaar"=>assert_eq!(draft.drawn_by.len(),2049),
                _=>unreachable!(),
            }
        }
    }
    #[test]
    fn file_availability_is_checked_when_starting_and_keeps_the_reviewed_form(){
        let dir=tempfile::tempdir().unwrap();let mut app=app();app.open_project_template_dialog();
        let output=dir.path().join("existing.dxf");
        let d=app.project_template_dialog.as_mut().unwrap();
        d.output=Some(output.clone());d.options.template=dir.path().join("missing-template.dxf");
        assert!(d.readiness().is_ok(),"drawing the form must not probe the template path");
        let options=d.options.clone();let allocation=app.virtual_trenches.as_ptr();
        app.start_project_template_export(options,output.clone());
        assert!(app.template_job.is_none());assert!(app.project_template_dialog.is_some());
        assert!(app.message.contains("eigen DXF-sjabloon ontbreekt"));
        let d=app.project_template_dialog.as_mut().unwrap();d.options.template=PathBuf::new();
        std::fs::write(&output,b"keep existing drawing").unwrap();
        assert!(d.readiness().is_ok(),"output availability is checked only on the export action");
        let options=d.options.clone();app.start_project_template_export(options,output.clone());
        assert!(app.template_job.is_none());assert!(app.project_template_dialog.is_some());
        assert!(app.message.contains("bestaat al"));assert_eq!(std::fs::read(output).unwrap(),b"keep existing drawing");
        assert_eq!(app.virtual_trenches.as_ptr(),allocation);assert!(app.template_preferences.is_none());
    }
    #[test]
    fn bulk_export_actions_preserve_excluded_details_and_empty_positions(){
        let mut app=app();
        app.virtual_trenches.push(VirtualTrench::new("PS13".into(),Vertex{x:155020.,y:463000.,z:Some(2.)},Vertex{x:155030.,y:463000.,z:Some(2.)},0.6).unwrap());
        app.open_project_template_dialog();let d=app.project_template_dialog.as_mut().unwrap();
        d.options.choices[1].include=false;
        d.options.choices[1].reverse=true;
        d.options.choices[1].manual_scale=Some(10_000);
        d.options.choices[1].comments="Bewaren".into();
        let mut empty=d.options.choices[0].clone();empty.source=Source::Empty(7);empty.include=true;empty.profile=false;
        d.options.choices.insert(1,empty);
        d.editor=sleufbase_rust::template_editor::Editor::new(&mut d.options.choices,&[],&app.virtual_trenches);
        d.profiles_for_included(false);
        assert!(!d.options.choices[0].profile);
        assert!(d.options.choices[2].profile,"excluded source keeps its own profile choice");
        assert_eq!(export_summary(&d.options),ExportSummary{trenches:1,profiles:0,empty:1});
        d.include_all(false);
        assert!(d.options.choices[1].include,"empty layout position must not be removed");
        assert!(d.readiness().unwrap_err().contains("Neem minimaal één proefsleuf"));
        d.include_all(true);
        assert_eq!(export_summary(&d.options),ExportSummary{trenches:2,profiles:1,empty:1});
        assert_eq!(d.options.choices.iter().map(|choice|choice.source).collect::<Vec<_>>(),[Source::Virtual(0),Source::Empty(7),Source::Virtual(1)]);
        assert!(d.options.choices[2].reverse);
        assert_eq!(d.options.choices[2].manual_scale,Some(10_000));
        assert_eq!(d.options.choices[2].comments,"Bewaren");
    }
    #[test]
    fn export_placement_preview_tracks_mixed_sources_empty_slots_and_auto_labels(){
        use ExportPlacement::{Empty,Excluded,Trench};
        let mut app=app();app.open_project_template_dialog();
        let d=app.project_template_dialog.as_mut().unwrap();
        let original=d.options.choices[0].clone();
        d.options.choices=[
            (Source::Job(0),false,"PS12AUTO"),
            (Source::Virtual(0),true,"PS12AUTO"),
            (Source::Empty(7),true,""),
            (Source::Raster(0),true,"PS12AUTO"),
            (Source::Empty(8),false,""),
            (Source::Virtual(1),true,"Werkvak west"),
        ].into_iter().map(|(source,include,label)| {
            let mut choice=original.clone();choice.source=source;choice.include=include;
            choice.label=Some(label.into());choice
        }).collect();
        d.editor=sleufbase_rust::template_editor::Editor::new(&mut d.options.choices,&[],&[]);
        let before=format!("{:?}",d.options.choices);
        assert_eq!(d.placements(),vec![Excluded,
            Trench{position:1,label:Some("PS12B".into())},Empty(2),
            Trench{position:3,label:Some("PS12C".into())},Excluded,
            Trench{position:4,label:Some("Werkvak west".into())}]);
        assert_eq!(format!("{:?}",d.options.choices),before,"preview must not replace stored AUTO labels or source details");
        d.include_all(true);
        d.editor.select(&d.options.choices,3,false,false);
        d.editor.move_selected(&mut d.options.choices,-1);
        d.editor.move_selected(&mut d.options.choices,-1);
        assert_eq!(d.options.choices.iter().map(|choice|choice.source).collect::<Vec<_>>(),
            [Source::Job(0),Source::Raster(0),Source::Virtual(0),Source::Empty(7),Source::Empty(8),Source::Virtual(1)]);
        assert_eq!(d.placements(),vec![Trench{position:1,label:Some("PS12B".into())},
            Trench{position:2,label:Some("PS12C".into())},Trench{position:3,label:Some("PS12D".into())},
            Empty(4),Excluded,Trench{position:5,label:Some("Werkvak west".into())}]);
        d.options.choices[0].include=false;d.options.choices[0].label=Some("".into());
        d.options.choices[2].label=Some("".into());
        assert_eq!(d.placements(),vec![Excluded,Trench{position:1,label:Some("PS12B".into())},
            Trench{position:2,label:None},Empty(3),Excluded,Trench{position:4,label:Some("Werkvak west".into())}]);
        d.output=Some("preview.dxf".into());
        assert!(d.readiness().unwrap_err().starts_with("Regel 03: Vul een proefsleuflabel"),
            "validation must identify the editable row, not its compacted export position");
    }
    #[test]
    fn export_placement_updates_in_the_same_frame_as_inclusion_without_changing_edit_selection(){
        let mut app=app();
        app.virtual_trenches.push(VirtualTrench::new("PS13".into(),Vertex{x:155020.,y:463000.,z:Some(2.)},Vertex{x:155030.,y:463000.,z:Some(2.)},0.6).unwrap());
        app.open_project_template_dialog();
        let d=app.project_template_dialog.as_mut().unwrap();
        for choice in &mut d.options.choices{choice.label=Some("PS12AUTO".into());}
        let selected=d.editor.selected.clone();
        let sources=d.options.choices.iter().map(|choice|choice.source).collect::<Vec<_>>();
        let ctx=egui::Context::default();
        let frame=|app:&mut SleufBaseApp,events|ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1500.,930.))),events,..Default::default()},|ctx|app.show_project_template_dialog(ctx));
        for _ in 0..4{frame(&mut app,vec![]);}
        let out=frame(&mut app,vec![]);
        let pos=out.shapes.iter().find_map(|shape|match &shape.shape{
            egui::Shape::Text(text) if text.galley.text()=="Meenemen"=>Some(text.pos+text.galley.rect.size()/2.),_=>None,
        }).expect("the first inclusion checkbox remains visible");
        frame(&mut app,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
        let out=frame(&mut app,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
        let labels=out.shapes.iter().filter_map(|shape|match &shape.shape{
            egui::Shape::Text(text)=>Some(text.galley.text().to_owned()),_=>None,
        }).collect::<Vec<_>>();
        assert!(labels.iter().any(|text|text=="Niet in export"));
        assert!(labels.iter().any(|text|text=="Exportvak 1 · PS12B"));
        assert!(!labels.iter().any(|text|text=="Exportvak 2 · PS12C"));
        let d=app.project_template_dialog.as_ref().unwrap();
        assert!(!d.options.choices[0].include);assert!(d.options.choices[1].include);
        assert_eq!(d.editor.selected,selected,"Meenemen must not change the orange edit selection");
        assert_eq!(d.options.choices.iter().map(|choice|choice.source).collect::<Vec<_>>(),sources);
        assert!(d.options.choices.iter().all(|choice|choice.label.as_deref()==Some("PS12AUTO")));
    }
    #[test]
    fn preflight_checks_only_included_sources_and_enabled_virtual_profiles(){
        let mut app=app();app.virtual_trenches[0].start.z=None;app.virtual_trenches[0].end.z=None;
        app.open_project_template_dialog();let mut options=app.project_template_dialog.as_ref().unwrap().options.clone();
        options.choices[0].profile=false;
        let mut excluded=options.choices[0].clone();excluded.source=Source::Virtual(99);excluded.include=false;
        excluded.photos.push(sleufbase_rust::template_photo::Photo{path:"missing-excluded-photo.png".into(),turns:0,area:Default::default()});
        options.choices.push(excluded);
        assert!(validate_export_sources(&options,&[],&[],&app.virtual_trenches).is_ok());
        options.choices[0].profile=true;
        let error=validate_export_sources(&options,&[],&[],&app.virtual_trenches).unwrap_err();
        assert!(error.contains("PS12")&&error.contains("begin- en eindhoogte"));
        options.choices[0].profile=false;
        options.choices[0].photos.push(sleufbase_rust::template_photo::Photo{path:"missing-included-photo.png".into(),turns:0,area:Default::default()});
        let allocation=app.virtual_trenches.as_ptr();let dir=tempfile::tempdir().unwrap();let output=dir.path().join("invalid.dxf");
        app.start_project_template_export(options,output.clone());
        assert!(app.template_job.is_none());assert!(app.project_template_dialog.is_some());
        assert_eq!(app.virtual_trenches.as_ptr(),allocation);
        assert!(app.message.contains("PS12")&&app.message.contains("Foto ontbreekt"));
        assert!(!output.exists());assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),0);
    }
    #[test]
    fn preflight_reports_missing_rasters_and_optional_logos_before_work_starts(){
        let mut app=app();app.open_project_template_dialog();let mut options=app.project_template_dialog.as_ref().unwrap().options.clone();
        let path=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters/gray16.tiff");
        let mut raster=sleufbase_rust::geotiff::read_geotiff_metadata(&path).unwrap();
        options.choices[0].raster=Some(0);
        assert!(validate_export_sources(&options,&[],&[raster.clone()],&app.virtual_trenches).is_ok());
        raster.path="missing-linked-source.tiff".into();
        let error=validate_export_sources(&options,&[],&[raster],&app.virtual_trenches).unwrap_err();
        assert!(error.contains("PS12")&&error.contains("GeoTIFF ontbreekt"));
        options.choices[0].raster=None;
        options.logos.client=Some("missing-client-logo.png".into());
        assert!(validate_export_sources(&options,&[],&[],&app.virtual_trenches).unwrap_err().contains("Opdrachtgeverlogo"));
        options.logos.client=None;
        assert!(validate_export_sources(&options,&[],&[],&app.virtual_trenches).is_ok());
    }
    #[test]
    fn opening_export_controls_preserves_valid_custom_dpi_scale_and_online_preference(){
        let mut app=app();app.open_project_template_dialog();
        let d=app.project_template_dialog.as_mut().unwrap();d.options.dpi=600;d.options.scale=225.5;d.options.choices[0].manual_scale=Some(10_000);
        d.options.server_mode=sleufbase_rust::template_server::Mode::Offline;
        assert_eq!(effective_map_label(&d.options),"Offline · witte kaart, zonder luchtfoto");
        assert_eq!(d.options.background,BackgroundKind::Pdok);
        let ctx=egui::Context::default();
        let frame=|app:&mut SleufBaseApp,events|ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1500.,930.))),events,..Default::default()},|ctx|app.show_project_template_dialog(ctx));
        for _ in 0..4{frame(&mut app,vec![]);}
        let out=frame(&mut app,vec![]);
        assert!(out.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text().contains("kaart 1:225.5"))),"summary must not round a fractional map scale");
        let pos=out.shapes.iter().find_map(|shape|match &shape.shape{egui::Shape::Text(text)if text.galley.text()=="Meer exportinstellingen"=>Some(text.pos+text.galley.rect.size()/2.),_=>None}).expect("advanced options remain accessible");
        for pressed in [true,false]{frame(&mut app,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
        for _ in 0..4{frame(&mut app,vec![]);}
        let d=app.project_template_dialog.as_mut().unwrap();
        assert_eq!(d.options.dpi,600);assert_eq!(d.options.scale,225.5);assert_eq!(d.options.choices[0].manual_scale,Some(10_000));
        assert_eq!(d.options.background,BackgroundKind::Pdok);
        d.options.server_mode=sleufbase_rust::template_server::Mode::Fresh;
        assert_eq!(effective_map_label(&d.options),"Online · PDOK-luchtfoto");
        for wanted in [1,10_000,1_000_000]{
            let mut manual=Some(wanted);
            for _ in 0..3{let _=ctx.run(egui::RawInput::default(),|ctx|{egui::CentralPanel::default().show(ctx,|ui|template_profile_ui::scale(ui,&mut manual));});}
            assert_eq!(manual,Some(wanted),"opening scale control must not clamp a valid saved scale");
        }
    }

    #[test]
    fn long_export_error_keeps_the_primary_action_visible_on_a_small_window(){
        let mut app=app();app.open_project_template_dialog();let dir=tempfile::tempdir().unwrap();
        let d=app.project_template_dialog.as_mut().unwrap();d.output=Some(dir.path().join("new.dxf"));
        d.error=format!("PS12: Foto ontbreekt: C:/{}werkfoto.png", "een-lange-projectmap/".repeat(100));
        let ctx=egui::Context::default();let mut out=None;
        for _ in 0..4{out=Some(ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(680.,420.))),..Default::default()},|ctx|app.show_project_template_dialog(ctx)));}
        let out=out.unwrap();let action=out.shapes.iter().find_map(|shape|match &shape.shape{
            egui::Shape::Text(text)if text.galley.text()=="Export starten"=>Some((shape.clip_rect,egui::Rect::from_min_size(text.pos,text.galley.rect.size()))),_=>None,
        }).expect("export action must remain drawn");
        assert!(action.1.min.y>=0.&&action.1.max.y<=420.,"action is outside screen: {:?}",action.1);
        assert!(action.0.contains_rect(action.1),"action must not be clipped: {action:?}");
        assert!(app.project_template_dialog.as_ref().unwrap().error.contains(&"een-lange-projectmap/".repeat(100)),"full diagnostic must remain available for hover");
        assert!(app.template_job.is_none());
    }

}
