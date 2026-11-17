//! Local, topic-based help for the working map and the existing desktop flows.
use super::{egui, theme, Color32, INK, ORANGE};

#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    query: String,
    selected: usize,
    focus_search: bool,
    reset_scroll: bool,
    copied_topic: Option<usize>,
}

impl State {
    pub fn open(&mut self) {
        self.open = true;
        self.focus_search = true;
    }
}

struct Topic {
    title: &'static str,
    summary: &'static str,
    keywords: &'static str,
    sections: &'static [(&'static str, &'static str)],
}

#[path = "ui_help_topics.rs"]
mod topics;
use topics::TOPICS;

fn matching_topics(query: &str) -> Vec<usize> {
    let words = query.to_lowercase();
    let words: Vec<_> = words.split_whitespace().collect();
    TOPICS.iter().enumerate().filter_map(|(index, topic)| {
        if words.is_empty() {
            return Some(index);
        }
        let mut text = format!("{} {} {}", topic.title, topic.summary, topic.keywords);
        for (heading, body) in topic.sections {
            text.push(' ');
            text.push_str(heading);
            text.push(' ');
            text.push_str(body);
            text.push(' ');
        }
        let text = text.to_lowercase();
        words.iter().all(|word| text.contains(*word)).then_some(index)
    }).collect()
}

fn topic_text(index: usize) -> String {
    let topic = &TOPICS[index];
    let mut text = format!("{}\n\n{}\n", topic.title, topic.summary);
    for (heading, body) in topic.sections {
        text.push_str(&format!("\n{heading}\n{body}\n"));
    }
    text
}

/// Match the white content sections used in Instellingen.
fn section_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(egui::Stroke::new(1_f32, Color32::from_rgb(208, 218, 231)))
        .corner_radius(10)
        .inner_margin(16)
}

fn topic_list(ui: &mut egui::Ui, matches: &[usize], selected: &mut usize, height: f32) -> bool {
    let mut changed = false;
    egui::ScrollArea::vertical()
        .id_salt("sleufbase-help-topics")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .max_height(height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // The surrounding two-column row is horizontal. Always give this
            // scroll content its own vertical layout, including the topic rows.
            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                ui.set_width(ui.available_width());
                section_frame().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 6.;
                    theme::strong(ui, "Onderwerpen");
                    ui.small(format!("{} onderwerpen", matches.len()));
                    ui.add_space(4.);
                    for index in matches.iter().copied() {
                        let active = *selected == index;
                        let text = egui::RichText::new(TOPICS[index].title)
                            .color(if active { Color32::WHITE } else { INK });
                        let button = egui::Button::new(text)
                            .selected(active)
                            .fill(if active { ORANGE } else { Color32::WHITE })
                            .corner_radius(6)
                            .wrap();
                        if ui.add_sized([ui.available_width(), 34.], button)
                            .on_hover_text(TOPICS[index].summary).clicked()
                        {
                            changed = *selected != index;
                            *selected = index;
                        }
                    }
                });
            });
        });
    changed
}

fn topic_content(ui: &mut egui::Ui, index: usize, height: f32, reset_scroll: bool) {
    let scroll = egui::ScrollArea::vertical()
        .id_salt(("sleufbase-help-topic", index))
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .max_height(height)
        .auto_shrink([false, false]);
    let scroll = if reset_scroll { scroll.vertical_scroll_offset(0.) } else { scroll };
    scroll.show(ui, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                ui.set_width(ui.available_width());
                let topic = &TOPICS[index];
                section_frame().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 10.;
                    ui.heading(topic.title);
                    ui.label(egui::RichText::new(topic.summary).color(theme::MUTED));
                    ui.separator();
                    for (heading, body) in topic.sections {
                        theme::strong(ui, *heading);
                        ui.label(*body);
                        ui.add_space(6.);
                    }
                });
            });
        });
}

fn help_contents(ui: &mut egui::Ui, state: &mut State) {
    let wide = ui.available_width() >= 660.;
    ui.label(if wide {
        "Kies links een onderwerp of zoek op de naam van een functie."
    } else {
        "Kies een onderwerp of zoek op de naam van een functie."
    });
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 30.),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
        if ui.add_enabled(!state.query.is_empty(), egui::Button::new("Wis")).clicked() {
            state.query.clear();
            state.focus_search = true;
            state.reset_scroll = true;
        }
        let response = ui.add_sized(
            [ui.available_width(), 30.],
            egui::TextEdit::singleline(&mut state.query)
                .id(egui::Id::new("sleufbase-help-search"))
                .hint_text("Zoek bijvoorbeeld: naambordjes, MarXact, diameter…"),
        );
        if state.focus_search {
            response.request_focus();
            state.focus_search = false;
        }
        if response.changed() {
            state.reset_scroll = true;
        }
    });
    ui.add_space(6.);

    let matches = matching_topics(&state.query);
    if matches.is_empty() {
        section_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::strong(ui, "Geen onderwerp gevonden");
            ui.label("Probeer een kortere zoekterm, zoals kaart, inloggen, kabel of export.");
            if ui.button("Alle onderwerpen tonen").clicked() {
                state.query.clear();
                state.reset_scroll = true;
                state.focus_search = true;
            }
        });
        return;
    }
    if !matches.contains(&state.selected) {
        state.selected = matches[0];
        state.reset_scroll = true;
        state.copied_topic = None;
    }
    let height = ui.available_height().max(40.);
    if wide {
        // Reserve both columns before drawing either of them. The topic list
        // cannot consume the explanation column or shrink later buttons to
        // single-letter lines when it is nested in this horizontal row.
        let list_width = 244.;
        let gap = 16.;
        let content_width = ui.available_width() - list_width - gap;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            ui.allocate_ui_with_layout(
                egui::vec2(list_width, height),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.set_width(list_width);
                    if topic_list(ui, &matches, &mut state.selected, height) {
                        state.reset_scroll = true;
                        state.copied_topic = None;
                    }
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(content_width, height),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.set_width(content_width);
                    topic_content(ui, state.selected, height, std::mem::take(&mut state.reset_scroll));
                },
            );
        });
    } else {
        egui::ComboBox::from_id_salt("sleufbase-help-topic-picker")
            .selected_text(TOPICS[state.selected].title)
            .width(ui.available_width())
            .truncate()
            .show_ui(ui, |ui| {
                for index in matches {
                    if ui.selectable_value(&mut state.selected, index, TOPICS[index].title).changed() {
                        state.reset_scroll = true;
                        state.copied_topic = None;
                    }
                }
            });
        let content_height = ui.available_height().max(40.);
        topic_content(ui, state.selected, content_height, std::mem::take(&mut state.reset_scroll));
    }
}

pub(super) fn show(ctx: &egui::Context, state: &mut State) {
    if !state.open {
        return;
    }
    let viewport = ctx.screen_rect();
    let maximum = egui::vec2((viewport.width() - 64.).max(280.), (viewport.height() - 88.).max(200.));
    let default_size = egui::vec2(1080., 700.).min(maximum);
    let mut open = true;
    let mut close = false;
    egui::Window::new("Info en handleiding · SleufBase")
        .id(egui::Id::new("sleufbase-help"))
        .default_pos(viewport.center() - default_size * 0.5)
        .default_size(default_size)
        .min_size(egui::vec2(480., 360.).min(maximum))
        .max_size(maximum)
        .constrain_to(viewport.shrink(16.))
        .open(&mut open)
        .movable(true)
        .collapsible(false)
        .resizable(true)
        .show(ctx, |ui| theme::bounded_window_body(ui, |ui| {
            super::settings_ui::control_style(ui);
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            // Match the settings footer: it remains outside both scroll areas.
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                ui.horizontal_wrapped(|ui| {
                    close = ui.button("Sluiten").clicked();
                    let copied = state.copied_topic == Some(state.selected);
                    let can_copy = matching_topics(&state.query).contains(&state.selected);
                    if ui.add_enabled(can_copy, egui::Button::new(
                        if copied { "Uitleg gekopieerd" } else { "Uitleg kopiëren" }))
                        .on_hover_text("Kopieer het gekozen onderwerp om de uitleg te bewaren of te delen.")
                        .clicked()
                    {
                        ctx.copy_text(topic_text(state.selected));
                        state.copied_topic = Some(state.selected);
                    }
                    theme::strong(ui, "Gemaakt door Luka Stuurman");
                    ui.colored_label(theme::MUTED,
                        format!("SleufBase Rust {}", env!("CARGO_PKG_VERSION")));
                });
                ui.separator();
                ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                    help_contents(ui, state);
                });
            });
        }));
    state.open = open && !close;
}
