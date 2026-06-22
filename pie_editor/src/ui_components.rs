//! Reusable UI primitives for the Pie Editor premium design system v3.
//!
//! Provides consistent styling helpers for section headers, cards, buttons,
//! chips, property rows, and other common editor UI patterns.
//!
//! v3 changes:
//! - Richer surface composition with painter-based depth cues
//! - Viewport shell with inner shadow framing for depth
//! - More deliberate control silhouettes with better spacing rhythm
//! - Polished empty states with layered circle + icon composition
//! - Status bar with proper visual weight and readable typography

use egui::{Color32, Frame, Margin, Response, Stroke, Ui, Widget};

use crate::theme;

// ═══════════════════════════════════════════════════════════════════════════════
//  Section Header
// ═══════════════════════════════════════════════════════════════════════════════

/// A compact section header bar with a label and optional right-aligned content.
/// Uses a subtle left accent bar and uppercase label for clear hierarchy.
pub fn section_header(ui: &mut Ui, label: &str, add_right: impl FnOnce(&mut Ui)) {
    Frame {
        inner_margin: Margin::symmetric(10, 5),
        outer_margin: Margin::ZERO,
        corner_radius: theme::RADIUS_NONE,
        shadow: egui::Shadow::NONE,
        fill: theme::BG_SECTION_HEADER,
        stroke: Stroke::new(1.0, theme::SEPARATOR),
    }
    .show(ui, |ui| {
        ui.horizontal(|ui| {
            // Branded accent indicator — thin vertical bar
            let bar_width = 2.0;
            let (_, painter) = ui.allocate_painter(
                egui::vec2(bar_width, ui.available_height()),
                egui::Sense::hover(),
            );
            painter.rect_filled(
                egui::Rect::from_min_size(
                    ui.min_rect().min,
                    egui::vec2(bar_width, ui.available_height()),
                ),
                theme::RADIUS_NONE,
                theme::ACCENT_PRIMARY,
            );

            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(label.to_uppercase())
                    .color(theme::TEXT_SECONDARY)
                    .size(theme::FONT_SIZE_SM)
                    .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                add_right(ui);
            });
        });
    });
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Card Container
// ═══════════════════════════════════════════════════════════════════════════════

/// A card-like container with elevated surface and subtle border.
pub fn card_frame() -> Frame {
    Frame {
        inner_margin: Margin::same(10),
        outer_margin: Margin::symmetric(0, 3),
        corner_radius: theme::RADIUS_SM,
        shadow: egui::Shadow::NONE,
        fill: theme::BG_ELEVATED,
        stroke: Stroke::new(1.0, theme::BORDER_SUBTLE),
    }
}

/// Show a card with content.
pub fn card(ui: &mut Ui, add_content: impl FnOnce(&mut Ui)) -> Response {
    card_frame().show(ui, add_content).response
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Buttons
// ═══════════════════════════════════════════════════════════════════════════════

/// A ghost button — no background, just text. Good for menu bar items.
pub fn ghost_button(ui: &mut Ui, text: &str) -> Response {
    egui::Button::new(
        egui::RichText::new(text)
            .color(theme::TEXT_SECONDARY)
            .size(theme::FONT_SIZE_SM),
    )
    .corner_radius(theme::RADIUS_SM)
    .fill(Color32::TRANSPARENT)
    .stroke(Stroke::NONE)
    .ui(ui)
}

/// A small icon-like button for compact toolbars.
pub fn tool_button(ui: &mut Ui, text: &str, active: bool) -> Response {
    let (bg, stroke, text_color) = if active {
        (
            theme::ACCENT_PRIMARY.linear_multiply(0.2),
            Stroke::new(1.0, theme::ACCENT_PRIMARY),
            theme::ACCENT_PRIMARY_LIGHT,
        )
    } else {
        (
            theme::BG_WIDGET,
            Stroke::new(1.0, theme::BORDER_STANDARD),
            theme::TEXT_SECONDARY,
        )
    };
    egui::Button::new(
        egui::RichText::new(text)
            .color(text_color)
            .size(theme::FONT_SIZE_BASE),
    )
    .corner_radius(theme::RADIUS_SM)
    .fill(bg)
    .stroke(stroke)
    .min_size(egui::vec2(28.0, 24.0))
    .ui(ui)
}

/// A play button — prominent green accent with clear active state.
pub fn play_button(ui: &mut Ui, text: &str, active: bool) -> Response {
    let (bg, stroke, text_color) = if active {
        (
            theme::ACCENT_PLAY.linear_multiply(0.22),
            Stroke::new(1.0, theme::ACCENT_PLAY),
            theme::ACCENT_PLAY,
        )
    } else {
        (
            theme::BG_WIDGET,
            Stroke::new(1.0, theme::BORDER_STANDARD),
            theme::ACCENT_PLAY,
        )
    };
    egui::Button::new(
        egui::RichText::new(text)
            .color(text_color)
            .size(theme::FONT_SIZE_BASE)
            .strong(),
    )
    .corner_radius(theme::RADIUS_SM)
    .fill(bg)
    .stroke(stroke)
    .min_size(egui::vec2(32.0, 24.0))
    .ui(ui)
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Chips / Tags / Pills
// ═══════════════════════════════════════════════════════════════════════════════

/// A chip with a colored dot indicator.
pub fn status_chip(ui: &mut Ui, text: &str, dot_color: Color32) -> Response {
    Frame {
        inner_margin: Margin::symmetric(6, 2),
        outer_margin: Margin::ZERO,
        corner_radius: theme::RADIUS_FULL,
        shadow: egui::Shadow::NONE,
        fill: theme::BG_CHIP,
        stroke: Stroke::new(1.0, theme::BORDER_SUBTLE),
    }
    .show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("●")
                    .color(dot_color)
                    .size(7.0),
            );
            ui.add_space(3.0);
            ui.label(
                egui::RichText::new(text)
                    .color(theme::TEXT_CHIP)
                    .size(theme::FONT_SIZE_XS),
            );
        });
    })
    .response
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Property Row
// ═══════════════════════════════════════════════════════════════════════════════

/// A labeled property row with a right-aligned value widget.
pub fn property_row(ui: &mut Ui, label: &str, add_value: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label)
                .color(theme::TEXT_SECONDARY)
                .size(theme::FONT_SIZE_BASE),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            add_value(ui);
        });
    });
}

/// A labeled property row with colored axis labels (X/Y/Z).
pub fn axis_row(
    ui: &mut Ui,
    label: &str,
    x: &mut f32,
    y: &mut f32,
    z: &mut f32,
    speed: f32,
) {
    ui.label(
        egui::RichText::new(label)
            .color(theme::TEXT_TERTIARY)
            .size(theme::FONT_SIZE_SM)
            .strong(),
    );
    ui.horizontal(|ui| {
        axis_drag_value(ui, "X", x, theme::AXIS_X, speed);
        axis_drag_value(ui, "Y", y, theme::AXIS_Y, speed);
        axis_drag_value(ui, "Z", z, theme::AXIS_Z, speed);
    });
}

fn axis_drag_value(ui: &mut Ui, label: &str, value: &mut f32, color: Color32, speed: f32) {
    ui.label(
        egui::RichText::new(label)
            .color(color)
            .size(theme::FONT_SIZE_SM)
            .monospace()
            .strong(),
    );
    ui.add(egui::DragValue::new(value).speed(speed).min_decimals(2));
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Separator
// ═══════════════════════════════════════════════════════════════════════════════

/// A styled separator with some vertical breathing room.
pub fn styled_separator(ui: &mut Ui) {
    ui.add_space(theme::SPACE_XS);
    ui.separator();
    ui.add_space(theme::SPACE_XS);
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Empty State
// ═══════════════════════════════════════════════════════════════════════════════

/// A centered empty state with icon and subtitle.
/// Polished and intentional — uses layered circles for depth.
pub fn empty_state(ui: &mut Ui, icon: &str, title: &str, subtitle: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(theme::SPACE_XL);

        // Outer ring — elevated surface
        let outer_size = 44.0;
        let (_, painter) = ui.allocate_painter(
            egui::vec2(outer_size, outer_size),
            egui::Sense::hover(),
        );
        let center = ui.min_rect().min + egui::vec2(outer_size / 2.0, outer_size / 2.0);

        // Outer circle fill
        painter.circle_filled(center, outer_size / 2.0, theme::BG_ELEVATED);
        // Outer circle border
        painter.circle_stroke(
            center,
            outer_size / 2.0,
            Stroke::new(1.0, theme::BORDER_SUBTLE),
        );
        // Inner subtle ring for depth
        painter.circle_stroke(
            center,
            outer_size / 2.0 - 4.0,
            Stroke::new(0.5, theme::SEPARATOR),
        );

        ui.add_space(theme::SPACE_SM);

        // Icon
        ui.label(
            egui::RichText::new(icon)
                .color(theme::ACCENT_PRIMARY.linear_multiply(0.7))
                .size(18.0),
        );
        ui.add_space(theme::SPACE_SM);

        // Title
        ui.label(
            egui::RichText::new(title)
                .color(theme::TEXT_SECONDARY)
                .size(theme::FONT_SIZE_MD),
        );
        ui.add_space(theme::SPACE_XS);

        // Subtitle
        ui.label(
            egui::RichText::new(subtitle)
                .color(theme::TEXT_DIM)
                .size(theme::FONT_SIZE_SM),
        );
        ui.add_space(theme::SPACE_XL);
    });
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Frame Presets
// ═══════════════════════════════════════════════════════════════════════════════

/// Frame for the top toolbar area.
pub fn toolbar_frame() -> Frame {
    Frame {
        inner_margin: Margin::symmetric(10, 4),
        outer_margin: Margin::ZERO,
        corner_radius: theme::RADIUS_NONE,
        shadow: egui::Shadow::NONE,
        fill: theme::BG_CHROME,
        stroke: Stroke::new(1.0, theme::SEPARATOR),
    }
}

/// Frame for the menu bar area.
pub fn menubar_frame() -> Frame {
    Frame {
        inner_margin: Margin::symmetric(8, 3),
        outer_margin: Margin::ZERO,
        corner_radius: theme::RADIUS_NONE,
        shadow: egui::Shadow::NONE,
        fill: theme::BG_CHROME,
        stroke: Stroke::new(1.0, theme::SEPARATOR),
    }
}

/// Frame for the status bar area.
pub fn statusbar_frame() -> Frame {
    Frame {
        inner_margin: Margin::symmetric(10, 4),
        outer_margin: Margin::ZERO,
        corner_radius: theme::RADIUS_NONE,
        shadow: egui::Shadow::NONE,
        fill: theme::BG_CHROME,
        stroke: Stroke::new(1.0, theme::SEPARATOR),
    }
}

/// Frame for the viewport area — with optional border color.
pub fn viewport_frame(border_color: Color32) -> Frame {
    Frame {
        inner_margin: Margin::ZERO,
        outer_margin: Margin::ZERO,
        corner_radius: theme::RADIUS_NONE,
        shadow: egui::Shadow::NONE,
        fill: theme::BG_VIEWPORT,
        stroke: Stroke::new(1.0, border_color),
    }
}

/// A panel surface frame with clear boundaries.
pub fn panel_frame() -> Frame {
    Frame {
        inner_margin: Margin::ZERO,
        outer_margin: Margin::ZERO,
        corner_radius: theme::RADIUS_NONE,
        shadow: egui::Shadow::NONE,
        fill: theme::BG_SURFACE,
        stroke: Stroke::new(1.0, theme::SEPARATOR),
    }
}

/// A subtle divider between panel sections.
pub fn panel_divider(ui: &mut Ui) {
    ui.add_space(1.0);
    ui.separator();
    ui.add_space(1.0);
}
