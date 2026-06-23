//! Pie Editor Theme — Premium dark design system v3
//!
//! A refined, modern dark theme inspired by high-end engine editors.
//! Preserves UE5-level clarity and density while introducing a branded,
//! premium visual identity.
//!
//! Design principles:
//! - Viewport-first: the 3D scene is always the hero
//! - Strong visual hierarchy through layered surfaces
//! - Restrained accent usage — never decorative, always functional
//! - Engine-grade polish: every pixel feels intentional
//!
//! v3 changes:
//! - Richer surface layering with more distinct elevation steps
//! - Viewport-specific dark treatment for maximum scene contrast
//! - Stronger visual weight differentiation between UI regions
//! - More deliberate shadow work for depth without flash

use egui::{
    Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, Style, TextStyle, Vec2, Visuals,
};

// ═══════════════════════════════════════════════════════════════════════════════
//  Design Tokens — Color System
// ═══════════════════════════════════════════════════════════════════════════════

// -- Background Layers --
// Seven distinct elevation levels create clear visual hierarchy.
// The viewport sits at the absolute bottom (darkest), panels float above,
// and interactive elements sit at the top.

/// Deepest layer — void behind all panels, never directly visible
pub const BG_WINDOW: Color32 = Color32::from_rgb(12, 12, 14);
/// Panel background — the primary surface for docked panels
pub const BG_SURFACE: Color32 = Color32::from_rgb(18, 18, 21);
/// Elevated surface — cards, popups, floating panels, tab bars
pub const BG_ELEVATED: Color32 = Color32::from_rgb(24, 24, 28);
/// Widget resting state — buttons, inputs at rest
pub const BG_WIDGET: Color32 = Color32::from_rgb(30, 30, 36);
/// Widget hover — slightly brighter
pub const BG_WIDGET_HOVER: Color32 = Color32::from_rgb(38, 38, 46);
/// Viewport — absolute darkest surface for maximum scene contrast
pub const BG_VIEWPORT: Color32 = Color32::from_rgb(6, 6, 8);
/// Inset / recessed — property groups, inner panels
pub const BG_INSET: Color32 = Color32::from_rgb(14, 14, 17);
/// Active tab surface — matches panel content area
pub const BG_TAB_ACTIVE: Color32 = Color32::from_rgb(20, 20, 24);
/// Toolbar / menubar surface — slightly distinct from panels
pub const BG_CHROME: Color32 = Color32::from_rgb(16, 16, 19);
/// Section header bar — subtle differentiation from panel body
pub const BG_SECTION_HEADER: Color32 = Color32::from_rgb(22, 22, 26);

// -- Accent System --
// Restrained and functional. Primary accent for selection, focus, key actions.

/// Primary accent — branded blue-violet for selection & focus
pub const ACCENT_PRIMARY: Color32 = Color32::from_rgb(95, 108, 245);
/// Lighter variant for hover emphasis and selected text
pub const ACCENT_PRIMARY_LIGHT: Color32 = Color32::from_rgb(128, 138, 255);
/// Secondary accent — warm violet for special states
pub const ACCENT_SECONDARY: Color32 = Color32::from_rgb(165, 128, 255);
/// Success — green for positive states
pub const ACCENT_SUCCESS: Color32 = Color32::from_rgb(68, 200, 130);
/// Warning — amber for caution states
pub const ACCENT_WARNING: Color32 = Color32::from_rgb(235, 175, 55);
/// Danger / error — red for destructive states
pub const ACCENT_DANGER: Color32 = Color32::from_rgb(230, 75, 75);
/// Play / go — bright green for transport controls
pub const ACCENT_PLAY: Color32 = Color32::from_rgb(55, 215, 125);

// -- Text Hierarchy --
// Four levels of text contrast for clear information hierarchy.

/// Primary text — headings, important labels, active content
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(235, 235, 242);
/// Secondary text — body content, descriptions, inactive labels
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(160, 160, 175);
/// Tertiary text — hints, metadata, supplementary info
pub const TEXT_TERTIARY: Color32 = Color32::from_rgb(100, 100, 115);
/// Dimmed — barely visible, decorative, disabled
pub const TEXT_DIM: Color32 = Color32::from_rgb(62, 62, 74);

// -- Borders & Separators --
// Borders define surfaces without noise. Three strengths + separator.

/// Subtle border — card edges, panel outlines (least prominent)
pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(30, 30, 36);
/// Standard border — inputs, buttons at rest
pub const BORDER_STANDARD: Color32 = Color32::from_rgb(42, 42, 52);
/// Strong border — focused elements, active states
pub const BORDER_STRONG: Color32 = Color32::from_rgb(58, 58, 72);
/// Separator lines — dividers between sections
pub const SEPARATOR: Color32 = Color32::from_rgb(26, 26, 31);

// -- Selection --
pub const SELECTION_BG: Color32 = Color32::from_rgb(36, 40, 75);
pub const SELECTION_BORDER: Color32 = ACCENT_PRIMARY;
pub const SELECTION_TEXT: Color32 = ACCENT_PRIMARY_LIGHT;

// -- Viewport --
pub const VIEWPORT_BORDER: Color32 = Color32::from_rgb(20, 20, 24);
pub const VIEWPORT_BORDER_ACTIVE: Color32 = ACCENT_PRIMARY;

// -- Chips / Tags --
pub const BG_CHIP: Color32 = Color32::from_rgb(30, 30, 36);
pub const TEXT_CHIP: Color32 = TEXT_SECONDARY;

// -- Status Indicators --
pub const STATUS_PLAYING: Color32 = ACCENT_SUCCESS;
pub const STATUS_STOPPED: Color32 = TEXT_DIM;

// -- Axis Colors (for transform gizmos and drag values) --
pub const AXIS_X: Color32 = Color32::from_rgb(230, 75, 75);
pub const AXIS_Y: Color32 = Color32::from_rgb(68, 200, 130);
pub const AXIS_Z: Color32 = Color32::from_rgb(95, 108, 245);

// ═══════════════════════════════════════════════════════════════════════════════
//  Design Tokens — Spacing Scale
// ═══════════════════════════════════════════════════════════════════════════════

pub const SPACE_XS: f32 = 2.0;
pub const SPACE_SM: f32 = 4.0;
pub const SPACE_MD: f32 = 8.0;
pub const SPACE_LG: f32 = 12.0;
pub const SPACE_XL: f32 = 16.0;

// ═══════════════════════════════════════════════════════════════════════════════
//  Design Tokens — Corner Radius Scale
// ═══════════════════════════════════════════════════════════════════════════════

pub const RADIUS_NONE: CornerRadius = CornerRadius::same(0);
pub const RADIUS_SM: CornerRadius = CornerRadius::same(3);
pub const RADIUS_MD: CornerRadius = CornerRadius::same(5);
pub const RADIUS_LG: CornerRadius = CornerRadius::same(6);
pub const RADIUS_FULL: CornerRadius = CornerRadius::same(255);

// ═══════════════════════════════════════════════════════════════════════════════
//  Design Tokens — Typography
// ═══════════════════════════════════════════════════════════════════════════════

pub const FONT_SIZE_XS: f32 = 9.0;
pub const FONT_SIZE_SM: f32 = 10.0;
pub const FONT_SIZE_BASE: f32 = 11.0;
pub const FONT_SIZE_MD: f32 = 12.0;
pub const FONT_SIZE_LG: f32 = 14.0;
pub const FONT_SIZE_DISPLAY: f32 = 28.0;

// ═══════════════════════════════════════════════════════════════════════════════
//  Theme Application
// ═══════════════════════════════════════════════════════════════════════════════

/// Apply the full Pie Editor premium theme to an egui context.
pub fn apply(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();

    // -- Widget States --
    visuals.widgets.noninteractive.bg_fill = Color32::TRANSPARENT;
    visuals.widgets.noninteractive.bg_stroke = Stroke::NONE;
    visuals.widgets.noninteractive.corner_radius = RADIUS_SM;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_SECONDARY);

    visuals.widgets.inactive.bg_fill = BG_WIDGET;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER_STANDARD);
    visuals.widgets.inactive.corner_radius = RADIUS_SM;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_SECONDARY);

    visuals.widgets.hovered.bg_fill = BG_WIDGET_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
    visuals.widgets.hovered.corner_radius = RADIUS_SM;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);

    visuals.widgets.active.bg_fill = ACCENT_PRIMARY.linear_multiply(0.18);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT_PRIMARY);
    visuals.widgets.active.corner_radius = RADIUS_SM;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, ACCENT_PRIMARY_LIGHT);

    visuals.widgets.open.bg_fill = BG_ELEVATED;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
    visuals.widgets.open.corner_radius = RADIUS_MD;

    visuals.selection.bg_fill = SELECTION_BG;
    visuals.selection.stroke = Stroke::new(1.0, SELECTION_BORDER);

    // Window / panel surfaces
    visuals.window_fill = BG_WINDOW;
    visuals.panel_fill = BG_SURFACE;
    visuals.window_stroke = Stroke::new(1.0, SEPARATOR);
    visuals.window_corner_radius = RADIUS_LG;
    visuals.window_shadow = egui::Shadow {
        offset: [0, 8],
        blur: 28,
        spread: 0,
        color: Color32::from_black_alpha(80),
    };

    visuals.popup_shadow = egui::Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: Color32::from_black_alpha(90),
    };

    visuals.hyperlink_color = ACCENT_PRIMARY_LIGHT;
    visuals.override_text_color = Some(TEXT_PRIMARY);
    visuals.faint_bg_color = Color32::from_rgb(16, 16, 19);
    visuals.extreme_bg_color = BG_VIEWPORT;
    visuals.clip_rect_margin = 2.0;

    // Scrollbar styling
    visuals.widgets.noninteractive.corner_radius = RADIUS_FULL;

    ctx.set_visuals(visuals);

    // -- Typography --
    let mut style = Style::default();
    style.spacing.item_spacing = Vec2::new(6.0, 4.0);
    style.spacing.button_padding = Vec2::new(8.0, 4.0);
    style.spacing.indent = 14.0;
    style.spacing.interact_size = Vec2::new(0.0, 26.0);
    style.spacing.icon_width = 14.0;
    style.spacing.menu_margin = Margin::same(6);
    style.spacing.window_margin = Margin::same(4);
    style.spacing.combo_height = 200.0;
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.scroll.bar_inner_margin = 2.0;
    style.spacing.scroll.bar_outer_margin = 2.0;
    style.spacing.scroll.handle_min_length = 20.0;

    style.text_styles = [
        (
            TextStyle::Heading,
            FontId::new(FONT_SIZE_LG, FontFamily::Proportional),
        ),
        (
            TextStyle::Body,
            FontId::new(FONT_SIZE_BASE, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(FONT_SIZE_SM, FontFamily::Monospace),
        ),
        (
            TextStyle::Button,
            FontId::new(FONT_SIZE_BASE, FontFamily::Proportional),
        ),
        (
            TextStyle::Small,
            FontId::new(FONT_SIZE_XS, FontFamily::Proportional),
        ),
    ]
    .into_iter()
    .collect();

    ctx.set_style(style);
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Clear Colors
// ═══════════════════════════════════════════════════════════════════════════════

/// The clear color used for the egui paint pass (behind all UI).
pub const CLEAR_COLOR: [f32; 4] = [
    BG_WINDOW.r() as f32 / 255.0,
    BG_WINDOW.g() as f32 / 255.0,
    BG_WINDOW.b() as f32 / 255.0,
    1.0,
];

/// The clear color used for the 3D viewport scene.
pub const VIEWPORT_CLEAR: wgpu::Color = wgpu::Color {
    r: 0.024,
    g: 0.024,
    b: 0.031,
    a: 1.0,
};
