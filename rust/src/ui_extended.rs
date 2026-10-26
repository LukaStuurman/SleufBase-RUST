//! Secondary native tools, retained outside the original main surface.
use super::*;
impl SleufBaseApp {
    pub(super) fn show_extended_tools(&mut self, ctx: &egui::Context) {
        if !self.shell.extended {
            return;
        }
        let mut open = true;
        egui::Window::new("Aanvullende hulpmiddelen")
            .open(&mut open)
            .default_width(900.)
            .vscroll(true)
            .show(ctx, |ui| {
                self.show_extended_toolbar(ui, ctx);
                self.show_extended_sources(ui);
            });
        self.shell.extended = open;
    }
    fn show_extended_toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(DARK, egui::RichText::new("TECHBASE").strong().size(19.0));
            ui.separator();
            ui.label(egui::RichText::new("SleufBase Rust").strong().size(18.0));
            ui.label(egui::RichText::new("Migratie-preview").color(Color32::GRAY));
            if self.dirty {
                ui.colored_label(DARK, "● Niet opgeslagen");
            }
            ui.separator();
            if ui.button("Nieuw project").clicked() {
                self.request_project_action(ProjectAction::New, ctx);
            }
            if ui.button("Open Rust-project").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Rust-project JSON", &["json"])
                    .pick_file()
                {
                    self.request_project_action(ProjectAction::Open(path), ctx);
                }
            }
            if ui.button("Opslaan").clicked() {
                self.save_project_file(false);
            }
            if ui.button("Opslaan als...").clicked() {
                self.save_project_file(true);
            }
            if ui.button("Instellingen").clicked() {
                self.open_settings();
            }
            if ui.button("Import oude voorkeuren").clicked() {
                self.import_old_settings();
            }
            if ui.button("Herstel back-up").clicked() {
                self.restore_backup(ctx);
            }
            if ui.button("Import Python-project").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Python-project", &["klicproject", "json"])
                    .pick_file()
                {
                    self.request_project_action(ProjectAction::ImportPython(path), ctx);
                }
            }
            if ui.button("Export Python-project").clicked() {
                self.choose_python_project_export();
            }
        });
        self.show_project_view_controls(ui);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Open jobFeatures.json").clicked() {
                if let Some(files) = rfd::FileDialog::new()
                    .add_filter("KickTheMap JSON", &["json"])
                    .pick_files()
                {
                    self.load_files(files);
                }
            }
            if ui.button("Open GeoTIFF").clicked() {
                if let Some(files) = rfd::FileDialog::new()
                    .add_filter("GeoTIFF", &["tif", "tiff"])
                    .pick_files()
                {
                    self.load_geotiffs(files, ctx);
                }
            }
            if ui.button("Virtuele proefsleuf").clicked() {
                self.show_trench_dialog = true;
            }
            if ui.button("Opmerking plaatsen").clicked() {
                self.start_comment_mode(comments_ui::Mode::Place);
            }
            self.show_map_actions(ui, ctx);
            if ui.button("Locatie zoeken").clicked() {
                self.open_location();
            }
            if ui.button("TIFF-adressen").clicked() {
                self.open_tiff_addresses();
            }
            if ui.button("Maaiveld via Gemma").clicked() {
                self.open_ai_maaiveld();
            }
            if ui.button("Export kaarten").clicked() {
                self.open_map_export();
            }
            if ui.button("Kadastrale DXF exporteren").clicked() {
                self.open_cadastral_overview();
            }
            if ui.button("Offline KickTheMap jobs").clicked() {
                self.open_cached_jobs();
            }
            if ui
                .add_enabled(!self.pdok_busy, egui::Button::new("PDOK luchtfoto"))
                .clicked()
            {
                self.start_pdok();
            }
            if ui
                .add_enabled(!self.osm_busy, egui::Button::new("OpenStreetMap"))
                .clicked()
            {
                self.start_osm();
            }
            if self.osm_texture.is_some()
                && ui.selectable_label(self.osm_visible, "OSM tonen").clicked()
            {
                self.osm_visible = true;
                self.project_view.use_background = true;
                self.project_view.background_source = "OpenStreetMap".into();
                self.dirty = true;
            }
            if self.pdok_texture.is_some()
                && ui
                    .selectable_label(!self.osm_visible, "PDOK tonen")
                    .clicked()
            {
                self.osm_visible = false;
                self.project_view.use_background = true;
                self.project_view.background_source =
                    if self.pdok_source == pdok_wms::Source::Cadastral {
                        "PDOK kadastrale kaart"
                    } else {
                        "PDOK Luchtfoto NL actueel (8 cm, deels 5 cm)"
                    }
                    .into();
                self.dirty = true;
            }
            if ui
                .add_enabled(!self.bgt_busy, egui::Button::new("BGT wegdelen"))
                .clicked()
            {
                self.start_bgt(BgtKind::Wegdeel);
            }
            if ui
                .add_enabled(!self.bgt_busy, egui::Button::new("BGT terrein"))
                .clicked()
            {
                self.start_bgt(BgtKind::OnbegroeidTerreindeel);
            }
            if ui
                .add_enabled(!self.cadastral_busy, egui::Button::new("Kadaster WFS"))
                .clicked()
            {
                self.start_cadastral();
            }
            if ui
                .add_enabled(!self.ahn_busy, egui::Button::new("AHN hoogte"))
                .clicked()
            {
                self.request_ahn();
            }
            if ui.button("MarXact importeren").clicked() {
                if let Some(files) = rfd::FileDialog::new()
                    .add_filter("MarXact DXF", &["dxf"])
                    .pick_files()
                {
                    self.import_marxact(files);
                }
            }
            if ui.button("Open DXF").clicked() {
                if let Some(files) = rfd::FileDialog::new()
                    .add_filter("AutoCAD DXF", &["dxf"])
                    .pick_files()
                {
                    self.load_overlays(files);
                }
            }
            ui.separator();
            if ui.button("Excel disciplines").clicked() {
                self.save_excel();
            }
            if ui.button("DXF objecten").clicked() {
                self.save_dxf();
            }
            ui.menu_button("DXF-sjabloon", |ui| {
                if ui.button("TIFF-bladen en PS-varianten…").clicked() {
                    self.open_template_export();
                    ui.close_menu();
                }
                if ui.button("Jobs en virtuele proefsleuven…").clicked() {
                    self.open_project_template_dialog();
                    ui.close_menu();
                }
                if ui.button("Opgeslagen exportinvoer…").clicked() {
                    self.open_template_dialog();
                    ui.close_menu();
                }
            });
            if ui.button("Kaart terugzetten").clicked() {
                self.navigation = Default::default();
                self.location.needs_aerial = false;
                self.zoom = 1.0;
                self.pan = egui::Vec2::ZERO;
            }
        });
        if !self.message.is_empty() {
            ui.label(egui::RichText::new(&self.message).small().color(DARK));
        }
    }
    fn show_extended_sources(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_salt("source_controls")
            .show(ui, |ui| {
                if let Some(archive) = &self.legacy_archive {
                    egui::CollapsingHeader::new("Python-project: importmeldingen")
                        .default_open(true)
                        .show(ui, |ui| {
                            for notice in &archive.notices {
                                ui.label(notice);
                            }
                        });
                    ui.separator();
                }
                ui.heading("KickTheMap online");
                if ui.button("Open KickTheMap browser & jobbeheer").clicked() {
                    self.open_kickthemap_workspace();
                }
                ui.horizontal(|ui| {
                    ui.label("E-mail");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.live_email)
                            .desired_width(170.0)
                            .hint_text("e-mailadres"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Wachtwoord");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.live_password)
                            .password(true)
                            .desired_width(150.0),
                    );
                });
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!self.live_operation_busy(), egui::Button::new("Inloggen"))
                        .clicked()
                    {
                        self.login_kickthemap();
                    }
                    if ui
                        .add_enabled(
                            !self.live_operation_busy() && self.live_session.is_some(),
                            egui::Button::new("Vernieuwen"),
                        )
                        .clicked()
                    {
                        self.refresh_live_jobs();
                    }
                    if ui.button("Uitloggen").clicked() {
                        self.logout_live_jobs();
                    }
                });
                if self.live_busy {
                    ui.spinner();
                }
                if self.live_session.is_some() {
                    ui.small(format!(
                        "{} live jobs (niet opgeslagen)",
                        self.live_jobs.len()
                    ));
                    let mut request_download = None;
                    egui::ScrollArea::vertical()
                        .max_height(175.0)
                        .show(ui, |ui| {
                            for job in &self.live_jobs {
                                ui.label(format!(
                                    "{} — {} ({})",
                                    job.job_id, job.title, job.municipality
                                ));
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(
                                            !self.live_operation_busy(),
                                            egui::Button::new("GeoTIFF"),
                                        )
                                        .clicked()
                                    {
                                        request_download =
                                            Some((job.job_id, RemoteFileKind::GeoTiff));
                                    }
                                    if ui
                                        .add_enabled(
                                            !self.live_operation_busy(),
                                            egui::Button::new("Objecten"),
                                        )
                                        .clicked()
                                    {
                                        request_download =
                                            Some((job.job_id, RemoteFileKind::JobFeatures));
                                    }
                                });
                            }
                        });
                    if let Some((id, kind)) = request_download {
                        self.download_live_job(id, kind);
                    }
                }
                ui.separator();
                ui.heading("Ingeladen proefsleuven");
                ui.checkbox(&mut self.active_only, "Alleen ingeladen in Rust");
                if self.active_only {
                    ui.small(
                        "Filtert uitsluitend objecten en GeoTIFFs die nu in Rust zijn geladen.",
                    );
                }
                let active_ids = self.active_job_ids();
                ui.text_edit_singleline(&mut self.query);
                ui.small("Zoek op naam of job-ID");
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for (index, ds) in self.datasets.iter().enumerate() {
                            if self.active_only && !active_ids.contains(&ds.job_id) {
                                continue;
                            }
                            let search = self.query.trim().to_lowercase();
                            if !search.is_empty()
                                && !ds.title.to_lowercase().contains(&search)
                                && !ds.job_id.to_string().contains(&search)
                            {
                                continue;
                            }
                            let label = format!("{}  ({} objecten)", ds.title, ds.features.len());
                            if ui
                                .selectable_label(self.selected == Some(index), label)
                                .clicked()
                            {
                                self.selected = Some(index);
                            }
                        }
                    });
                ui.separator();
                ui.heading("GeoTIFF-bestanden");
                egui::ScrollArea::vertical()
                    .max_height(130.0)
                    .show(ui, |ui| {
                        for (index, info) in self.geotiffs.iter().enumerate() {
                            if ui
                                .selectable_label(
                                    self.raster_points.selected.as_ref() == Some(&info.path),
                                    info.path
                                        .file_name()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("TIFF"),
                                )
                                .clicked()
                            {
                                self.raster_points.selected = Some(info.path.clone());
                                self.raster_points.mode = None;
                                self.raster_points.rows.clear();
                            }
                            ui.small(format!(
                                "{}x{}, EPSG {}, job {}",
                                info.width,
                                info.height,
                                info.epsg
                                    .map(|v| v.to_string())
                                    .unwrap_or_else(|| "?".into()),
                                info.kickthemap_job_id
                                    .map(|v| v.to_string())
                                    .unwrap_or_else(|| "?".into())
                            ));
                            if let Some(error) =
                                self.raster_errors.get(index).and_then(|e| e.as_ref())
                            {
                                ui.colored_label(DARK, error);
                            } else if self.raster_textures.get(index).is_none_or(|t| t.is_none()) {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.small("Raster laden...");
                                });
                            }
                        }
                    });
                ui.separator();

                self.show_point_controls(ui);
                self.show_tiff_comment_controls(ui);
                self.show_raster_rotation_controls(ui);
                if let Some(cache) = &self.jobs_cache {
                    ui.separator();
                    ui.heading("KickTheMap jobs (offline cache)");
                    ui.small(format!(
                        "{} jobs, opgeslagen op {:.0} (Unix)",
                        cache.jobs.len(),
                        cache.saved_at
                    ));
                    ui.checkbox(&mut self.only_active_cached, "Alleen actief geladen");
                    ui.text_edit_singleline(&mut self.jobs_query);
                    let needle = self.jobs_query.trim().to_lowercase();
                    egui::ScrollArea::vertical()
                        .max_height(130.0)
                        .show(ui, |ui| {
                            for job in cache.jobs.iter().filter(|j| {
                                (!self.only_active_cached || active_ids.contains(&j.job_id))
                                    && (needle.is_empty()
                                        || j.title.to_lowercase().contains(&needle)
                                        || j.job_id.to_string().contains(&needle))
                            }) {
                                ui.label(format!(
                                    "{}: {} ({})",
                                    job.job_id, job.title, job.municipality
                                ));
                            }
                        });
                }
                ui.separator();
                ui.heading("Virtuele proefsleuven");
                let mut edit_virtual = None;
                for (index, trench) in self.virtual_trenches.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(
                                self.virtual_map.selected == Some(index),
                                format!(
                                    "{} ({:.2} m, {} objecten{})",
                                    trench.label,
                                    trench.length(),
                                    trench.objects.len(),
                                    if trench.boundary_3d.is_empty() {
                                        ""
                                    } else {
                                        ", MarXact 3D"
                                    }
                                ),
                            )
                            .clicked()
                        {
                            self.virtual_map.selected = Some(index);
                        }
                        if ui.small_button("Bewerken").clicked() {
                            edit_virtual = Some(index);
                        }
                    });
                    if !trench.boundary_3d.is_empty() {
                        ui.push_id(index, |ui| {
                            let mut rotation = trench
                                .alignment
                                .as_ref()
                                .map_or(0.0, |a| a.rotation_degrees);
                            if ui
                                .add(
                                    egui::Slider::new(&mut rotation, -90.0..=90.0)
                                        .text("Rotatie °"),
                                )
                                .changed()
                            {
                                match marxact_alignment::apply_rotation(trench, rotation) {
                                    Ok(()) => {
                                        self.dirty = true;
                                        self.trench_markers.clear();
                                    }
                                    Err(err) => self.message = format!("Uitlijning mislukt: {err}"),
                                }
                            }
                            if ui.small_button("Automatisch uitlijnen").clicked() {
                                match marxact_alignment::recalculate(trench) {
                                    Ok(()) => {
                                        self.dirty = true;
                                        self.trench_markers.clear();
                                    }
                                    Err(err) => self.message = format!("Uitlijning mislukt: {err}"),
                                }
                            }
                        });
                    }
                }
                if let Some(index) = edit_virtual {
                    self.open_virtual_editor(index);
                }
                self.show_virtual_map_controls(ui);
                ui.separator();
                self.show_map_comment_controls(ui);
                ui.separator();
                ui.heading("Kadaster WFS");
                if let Some(layer) = &mut self.cadastral {
                    let path_count = layer.linework.iter().map(|l| l.paths.len()).sum::<usize>();
                    if ui
                        .checkbox(
                            &mut layer.visible,
                            format!("{path_count} lijnen, {} labels", layer.labels.len()),
                        )
                        .changed()
                    {
                        self.dirty = true;
                    }
                }
                ui.separator();
                ui.heading("PDOK BGT-lagen");
                for layer in &mut self.bgt_layers {
                    if ui
                        .checkbox(
                            &mut layer.visible,
                            format!("{} ({} paden)", layer.kind.label(), layer.paths.len()),
                        )
                        .changed()
                    {
                        self.dirty = true;
                    }
                }
                ui.separator();
                ui.heading("DXF-lagen");
                let mut choose_dxf = None;
                let mut replace_dxf = None;
                let selected_dxf = self.dxf_selection.as_ref().map(|s| (s.overlay, s.feature));
                egui::ScrollArea::vertical()
                    .max_height(150.0)
                    .show(ui, |ui| {
                        for (overlay_index, overlay) in self.overlays.iter_mut().enumerate() {
                            let label = overlay
                                .path
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("DXF");
                            let changed = ui.checkbox(&mut overlay.visible, label).changed();
                            if changed {
                                self.dirty = true;
                            }
                            ui.small(format!(
                                "{} bronentiteiten, {} blokonderdelen",
                                overlay.features.len(),
                                overlay.expanded.features.len()
                            ));
                            for warning in overlay.warnings.iter().chain(&overlay.block_warnings) {
                                ui.colored_label(DARK, cad_preview(warning));
                            }
                            ui.push_id(overlay_index, |ui| {
                                ui.horizontal(|ui| {
                                    if ui.small_button("Opnieuw laden").clicked() {
                                        replace_dxf = Some((overlay_index, overlay.path.clone()));
                                    }
                                    if ui.small_button("Bron kiezen...").clicked() {
                                        if let Some(path) = rfd::FileDialog::new()
                                            .add_filter("AutoCAD DXF", &["dxf"])
                                            .pick_file()
                                        {
                                            replace_dxf = Some((overlay_index, path));
                                        }
                                    }
                                });
                                let mut layers_changed = false;
                                ui.collapsing("CAD-lagen", |ui| {
                                    for (name, layer) in &mut overlay.layers {
                                        if ui
                                            .checkbox(&mut layer.visible, cad_preview(name))
                                            .changed()
                                        {
                                            self.dirty = true;
                                            layers_changed = true;
                                        }
                                    }
                                });
                                if layers_changed {
                                    overlay.refresh_block_visibility();
                                }
                                ui.collapsing("Objecten bekijken", |ui| {
                                    egui::ScrollArea::vertical()
                                        .id_salt("objects")
                                        .max_height(140.0)
                                        .show_rows(
                                            ui,
                                            20.0,
                                            overlay.features.len()
                                                + overlay.expanded.features.len(),
                                            |ui, rows| {
                                                for index in rows {
                                                    let feature =
                                                        overlay.inspect_feature(index).unwrap();
                                                    let label = format!(
                                                        "{} {} [{}]",
                                                        feature.source.entity_type,
                                                        cad_preview(&feature.source.handle),
                                                        cad_preview(feature.layer)
                                                    );
                                                    if ui
                                                        .selectable_label(
                                                            selected_dxf
                                                                == Some((overlay_index, index)),
                                                            label,
                                                        )
                                                        .clicked()
                                                    {
                                                        choose_dxf = Some((overlay_index, index));
                                                    }
                                                }
                                            },
                                        );
                                });
                            });
                        }
                    });
                if let Some((overlay, feature)) = choose_dxf {
                    self.select_dxf(overlay, feature);
                }
                if let Some((overlay, path)) = replace_dxf {
                    self.replace_dxf_source(overlay, path);
                }
                if let Some(selection) = &self.dxf_selection {
                    ui.collapsing("CAD-objectgegevens", |ui| {
                        egui::Grid::new("cad-details").striped(true).show(ui, |ui| {
                            for (key, value) in &selection.details {
                                ui.label(key);
                                ui.vertical(|ui| {
                                    ui.label(cad_preview(value));
                                    if cad_preview(value).len() < value.len() {
                                        ui.small(format!("Verkort getoond, {} bytes", value.len()));
                                    }
                                    if ui.small_button("Kopiëren").clicked() {
                                        ui.ctx().copy_text(value.clone());
                                    }
                                });
                                ui.end_row();
                            }
                        });
                    });
                }
                ui.separator();
                if let Some(ds) = self.selected.and_then(|i| self.datasets.get(i)) {
                    ui.strong(&ds.title);
                    let rules = &self
                        .project_settings
                        .as_ref()
                        .unwrap_or(&self.migrated_settings)
                        .object_rules;
                    let summary = sleufbase_rust::summarize(ds, rules);
                    ui.label(format!(
                        "Job-ID {}, {} objecten, {} disciplines",
                        ds.job_id,
                        ds.features.len(),
                        summary.distinct_count()
                    ));
                    for (rule, count) in rules.iter().zip(summary.counts.iter()) {
                        if *count > 0 {
                            ui.label(format!("{}: {}", rule.display_label(), count));
                        }
                    }
                }
            });
    }
    pub(super) fn show_shell_canvas(&mut self, ui: &mut egui::Ui) {
        let selected: &[Dataset] = self
            .selected
            .and_then(|i| self.datasets.get(i))
            .map(std::slice::from_ref)
            .unwrap_or(&[]);
        self.navigation.pointer_tool = self.raster_rotation.active()
            || self.virtual_map.captures()
            || self.comment_tools.captures()
            || self.maaiveld_ui.captures()
            || self.raster_points.mode.is_some();
        self.navigation.raster_opacity = Some((self.project_view.opacity_percent / 100.) as f32);
        self.navigation.show_background = Some(self.project_view.use_background);
        let canvas_rect = ui.available_rect_before_wrap();
        let view = canvas(
            ui,
            selected,
            &self.geotiffs,
            &self.raster_textures,
            &self.overlays,
            &self.virtual_trenches,
            &self.trench_markers,
            &self.bgt_layers,
            self.cadastral.as_ref(),
            &self.comments,
            if self.osm_visible && self.osm_texture.is_some() {
                self.osm_texture.as_ref()
            } else {
                self.pdok_texture.as_ref()
            },
            if self.osm_visible && self.osm_texture.is_some() {
                self.osm_bounds
            } else {
                self.pdok_bounds
            },
            self.osm_visible && self.osm_texture.is_some(),
            &mut self.zoom,
            &mut self.pan,
            &self
                .project_settings
                .as_ref()
                .unwrap_or(&self.migrated_settings)
                .object_rules,
            Some(&mut self.navigation),
        );
        if let Some(view) = view {
            if let Some(pos) = view.response.hover_pos() {
                let p = view.world(pos);
                self.shell.coords = format!("RD: {:.2} / {:.2}", p[0], p[1]);
            }
            self.request_viewport_previews(&view);
            let camera = project_view_ui::capture_camera(&view);
            self.update_map_background(&view);
            if self.project_view.camera.as_ref() != Some(&camera)
                && self.navigation.camera_changed
            {
                self.dirty = true;
            }
            // Keep navigation in f64 world coordinates. Carrying an ever larger
            // f32 pixel pan through street-to-country zooms loses pick precision.
            self.navigation.focus = Some(location_ui::Focus {
                center: camera.center,
                meters_per_pixel: camera.meters_per_pixel,
            });
            self.zoom = 1.;
            self.pan = egui::Vec2::ZERO;
            self.project_view.camera = Some(camera);
            let old_virtual = self.virtual_map.selected;
            let old_raster = self.raster_points.selected.clone();
            if !self.handle_manual_maaiveld(&view) {
                self.handle_cable_information(&view);
                self.handle_raster_rotation(&view);
                self.handle_raster_points(&view);
                self.handle_virtual_map(&view);
                self.handle_map_comments(&view);
            }
            self.update_view_selection(old_virtual, old_raster.as_deref());
        }
        self.show_location_on_map(ui, canvas_rect);
    }
}
