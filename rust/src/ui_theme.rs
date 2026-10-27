//! Effective original Tk palette and logical pixel sizes (Segoe UI 10 pt).
use super::*;

pub(super) const APP_BG: Color32 = Color32::from_rgb(245, 247, 251);
pub(super) const INPUT_BG: Color32 = Color32::from_rgb(251, 252, 254);
pub(super) const SURFACE_ALT: Color32 = Color32::from_rgb(250, 251, 253);
pub(super) const BORDER: Color32 = Color32::from_rgb(233, 238, 245);
pub(super) const MUTED: Color32 = Color32::from_rgb(102, 112, 133);
pub(super) const FONT_SIZE: f32 = 40. / 3.;

pub(super) fn viewport() -> egui::ViewportBuilder {
    let bytes = include_bytes!("../../assets/sleufbase_icon.ico");
    let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
    let icon = (0..count).rev().find_map(|index| {
        let entry = bytes.get(6 + index * 16..6 + (index + 1) * 16)?;
        let length = u32::from_le_bytes(entry[8..12].try_into().ok()?) as usize;
        let offset = u32::from_le_bytes(entry[12..16].try_into().ok()?) as usize;
        let png = bytes.get(offset..offset.checked_add(length)?)?;
        eframe::icon_data::from_png_bytes(png).ok()
    });
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([1500., 930.])
        .with_min_inner_size([1200., 760.]);
    if let Some(icon) = icon {
        viewport.with_icon(icon)
    } else {
        viewport
    }
}

pub(super) fn install(ctx: &egui::Context) {
    // The original uses the Windows system font. Do not distribute font files.
    let mut fonts = egui::FontDefinitions::default();
    #[cfg(windows)]
    if let Some(root) = std::env::var_os("WINDIR") {
        if let Ok(bytes) = std::fs::read(PathBuf::from(root).join("Fonts/segoeui.ttf")) {
            fonts
                .font_data
                .insert("Segoe UI".into(), egui::FontData::from_owned(bytes).into());
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "Segoe UI".into());
        }
    }
    let mut bold = fonts.families[&egui::FontFamily::Proportional].clone();
    #[cfg(windows)]
    if let Some(root) = std::env::var_os("WINDIR") {
        if let Ok(bytes) = std::fs::read(PathBuf::from(root).join("Fonts/segoeuib.ttf")) {
            fonts.font_data.insert(
                "Segoe UI Bold".into(),
                egui::FontData::from_owned(bytes).into(),
            );
            bold.insert(0, "Segoe UI Bold".into());
        }
    }
    fonts
        .families
        .insert(egui::FontFamily::Name("SleufBase title".into()), bold);
    ctx.set_fonts(fonts);
    // Tk changes hover states and opens its dialogs immediately.
    let mut style = egui::Style {
        animation_time: 0.,
        ..Default::default()
    };
    for size in style.text_styles.values_mut() {
        size.size = FONT_SIZE;
    }
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::proportional(56. / 3.),
    );
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(12.));
    style.spacing.item_spacing = egui::vec2(8., 6.);
    style.spacing.button_padding = egui::vec2(10., 6.);
    style.spacing.interact_size.y = 28.;
    style.spacing.scroll = egui::style::ScrollStyle::solid();
    style.spacing.scroll.bar_width = 12.;
    style.visuals.menu_corner_radius = 12.into();
    style.visuals = egui::Visuals::light();
    style.visuals.override_text_color = Some(INK);
    style.visuals.panel_fill = APP_BG;
    style.visuals.window_fill = APP_BG;
    style.visuals.window_corner_radius = 18.into();
    style.visuals.window_stroke = Stroke::new(1_f32, BORDER);
    style.visuals.faint_bg_color = INPUT_BG;
    style.visuals.extreme_bg_color = INPUT_BG;
    style.visuals.selection.bg_fill = ORANGE;
    style.visuals.selection.stroke = Stroke::new(1_f32, Color32::WHITE);
    for widgets in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.noninteractive,
    ] {
        widgets.bg_fill = SURFACE_ALT;
        widgets.weak_bg_fill = SURFACE_ALT;
        widgets.bg_stroke = Stroke::NONE;
        widgets.fg_stroke = Stroke::new(1_f32, INK);
        widgets.corner_radius = 13.into();
    }
    style.visuals.widgets.inactive.bg_stroke=Stroke::new(1_f32,Color32::from_rgb(190,203,220));
    style.visuals.widgets.inactive.corner_radius=6.into();
    style.spacing.icon_width=18.;
    style.spacing.icon_width_inner=12.;
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(255, 224, 199);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(255, 224, 199);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1_f32, Color32::from_rgb(206, 100, 35));
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1_f32, INK);
    style.visuals.widgets.hovered.corner_radius = 6.into();
    style.visuals.widgets.active = style.visuals.widgets.hovered;
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(255, 203, 162);
    style.visuals.widgets.active.weak_bg_fill = Color32::from_rgb(255, 203, 162);
    style.visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    ctx.set_style(style);
}

pub(super) fn card(radius: u8, x: i8, y: i8) -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1_f32, BORDER))
        .corner_radius(radius)
        .inner_margin(egui::Margin::symmetric(x, y))
}

pub(super) fn title(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: 4,
            right: 0,
            top: 0,
            bottom: 0,
        })
        .show(ui, |ui| {
            let named = egui::FontFamily::Name("SleufBase title".into());
            let family = if ui.fonts(|fonts| fonts.families().contains(&named)) {
                named
            } else {
                egui::FontFamily::Proportional
            };
            ui.label(egui::RichText::new(text).font(egui::FontId::new(FONT_SIZE, family)));
        });
}

pub(super) fn strong(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    let named = egui::FontFamily::Name("SleufBase title".into());
    let family = if ui.fonts(|fonts| fonts.families().contains(&named)) {
        named
    } else {
        egui::FontFamily::Proportional
    };
    ui.label(egui::RichText::new(text).font(egui::FontId::new(FONT_SIZE, family)))
}
