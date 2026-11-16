//! Persistent work-map candidate panel; selection remains owned by the app.
use super::{theme, DxfOverlay, APP_BG, MUTED, ORANGE};
use super::super::cable_information_ui;
use eframe::egui::{self, Color32};
use sleufbase_rust::{cad::Tag, cad_scene::Scene};
use std::{collections::{BTreeSet, HashMap}, path::PathBuf, sync::{Arc, Weak}};

type Candidate = (usize, usize);
const MAX_CACHED_ROWS: usize = 512;

#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    generation: Option<u64>,
    candidates: Vec<Candidate>,
    sources: Vec<(usize, SourceStamp)>,
    rows: HashMap<Candidate, Row>,
    row_heights: Vec<Option<f32>>,
    row_layout: Option<[f32; 5]>,
    // Scroll history only: the active row always comes from dxf_selection.
    last_scroll_target: Option<Candidate>,
    reveal_selection: bool,
}

struct SourceStamp {
    scene: Weak<Scene>,
    address: usize,
    count: usize,
    path: PathBuf,
}
impl SourceStamp {
    fn capture(overlay: &DxfOverlay) -> Self {
        Self { scene: Arc::downgrade(&overlay.expanded), address: overlay.features.as_ptr() as usize,
            count: overlay.features.len(), path: overlay.path.clone() }
    }
    fn matches(&self, overlay: &DxfOverlay) -> bool {
        self.address == overlay.features.as_ptr() as usize && self.count == overlay.features.len()
            && self.path == overlay.path
            && self.scene.upgrade().is_some_and(|scene| Arc::ptr_eq(&scene, &overlay.expanded))
    }
}

struct Row { layer: String, context: String, infracad: Option<String> }

pub(super) fn visible_matches(overlays: &[DxfOverlay], candidates: &[Candidate]) -> Vec<Candidate> {
    candidates.iter().copied().filter(|&(overlay, feature)|
        cable_information_ui::candidate_visible(overlays, overlay, feature)).collect()
}

impl State {
    pub fn open_and_reveal(&mut self) {
        self.open = true;
        self.reveal_selection = true;
    }
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.reveal_selection = self.open;
    }
    fn sync(&mut self, generation: u64, overlays: &[DxfOverlay], candidates: &[Candidate]) {
        if self.generation == Some(generation) && self.candidates == candidates
            && self.sources.iter().all(|(index, stamp)| overlays.get(*index).is_some_and(|o| stamp.matches(o)))
        { return; }
        self.generation = Some(generation);
        self.candidates.clear();
        self.candidates.extend_from_slice(candidates);
        self.rows.clear();
        self.row_heights = vec![None; candidates.len()];
        self.row_layout = None;
        self.sources = candidates.iter().map(|&(overlay, _)| overlay).collect::<BTreeSet<_>>()
            .into_iter().filter_map(|index| overlays.get(index).map(|o| (index, SourceStamp::capture(o)))).collect();
        self.reveal_selection = true;
    }
}

fn shortened(value: &str, characters: usize) -> String {
    if let Some((end, _)) = value.char_indices().nth(characters) { format!("{}…", &value[..end]) }
    else { value.to_owned() }
}
fn infracad(overlay: &DxfOverlay, index: usize) -> Option<&[Tag]> {
    overlay.inspect_feature(index)?.source.cad.as_deref()?.xdata.iter()
        .find(|(name, tags)| name.eq_ignore_ascii_case("InfraCAD") && !tags.is_empty())
        .map(|(_, tags)| tags.as_slice())
}
fn preview(tags: &[Tag]) -> String {
    // Bound both the number and size of copied tags before formatting. A single
    // enormous XDATA value must not be cloned just to make a small row preview.
    let mut prefix = Vec::new();
    let mut remaining = 600usize;
    let mut shortened_input = false;
    for tag in tags.iter().take(64) {
        let end = tag.value.char_indices().nth(remaining).map(|(index, _)| index);
        let value = end.map_or(tag.value.as_str(), |end| &tag.value[..end]);
        remaining = remaining.saturating_sub(value.chars().count() + 2);
        prefix.push(Tag { code: tag.code, value: value.to_owned() });
        shortened_input |= end.is_some();
        if remaining == 0 { break; }
    }
    shortened_input |= prefix.len() < tags.len();
    let mut text = shortened(&sleufbase_rust::cad::xdata_text(&prefix), 600);
    if shortened_input && !text.ends_with('…') { text.push('…'); }
    text
}
impl Row {
    fn build(overlay: &DxfOverlay, index: usize) -> Option<Self> {
        let feature = overlay.inspect_feature(index)?;
        let filename = overlay.path.file_name().unwrap_or_default().to_string_lossy();
        Some(Self {
            layer: shortened(if feature.layer.is_empty() { "(geen laagnaam)" } else { feature.layer }, 160),
            context: format!("{} · {}", shortened(&filename, 160), shortened(&feature.source.entity_type, 32)),
            infracad: infracad(overlay, index).map(preview),
        })
    }
}

fn row_fonts(ui: &egui::Ui) -> (egui::FontId, egui::FontId, egui::FontId) {
    let body = egui::TextStyle::Body.resolve(ui.style());
    let small = egui::TextStyle::Small.resolve(ui.style());
    let named = egui::FontFamily::Name("SleufBase title".into());
    let bold = egui::FontId::new(body.size, if ui.fonts(|f| f.families().contains(&named)) {
        named
    } else { body.family.clone() });
    (bold, small, body)
}
fn estimated_row_height(ui: &egui::Ui) -> f32 {
    let (bold, small, body) = row_fonts(ui);
    ui.fonts(|fonts| fonts.row_height(&bold).ceil()
        + fonts.row_height(&small).ceil() * 2. + fonts.row_height(&body).ceil()) + 20.
}
struct RowLayout {
    blocks: Vec<(Arc<egui::Galley>, Color32)>,
    height: f32,
}
fn row_layout(ui: &egui::Ui, row: &Row, active: bool, width: f32) -> RowLayout {
    let (bold, small, body) = row_fonts(ui);
    let mut blocks = Vec::with_capacity(4);
    let mut height = 14.;
    for (text, font, lines, color) in [
        (row.layer.as_str(), bold, 2, ui.visuals().text_color()),
        (row.context.as_str(), small.clone(), 1, MUTED),
        (if active { "Actief · XDATA InfraCAD" } else { "XDATA InfraCAD" }, small, 1, MUTED),
        (row.infracad.as_deref().unwrap_or("Geen InfraCAD-informatie"), body, 2, ui.visuals().text_color()),
    ] {
        let mut job = egui::text::LayoutJob::simple(text.to_owned(), font, color, (width - 20.).max(1.));
        job.wrap.max_rows = lines;
        let galley = ui.fonts(|fonts| fonts.layout_job(job));
        height += galley.size().y;
        if !blocks.is_empty() { height += 2.; }
        blocks.push((galley, color));
    }
    RowLayout { blocks, height: height.ceil() }
}
fn row_widget(ui: &mut egui::Ui, active: bool, layout: RowLayout) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), layout.height), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let highlighted = response.hovered() || response.has_focus();
        ui.painter().rect(rect, 10_u8,
            if active { Color32::from_rgb(255, 224, 199) } else if highlighted { APP_BG } else { Color32::WHITE },
            egui::Stroke::new(if active { 2_f32 } else { 1_f32 },
                if active || highlighted { ORANGE } else { theme::BORDER }), egui::StrokeKind::Inside);
        let mut position = rect.min + egui::vec2(10., 7.);
        for (galley, color) in layout.blocks {
            let height = galley.size().y;
            ui.painter().galley(position, galley, color);
            position.y += height + 2.;
        }
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub(super) fn show(
    ctx: &egui::Context, state: &mut State, generation: u64, overlays: &[DxfOverlay],
    candidates: &[Candidate], selected: Option<Candidate>,
) -> Option<Candidate> {
    if !state.open { return None; }
    state.sync(generation, overlays, candidates);
    let reveal = state.reveal_selection || state.last_scroll_target != selected;
    state.last_scroll_target = selected;
    state.reveal_selection = false;
    let available = ctx.available_rect().width();
    // Leave useful map space at normal window sizes, including after resizing.
    let maximum = (available - 520.).max(available * 0.30).min(460.).max(1.);
    let minimum = maximum.min(240.);
    let mut choose = None;
    egui::SidePanel::right("cable-object-list").resizable(true)
        .default_width(330_f32.min(maximum)).width_range(minimum..=maximum)
        .frame(egui::Frame::new().fill(APP_BG).inner_margin(egui::Margin {
            left: 8, right: 18, top: 18, bottom: 18,
        })).show(ctx, |ui| {
            ui.label(egui::RichText::new("Kabelobjecten").font(row_fonts(ui).0));
            ui.add_space(6.);
            if candidates.is_empty() {
                theme::card(12, 12, 12).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(egui::Label::new("Klik op een object in de kaart, of gebruik de lijnknop naast Lijst en klik twee punten. Hier verschijnen de gevonden zichtbare objecten.").wrap());
                });
                return;
            }
            let estimated_height = estimated_row_height(ui);
            let spacing = 5.;
            ui.spacing_mut().item_spacing.y = spacing;
            let scroll = egui::ScrollArea::vertical().id_salt("cable-object-list-rows")
                .auto_shrink([false, false])
                .animated(false)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible);
            if reveal {
                let id = ui.make_persistent_id(egui::Id::new("cable-object-list-rows"));
                let previous = egui::scroll_area::State::load(ui.ctx(), id).unwrap_or_default().offset.y;
                // Stop stale inertia on selection changes, retaining the user's
                // current scroll position when the selected row is already visible.
                let mut fresh = egui::scroll_area::State::default();
                fresh.offset.y = previous;
                fresh.store(ui.ctx(), id);
            }
            scroll.show_viewport(ui, |ui, viewport| {
                let width = ui.available_width();
                let (bold, small, body) = row_fonts(ui);
                let stamp = [width, ui.ctx().pixels_per_point(), bold.size, small.size, body.size];
                if state.row_layout != Some(stamp) {
                    state.row_layout = Some(stamp);
                    state.row_heights.fill(None);
                    // Replacing measured heights shifts every following row.
                    // Reveal again after the resized viewport has been measured.
                    state.reveal_selection = true;
                    ui.ctx().request_repaint();
                }
                let origin = ui.min_rect().min;
                let target = selected.and_then(|value| candidates.iter().position(|&candidate| candidate == value)).unwrap_or(0);
                let mut target_rect = None;
                let mut top = 0.;
                // Cache only cheap heights for offscreen rows. Building and
                // measuring their text remains limited to the visible viewport.
                for (row_index, &candidate) in candidates.iter().enumerate() {
                    let mut height = state.row_heights[row_index].unwrap_or(estimated_height);
                    let visible = top + height >= viewport.top() - estimated_height
                        && top <= viewport.bottom() + estimated_height;
                    if !visible {
                        if reveal && row_index == target {
                            target_rect = Some(egui::Rect::from_min_size(origin + egui::vec2(0., top), egui::vec2(width, height)));
                            state.reveal_selection = true;
                        }
                        top += height + spacing;
                        continue;
                    }
                    let Some(overlay) = overlays.get(candidate.0) else { top += height + spacing; continue; };
                    if !state.rows.contains_key(&candidate) {
                        if state.rows.len() >= MAX_CACHED_ROWS { state.rows.clear(); }
                        let Some(row) = Row::build(overlay, candidate.1) else { top += height + spacing; continue; };
                        state.rows.insert(candidate, row);
                    }
                    let row = &state.rows[&candidate];
                    let active = selected == Some(candidate);
                    let layout = row_layout(ui, row, active, width);
                    height = layout.height;
                    state.row_heights[row_index] = Some(height);
                    let rect = egui::Rect::from_min_size(origin + egui::vec2(0., top), egui::vec2(width, height));
                    let mut row_ui = ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt(candidate)
                        .layout(egui::Layout::top_down(egui::Align::Min)));
                    let response = row_widget(&mut row_ui, active, layout);
                    if reveal && row_index == target { target_rect = Some(rect); }
                    top += height + spacing;
                    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel,
                        ui.is_enabled(), active, &row.layer));
                    if response.clicked() { choose = Some(candidate); }
                    response.on_hover_ui(|ui| {
                        ui.set_max_width(420.);
                        if let Some(feature) = overlay.inspect_feature(candidate.1) {
                            theme::strong(ui, shortened(feature.layer, 600));
                        }
                        ui.label(shortened(&overlay.path.to_string_lossy(), 600));
                        if let Some(text) = &row.infracad { ui.add(egui::Label::new(text).wrap()); }
                        ui.small("Klik om dit object en de volledige kabelinformatie te selecteren.");
                        if row.infracad.is_some() && ui.small_button("Volledige InfraCAD-informatie kopiëren").clicked() {
                            if let Some(tags) = infracad(overlay, candidate.1) { ui.ctx().copy_text(sleufbase_rust::cad::xdata_text(tags)); }
                        }
                    });
                }
                ui.set_min_height((top - spacing).max(0.));
                if let Some(rect) = target_rect { ui.scroll_to_rect(rect, None); }
            });
        });
    choose
}

#[cfg(test)]
#[path = "ui_cable_list_tests.rs"]
mod tests;
