//! Compact progress windows shared by loading and export tasks.
//! Counts are only shown when the worker exposes a real completed/total pair.
use super::{egui, theme, Color32, ORANGE};

fn progress_bar(ui: &mut egui::Ui, counts: Option<(usize, usize)>) {
    if let Some((completed, total)) = counts.filter(|(_, total)| *total > 0) {
        let completed = completed.min(total);
        ui.add(
            egui::ProgressBar::new(completed as f32 / total as f32)
                .desired_width(ui.available_width())
                .desired_height(20.)
                .text(format!("{completed} van {total} voltooid")),
        );
    } else {
        // A moving segment conveys activity without claiming an estimated
        // percentage for fetching, geometry preparation or publication.
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().max(1.), 14.),
            egui::Sense::hover(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 7., Color32::from_rgb(226, 233, 242));
        let segment_width = (rect.width() * 0.24).max(12.);
        let phase = ui.input(|input| (input.time * 0.55).rem_euclid(1.)) as f32;
        let left = rect.left() - segment_width + (rect.width() + segment_width) * phase;
        painter.rect_filled(
            egui::Rect::from_min_size(egui::pos2(left, rect.top()), egui::vec2(segment_width, rect.height())),
            7., ORANGE,
        );
    }
}

/// Return the user's cancellation request; the caller keeps responsibility for
/// processing cancellation before polling and restoring its owned source data.
pub(super) fn window(
    ctx: &egui::Context,
    id: &'static str,
    title: &str,
    status: &str,
    counts: Option<(usize, usize)>,
    can_cancel: bool,
    cancelled: bool,
    details: impl FnOnce(&mut egui::Ui),
) -> bool {
    let available = ctx.screen_rect().shrink(12.);
    let size = egui::vec2(
        (available.width() - 36.).clamp(120., 480.),
        (available.height() - 72.).clamp(96., 224.),
    );
    let mut cancel = can_cancel && !cancelled
        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    egui::Window::new(title)
        .id(egui::Id::new(id))
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .fixed_size(size)
        .movable(false)
        .collapsible(false)
        .resizable(false)
        .constrain_to(available)
        .frame(theme::card(16, 16, 12).fill(theme::APP_BG))
        .show(ctx, |ui| {
            ui.set_max_width(ui.available_width());
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            egui::TopBottomPanel::bottom(egui::Id::new(id).with("actions"))
                .frame(egui::Frame::new())
                .show_inside(ui, |ui| {
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        cancel |= ui.add_enabled(
                            can_cancel && !cancelled,
                            egui::Button::new("Annuleren").min_size(egui::vec2(112., 30.)),
                        ).clicked();
                        ui.small(if cancelled { "Annuleren wordt afgerond…" }
                            else if can_cancel { "Esc om te annuleren" }
                            else { "Afronden…" });
                    });
                });

            progress_bar(ui, counts);
            ui.add_space(8.);
            egui::ScrollArea::vertical()
                .id_salt((id, "details"))
                .max_height(ui.available_height().max(16.))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.strong(if cancelled { "Annuleren wordt afgerond…" } else { status });
                    ui.add_space(6.);
                    details(ui);
                });
        });
    ctx.request_repaint_after(std::time::Duration::from_millis(40));
    cancel
}
