//! Pie Editor Theme
//!
//! A cohesive dark theme with warm amber accents for Pie Editor.
//! Colors are chosen for readability and a professional game-editor feel.

use egui::{Color32, CornerRadius, Margin, Stroke, Vec2, Visuals, Style, FontId, FontFamily, TextStyle};

// -- Palette --
// Backgrounds
pub const BG_WINDOW: Color32 = Color32::from_rgb(16, 17, 22);       // near-black
pub const BG_SIDEBAR: Color32 = Color32::from_rgb(20, 22, 30);      // deep navy
pub const BG_TOOLBAR: Color32 = Color32::from_rgb(22, 24, 33);      // slightly lighter
pub const BG_VIEWPORT: Color32 = Color32::from_rgb(12, 14, 20);     // darkest
pub const BG_WIDGET: Color32 = Color32::from_rgb(28, 30, 40);       // input bg
pub const BG_WIDGET_HOVER: Color32 = Color32::from_rgb(36, 38, 52);

// Accents
pub const ACCENT_PRIMARY: Color32 = Color32::from_rgb(255, 176, 50);    // warm amber
pub const ACCENT_SECONDARY: Color32 = Color32::from_rgb(80, 160, 255);  // soft blue
pub const ACCENT_SUCCESS: Color32 = Color32::from_rgb(80, 220, 130);    // green
pub const ACCENT_PLAY: Color32 = Color32::from_rgb(70, 200, 120);      // play green
pub const ACCENT_DANGER: Color32 = Color32::from_rgb(255, 90, 90);     // red

// Text
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(225, 228, 235);     // off-white
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(140, 148, 165);  // muted grey
pub const TEXT_DIM: Color32 = Color32::from_rgb(80, 86, 100);          // very muted

// Borders & separators
pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(32, 35, 48);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(50, 55, 75);
pub const SEPARATOR: Color32 = Color32::from_rgb(38, 42, 56);

// Selection
pub const SELECTION_BG: Color32 = Color32::from_rgb(50, 45, 25);       // dark amber tint
pub const SELECTION_BORDER: Color32 = ACCENT_PRIMARY;

// Viewport border
pub const VIEWPORT_BORDER: Color32 = Color32::from_rgb(40, 44, 60);
pub const VIEWPORT_BORDER_ACTIVE: Color32 = ACCENT_PRIMARY;

// -- CornerRadius --
pub const ROUNDING_SM: CornerRadius = CornerRadius::same(4);
pub const ROUNDING_MD: CornerRadius = CornerRadius::same(6);

// -- Spacing --
pub const SPACING_XS: f32 = 2.0;
pub const SPACING_SM: f32 = 4.0;
pub const SPACING_MD: f32 = 8.0;
pub const SPACING_LG: f32 = 12.0;

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
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.5, TEXT_SECONDARY);

    visuals.widgets.hovered.bg_fill = BG_WIDGET_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
    visuals.widgets.hovered.corner_radius = ROUNDING_SM;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, TEXT_PRIMARY);

    visuals.widgets.active.bg_fill = ACCENT_PRIMARY.linear_multiply(0.15);
    visuals.widgets.active.bg_stroke = Stroke::new(1.5, ACCENT_PRIMARY);
    visuals.widgets.active.corner_radius = ROUNDING_SM;
    visuals.widgets.active.fg_stroke = Stroke::new(1.5, ACCENT_PRIMARY);

    visuals.widgets.open.bg_fill = BG_WIDGET_HOVER;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
    visuals.widgets.open.corner_radius = ROUNDING_SM;

    // Selection highlight
    visuals.selection.bg_fill = SELECTION_BG;
    visuals.selection.stroke = Stroke::new(1.0, SELECTION_BORDER);

    // Window fill
    visuals.window_fill = BG_WINDOW;
    visuals.panel_fill = BG_SIDEBAR;
    visuals.window_stroke = Stroke::new(1.0, BORDER_SUBTLE);
    visuals.window_corner_radius = ROUNDING_MD;
    visuals.window_shadow = egui::Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(80),
    };

    // Popup / menu
    visuals.popup_shadow = egui::Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: Color32::from_black_alpha(60),
    };

    // Hyperlink / accent
    visuals.hyperlink_color = ACCENT_PRIMARY;

    // Text colors
    visuals.override_text_color = Some(TEXT_PRIMARY);

    // Faint background for striped rows
    visuals.faint_bg_color = Color32::from_rgb(24, 26, 35);

    // Extreme background (very dark areas)
    visuals.extreme_bg_color = BG_VIEWPORT;

    // Clip rectangle rounding
    visuals.clip_rect_margin = 2.0;

    ctx.set_visuals(visuals);

    // -- Font sizes & style overrides --
    let mut style = Style::default();
    style.spacing.item_spacing = Vec2::new(6.0, 4.0);
    style.spacing.button_padding = Vec2::new(8.0, 4.0);
    style.spacing.indent = 14.0;
    style.spacing.interact_size = Vec2::new(0.0, 28.0);
    style.spacing.icon_width = 14.0;
    style.spacing.menu_margin = Margin::same(8);
    style.spacing.window_margin = Margin::same(4);

    // Text styles
    let mut text_styles = [
        (TextStyle::Heading, FontId::new(16.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace)),
        (TextStyle::Button, FontId::new(13.0, FontFamily::Proportional)),
        (TextStyle::Small, FontId::new(11.0, FontFamily::Proportional)),
    ];
    for (text_style, font_id) in &mut text_styles {
        style.text_styles.insert(text_style.clone(), font_id.clone());
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
    r: 0.04,
    g: 0.05,
    b: 0.08,
    a: 1.0,
};
