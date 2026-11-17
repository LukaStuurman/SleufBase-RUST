//! Persistent work-map candidate panel; selection remains owned by the app.
use super::{theme, DxfOverlay, APP_BG, ORANGE};
use super::super::cable_information_ui;
use eframe::egui::{self, Color32};
use sleufbase_rust::{cad::Tag, cad_scene::Scene};
use std::{collections::{BTreeMap, BTreeSet, HashMap}, path::PathBuf, sync::{Arc, Weak}};

type Candidate = (usize, usize);
const MAX_CACHED_ROWS: usize = 512;

#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    generation: Option<u64>,
    candidates: Vec<Candidate>,
    sources: Vec<(usize, SourceStamp)>,
    rows: HashMap<Candidate, Row>,
    // Membership is tied to object identity, never to indices in a filtered
    // result list. Visibility changes can therefore safely shrink a group.
    group_membership: HashMap<Candidate, usize>,
    expanded_groups: BTreeSet<usize>,
    groups: Vec<Group>,
    entries: Vec<Entry>,
    groups_changed: bool,
    row_heights: Vec<Option<f32>>,
    row_layout: Option<[f32; 4]>,
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

struct Row { layer: String, infracad: Option<String> }

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Entry { Object(Candidate), Group(usize) }

#[derive(Default)]
struct Group { members: Vec<Candidate>, label: Option<String> }

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
    pub fn clear_groups(&mut self) {
        if self.group_membership.is_empty() { return; }
        self.group_membership.clear();
        self.expanded_groups.clear();
        self.groups.clear();
        self.groups_changed = true;
    }
    pub fn set_line_groups(
        &mut self, generation: u64, overlays: &[DxfOverlay], candidates: &[Candidate],
        ranges: Vec<std::ops::Range<usize>>,
    ) {
        // First bind the cached rows to the exact sources that produced these
        // hits. A repeated line with identical candidates is still a new list:
        // its group membership and expanded state must both be replaced.
        self.clear_groups();
        self.sync(generation, overlays, candidates);
        for range in ranges {
            let Some(members) = candidates.get(range) else { continue; };
            if members.len() < 2 { continue; }
            let id = self.groups.len();
            self.groups.push(Group::default());
            for &candidate in members { self.group_membership.entry(candidate).or_insert(id); }
        }
        self.groups_changed = true;
        self.sync(generation, overlays, candidates);
    }
    fn sync(&mut self, generation: u64, overlays: &[DxfOverlay], candidates: &[Candidate]) {
        let same_sources = self.generation == Some(generation)
            && self.sources.iter().all(|(index, stamp)| overlays.get(*index).is_some_and(|o| stamp.matches(o)));
        if !same_sources { self.clear_groups(); }
        if same_sources && self.candidates == candidates && !self.groups_changed { return; }
        self.generation = Some(generation);
        self.candidates.clear();
        self.candidates.extend_from_slice(candidates);
        self.rows.clear();
        for group in &mut self.groups {
            group.members.clear();
            group.label = None;
        }
        for &candidate in candidates {
            if let Some(&id) = self.group_membership.get(&candidate) {
                if let Some(group) = self.groups.get_mut(id) { group.members.push(candidate); }
            }
        }
        self.rebuild_entries(false);
        self.groups_changed = false;
        self.row_layout = None;
        self.sources = candidates.iter().map(|&(overlay, _)| overlay).collect::<BTreeSet<_>>()
            .into_iter().filter_map(|index| overlays.get(index).map(|o| (index, SourceStamp::capture(o)))).collect();
        self.reveal_selection = true;
    }
    fn rebuild_entries(&mut self, preserve_heights: bool) {
        let heights: HashMap<_, _> = if preserve_heights {
            self.entries.iter().copied().zip(self.row_heights.iter().copied()).collect()
        } else { HashMap::new() };
        self.entries.clear();
        for &candidate in &self.candidates {
            if let Some((&id, group)) = self.group_membership.get(&candidate)
                .and_then(|id| self.groups.get(*id).map(|group| (id, group)))
                .filter(|(_, group)| group.members.len() > 1)
            {
                if group.members.first() == Some(&candidate) {
                    self.entries.push(Entry::Group(id));
                    if self.expanded_groups.contains(&id) {
                        self.entries.extend(group.members.iter().copied().map(Entry::Object));
                    }
                }
            } else {
                self.entries.push(Entry::Object(candidate));
            }
        }
        self.row_heights = self.entries.iter().map(|entry| heights.get(entry).copied().flatten()).collect();
    }
    fn toggle_group(&mut self, id: usize) {
        if !self.expanded_groups.remove(&id) { self.expanded_groups.insert(id); }
        self.rebuild_entries(true);
    }
    fn selected_entry(&self, selected: Candidate) -> Option<usize> {
        self.entries.iter().position(|entry| *entry == Entry::Object(selected)).or_else(|| {
            let &id = self.group_membership.get(&selected)?;
            self.groups.get(id)?.members.contains(&selected).then(||
                self.entries.iter().position(|entry| *entry == Entry::Group(id))).flatten()
        })
    }
}

fn group_label(overlays: &[DxfOverlay], members: &[Candidate]) -> String {
    // Count the effective layer (including block inheritance), with full names
    // as keys. Shortening display text must never merge two distinct layers.
    let mut counts = BTreeMap::<&str, (usize, usize)>::new();
    for &(overlay, feature) in members {
        if let Some(feature) = overlays.get(overlay).and_then(|o| o.inspect_feature(feature)) {
            let order = counts.len();
            let count = counts.entry(feature.layer).or_insert((order, 0));
            count.1 += 1;
        }
    }
    let mut layers: Vec<_> = counts.into_iter().collect();
    layers.sort_by_key(|(_, (order, _))| *order);
    layers.into_iter().map(|(layer, (_, count))| format!("{count}× {}",
        shortened(if layer.is_empty() { "(geen laagnaam)" } else { layer }, 160)))
        .collect::<Vec<_>>().join("\n")
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
        Some(Self {
            layer: shortened(if feature.layer.is_empty() { "(geen laagnaam)" } else { feature.layer }, 160),
            infracad: infracad(overlay, index).map(preview),
        })
    }
}

fn row_fonts(ui: &egui::Ui) -> (egui::FontId, egui::FontId) {
    let body = egui::TextStyle::Body.resolve(ui.style());
    let named = egui::FontFamily::Name("SleufBase title".into());
    let bold = egui::FontId::new(body.size, if ui.fonts(|f| f.families().contains(&named)) {
        named
    } else { body.family.clone() });
    (bold, body)
}
fn estimated_row_height(ui: &egui::Ui) -> f32 {
    let (bold, body) = row_fonts(ui);
    ui.fonts(|fonts| fonts.row_height(&bold).ceil()
        + fonts.row_height(&body).ceil()) + 16.
}
struct RowLayout {
    blocks: Vec<(Arc<egui::Galley>, Color32)>,
    height: f32,
}
fn row_layout(ui: &egui::Ui, row: &Row, width: f32) -> RowLayout {
    let (bold, body) = row_fonts(ui);
    let mut blocks = Vec::with_capacity(2);
    let mut height = 14.;
    for (text, font, lines, color) in [
        (row.layer.as_str(), bold, 2, ui.visuals().text_color()),
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
fn group_layout(ui: &egui::Ui, label: &str, width: f32) -> RowLayout {
    let color = ui.visuals().text_color();
    let job = egui::text::LayoutJob::simple(label.to_owned(), row_fonts(ui).0,
        color, (width - 38.).max(1.));
    let galley = ui.fonts(|fonts| fonts.layout_job(job));
    RowLayout { height: (galley.size().y + 14.).ceil().max(30.), blocks: vec![(galley, color)] }
}
fn row_widget(ui: &mut egui::Ui, active: bool, layout: RowLayout, expanded: Option<bool>) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), layout.height), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let highlighted = response.hovered() || response.has_focus();
        ui.painter().rect(rect, 10_u8,
            if active { Color32::from_rgb(255, 224, 199) } else if highlighted { APP_BG } else { Color32::WHITE },
            egui::Stroke::new(if active { 2_f32 } else { 1_f32 },
                if active || highlighted { ORANGE } else { theme::BORDER }), egui::StrokeKind::Inside);
        let mut position = rect.min + egui::vec2(10., 7.);
        if let Some(expanded) = expanded {
            let center = rect.min + egui::vec2(16., 15.);
            let points = if expanded { [egui::vec2(-4., -2.), egui::vec2(0., 2.), egui::vec2(4., -2.)] }
                else { [egui::vec2(-2., -4.), egui::vec2(2., 0.), egui::vec2(-2., 4.)] };
            ui.painter().add(egui::Shape::line(points.into_iter().map(|offset| center + offset).collect(),
                egui::Stroke::new(1.5, ui.visuals().text_color())));
            position.x += 18.;
        }
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
    let mut toggle_group = None;
    egui::SidePanel::right("cable-object-list").resizable(true)
        .default_width(330_f32.min(maximum)).width_range(minimum..=maximum)
        .frame(egui::Frame::new().fill(APP_BG).inner_margin(egui::Margin {
            left: 8, right: 18, top: 18, bottom: 18,
        })).show(ctx, |ui| {
            // Match the map's title block: its normal item spacing already
            // aligns the first card with the neighbouring white map container.
            theme::title(ui, "Kabelobjecten");
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
                let (bold, body) = row_fonts(ui);
                let stamp = [width, ui.ctx().pixels_per_point(), bold.size, body.size];
                if state.row_layout != Some(stamp) {
                    state.row_layout = Some(stamp);
                    state.row_heights.fill(None);
                    // Replacing measured heights shifts every following row.
                    // Reveal again after the resized viewport has been measured.
                    state.reveal_selection = true;
                    ui.ctx().request_repaint();
                }
                let origin = ui.min_rect().min;
                let target = selected.and_then(|value| state.selected_entry(value)).unwrap_or(0);
                let mut target_rect = None;
                let mut top = 0.;
                // Cache only cheap heights for offscreen rows. Building and
                // measuring their text remains limited to the visible viewport.
                for row_index in 0..state.entries.len() {
                    let entry = state.entries[row_index];
                    let estimate = if matches!(entry, Entry::Group(_)) { 30. } else { estimated_height };
                    let mut height = state.row_heights[row_index].unwrap_or(estimate);
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
                    let (layout, active, indent, expanded) = match entry {
                        Entry::Object(candidate) => {
                            let Some(overlay) = overlays.get(candidate.0) else { top += height + spacing; continue; };
                            if !state.rows.contains_key(&candidate) {
                                if state.rows.len() >= MAX_CACHED_ROWS { state.rows.clear(); }
                                let Some(row) = Row::build(overlay, candidate.1) else { top += height + spacing; continue; };
                                state.rows.insert(candidate, row);
                            }
                            let indent = if state.group_membership.get(&candidate)
                                .and_then(|id| state.groups.get(*id)).is_some_and(|group| group.members.len() > 1)
                                { 12. } else { 0. };
                            (row_layout(ui, &state.rows[&candidate], width - indent),
                                selected == Some(candidate), indent, None)
                        }
                        Entry::Group(id) => {
                            let group = &mut state.groups[id];
                            let label = group.label.get_or_insert_with(|| group_label(overlays, &group.members));
                            (group_layout(ui, label, width),
                                selected.is_some_and(|candidate| group.members.contains(&candidate)),
                                0., Some(state.expanded_groups.contains(&id)))
                        }
                    };
                    height = layout.height;
                    state.row_heights[row_index] = Some(height);
                    let rect = egui::Rect::from_min_size(origin + egui::vec2(indent, top), egui::vec2(width - indent, height));
                    let mut row_ui = ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt(entry)
                        .layout(egui::Layout::top_down(egui::Align::Min)));
                    let response = row_widget(&mut row_ui, active, layout, expanded);
                    if reveal && row_index == target { target_rect = Some(rect); }
                    top += height + spacing;
                    match entry {
                        Entry::Object(candidate) => {
                            let row = &state.rows[&candidate];
                            let overlay = &overlays[candidate.0];
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
                        Entry::Group(id) => {
                            let label = state.groups[id].label.as_deref().unwrap_or_default();
                            let expanded = state.expanded_groups.contains(&id);
                            response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::CollapsingHeader,
                                ui.is_enabled(), expanded, label));
                            if response.clicked() { toggle_group = Some(id); }
                            response.on_hover_ui(|ui| {
                                ui.set_max_width(420.);
                                ui.add(egui::Label::new(label).wrap());
                                ui.small(if expanded { "Klik om deze kabelobjecten in te klappen." }
                                    else { "Klik om de afzonderlijke kabelobjecten te tonen en te selecteren." });
                            });
                        }
                    }
                }
                ui.set_min_height((top - spacing).max(0.));
                if let Some(rect) = target_rect { ui.scroll_to_rect(rect, None); }
            });
        });
    if let Some(id) = toggle_group {
        state.toggle_group(id);
        ctx.request_repaint();
    }
    choose
}

#[cfg(test)]
#[path = "ui_cable_list_tests.rs"]
mod tests;
