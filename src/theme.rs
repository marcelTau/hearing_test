//! Shared light palette and small, code-drawn components for the desktop UI.
use eframe::egui::{self, Color32, FontId, RichText, Sense, Stroke, Vec2};

pub const BACKGROUND: Color32 = Color32::from_rgb(246, 248, 251);
pub const INK: Color32 = Color32::from_rgb(35, 54, 75);
pub const MUTED: Color32 = Color32::from_rgb(87, 108, 130);
pub const BLUE: Color32 = Color32::from_rgb(38, 100, 158);
pub const BABY_BLUE: Color32 = Color32::from_rgb(224, 240, 252);
pub const LINE: Color32 = Color32::from_rgb(221, 230, 239);
pub const GREEN: Color32 = Color32::from_rgb(37, 113, 89);
pub const GREEN_BG: Color32 = Color32::from_rgb(231, 245, 237);
pub const AMBER: Color32 = Color32::from_rgb(134, 83, 32);
pub const AMBER_BG: Color32 = Color32::from_rgb(255, 246, 229);
pub const RED: Color32 = Color32::from_rgb(169, 59, 68);

pub fn configure(context: &egui::Context) {
    context.set_theme(egui::Theme::Light);
    context.style_mut_of(egui::Theme::Light, |style| {
        style
            .text_styles
            .insert(egui::TextStyle::Heading, FontId::proportional(26.0));
        style
            .text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(15.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(15.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, FontId::proportional(12.0));
        style.spacing.item_spacing = Vec2::new(10.0, 10.0);
        style.spacing.button_padding = Vec2::new(16.0, 10.0);
        style.spacing.interact_size.y = 36.0;
        style.spacing.combo_width = 240.0;
        let visuals = &mut style.visuals;
        visuals.override_text_color = Some(INK);
        visuals.weak_text_color = Some(MUTED);
        visuals.panel_fill = BACKGROUND;
        visuals.window_fill = Color32::WHITE;
        visuals.window_stroke = Stroke::new(1.0, LINE);
        visuals.window_corner_radius = 16.into();
        visuals.menu_corner_radius = 10.into();
        visuals.extreme_bg_color = Color32::WHITE;
        visuals.faint_bg_color = BACKGROUND;
        visuals.selection.bg_fill = BABY_BLUE;
        visuals.selection.stroke = Stroke::new(1.5, BLUE);
        visuals.hyperlink_color = BLUE;
        visuals.warn_fg_color = AMBER;
        visuals.error_fg_color = RED;
        visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
        for widget in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.corner_radius = 6.into();
            widget.bg_stroke = Stroke::new(1.0, LINE);
            widget.fg_stroke = Stroke::new(1.5, INK);
            widget.expansion = 0.0;
        }
        visuals.widgets.inactive.bg_fill = Color32::WHITE;
        visuals.widgets.inactive.weak_bg_fill = Color32::WHITE;
        visuals.widgets.hovered.bg_fill = BABY_BLUE;
        visuals.widgets.hovered.weak_bg_fill = BABY_BLUE;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BLUE);
        visuals.widgets.active.bg_fill = Color32::from_rgb(200, 225, 246);
        visuals.widgets.active.weak_bg_fill = visuals.widgets.active.bg_fill;
        visuals.widgets.active.bg_stroke = Stroke::new(1.5, BLUE);
        visuals.widgets.open.bg_fill = BABY_BLUE;
        visuals.widgets.open.weak_bg_fill = BABY_BLUE;
    });
}

pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(18)
        .inner_margin(20)
}

pub fn primary(text: &str) -> egui::Button<'_> {
    primary_text(RichText::new(text))
}

pub fn primary_text(text: RichText) -> egui::Button<'static> {
    egui::Button::new(text.color(Color32::WHITE).strong())
        .fill(BLUE)
        .stroke(Stroke::NONE)
        .corner_radius(12)
        .min_size(Vec2::new(180.0, 44.0))
}

pub fn muted(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).color(MUTED));
}

pub fn eyebrow(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(12.0).strong().color(BLUE));
}

pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32, fill: Color32) {
    egui::Frame::new()
        .fill(fill)
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(9, 5))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(12.0).strong().color(color));
        });
}

/// Static decorative mark. Its geometry never depends on audio or a trial.
pub fn sound_mark(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let painter = ui.painter_at(rect);
    let center = rect.center();
    painter.circle_filled(center, size * 0.47, BABY_BLUE);
    painter.circle_stroke(
        center,
        size * 0.37,
        Stroke::new(1.0, Color32::from_rgb(184, 216, 241)),
    );
    for (index, height) in [0.18, 0.34, 0.52, 0.72, 0.42, 0.58, 0.26]
        .into_iter()
        .enumerate()
    {
        let x = center.x + (index as f32 - 3.0) * size * 0.075;
        let half = height * size * 0.30;
        painter.line_segment(
            [
                egui::pos2(x, center.y - half),
                egui::pos2(x, center.y + half),
            ],
            Stroke::new(size * 0.035, BLUE),
        );
    }
}
