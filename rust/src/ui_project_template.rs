//! End-user GUI for building template exports directly from project sources.
use super::*;
use sleufbase_rust::template_project::{self, BackgroundKind, Options, Source};
use sleufbase_rust::template_order::{self, SortMode};

fn configure_options(mut options:Options,settings:&MigratedSettings,saved:Option<&sleufbase_rust::template_prefs::TemplatePreferences>,jobs:&[Dataset],rasters:&[GeoTiffInfo],trenches:&[VirtualTrench])->Options{
    let preferences=&settings.export_preferences;
    options.overview_settings=sleufbase_rust::cadastral_overview::Settings::from_settings(settings).expect("validated project export settings");
    options.template=preferences.cadastral_template_path.clone().into();
    options.profile_style=sleufbase_rust::settings_compat::ProfilePreferences::from_settings(settings);
    options.logos=crate::ui_preferences::logos(preferences);
    // template_reverse_patch.py:412 always includes both variants, starting
    // normally. The archived legacy reverse-sheet flag must not flip the base.
    for choice in &mut options.choices{choice.profile=preferences.template_include_cross_sections;}
    if let Some(saved)=saved{
        let mut restored=saved.restore(jobs,rasters,trenches);
        if saved.legacy_layout_only{
            for choice in &mut restored.choices{choice.profile=preferences.template_include_cross_sections;}
            options.choices=restored.choices;options.raster_layout=restored.raster_layout;
            options.background=restored.background;
            return options;
        }
        return restored;
    }
    options.include_bgt=true;
    options
}

pub(super) struct Dialog {
    pub options: Options,
    pub output: Option<PathBuf>,
    pub error: String,
    pub automatic: Vec<sleufbase_rust::template_match::Suggestion>,
    pub editor:sleufbase_rust::template_editor::Editor,
    sort_mode: Option<SortMode>,
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
        // Saved project choices override the user's general export defaults.
        let mut options=configure_options(options,settings,app.template_preferences.as_ref(),&app.datasets,&app.geotiffs,&app.virtual_trenches);
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
        Self { options, output: None, error: String::new(), automatic, editor,
            sort_mode: fresh.then_some(SortMode::NumberAsc),
            jobs:app.datasets.iter().map(|d|(d.job_id,d.source_path.clone())).collect(),
            rasters:app.geotiffs.iter().map(|r|(r.path.clone(),r.kickthemap_job_id,r.metadata.get("_sleufbase_python_tiff_index").and_then(serde_json::Value::as_u64))).collect(),
            trenches:app.virtual_trenches.iter().map(|t|(t.job_id,t.label.clone())).collect(),
        }
    }
    fn readiness(&self)->Result<(),String>{
        if !self.options.template.as_os_str().is_empty()&&!self.options.template.is_file(){return Err("Het eigen DXF-sjabloon ontbreekt. Kies het opnieuw of gebruik het ingebouwde sjabloon.".into());}
        let output=self.output.as_ref().ok_or("Kies eerst een nieuwe uitvoer-DXF.")?;
        if output.extension().is_none_or(|ext|!ext.to_string_lossy().eq_ignore_ascii_case("dxf")){return Err("De uitvoernaam moet eindigen op .dxf.".into());}
        if output.exists(){return Err("Deze uitvoer bestaat al. Kies een nieuwe naam; bestaande bestanden blijven behouden.".into());}
        let selected=self.options.choices.iter().filter(|c|c.include).collect::<Vec<_>>();
        if !selected.iter().any(|c|!matches!(c.source,Source::Empty(_))){return Err("Neem minimaal één proefsleuf mee.".into());}
        if selected.len()>template_project::MAX_SLOTS{return Err("Er zijn te veel exportvakken. Verminder de selectie.".into());}
        if !self.editor.preview(&self.options.choices).1.is_empty(){return Err("Controleer de PS-nummers van de geselecteerde vakken.".into());}
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
        let rasters = &self.geotiffs;
        let raster_previews = &self.raster_textures;
        if let Some(dialog) = &mut self.project_template_dialog {
            let mut clicked_row=None;let mut sync_editor=false;
            egui::Window::new("DXF-sjabloon exporteren")
                .open(&mut open)
                .default_width(730.)
                .default_height((ctx.screen_rect().height()-100.).clamp(240.,650.))
                .constrain_to(ctx.screen_rect().shrink(12.))
                .resizable(true)
                .show(ctx, |ui| {
                    settings_ui::control_style(ui);
                    ui.style_mut().wrap_mode=Some(egui::TextWrapMode::Wrap);
                    egui::ScrollArea::vertical().id_salt("project-template-body")
                        .max_height((ui.available_height()-105.).max(80.)).auto_shrink([false,false]).show(ui, |ui| {
                        ui.heading("1. Uitvoer en project");
                        if dialog.options.template.as_os_str().is_empty(){ui.label("Sjabloon: ingebouwd SleufBase-sjabloon");}
                        else{ui.label(format!("Eigen sjabloon: {}",dialog.options.template.display()));}
                        ui.collapsing("Eigen DXF-sjabloon gebruiken (optioneel)",|ui|{
                            ui.small("Voor de standaardexport is geen bestand nodig.");
                            if ui.button("Eigen DXF-sjabloon kiezen…").clicked(){
                                if let Some(path)=rfd::FileDialog::new().add_filter("AutoCAD DXF",&["dxf"]).pick_file(){dialog.options.template=path;}
                            }
                            if !dialog.options.template.as_os_str().is_empty()&&ui.button("Ingebouwd sjabloon gebruiken").clicked(){dialog.options.template=PathBuf::new();}
                        });
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Nieuwe uitvoer-DXF kiezen…").clicked() {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("AutoCAD DXF", &["dxf"])
                                    .set_file_name("SleufBase-Project-Sjabloon-Rust.dxf")
                                    .save_file() { dialog.output = Some(path); }
                            }
                            if let Some(path) = &dialog.output {
                                ui.label(path.display().to_string());
                            }
                        });
                        let field_width=(ui.available_width()-110.).max(80.);
                        egui::Grid::new("project-template-identity").num_columns(2).min_col_width(90.).show(ui,|ui|{
                            ui.label("Projectnaam");ui.add(egui::TextEdit::singleline(&mut dialog.options.project).desired_width(field_width));ui.end_row();
                            ui.label("Tekenaar");ui.add(egui::TextEdit::singleline(&mut dialog.options.drawn_by).desired_width(field_width));ui.end_row();
                        });
                        ui.separator();
                        ui.heading("2. Kaarten en profielen");
                        crate::template_dialog::font_folders(ui, &mut dialog.options.font_directories);
                        crate::template_dialog::logos(ui,&mut dialog.options.logos);
                        crate::template_dialog::server_mode(ui,&mut dialog.options.server_mode);
                        ui.checkbox(&mut dialog.options.paired,"Beide richtingen (Normaal / Reverse)");
                        if dialog.options.paired {ui.small("Kies na export in AutoCAD de gewenste richting met Versie.");}
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Resolutie (DPI)");
                            ui.add(egui::DragValue::new(&mut dialog.options.dpi).range(72..=300));
                            ui.label("Kaartschaal 1:");
                            ui.add(egui::DragValue::new(&mut dialog.options.scale).range(1.0..=100_000.).speed(1.));
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.radio_value(&mut dialog.options.background, BackgroundKind::Blank,
                                "Geen luchtfoto (witte kaart)");
                            ui.add_enabled_ui(dialog.options.server_mode==sleufbase_rust::template_server::Mode::Fresh,|ui|{ui.radio_value(&mut dialog.options.background, BackgroundKind::Pdok,
                                "PDOK-luchtfoto (netwerk)");});
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.checkbox(&mut dialog.options.include_bgt, "BGT-gegevens meenemen");
                            ui.checkbox(&mut dialog.options.include_cadastral,
                                "Kadaster-gegevens meenemen");
                            ui.checkbox(&mut dialog.options.include_cad,
                                "Zichtbare CAD/DXF-lagen (ook geneste blokken)");
                        });
                        ui.label("Jobs gebruiken de volledige GeoTIFF. Virtuele/MarXact-sleuven kunnen zonder externe TIFF hun eigen transparante raster met meetpunten en grens gebruiken.");
                        ui.separator();
                        ui.heading("3. Proefsleufvakken en GeoTIFF-koppelingen");
                        let included=dialog.options.choices.iter().filter(|c|c.include&&!matches!(c.source,Source::Empty(_))).count();
                        let empty=dialog.options.choices.iter().filter(|c|matches!(c.source,Source::Empty(_))).count();
                        ui.label(format!("Meenemen: {included} | Niet meenemen: {} | Lege vakken: {empty}",dialog.options.choices.len()-included-empty));
                        if included+empty>template_project::MAX_SLOTS {ui.colored_label(DARK,"Te veel proefsleufvakken voor één export");}
                        ui.small("Standaard worden proefsleuven numeriek gesorteerd (PS1, PS2, PS10). Handmatig verplaatsen blijft mogelijk; lege vakken houden hun positie.");
                        let mut requested_sort=dialog.sort_mode;
                        ui.horizontal_wrapped(|ui|{
                            ui.label("Sorteren op");
                            egui::ComboBox::from_id_salt("project-template-sort")
                                .selected_text(requested_sort.map_or("Handmatig / opgeslagen",SortMode::label))
                                .show_ui(ui,|ui|{
                                    for mode in SortMode::ALL {
                                        ui.selectable_value(&mut requested_sort,Some(mode),mode.label());
                                    }
                                });
                            if ui.small_button("Opnieuw sorteren").clicked(){
                                // After editing a PS-label, apply the current sort again.
                                let mode=requested_sort.unwrap_or(SortMode::NumberAsc);
                                requested_sort=Some(mode);
                                dialog.sort_mode=Some(mode);
                                let order=dialog.editor.sort_choices(&mut dialog.options.choices,mode);
                                template_order::reorder(&mut dialog.automatic,&order);
                            }
                        });
                        if requested_sort!=dialog.sort_mode {
                            dialog.sort_mode=requested_sort;
                            if let Some(mode)=requested_sort {
                                let order=dialog.editor.sort_choices(&mut dialog.options.choices,mode);
                                template_order::reorder(&mut dialog.automatic,&order);
                            }
                        }
                        ui.horizontal_wrapped(|ui|{
                            if ui.small_button("Alle regels selecteren").clicked(){dialog.editor.select_all(&dialog.options.choices,true);}
                            if ui.small_button("Selectie wissen").clicked(){dialog.editor.select_all(&dialog.options.choices,false);}
                            if ui.small_button("Meenemen").clicked(){dialog.editor.included(&mut dialog.options.choices,true);}
                            if ui.small_button("Niet meenemen").clicked(){dialog.editor.included(&mut dialog.options.choices,false);}
                            for(label,direction)in[("Omhoog",-1),("Omlaag",1)]{if ui.small_button(label).clicked(){let swaps=dialog.editor.move_selected(&mut dialog.options.choices,direction);if !swaps.is_empty(){dialog.sort_mode=None;}for(a,b)in swaps{dialog.automatic.swap(a,b);}}}
                            if ui.small_button("Leeg vak invoegen").clicked(){match dialog.editor.insert_empty(&mut dialog.options.choices){Ok(at)=>dialog.automatic.insert(at,sleufbase_rust::template_match::Suggestion{raster:None,reason:sleufbase_rust::template_match::MatchKind::Missing}),Err(e)=>dialog.error=e}}
                            if ui.small_button("Lege vakken verwijderen").clicked(){for i in dialog.editor.remove_empty(&mut dialog.options.choices){dialog.automatic.remove(i);}}
                        });
                        ui.horizontal_wrapped(|ui|{
                            ui.label("PS-nummer");ui.add(egui::TextEdit::singleline(&mut dialog.editor.number).desired_width(75.));
                            ui.label("Variant");egui::ComboBox::from_id_salt("mixed-template-variant").selected_text(if dialog.editor.variant.is_empty(){"Geen variant"}else{&dialog.editor.variant}).show_ui(ui,|ui|{ui.selectable_value(&mut dialog.editor.variant,String::new(),"Geen variant");ui.selectable_value(&mut dialog.editor.variant,"AUTO".into(),"Automatisch (B/C/D...)");for c in 'B'..='Z'{ui.selectable_value(&mut dialog.editor.variant,c.to_string(),format!("Variant {c}"));}});
                            if ui.small_button("Nummer/variant toepassen").clicked(){match dialog.editor.apply_label(&mut dialog.options.choices){Ok(())=>{dialog.error.clear();if let Some(mode)=dialog.sort_mode{let order=dialog.editor.sort_choices(&mut dialog.options.choices,mode);template_order::reorder(&mut dialog.automatic,&order);}},Err(e)=>dialog.error=e}}
                            ui.label("Profielschaal");ui.add(egui::TextEdit::singleline(&mut dialog.editor.scale).desired_width(75.));
                            if ui.small_button("Schaal toepassen").clicked(){dialog.error=dialog.editor.apply_scale(&mut dialog.options.choices).err().unwrap_or_default();}
                        });
                        let(preview_labels,invalid_labels)=dialog.editor.preview(&dialog.options.choices);
                        if !invalid_labels.is_empty(){ui.colored_label(DARK,"Vul geldige PS-nummers in voor de geselecteerde exportvakken.");}
                        for (idx, slot) in dialog.options.choices.iter_mut().enumerate() {
                            ui.push_id(idx, |ui| {
                                ui.group(|ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        if ui.selectable_label(dialog.editor.selected[idx],format!("{:02}",idx+1)).clicked(){let modifiers=ui.input(|i|i.modifiers);clicked_row=Some((idx,modifiers.ctrl||modifiers.command,modifiers.shift));}
                                        if !matches!(slot.source,Source::Empty(_)){if ui.checkbox(&mut slot.include,"Meenemen").changed(){sync_editor=true;}}
                                        ui.strong(match slot.source {
                                            Source::Job(i) => format!("KickTheMap-job {}", i + 1),
                                            Source::Virtual(i) => format!("Virtuele sleuf {}", i + 1),
                                            Source::Raster(i)=>format!("GeoTIFF {}",i+1),
                                            Source::Empty(_)=>"Leeg vak".into(),
                                        });
                                        ui.text_edit_singleline(&mut slot.title);
                                    });
                                    if matches!(slot.source,Source::Empty(_)){ui.small("Dit vak blijft leeg op het blad.");return;}
                                    ui.label(format!("Exportlabel: {}",preview_labels[idx].as_deref().unwrap_or("niet meenemen")));

                                    ui.horizontal_wrapped(|ui| {
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
                                            .selected_text(selected_name)
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
                                            });
                                        ui.checkbox(&mut slot.profile, "Profiel tekenen");
                                        ui.add_enabled(!dialog.options.paired,egui::Checkbox::new(&mut slot.reverse,"Alleen omgekeerde richting"));
                                    });
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
                                            ui.horizontal_wrapped(|ui| {
                                                if let Some(Some(preview))=raster_previews.get(raster_index) {
                                                    ui.add(egui::Image::new((preview.id(),egui::vec2(190.,120.))));
                                                }
                                                ui.vertical(|ui| {
                                                    ui.small(format!("RD X {:.1}–{:.1}",info.bounds.min_x,info.bounds.max_x));
                                                    ui.small(format!("RD Y {:.1}–{:.1}",info.bounds.min_y,info.bounds.max_y));
                                                    ui.small(format!("CRS: {}",info.epsg.map_or("onbekend".into(),|id|format!("EPSG:{id}"))));
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
                                        let mut remove=None;
                                        for (photo_index,photo) in slot.photos.iter_mut().enumerate() {
                                            ui.push_id(photo_index,|ui|{
                                                ui.horizontal(|ui|{
                                                    ui.label(photo.path.file_name()
                                                        .map(|s|s.to_string_lossy().to_string())
                                                        .unwrap_or_else(||"Werkfoto".into()));
                                                    egui::ComboBox::from_id_salt("rotate")
                                                        .selected_text(format!("{}°",u32::from(photo.turns)*90))
                                                        .show_ui(ui,|ui|{
                                                            for turn in 0..4 {
                                                                ui.selectable_value(&mut photo.turns,turn,format!("{}°",u32::from(turn)*90));
                                                            }
                                                        });
                                                    egui::ComboBox::from_id_salt("area")
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
                                                if !photo.path.is_file(){ui.colored_label(DARK,"Fotobestand ontbreekt");}
                                            });
                                        }
                                        if let Some(index)=remove {slot.photos.remove(index);}
                                    });
                                    ui.collapsing("Opmerkingen", |ui| {
                                        ui.text_edit_multiline(&mut slot.comments);
                                    });
                                });
                            });
                        }
                    });
                        ui.separator();
                        if !dialog.error.is_empty() {
                            ui.colored_label(DARK, &dialog.error);
                        }
                        ui.small("De oorspronkelijke jobs, virtuele sleuven en projectbestanden blijven behouden. Een bestaande uitvoer wordt niet overschreven.");
                        let readiness=dialog.readiness();
                        let ready=readiness.is_ok();
                        if let Err(reason)=readiness{ui.small(reason);}
                        if ui.add_enabled(ready, egui::Button::new(egui::RichText::new("Volledige project-sjabloonexport starten").color(Color32::WHITE)).fill(ORANGE)).clicked() {
                            launch = dialog.output.clone();
                        }
                });
            if let Some((index,control,shift))=clicked_row{dialog.editor.select(&dialog.options.choices,index,control,shift);}else if sync_editor{dialog.editor.sync(&dialog.options.choices);}

        }
        if !open { self.project_template_dialog = None; }
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
    fn project_dialog_restores_saved_choices_ahead_of_general_defaults(){
        let jobs=jobs();let settings=settings();
        let mut saved_options=Options{template:"project.dxf".into(),choices:template_project::suggest(&jobs,&[],&[]),..Default::default()};
        saved_options.profile_style.marker_diameter=0.03;
        saved_options.logos.client=Some("project-logo.png".into());saved_options.include_bgt=false;
        let saved=sleufbase_rust::template_prefs::TemplatePreferences::capture(&saved_options,&jobs,&[],&[]);
        let configured=configure_options(Options::default(),&settings,Some(&saved),&jobs,&[],&[]);
        assert_eq!(configured.template,PathBuf::from("project.dxf"));
        assert_eq!(configured.logos.client,Some(PathBuf::from("project-logo.png")));
        assert_eq!(configured.profile_style.marker_diameter,0.03);assert!(!configured.include_bgt);
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
    fn ordinary_export_includes_virtual_trenches_alongside_loaded_tiffs(){
        let mut app=app();let path=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters/gray16.tiff");
        app.geotiffs.push(sleufbase_rust::geotiff::read_geotiff_metadata(&path).unwrap());app.open_template_export();
        let d=app.project_template_dialog.as_ref().expect("mixed project dialog");assert!(app.template_order_dialog.is_none());
        assert!(d.options.choices.iter().any(|c|matches!(c.source,Source::Virtual(0))));assert!(d.options.choices.iter().any(|c|matches!(c.source,Source::Raster(0))));
    }
    #[test]
    fn preflight_explains_invalid_output_and_busy_launch_keeps_the_reviewed_form(){
        let dir=tempfile::tempdir().unwrap();let mut app=app();app.open_project_template_dialog();
        let d=app.project_template_dialog.as_mut().unwrap();assert!(d.readiness().unwrap_err().contains("uitvoer-DXF"));
        d.output=Some(dir.path().join("wrong.txt"));assert!(d.readiness().unwrap_err().contains(".dxf"));
        d.output=Some(dir.path().join("correct.dxf"));assert!(d.readiness().is_ok());
        std::fs::write(d.output.as_ref().unwrap(),b"existing drawing").unwrap();assert!(d.readiness().unwrap_err().contains("bestaat al"));
        d.output=Some(dir.path().join("new.dxf"));d.options.project="Behouden keuze".into();app.live_busy=true;
        let ctx=egui::Context::default();
        let frame=|app:&mut SleufBaseApp,events|ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(680.,420.))),events,..Default::default()},|ctx|app.show_project_template_dialog(ctx));
        for _ in 0..4{frame(&mut app,vec![]);}
        let out=frame(&mut app,vec![]);let pos=out.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()=="Volledige project-sjabloonexport starten"=>Some(t.pos+t.galley.rect.size()/2.),_=>None}).unwrap();assert!(pos.y<420.);
        for pressed in [true,false]{frame(&mut app,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
        let d=app.project_template_dialog.as_ref().expect("rejected launch must retain form");assert_eq!(d.options.project,"Behouden keuze");assert!(d.error.contains("downloads"));assert!(app.template_job.is_none());assert_eq!(app.virtual_trenches.len(),1);
    }
}
