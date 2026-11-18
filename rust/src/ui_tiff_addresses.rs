//! Original TIFF-address overview and clipboard actions; never owns pixels.
use super::*;
use sleufbase_rust::raster_addresses::{self, Record, Worker};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[derive(Default)]
pub(super) struct Dialog {
    pub open: bool,
    worker: Option<Worker>,
    ticket: Option<u64>,
    progress: Option<Arc<AtomicUsize>>,
    total: usize,
    records: Vec<Record>,
    selected: Option<usize>,
    error: Option<String>,
}
impl Dialog {
    pub fn reset(&mut self) {
        self.cancel();
        let worker = self.worker.take();
        *self = Self {
            worker,
            ..Default::default()
        };
    }
    fn cancel(&mut self) {
        if let Some(worker) = &self.worker {
            worker.cancel();
        }
        self.ticket = None;
        self.progress = None;
    }
    fn accept(&mut self, ready: raster_addresses::Ready) {
        if !self.open || self.ticket != Some(ready.ticket) {
            return;
        }
        self.ticket = None;
        self.progress = None;
        match ready.result {
            Ok(records) => {
                self.records = records;
                self.selected = (!self.records.is_empty()).then_some(0);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }
    fn start(&mut self, sources: Vec<raster_addresses::Source>) -> Result<(), String> {
        self.cancel();
        self.total = sources.len();
        self.records.clear();
        self.selected = None;
        self.error = None;
        if self.worker.is_none() {
            self.worker = Some(Worker::new()?);
        }
        let ticket = self.worker.as_ref().unwrap().start(sources)?;
        self.ticket = Some(ticket.id);
        self.progress = Some(ticket.progress);
        Ok(())
    }
}

/// Lay out all cells from the width of the scroll content, so the fixed header
/// stays aligned with the rows even when the scrollbar takes horizontal space.
fn address_columns(rect: egui::Rect) -> [egui::Rect; 3] {
    let inner = rect.shrink2(egui::vec2(8., 4.));
    let gap = 12.;
    let distance_width = 78_f32.min(inner.width() * 0.3);
    let distance_left = inner.right() - distance_width;
    let text_width = (distance_left - gap - inner.left()).max(0.);
    let name_width = ((text_width - gap) * 0.38).max(0.);
    [
        egui::Rect::from_min_max(inner.min, egui::pos2(inner.left() + name_width, inner.bottom())),
        egui::Rect::from_min_max(egui::pos2(inner.left() + name_width + gap, inner.top()),
            egui::pos2(distance_left - gap, inner.bottom())),
        egui::Rect::from_min_max(egui::pos2(distance_left, inner.top()), inner.max),
    ]
}

fn address_cell(ui: &mut egui::Ui, rect: egui::Rect, text: egui::RichText) {
    ui.put(rect, egui::Label::new(text).truncate().halign(egui::Align::Min).selectable(false));
}

#[derive(Default)]
struct AddressActions {
    copy: Option<(usize, bool)>,
    refresh: bool,
    cancel: bool,
}

/// Measure this frame's button rows before positioning the footer. A bottom
/// panel would start from its previous height, clipping a newly wrapped row
/// for one frame when the window narrows or the cancel action appears.
fn address_actions(ui: &mut egui::Ui, state: &Dialog) -> (egui::Rect, AddressActions) {
    let rect = ui.available_rect_before_wrap();
    let spacing = ui.spacing().item_spacing;
    let padding = ui.spacing().button_padding;
    let captions = ["Kopieer adres", "Kopieer met bestandsnaam", "Vernieuwen", "Ophalen annuleren"];
    let count = if state.progress.is_some() { 4 } else { 3 };
    let texts = captions[..count].iter().map(|caption| egui::WidgetText::from(*caption)
        .into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Button))
        .collect::<Vec<_>>();
    let sizes = texts.iter().map(|text| {
        let size = text.size() + 2. * padding;
        egui::vec2(size.x.ceil(), size.y.max(ui.spacing().interact_size.y).ceil())
    }).collect::<Vec<_>>();
    let mut rows = Vec::new();
    let mut start = 0;
    let mut width = 0.;
    let mut height: f32 = 0.;
    for (index, size) in sizes.iter().enumerate() {
        let next_width = width + if index == start { 0. } else { spacing.x } + size.x;
        if index > start && next_width > rect.width() {
            rows.push((start..index, height));
            start = index;
            width = size.x;
            height = size.y;
        } else {
            width = next_width;
            height = height.max(size.y);
        }
    }
    rows.push((start..sizes.len(), height));
    let top_padding = 9.;
    let bottom_padding = 4.;
    let footer_height = rows.iter().map(|(_, height)| height).sum::<f32>()
        + spacing.y * (rows.len() - 1) as f32 + top_padding + bottom_padding;
    let footer_top = (rect.bottom() - footer_height).max(rect.top());
    let content_rect = egui::Rect::from_min_max(rect.min,
        egui::pos2(rect.right(), (footer_top - spacing.y).max(rect.top())));
    ui.painter().hline(rect.x_range(), footer_top + 0.5, Stroke::new(1_f32, theme::BORDER));
    let selected = state.selected.filter(|&index| index < state.records.len());
    let mut actions = AddressActions::default();
    let mut top = footer_top + top_padding;
    for (row_index, (items, height)) in rows.into_iter().enumerate() {
        let row_rect = egui::Rect::from_min_size(egui::pos2(rect.left(), top), egui::vec2(rect.width(), height));
        let mut row = ui.new_child(egui::UiBuilder::new()
            .id_salt(("tiff-address-actions", row_index))
            .max_rect(row_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)));
        row.set_clip_rect(ui.clip_rect().intersect(rect));
        for index in items {
            let enabled = index >= 2 || selected.is_some();
            if row.add_enabled(enabled, egui::Button::new(texts[index].clone()).min_size(sizes[index])).clicked() {
                match index {
                    0 => actions.copy = selected.map(|index| (index, false)),
                    1 => actions.copy = selected.map(|index| (index, true)),
                    2 => actions.refresh = true,
                    3 => actions.cancel = true,
                    _ => unreachable!(),
                }
            }
        }
        top += height + spacing.y;
    }
    (content_rect, actions)
}

fn address_table(ui: &mut egui::Ui, state: &mut Dialog) -> Option<usize> {
    let rect = ui.available_rect_before_wrap();
    let line_height = ui.text_style_height(&egui::TextStyle::Body).ceil().max(18.);
    let header_height = line_height + 10.;
    if rect.width() < 120. || rect.height() <= header_height + 20. {
        return None;
    }
    // Allocate exactly the remaining slot. Child text must never feed back
    // into Window's desired size or leave a fixed-height list in a taller window.
    ui.allocate_rect(rect, egui::Sense::hover());
    ui.painter().rect_filled(rect, 10., Color32::WHITE);
    ui.painter().rect_stroke(rect, 10., Stroke::new(1_f32, theme::BORDER), egui::StrokeKind::Inside);
    let inner = rect.shrink2(egui::vec2(8., 8.));
    let compact = inner.width() < 580.;
    let row_height = if compact { 2. * line_height + 12. } else { line_height + 12. };
    let header_rect = egui::Rect::from_min_size(inner.min, egui::vec2(inner.width(), header_height));
    let rows_rect = egui::Rect::from_min_max(
        egui::pos2(inner.left(), header_rect.bottom() + 4.), inner.max,
    );
    let mut rows_ui = ui.new_child(egui::UiBuilder::new()
        .id_salt("tiff-address-table-body")
        .max_rect(rows_rect)
        .layout(egui::Layout::top_down(egui::Align::LEFT)));
    rows_ui.set_clip_rect(ui.clip_rect().intersect(rows_rect));
    rows_ui.spacing_mut().item_spacing.y = 3.;
    let mut row_width = inner.width();
    let mut copy = None;
    egui::ScrollArea::vertical()
        .id_salt("tiff-address-table-scroll")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .auto_shrink([false, false])
        .min_scrolled_height(0.)
        .max_height(rows_rect.height().max(0.))
        .show_rows(&mut rows_ui, row_height, state.records.len().max(1), |ui, rows| {
            row_width = ui.available_width();
            for index in rows {
                let Some(record) = state.records.get(index) else {
                    ui.add_sized([row_width, row_height], egui::Label::new(
                        if state.ticket.is_some() { "Adressen worden opgehaald…" }
                        else { "Geen adressen beschikbaar. Klik op Vernieuwen om ze op te halen." })
                        .wrap());
                    continue;
                };
                ui.push_id(("tiff-address-row", index), |ui| {
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(row_width, row_height), egui::Sense::click(),
                    );
                    let selected = state.selected == Some(index);
                    let fill = if selected { Color32::from_rgb(255, 224, 199) }
                        else if response.hovered() { Color32::from_rgb(242, 245, 250) }
                        else if index % 2 == 1 { theme::SURFACE_ALT }
                        else { Color32::WHITE };
                    ui.painter().rect_filled(rect, 6., fill);
                    if selected {
                        ui.painter().rect_stroke(rect, 6., Stroke::new(1_f32, ORANGE), egui::StrokeKind::Inside);
                    }
                    let [name, address, distance] = address_columns(rect);
                    let mut cells = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                    cells.set_clip_rect(ui.clip_rect().intersect(rect));
                    if compact {
                        let text = name.union(address);
                        let upper = egui::Rect::from_min_size(text.min, egui::vec2(text.width(), line_height));
                        let lower = egui::Rect::from_min_size(
                            text.min + egui::vec2(0., line_height + 2.), egui::vec2(text.width(), line_height),
                        );
                        address_cell(&mut cells, upper, egui::RichText::new(cad_preview(&record.name)).strong());
                        address_cell(&mut cells, lower, egui::RichText::new(cad_preview(&record.address)));
                    } else {
                        address_cell(&mut cells, name, egui::RichText::new(cad_preview(&record.name)).strong());
                        address_cell(&mut cells, address, egui::RichText::new(cad_preview(&record.address)));
                    }
                    address_cell(&mut cells, distance, egui::RichText::new(&record.distance).color(theme::MUTED));
                    response.widget_info(|| egui::WidgetInfo::selected(
                        egui::WidgetType::SelectableLabel, ui.is_enabled(), selected, &record.name,
                    ));
                    if response.clicked() { state.selected = Some(index); }
                    if response.double_clicked() { copy = Some(index); }
                    response.on_hover_ui(|ui| {
                        ui.set_max_width(520.);
                        ui.strong(cad_preview(&record.name));
                        ui.label(cad_preview(&record.address));
                        ui.label(format!("Afstand tot adres: {}", record.distance));
                    });
                });
            }
        });
    let header = egui::Rect::from_min_size(header_rect.min, egui::vec2(row_width, header_height));
    let [name, address, distance] = address_columns(header);
    let mut headings = ui.new_child(egui::UiBuilder::new().max_rect(header_rect)
        .id_salt("tiff-address-table-header"));
    headings.set_clip_rect(ui.clip_rect().intersect(header_rect));
    if compact {
        address_cell(&mut headings, name.union(address), egui::RichText::new("Afbeelding / adres").strong());
    } else {
        address_cell(&mut headings, name, egui::RichText::new("Afbeelding").strong());
        address_cell(&mut headings, address, egui::RichText::new("Adres").strong());
    }
    address_cell(&mut headings, distance, egui::RichText::new("Afstand").strong());
    ui.painter().hline(inner.x_range(), header_rect.bottom(), Stroke::new(1_f32, theme::BORDER));
    copy
}

impl SleufBaseApp {
    pub(super) fn open_tiff_addresses(&mut self) {
        if self.geotiffs.is_empty() {
            self.message = "Laad eerst minimaal een GeoTIFF-bestand.".into();
            return;
        }
        self.tiff_addresses.open = true;
        let sources = self
            .geotiffs
            .iter()
            .map(raster_addresses::Source::from_raster)
            .collect();
        if let Err(error) = self.tiff_addresses.start(sources) {
            self.tiff_addresses.error = Some(error);
        }
    }
    pub(super) fn poll_tiff_addresses(&mut self, ctx: &egui::Context) {
        if let Some(ready) = self.tiff_addresses.worker.as_ref().and_then(Worker::poll) {
            let accepted =
                self.tiff_addresses.open && self.tiff_addresses.ticket == Some(ready.ticket);
            self.tiff_addresses.accept(ready);
            if accepted {
                self.message = self.tiff_addresses.error.as_ref().map_or_else(
                    || {
                        format!(
                            "Adressen opgehaald voor {} GeoTIFF-bestand(en).",
                            self.tiff_addresses.records.len()
                        )
                    },
                    |e| format!("TIFF-adressen ophalen mislukt: {e}"),
                );
            }
        }
        if self.tiff_addresses.ticket.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }
    fn copy_tiff_address(&mut self, ctx: &egui::Context, index: usize, include_name: bool) {
        if let Some(row) = self.tiff_addresses.records.get(index) {
            ctx.copy_text(row.copied(include_name).to_owned());
            self.message = format!("Adres gekopieerd: {}", row.name);
        }
    }
    pub(super) fn show_tiff_addresses(&mut self, ctx: &egui::Context) {
        if !self.tiff_addresses.open {
            return;
        }
        let mut open = true;
        let mut cancel = false;
        let mut refresh = false;
        let mut copy = None;
        let viewport = ctx.screen_rect();
        let maximum = egui::vec2(
            (viewport.width() - 64.).max(280.),
            (viewport.height() - 88.).max(200.),
        );
        let default_size = egui::vec2(850., 460.).min(maximum);
        egui::Window::new("TIFF-adressen")
            .id(egui::Id::new("tiff-addresses"))
            .open(&mut open)
            .default_pos(viewport.center() - default_size * 0.5)
            .default_size(default_size)
            .min_size(egui::vec2(380., 320.).min(maximum))
            .max_size(maximum)
            .constrain_to(viewport.shrink(16.))
            .movable(true)
            .resizable(true)
            .show(ctx, |ui| theme::bounded_window_body(ui, |ui| {
                settings_ui::control_style(ui);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                let state = &mut self.tiff_addresses;
                let (content_rect, actions) = address_actions(ui, state);
                copy = actions.copy;
                cancel = actions.cancel;
                refresh = actions.refresh;
                let mut content = ui.new_child(egui::UiBuilder::new()
                    .id_salt("tiff-address-content")
                    .max_rect(content_rect)
                    .layout(egui::Layout::top_down(egui::Align::LEFT)));
                content.set_clip_rect(ui.clip_rect().intersect(content_rect));
                let ui = &mut content;
                ui.horizontal_wrapped(|ui| {
                    theme::strong(ui, "Adressen bij de geladen GeoTIFFs");
                    ui.label(egui::RichText::new(format!("{} TIFFs", state.records.len()))
                        .small().color(theme::MUTED))
                        .on_hover_text("Adressen bij het midden van de GeoTIFFs die bij het openen geladen waren. Vernieuwen gebruikt de huidige geladen bestanden.");
                });
                ui.add(egui::Label::new(egui::RichText::new(
                    "Selecteer een regel om te kopiëren. Dubbelklik om het adres direct te kopiëren.")
                    .small().color(theme::MUTED)).wrap());
                if let Some(progress) = &state.progress {
                    let done = progress.load(Ordering::Relaxed).min(state.total);
                    ui.add(egui::ProgressBar::new(done as f32 / state.total.max(1) as f32)
                        .desired_width(ui.available_width())
                        .text(format!("Adressen ophalen: {done} / {}", state.total)));
                }
                if let Some(error) = &state.error {
                    ui.add(egui::Label::new(egui::RichText::new(cad_preview(error)).color(DARK)).truncate())
                        .on_hover_text(cad_preview(error));
                }
                if let Some(index) = address_table(ui, state) {
                    copy = Some((index, false));
                }
            }));
        if let Some((index, include_name)) = copy {
            self.copy_tiff_address(ctx, index, include_name);
        }
        if cancel {
            self.tiff_addresses.cancel();
            self.message = "TIFF-adressen ophalen geannuleerd".into();
        }
        if refresh {
            self.open_tiff_addresses();
        }
        if !open {
            self.tiff_addresses.open = false;
            self.tiff_addresses.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sleufbase_rust::location_search::ReverseLocation;
    fn row() -> Record {
        Record::from_result(
            "Proefsleuf Ø.tif".into(),
            Ok(vec![ReverseLocation {
                label: "Straat Ø 12, Testplaats".into(),
                distance_m: 1.25,
                location_type: "adres".into(),
            }]),
        )
    }
    #[test]
    fn actual_egui_clipboard_commands_use_original_address_and_filename_text() {
        let ctx = egui::Context::default();
        let mut app = SleufBaseApp {
            tiff_addresses: Dialog {
                open: true,
                records: vec![row()],
                selected: Some(0),
                ..Default::default()
            },
            ..Default::default()
        };
        for include_name in [false, true] {
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                app.show_tiff_addresses(ctx);
                app.copy_tiff_address(ctx, 0, include_name);
            });
            let copies = output
                .platform_output
                .commands
                .iter()
                .filter_map(|c| match c {
                    egui::OutputCommand::CopyText(text) => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(copies, vec![row().copied(include_name)]);
            assert_eq!(app.message, "Adres gekopieerd: Proefsleuf Ø.tif");
            assert!(!app.dirty);
        }
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            app.copy_tiff_address(ctx, 99, false)
        });
        assert!(output.platform_output.commands.is_empty());
    }
    #[test]
    fn cancelled_closed_and_project_switched_dialogs_ignore_late_rows_and_keep_the_worker() {
        let mut dialog = Dialog {
            open: true,
            ticket: Some(2),
            ..Default::default()
        };
        dialog.accept(raster_addresses::Ready {
            ticket: 1,
            result: Ok(vec![row()]),
        });
        assert!(dialog.records.is_empty());
        dialog.accept(raster_addresses::Ready {
            ticket: 2,
            result: Ok(vec![row()]),
        });
        assert_eq!(dialog.selected, Some(0));
        dialog.ticket = Some(3);
        dialog.open = false;
        dialog.accept(raster_addresses::Ready {
            ticket: 3,
            result: Err("old error".into()),
        });
        assert!(dialog.error.is_none());
        let worker = Worker::with_lookup(|_, _, _| Ok(Vec::new())).unwrap();
        let mut app = SleufBaseApp {
            tiff_addresses: Dialog {
                open: true,
                worker: Some(worker),
                records: vec![row()],
                ticket: Some(3),
                ..Default::default()
            },
            ..Default::default()
        };
        app.apply_project(
            NativeProject::default(),
            None,
            false,
            Vec::new(),
            &egui::Context::default(),
        );
        assert!(
            !app.tiff_addresses.open
                && app.tiff_addresses.records.is_empty()
                && app.tiff_addresses.ticket.is_none()
        );
        assert!(app.tiff_addresses.worker.is_some());
        assert!(!app.dirty);
        app.open_tiff_addresses();
        assert!(!app.tiff_addresses.open);
        assert_eq!(app.message, "Laad eerst minimaal een GeoTIFF-bestand.");
    }
    #[test]
    fn dialog_worker_queries_a_real_geotiff_center_and_preserves_loaded_sources() {
        use tiff::{
            encoder::{colortype::RGB8, TiffEncoder},
            tags::Tag,
        };
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("Voorbeeld Ø.tif");
        {
            let mut encoder = TiffEncoder::new(std::fs::File::create(&path).unwrap()).unwrap();
            let mut image = encoder.new_image::<RGB8>(12, 8).unwrap();
            image
                .encoder()
                .write_tag(Tag::ModelPixelScaleTag, &[0.5f64, 0.25, 0.][..])
                .unwrap();
            image
                .encoder()
                .write_tag(
                    Tag::ModelTiepointTag,
                    &[0f64, 0., 0., 168000., 401000., 0.][..],
                )
                .unwrap();
            image
                .encoder()
                .write_tag(
                    Tag::GeoKeyDirectoryTag,
                    &[1u16, 1, 0, 1, 3072, 0, 1, 28992][..],
                )
                .unwrap();
            image.write_data(&[255; 12 * 8 * 3]).unwrap();
        }
        let raster = sleufbase_rust::geotiff::read_geotiff_metadata(&path).unwrap();
        let worker = Worker::with_lookup(|x, y, _| {
            assert_eq!([x, y], [168003., 400999.]);
            Ok(vec![ReverseLocation {
                label: "Gevonden adres".into(),
                distance_m: 0.25,
                location_type: "adres".into(),
            }])
        })
        .unwrap();
        let mut app = SleufBaseApp {
            geotiffs: vec![raster],
            tiff_addresses: Dialog {
                worker: Some(worker),
                ..Default::default()
            },
            ..Default::default()
        };
        let sources = app.geotiffs.as_ptr();
        app.open_tiff_addresses();
        let ctx = egui::Context::default();
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.tiff_addresses.ticket.is_some() {
            app.poll_tiff_addresses(&ctx);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(app.tiff_addresses.records[0].name, "Voorbeeld Ø.tif");
        assert_eq!(app.tiff_addresses.records[0].address, "Gevonden adres");
        assert_eq!(
            app.message,
            "Adressen opgehaald voor 1 GeoTIFF-bestand(en)."
        );
        assert_eq!(app.geotiffs.as_ptr(), sources);
        assert!(!app.dirty && app.raster_textures.is_empty());
    }
}
