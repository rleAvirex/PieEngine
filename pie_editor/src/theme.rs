//! Pie Editor Theme — Unreal Engine 5 inspired dark theme
//!
//! A professional, dark charcoal UI with subtle blue accents matching
//! the Unreal Engine 5 editor aesthetic. Flat panels, minimal rounding,
//! compact spacing, and a clean industrial look.

use egui::{
    Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, Style, TextStyle, Vec2, Visuals,
};

// -- UE5-inspired Palette --
// Backgrounds (charcoal grays, no blue/purple tint)
pub const BG_WINDOW: Color32 = Color32::from_rgb(21, 21, 21); // #151515 — main window
pub const BG_SIDEBAR: Color32 = Color32::from_rgb(30, 30, 30); // #1E1E1E — panels
pub const BG_TOOLBAR: Color32 = Color32::from_rgb(35, 35, 35); // #232323 — toolbar
pub const BG_VIEWPORT: Color32 = Color32::from_rgb(12, 12, 12); // #0C0C0C — darkest
pub const BG_WIDGET: Color32 = Color32::from_rgb(42, 42, 42); // #2A2A2A — inputs
pub const BG_WIDGET_HOVER: Color32 = Color32::from_rgb(52, 52, 52); // #343434 — hover

// Accents (UE5 blue-orange system)
pub const ACCENT_PRIMARY: Color32 = Color32::from_rgb(46, 120, 210); // UE5 blue
pub const ACCENT_SECONDARY: Color32 = Color32::from_rgb(120, 175, 255); // light blue
pub const ACCENT_SUCCESS: Color32 = Color32::from_rgb(60, 200, 110); // green
pub const ACCENT_PLAY: Color32 = Color32::from_rgb(60, 200, 110); // play green
pub const ACCENT_DANGER: Color32 = Color32::from_rgb(220, 60, 60); // red
pub const ACCENT_ORANGE: Color32 = Color32::from_rgb(255, 140, 30); // UE5 orange accent

// Text (high contrast on dark)
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(220, 220, 220); // #DCDCDC
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(155, 155, 155); // #9B9B9B
pub const TEXT_DIM: Color32 = Color32::from_rgb(90, 90, 90); // #5A5A5A

// Borders & separators (subtle gray)
pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(50, 50, 50); // #323232
pub const BORDER_STRONG: Color32 = Color32::from_rgb(70, 70, 70); // #464646
pub const SEPARATOR: Color32 = Color32::from_rgb(50, 50, 50); // #323232

// Selection (UE5 blue)
pub const SELECTION_BG: Color32 = Color32::from_rgb(30, 55, 90); // dark blue tint
pub const SELECTION_BORDER: Color32 = ACCENT_PRIMARY;

// Viewport border
pub const VIEWPORT_BORDER: Color32 = Color32::from_rgb(50, 50, 50);
pub const VIEWPORT_BORDER_ACTIVE: Color32 = ACCENT_PRIMARY;

// Section header background
pub const BG_SECTION: Color32 = Color32::from_rgb(40, 40, 40); // #282828

// -- CornerRadius (UE5 = flat, no rounding) --
pub const ROUNDING_SM: CornerRadius = CornerRadius::same(2);
pub const ROUNDING_MD: CornerRadius = CornerRadius::same(2);

// -- Spacing (compact like UE5) --
pub const SPACING_XS: f32 = 2.0;
pub const SPACING_SM: f32 = 4.0;
pub const SPACING_MD: f32 = 6.0;
pub const SPACING_LG: f32 = 10.0;

/// Apply the full Pie Editor theme to an egui context.
pub fn apply(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();

    // Window / panel backgrounds
    visuals.widgets.noninteractive.bg_fill = BG_WIDGET;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER_SUBTLE);
    visuals.widgets.noninteractive.corner_radius = ROUNDING_SM;

    // Interactive widgets
    visuals.widgets.inactive.bg_fill = BG_WIDGET;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER_SUBTLE);
    visuals.widgets.inactive.corner_radius = ROUNDING_SM;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_SECONDARY);

    visuals.widgets.hovered.bg_fill = BG_WIDGET_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
    visuals.widgets.hovered.corner_radius = ROUNDING_SM;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);

    visuals.widgets.active.bg_fill = ACCENT_PRIMARY.linear_multiply(0.2);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT_PRIMARY);
    visuals.widgets.active.corner_radius = ROUNDING_SM;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, ACCENT_PRIMARY);

    visuals.widgets.open.bg_fill = BG_WIDGET_HOVER;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
    visuals.widgets.open.corner_radius = ROUNDING_SM;

    // Selection highlight (UE5 blue)
    visuals.selection.bg_fill = SELECTION_BG;
    visuals.selection.stroke = Stroke::new(1.0, SELECTION_BORDER);

    // Window fill
    visuals.window_fill = BG_WINDOW;
    visuals.panel_fill = BG_SIDEBAR;
    visuals.window_stroke = Stroke::new(1.0, SEPARATOR);
    visuals.window_corner_radius = ROUNDING_MD;
    visuals.window_shadow = egui::Shadow {
        offset: [0, 2],
        blur: 8,
        spread: 0,
        color: Color32::from_black_alpha(60),
    };

    // Popup / menu
    visuals.popup_shadow = egui::Shadow {
        offset: [0, 2],
        blur: 8,
        spread: 0,
        color: Color32::from_black_alpha(50),
    };

    // Hyperlink / accent
    visuals.hyperlink_color = ACCENT_PRIMARY;

    // Text colors
    visuals.override_text_color = Some(TEXT_PRIMARY);

    // Faint background for striped rows
    visuals.faint_bg_color = Color32::from_rgb(34, 34, 34);

    // Extreme background (very dark areas)
    visuals.extreme_bg_color = BG_VIEWPORT;

    // Clip rectangle rounding
    visuals.clip_rect_margin = 1.0;

    ctx.set_visuals(visuals);

    // -- Font sizes & style overrides (compact like UE5) --
    let mut style = Style::default();
    style.spacing.item_spacing = Vec2::new(5.0, 3.0);
    style.spacing.button_padding = Vec2::new(6.0, 3.0);
    style.spacing.indent = 12.0;
    style.spacing.interact_size = Vec2::new(0.0, 24.0);
    style.spacing.icon_width = 12.0;
    style.spacing.menu_margin = Margin::same(4);
    style.spacing.window_margin = Margin::same(2);

    // Text styles (smaller, tighter like UE5)
    let mut text_styles = [
        (
            TextStyle::Heading,
            FontId::new(14.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(12.0, FontFamily::Proportional)),
        (
            TextStyle::Monospace,
            FontId::new(11.0, FontFamily::Monospace),
        ),
        (
            TextStyle::Button,
            FontId::new(12.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Small,
            FontId::new(10.0, FontFamily::Proportional),
        ),
    ];
    for (text_style, font_id) in &mut text_styles {
        style
            .text_styles
            .insert(text_style.clone(), font_id.clone());
    }

    ctx.set_style(style);
}

/// The clear color used for the egui paint pass (behind all UI).
pub const CLEAR_COLOR: [f32; 4] = [
    BG_WINDOW.r() as f32 / 255.0,
    BG_WINDOW.g() as f32 / 255.0,
    BG_WINDOW.b() as f32 / 255.0,
    1.0,
];

/// The clear color used for the 3D viewport scene.
pub const VIEWPORT_CLEAR: wgpu::Color = wgpu::Color {
    r: 0.047,
    g: 0.047,
    b: 0.047,
    a: 1.0,
};
