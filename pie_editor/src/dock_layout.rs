//! Dockable panel layout system for the Pie Editor — Unreal Engine–style.
//!
//! Features:
//! • **Tab stacking** — multiple panels share a docked area; click tabs to switch.
//! • **Drag-to-dock** — drag a tab to split an area or dock to a zone (left/right/bottom).
//! • **Floating windows** — panels can float as independent windows.
//! • **Collapse** — panels collapse to a thin sidebar strip.
//! • **Resize handles** — drag edges to resize docked areas.
//!
//! # Architecture
//!
//! The layout is a tree of [`DockNode`]s. Each node is either:
//! - A **Leaf** — holds an ordered list of panel IDs (tabs) and the index of the active tab.
//! - A **Split** — splits a rect into two child nodes (horizontal or vertical).
//!
//! The tree is flat-serialized into [`DockState`] as a `Vec<DockNode>` with parent/child
//! indices, mirroring UE's `FLayoutNode` approach but simplified for our needs.

use egui::{Area, Color32, Context, CursorIcon, Id, Order, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::theme;
use crate::theme::*;

// ═══════════════════════════════════════════════════════════════════════════════
//  Types
// ═══════════════════════════════════════════════════════════════════════════════

/// Unique identifier for each dockable panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelId {
    Outliner,
    Details,
}

impl PanelId {
    pub const ALL: [PanelId; 2] = [PanelId::Outliner, PanelId::Details];

    pub fn title(self) -> &'static str {
        match self {
            PanelId::Outliner => "Outliner",
            PanelId::Details => "Details",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            PanelId::Outliner => "Outliner",
            PanelId::Details => "Details",
        }
    }

    pub fn default_size(self) -> Vec2 {
        match self {
            PanelId::Outliner => Vec2::new(260.0, 500.0),
            PanelId::Details => Vec2::new(320.0, 500.0),
        }
    }

    pub fn min_size(self) -> Vec2 {
        match self {
            PanelId::Outliner => Vec2::new(160.0, 120.0),
            PanelId::Details => Vec2::new(200.0, 120.0),
        }
    }

    pub fn max_size(self) -> Vec2 {
        match self {
            PanelId::Outliner => Vec2::new(600.0, 1200.0),
            PanelId::Details => Vec2::new(800.0, 1200.0),
        }
    }
}

/// Where a panel can be docked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DockZone {
    Left,
    Right,
    Bottom,
}

impl DockZone {
    pub const ALL: [DockZone; 3] = [DockZone::Left, DockZone::Right, DockZone::Bottom];
}

/// A command emitted by the dock UI for the caller to apply after rendering.
#[derive(Debug, Clone)]
pub enum DockCmd {
    /// Move a panel to a specific zone (creates or joins a leaf there).
    DockPanel(PanelId, DockZone),
    /// Undock a panel into a floating window.
    FloatPanel(PanelId, Pos2, Vec2),
    /// Collapse a panel to the sidebar.
    CollapsePanel(PanelId),
    /// Resize a zone (width for left/right, height for bottom).
    ResizeZone(DockZone, f32),
    /// Set the active tab within a leaf.
    SetActiveTab(usize, usize), // (leaf_index, tab_index)
}

/// Per-zone size state.
#[derive(Debug, Clone, Copy)]
pub struct ZoneSize {
    pub width: f32,
}

impl Default for ZoneSize {
    fn default() -> Self {
        Self { width: 280.0 }
    }
}

/// A node in the dock tree.
#[derive(Debug, Clone)]
pub enum DockNode {
    /// A leaf node containing one or more panel tabs.
    Leaf {
        /// Panel IDs in this leaf (tabs). At least 1.
        tabs: Vec<PanelId>,
        /// Index of the currently active tab.
        active_tab: usize,
        /// The screen rect allocated to this leaf.
        rect: Rect,
        /// Which dock zone this leaf is attached to, or `None` if it's a
        /// floating window. Stored on the leaf itself (rather than inferred
        /// from the node index) so the Vec can be compacted by `retain`
        /// without breaking zone lookups.
        zone: Option<DockZone>,
    },
    /// A split node dividing space between two children.
    Split {
        /// Direction of the split.
        vertical: bool,
        /// Fraction of space given to the first child (0.0–1.0).
        split_frac: f32,
        /// Index of the first child in `DockState::nodes`.
        first: usize,
        /// Index of the second child in `DockState::nodes`.
        second: usize,
        /// The screen rect allocated to this split.
        rect: Rect,
    },
}

impl DockNode {
    fn rect(&self) -> Rect {
        match self {
            DockNode::Leaf { rect, .. } | DockNode::Split { rect, .. } => *rect,
        }
    }

    fn rect_mut(&mut self) -> &mut Rect {
        match self {
            DockNode::Leaf { rect, .. } | DockNode::Split { rect, .. } => rect,
        }
    }

    fn is_leaf(&self) -> bool {
        matches!(self, DockNode::Leaf { .. })
    }

    fn is_split(&self) -> bool {
        matches!(self, DockNode::Split { .. })
    }
}

/// Global dock layout state.
#[derive(Debug, Clone)]
pub struct DockState {
    /// Flat tree storage. Node 0 is the root.
    nodes: Vec<DockNode>,
    /// Per-zone sizes.
    zone_sizes: std::collections::HashMap<DockZone, f32>,
    /// Commands collected during this frame's UI rendering.
    cmds: Vec<DockCmd>,
    /// The panel currently being dragged, if any.
    dragging: Option<PanelId>,
    /// Mouse position of the drag (for drop target detection).
    drag_pos: Option<Pos2>,
    /// The leaf index from which a drag started (to detect re-drag).
    drag_source_leaf: Option<usize>,
}

impl Default for DockState {
    fn default() -> Self {
        let mut zone_sizes = std::collections::HashMap::new();
        zone_sizes.insert(DockZone::Left, 280.0);
        zone_sizes.insert(DockZone::Right, 320.0);
        zone_sizes.insert(DockZone::Bottom, 200.0);

        // Default layout: root split (vertical) → left leaf | right leaf.
        // Left leaf: [Outliner] docked to Left zone.
        // Right leaf: [Details] docked to Right zone.
        let nodes = vec![
            DockNode::Split {
                vertical: true,
                split_frac: 0.5,
                first: 1,
                second: 2,
                rect: Rect::NOTHING,
            },
            DockNode::Leaf {
                tabs: vec![PanelId::Outliner],
                active_tab: 0,
                rect: Rect::NOTHING,
                zone: Some(DockZone::Left),
            },
            DockNode::Leaf {
                tabs: vec![PanelId::Details],
                active_tab: 0,
                rect: Rect::NOTHING,
                zone: Some(DockZone::Right),
            },
        ];

        Self {
            nodes,
            zone_sizes,
            cmds: Vec::new(),
            dragging: None,
            drag_pos: None,
            drag_source_leaf: None,
        }
    }
}

impl DockState {
    // ── Command queue ──

    fn push_cmd(&mut self, cmd: DockCmd) {
        self.cmds.push(cmd);
    }

    pub fn drain_cmds(&mut self) -> Vec<DockCmd> {
        self.cmds.drain(..).collect()
    }

    pub fn apply_cmd(&mut self, cmd: DockCmd) {
        match cmd {
            DockCmd::DockPanel(panel_id, zone) => {
                self.dock_panel_to_zone(panel_id, zone);
            }
            DockCmd::FloatPanel(panel_id, pos, size) => {
                self.float_panel(panel_id, pos, size);
            }
            DockCmd::CollapsePanel(panel_id) => {
                self.collapse_panel(panel_id);
            }
            DockCmd::ResizeZone(zone, new_size) => {
                let clamped = new_size.clamp(120.0, 800.0);
                self.zone_sizes.insert(zone, clamped);
            }
            DockCmd::SetActiveTab(leaf_idx, tab_idx) => {
                if let Some(DockNode::Leaf { active_tab, .. }) = self.nodes.get_mut(leaf_idx) {
                    *active_tab = tab_idx;
                }
            }
        }
    }

    // ── Layout ──

    /// Compute the central viewport rect after docking all zones.
    pub fn viewport_rect(&self, full_rect: Rect) -> Rect {
        let mut rect = full_rect;
        for zone in DockZone::ALL {
            if self.zone_has_tabs(zone) {
                let size = self.zone_sizes[&zone];
                match zone {
                    DockZone::Left => rect.min.x += size,
                    DockZone::Right => rect.max.x -= size,
                    DockZone::Bottom => rect.max.y -= size,
                }
            }
        }
        rect.max.x = rect.max.x.max(rect.min.x);
        rect.max.y = rect.max.y.max(rect.min.y);
        rect
    }

    /// Check if any leaf is assigned to a given zone.
    fn zone_has_tabs(&self, zone: DockZone) -> bool {
        self.nodes.iter().enumerate().any(|(idx, node)| {
            if let DockNode::Leaf { tabs, .. } = node {
                !tabs.is_empty() && node_zone_by_index(self, idx, node) == Some(zone)
            } else {
                false
            }
        })
    }

    // ── Panel management ──

    fn dock_panel_to_zone(&mut self, panel_id: PanelId, zone: DockZone) {
        // Remove panel from its current location
        self.remove_panel_from_all(panel_id);

        // Find an existing leaf already docked to this zone. We look up by
        // the leaf's `zone` field rather than by hardcoded node index — the
        // old code assumed `nodes[1]=Left, nodes[2]=Right, nodes[3]=Bottom`
        // forever, but `remove_panel_from_all` calls `retain` which compacts
        // the Vec and shifts indices, so the hardcoded mapping broke after
        // any float/remove.
        let existing_leaf_idx = self.nodes.iter().enumerate().find_map(|(idx, node)| match node {
            DockNode::Leaf { zone: leaf_zone, tabs, .. }
                if *leaf_zone == Some(zone) && !tabs.is_empty() =>
            {
                Some(idx)
            }
            _ => None,
        });

        let target_idx = match existing_leaf_idx {
            Some(idx) => idx,
            None => {
                // No existing leaf for this zone — create a new one.
                let idx = self.nodes.len();
                self.nodes.push(DockNode::Leaf {
                    tabs: vec![],
                    active_tab: 0,
                    rect: Rect::NOTHING,
                    zone: Some(zone),
                });
                idx
            }
        };

        if let Some(DockNode::Leaf { tabs, .. }) = self.nodes.get_mut(target_idx) {
            if !tabs.contains(&panel_id) {
                tabs.push(panel_id);
            }
        }
    }

    fn float_panel(&mut self, panel_id: PanelId, pos: Pos2, size: Vec2) {
        self.remove_panel_from_all(panel_id);
        self.nodes.push(DockNode::Leaf {
            tabs: vec![panel_id],
            active_tab: 0,
            rect: Rect::from_min_size(pos, size),
            // Floating leaves have no zone.
            zone: None,
        });
    }

    fn collapse_panel(&mut self, panel_id: PanelId) {
        self.remove_panel_from_all(panel_id);
    }

    fn remove_panel_from_all(&mut self, panel_id: PanelId) {
        for node in self.nodes.iter_mut() {
            if let DockNode::Leaf { tabs, active_tab, .. } = node {
                if let Some(pos) = tabs.iter().position(|p| *p == panel_id) {
                    tabs.remove(pos);
                    if *active_tab >= tabs.len() && *active_tab > 0 {
                        *active_tab = tabs.len() - 1;
                    }
                }
            }
        }
        // Clean up empty leaves (but not the root split)
        self.nodes.retain(|node| {
            if let DockNode::Leaf { tabs, .. } = node {
                !tabs.is_empty()
            } else {
                true
            }
        });
    }

    // ── Drag & Drop state ──

    pub fn start_drag(&mut self, panel_id: PanelId) {
        self.dragging = Some(panel_id);
    }

    pub fn update_drag(&mut self, pos: Pos2) {
        self.drag_pos = Some(pos);
    }

    pub fn end_drag(&mut self) {
        self.dragging = None;
        self.drag_pos = None;
        self.drag_source_leaf = None;
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging.is_some()
    }

    pub fn drag_panel(&self) -> Option<PanelId> {
        self.dragging
    }

    pub fn drag_pos(&self) -> Option<Pos2> {
        self.drag_pos
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Public API
// ═══════════════════════════════════════════════════════════════════════════════

/// Render the entire dock system. This replaces the old individual panel calls.
pub fn show_dock(
    dock: &mut DockState,
    ctx: &Context,
    full_rect: Rect,
    mut render_panel: impl FnMut(PanelId, &mut Ui),
) {
    // Assign rects to nodes based on zone sizes
    assign_rects(dock, full_rect);

    // Collect render info to avoid borrow conflicts
    let mut leaf_renders: Vec<(usize, Rect, Vec<PanelId>, usize)> = Vec::new();
    let mut float_renders: Vec<(usize, Rect, Vec<PanelId>, usize)> = Vec::new();

    for (idx, node) in dock.nodes.iter().enumerate() {
        if let DockNode::Leaf { tabs, active_tab, rect, .. } = node {
            if tabs.is_empty() {
                continue;
            }
            let is_floating = !full_rect.contains_rect(*rect) && !full_rect.intersects(*rect);
            if is_floating {
                float_renders.push((idx, *rect, tabs.clone(), *active_tab));
            } else {
                leaf_renders.push((idx, *rect, tabs.clone(), *active_tab));
            }
        }
    }

    // Render docked leaves
    for (idx, rect, tabs, active_tab) in &leaf_renders {
        render_leaf(ctx, dock, *idx, *rect, tabs, *active_tab, &mut render_panel);
    }

    // Render floating windows
    for (idx, rect, tabs, active_tab) in &float_renders {
        render_floating(ctx, dock, *idx, *rect, tabs, *active_tab, &mut render_panel);
    }

    // Render drag drop targets if dragging
    if dock.is_dragging() {
        render_drop_targets(dock, ctx, full_rect);
    }

    // Render collapsed tab strips
    show_collapsed_tabs(dock, ctx, full_rect);
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Rendering
// ═══════════════════════════════════════════════════════════════════════════════

fn assign_rects(dock: &mut DockState, full_rect: Rect) {
    let rect = full_rect;

    // Simple layout: left zone, right zone, bottom zone
    let left_size = if dock.zone_has_tabs(DockZone::Left) {
        dock.zone_sizes[&DockZone::Left]
    } else {
        0.0
    };
    let right_size = if dock.zone_has_tabs(DockZone::Right) {
        dock.zone_sizes[&DockZone::Right]
    } else {
        0.0
    };
    let bottom_size = if dock.zone_has_tabs(DockZone::Bottom) {
        dock.zone_sizes[&DockZone::Bottom]
    } else {
        0.0
    };

    // Pre-compute zone for each node index to avoid borrow conflicts
    let zone_per_node: Vec<Option<DockZone>> = (0..dock.nodes.len())
        .map(|idx| {
            if let Some(node) = dock.nodes.get(idx) {
                node_zone_by_index(dock, idx, node)
            } else {
                None
            }
        })
        .collect();

    for (idx, node) in dock.nodes.iter_mut().enumerate() {
        if let DockNode::Leaf { tabs, rect: node_rect, .. } = node {
            if tabs.is_empty() {
                continue;
            }

            let zone = zone_per_node.get(idx).copied().flatten();
            *node_rect = match zone {
                Some(DockZone::Left) => {
                    Rect::from_min_size(rect.min, Vec2::new(left_size, rect.height() - bottom_size))
                }
                Some(DockZone::Right) => {
                    Rect::from_min_max(
                        Pos2::new(rect.max.x - right_size, rect.min.y),
                        Pos2::new(rect.max.x, rect.max.y - bottom_size),
                    )
                }
                Some(DockZone::Bottom) => {
                    Rect::from_min_max(
                        Pos2::new(rect.min.x, rect.max.y - bottom_size),
                        rect.max,
                    )
                }
                None => {
                    // Floating — keep existing rect
                    *node_rect
                }
            };
        }
    }
}

/// Determine zone by inspecting the leaf's stored `zone` field. Floating
/// leaves (zone == None) return None. Split nodes always return None.
fn node_zone_by_index(_dock: &DockState, _idx: usize, node: &DockNode) -> Option<DockZone> {
    match node {
        DockNode::Leaf { zone, .. } => *zone,
        DockNode::Split { .. } => None,
    }
}

fn render_leaf(
    ctx: &Context,
    dock: &mut DockState,
    leaf_idx: usize,
    rect: Rect,
    tabs: &[PanelId],
    active_tab: usize,
    render_panel: &mut impl FnMut(PanelId, &mut Ui),
) {
    let active_idx = active_tab.min(tabs.len() - 1);
    let active_panel = tabs[active_idx];

    Area::new(Id::new(("dock_leaf", leaf_idx)))
        .fixed_pos(rect.min)
        .order(Order::Middle)
        .show(ctx, |ui| {
            ui.set_max_size(rect.size());

            // Panel surface with clear border
            egui::Frame {
                inner_margin: egui::Margin::ZERO,
                outer_margin: egui::Margin::ZERO,
                corner_radius: theme::RADIUS_NONE,
                shadow: egui::Shadow::NONE,
                fill: BG_SURFACE,
                stroke: Stroke::new(1.0, BORDER_SUBTLE),
            }
            .show(ui, |ui| {
                // Tab bar
                render_tab_bar(ui, dock, leaf_idx, tabs, active_idx);

                // Content area — slightly inset from panel edge
                egui::Frame {
                    inner_margin: egui::Margin::symmetric(6, 4),
                    outer_margin: egui::Margin::ZERO,
                    corner_radius: theme::RADIUS_NONE,
                    shadow: egui::Shadow::NONE,
                    fill: BG_SURFACE,
                    stroke: Stroke::NONE,
                }
                .show(ui, |ui| {
                    ui.allocate_ui(ui.available_size(), |ui| {
                        render_panel(active_panel, ui);
                    });
                });
            });
        });

    // Resize handle for docked leaves
    if node_zone_by_index(dock, leaf_idx, &dock.nodes[leaf_idx]).is_some() {
        render_resize_handle(ctx, dock, leaf_idx, rect);
    }
}

fn render_tab_bar(
    ui: &mut Ui,
    dock: &mut DockState,
    leaf_idx: usize,
    tabs: &[PanelId],
    active_idx: usize,
) {
    // Tab bar sits in an elevated band above the panel content
    egui::Frame {
        inner_margin: egui::Margin::symmetric(3, 2),
        outer_margin: egui::Margin::ZERO,
        corner_radius: theme::RADIUS_NONE,
        shadow: egui::Shadow::NONE,
        fill: BG_SECTION_HEADER,
        stroke: Stroke::new(1.0, BORDER_SUBTLE),
    }
    .show(ui, |ui| {
        ui.horizontal(|ui| {
            // Render each tab
            for (tab_idx, panel_id) in tabs.iter().enumerate() {
                let is_active = tab_idx == active_idx;
                let title = panel_id.title();

                // Tab background
                let tab_bg = if is_active {
                    BG_TAB_ACTIVE
                } else {
                    BG_SECTION_HEADER
                };

                let (resp, painter) = ui.allocate_painter(
                    Vec2::new(80.0, 24.0),
                    Sense::click_and_drag(),
                );

                // Draw tab background
                painter.rect_filled(resp.rect, RADIUS_SM, tab_bg);

                // Active tab gets a top accent line
                if is_active {
                    painter.rect_filled(
                        Rect::from_min_size(
                            resp.rect.min,
                            Vec2::new(resp.rect.width(), 2.0),
                        ),
                        theme::RADIUS_NONE,
                        ACCENT_PRIMARY,
                    );
                    // Subtle bottom border to connect tab to content
                    painter.rect_filled(
                        Rect::from_min_size(
                            Pos2::new(resp.rect.min.x, resp.rect.max.y - 1.0),
                            Vec2::new(resp.rect.width(), 1.0),
                        ),
                        theme::RADIUS_NONE,
                        BG_TAB_ACTIVE,
                    );
                }

                // Inactive tab border
                if !is_active {
                    painter.rect_stroke(
                        resp.rect,
                        RADIUS_SM,
                        Stroke::new(1.0, BORDER_SUBTLE),
                        StrokeKind::Inside,
                    );
                }

                // Tab text
                let text_color = if is_active {
                    TEXT_PRIMARY
                } else {
                    TEXT_TERTIARY
                };
                let galley = ui.fonts(|f| {
                    f.layout_no_wrap(
                        title.to_string(),
                        egui::FontId::new(FONT_SIZE_SM, egui::FontFamily::Proportional),
                        text_color,
                    )
                });
                let text_pos = Pos2::new(
                    resp.rect.center().x - galley.size().x / 2.0,
                    resp.rect.center().y - galley.size().y / 2.0,
                );
                painter.galley(text_pos, galley, text_color);

                // Click to activate
                if resp.clicked() {
                    dock.push_cmd(DockCmd::SetActiveTab(leaf_idx, tab_idx));
                }

                // Drag to undock / re-dock
                if resp.drag_started() {
                    dock.start_drag(*panel_id);
                    dock.drag_source_leaf = Some(leaf_idx);
                }
            }

            // Spacer with action buttons
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Float button
                if small_icon_btn(ui, "⤢", "Float").clicked() {
                    if let Some(&active_panel) = tabs.get(active_idx) {
                        let pos = ui.min_rect().min + Vec2::new(100.0, 50.0);
                        dock.push_cmd(DockCmd::FloatPanel(
                            active_panel,
                            pos,
                            active_panel.default_size(),
                        ));
                    }
                }
                // Collapse button
                if small_icon_btn(ui, "−", "Collapse").clicked() {
                    if let Some(&active_panel) = tabs.get(active_idx) {
                        dock.push_cmd(DockCmd::CollapsePanel(active_panel));
                    }
                }
            });
        });
    });

    // Subtle divider between tab bar and content
    let painter = ui.painter();
    painter.rect_filled(
        egui::Rect::from_min_size(
            ui.min_rect().min + egui::vec2(0.0, 0.0),
            egui::vec2(ui.available_width(), 1.0),
        ),
        0,
        BORDER_SUBTLE,
    );
}

fn render_floating(
    ctx: &Context,
    dock: &mut DockState,
    leaf_idx: usize,
    rect: Rect,
    tabs: &[PanelId],
    active_tab: usize,
    render_panel: &mut impl FnMut(PanelId, &mut Ui),
) {
    let active_idx = active_tab.min(tabs.len().saturating_sub(1));
    let active_panel = tabs[active_idx];

    let title = if tabs.len() == 1 {
        tabs[0].title().to_string()
    } else {
        format!("{} +{}", tabs[0].title(), tabs.len() - 1)
    };

    egui::Window::new(&title)
        .id(Id::new(("float_win", leaf_idx)))
        .fixed_rect(rect)
        .min_width(160.0)
        .min_height(100.0)
        .collapsible(false)
        .frame(egui::Frame {
            inner_margin: egui::Margin::ZERO,
            outer_margin: egui::Margin::ZERO,
            corner_radius: RADIUS_LG,
            shadow: egui::Shadow {
                offset: [0, 8],
                blur: 24,
                spread: 0,
                color: Color32::from_black_alpha(100),
            },
            fill: BG_SURFACE,
            stroke: Stroke::new(1.0, BORDER_STANDARD),
        })
        .show(ctx, |ui| {
            // Tab bar for floating window too
            if tabs.len() > 1 {
                render_tab_bar(ui, dock, leaf_idx, tabs, active_idx);
            }

            // Dock buttons
            ui.horizontal(|ui| {
                for zone in DockZone::ALL {
                    let label = match zone {
                        DockZone::Left => "◀ Left",
                        DockZone::Right => "▶ Right",
                        DockZone::Bottom => "▼ Bottom",
                    };
                    if small_icon_btn(ui, label, &format!("Dock to {zone:?}")).clicked() {
                        dock.push_cmd(DockCmd::DockPanel(active_panel, zone));
                    }
                }
            });
            ui.add_space(SPACE_XS);

            // Content
            ui.allocate_ui(ui.available_size(), |ui| {
                render_panel(active_panel, ui);
            });

            // Track floating position/size
            let new_rect = ui.min_rect();
            if let Some(DockNode::Leaf { rect: node_rect, .. }) = dock.nodes.get_mut(leaf_idx) {
                *node_rect = Rect::from_min_size(new_rect.min, new_rect.size());
            }
        });
}

fn render_resize_handle(ctx: &Context, dock: &mut DockState, leaf_idx: usize, rect: Rect) {
    let handle_w = 6.0;
    let zone = node_zone_by_index(dock, leaf_idx, &dock.nodes[leaf_idx]);

    // Resize handle position and orientation depends on the zone:
    //  - Left zone: handle on the RIGHT edge of the rect, drag horizontally.
    //    Dragging right grows the zone (correct).
    //  - Right zone: handle on the LEFT edge of the rect, drag horizontally.
    //    Dragging right SHRINKS the zone — so we use -delta.x.
    //  - Bottom zone: handle on the TOP edge of the rect, drag vertically.
    //    Dragging down SHRINKS the zone — so we use -delta.y, and we measure
    //    rect.height() rather than rect.width().
    // The old code always put the handle on the right edge and used
    // `rect.width() + delta.x`, which (a) grew the Right zone when it should
    // have shrunk, and (b) used the horizontal axis + width for the Bottom
    // zone (whose rect spans the full viewport width).
    let (handle_rect, cursor_icon, is_vertical_zone) = match zone {
        Some(DockZone::Left) => (
            Rect::from_min_size(
                Pos2::new(rect.max.x - handle_w * 0.5, rect.min.y),
                Vec2::new(handle_w, rect.height()),
            ),
            CursorIcon::ResizeHorizontal,
            false,
        ),
        Some(DockZone::Right) => (
            Rect::from_min_size(
                Pos2::new(rect.min.x - handle_w * 0.5, rect.min.y),
                Vec2::new(handle_w, rect.height()),
            ),
            CursorIcon::ResizeHorizontal,
            false,
        ),
        Some(DockZone::Bottom) => (
            Rect::from_min_size(
                Pos2::new(rect.min.x, rect.min.y - handle_w * 0.5),
                Vec2::new(rect.width(), handle_w),
            ),
            CursorIcon::ResizeVertical,
            true,
        ),
        None => {
            // Floating leaf — no resize handle (floating windows use their
            // own window-frame resize provided by the OS/egui Area).
            return;
        }
    };

    Area::new(Id::new(("resize", leaf_idx)))
        .fixed_pos(handle_rect.min)
        .order(Order::Tooltip)
        .show(ctx, |ui| {
            ui.set_max_size(handle_rect.size());
            let (resp, painter) =
                ui.allocate_painter(handle_rect.size(), Sense::drag());
            let hover = resp.hovered() || resp.dragged();

            painter.rect_filled(
                resp.rect,
                0.0,
                if hover {
                    ACCENT_PRIMARY.linear_multiply(0.35)
                } else {
                    Color32::TRANSPARENT
                },
            );

            if resp.dragged() {
                let delta = resp.drag_delta();
                let new_size = if is_vertical_zone {
                    // Bottom zone: vertical drag on top edge.
                    // Dragging the handle DOWN shrinks the bottom zone (the
                    // handle moves down, leaving less space below).
                    (rect.height() - delta.y).clamp(120.0, 800.0)
                } else {
                    match zone {
                        Some(DockZone::Right) => {
                            // Right zone: handle on LEFT edge; dragging right
                            // shrinks the zone.
                            (rect.width() - delta.x).clamp(120.0, 800.0)
                        }
                        Some(DockZone::Left) => {
                            // Left zone: handle on RIGHT edge; dragging right
                            // grows the zone.
                            (rect.width() + delta.x).clamp(120.0, 800.0)
                        }
                        _ => return, // unreachable given is_vertical_zone=false here
                    }
                };
                if let Some(z) = zone {
                    dock.push_cmd(DockCmd::ResizeZone(z, new_size));
                }
            }

            if hover {
                ctx.set_cursor_icon(cursor_icon);
            }
        });
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Drag & Drop Targets
// ═══════════════════════════════════════════════════════════════════════════════

fn render_drop_targets(dock: &mut DockState, ctx: &Context, full_rect: Rect) {
    let Some(drag_pos) = dock.drag_pos() else { return };
    if !full_rect.contains(drag_pos) {
        return;
    }

    // Draw drop zone indicators
    let zone_size = 40.0;
    for zone in DockZone::ALL {
        let (zone_rect, _label) = match zone {
            DockZone::Left => {
                let r = Rect::from_min_size(
                    full_rect.min,
                    Vec2::new(zone_size, full_rect.height()),
                );
                (r, "◀")
            }
            DockZone::Right => {
                let r = Rect::from_min_max(
                    Pos2::new(full_rect.max.x - zone_size, full_rect.min.y),
                    full_rect.max,
                );
                (r, "▶")
            }
            DockZone::Bottom => {
                let r = Rect::from_min_max(
                    Pos2::new(full_rect.min.x, full_rect.max.y - zone_size),
                    full_rect.max,
                );
                (r, "▼")
            }
        };

        let hovered = zone_rect.contains(drag_pos);

        Area::new(Id::new(("drop_target", zone as usize)))
            .fixed_pos(zone_rect.min)
            .order(Order::Tooltip)
            .show(ctx, |ui| {
                ui.set_max_size(zone_rect.size());
                let (resp, painter) =
                    ui.allocate_painter(zone_rect.size(), Sense::hover());

                if hovered || resp.hovered() {
                    painter.rect_filled(
                        resp.rect,
                        RADIUS_SM,
                        ACCENT_PRIMARY.linear_multiply(0.2),
                    );
                    painter.rect_stroke(
                        resp.rect,
                        RADIUS_SM,
                        Stroke::new(2.0, ACCENT_PRIMARY),
                        StrokeKind::Inside,
                    );

                    // Label
                    let label = match zone {
                        DockZone::Left => "◀ Left",
                        DockZone::Right => "Right ▶",
                        DockZone::Bottom => "▼ Bottom",
                    };
                    let galley = ui.fonts(|f| {
                        f.layout_no_wrap(
                            label.to_string(),
                            egui::FontId::new(FONT_SIZE_SM, egui::FontFamily::Proportional),
                            ACCENT_PRIMARY,
                        )
                    });
                    let text_pos = Pos2::new(
                        resp.rect.center().x - galley.size().x / 2.0,
                        resp.rect.center().y - galley.size().y / 2.0,
                    );
                    painter.galley(text_pos, galley, ACCENT_PRIMARY);
                }

                // Drop detection
                if resp.hovered() {
                    ctx.set_cursor_icon(CursorIcon::PointingHand);
                }
            });
    }

    // Handle drag end — check if we're over a drop target.
    // Only respond to the primary mouse button being released; the old code
    // used `any_released()` which would end the drag if a secondary button
    // was clicked during the drag (e.g. right-click for context menu).
    let input = ctx.input(|i| i.clone());
    if input.pointer.button_released(egui::PointerButton::Primary) {
        for zone in DockZone::ALL {
            let zone_rect = match zone {
                DockZone::Left => Rect::from_min_size(
                    full_rect.min,
                    Vec2::new(zone_size, full_rect.height()),
                ),
                DockZone::Right => Rect::from_min_max(
                    Pos2::new(full_rect.max.x - zone_size, full_rect.min.y),
                    full_rect.max,
                ),
                DockZone::Bottom => Rect::from_min_max(
                    Pos2::new(full_rect.min.x, full_rect.max.y - zone_size),
                    full_rect.max,
                ),
            };

            if zone_rect.contains(drag_pos) {
                if let Some(panel_id) = dock.drag_panel() {
                    dock.push_cmd(DockCmd::DockPanel(panel_id, zone));
                }
                break;
            }
        }
        dock.end_drag();
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Collapsed Tabs
// ═══════════════════════════════════════════════════════════════════════════════

fn show_collapsed_tabs(dock: &mut DockState, ctx: &Context, full_rect: Rect) {
    // Find panels that are not in any leaf (collapsed)
    let mut in_leaf = std::collections::HashSet::new();
    for node in dock.nodes.iter() {
        if let DockNode::Leaf { tabs, .. } = node {
            for tab in tabs {
                in_leaf.insert(*tab);
            }
        }
    }

    let collapsed: Vec<_> = PanelId::ALL
        .iter()
        .filter(|id| !in_leaf.contains(id))
        .copied()
        .collect();

    if collapsed.is_empty() {
        return;
    }

    let tab_w = 28.0;
    let tab_h = 90.0;
    let gap = 4.0;
    let total_h = collapsed.len() as f32 * (tab_h + gap);
    let start_y = full_rect.center().y - total_h / 2.0;

    for (i, id) in collapsed.iter().enumerate() {
        let y = start_y + i as f32 * (tab_h + gap);
        let pos = Pos2::new(full_rect.min.x + 2.0, y);

        Area::new(Id::new(("collapsed_tab", id)))
            .fixed_pos(pos)
            .order(Order::Foreground)
            .show(ctx, |ui| {
                ui.set_max_size(Vec2::new(tab_w, tab_h));
                let (resp, painter) =
                    ui.allocate_painter(Vec2::new(tab_w, tab_h), Sense::click());
                let bg = if resp.hovered() {
                    BG_WIDGET_HOVER
                } else {
                    BG_ELEVATED
                };
                painter.rect_filled(resp.rect, RADIUS_SM, bg);
                painter.rect_stroke(
                    resp.rect,
                    RADIUS_SM,
                    Stroke::new(1.0, BORDER_SUBTLE),
                    StrokeKind::Inside,
                );

                let galley = ui.fonts(|f| {
                    f.layout_no_wrap(
                        id.short_label().to_string(),
                        egui::FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional),
                        TEXT_SECONDARY,
                    )
                });
                let tp = Pos2::new(
                    resp.rect.center().x - galley.size().x / 2.0,
                    resp.rect.center().y - galley.size().y / 2.0,
                );
                painter.galley(tp, galley, TEXT_SECONDARY);

                if resp.clicked() {
                    // Restore to default zone
                    let zone = match id {
                        PanelId::Outliner => DockZone::Left,
                        PanelId::Details => DockZone::Right,
                    };
                    dock.push_cmd(DockCmd::DockPanel(*id, zone));
                }
            });
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Helpers
// ═══════════════════════════════════════════════════════════════════════════════

fn small_icon_btn(ui: &mut Ui, icon: &str, tooltip: &str) -> Response {
    ui.add(
        egui::Button::new(egui::RichText::new(icon).color(TEXT_DIM).size(FONT_SIZE_SM))
            .corner_radius(RADIUS_SM)
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::NONE),
    )
    .on_hover_text(tooltip)
}
