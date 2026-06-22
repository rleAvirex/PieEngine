//! Editor UI panels — premium dark theme v3.
//!
//! Redesigned for stronger visual hierarchy, clearer panel surfaces,
//! better viewport presentation, and more polished controls.
//!
//! v3 changes:
//! - Viewport is the undeniable center of gravity with deep framing
//! - Side panels have richer surface definition and purposeful framing
//! - Toolbar has clearer visual weight and control grouping
//! - Status bar is more legible with proper typographic hierarchy
//! - Empty states feel like polished tool states, not placeholders
//! - Overall composition reads as a cohesive engine editor

use egui::{
    Color32, Context, Frame, Image, Key, Margin, Sense, Stroke, TextureId,
    TopBottomPanel, Widget, load::SizedTexture, vec2,
};
use hecs::Entity;
use pie_runtime::components::{Camera, DirectionalLight, Name, SkyLight, Transform};
use pie_runtime::core::RuntimeApp;

use crate::dock_layout as dock;
use crate::gizmo::GizmoState;
use crate::theme::*;
use crate::ui_components as uc;

/// A spawnable entity type shown in the Assets menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnableEntity {
    Empty,
    Camera,
    DirectionalLight,
    SkyLight,
}

impl SpawnableEntity {
    /// All spawnable entity types in display order.
    pub const ALL: [SpawnableEntity; 4] = [
        SpawnableEntity::Empty,
        SpawnableEntity::Camera,
        SpawnableEntity::DirectionalLight,
        SpawnableEntity::SkyLight,
    ];

    /// Human-readable label for the entity type.
    pub fn label(self) -> &'static str {
        match self {
            SpawnableEntity::Empty => "Empty Entity",
            SpawnableEntity::Camera => "Camera",
            SpawnableEntity::DirectionalLight => "Directional Light",
            SpawnableEntity::SkyLight => "Sky Light",
        }
    }

    /// Short description of what this entity type does.
    pub fn description(self) -> &'static str {
        match self {
            SpawnableEntity::Empty => "A plain transform — useful as a parent or marker",
            SpawnableEntity::Camera => "Perspective camera with configurable FOV",
            SpawnableEntity::DirectionalLight => "Sun/moon light with atmosphere support",
            SpawnableEntity::SkyLight => "Captures sky for indirect lighting (IBL)",
        }
    }

    /// Icon glyph for the entity type.
    pub fn icon(self) -> &'static str {
        match self {
            SpawnableEntity::Empty => "◇",
            SpawnableEntity::Camera => "◉",
            SpawnableEntity::DirectionalLight => "☀",
            SpawnableEntity::SkyLight => "◌",
        }
    }
}

/// Commands emitted by the UI that the main app loop must process.
#[derive(Default)]
pub struct EditorCommands {
    pub reload_scene: bool,
    /// If set, the main loop should spawn a new entity of this type.
    pub spawn_entity: Option<SpawnableEntity>,
    pub viewport_size: Option<[u32; 2]>,
    pub viewport_hovered: bool,
    pub viewport_look_delta: Option<(f32, f32)>,
    pub viewport_rect: Option<egui::Rect>,
    pub viewport_click_pos: Option<egui::Pos2>,
    pub viewport_hover_pos: Option<egui::Pos2>,
    pub viewport_primary_drag_started: bool,
    pub viewport_primary_drag_start_pos: Option<egui::Pos2>,
    pub gizmo_drag_delta: Option<(f32, f32)>,
    pub gizmo_drag_end: bool,
}

/// Scene info displayed in the hierarchy panel.
pub struct EditorSceneInfo {
    pub scene_path: String,
    pub mesh_count: usize,
    pub texture_count: usize,
    pub material_count: usize,
}

/// Parameters needed by the editor UI each frame.
pub struct EditorUiParams<'a> {
    pub ctx: &'a Context,
    pub runtime: &'a mut RuntimeApp,
    pub scene_info: &'a EditorSceneInfo,
    pub selected_entity: &'a mut Option<Entity>,
    pub viewport_texture_id: Option<TextureId>,
    pub commands: &'a mut EditorCommands,
    pub gizmo_state: GizmoState,
    pub smoothed_delta: f64,
    pub cam_pos: glam::Vec3,
    pub cam_speed: f32,
    pub dock: &'a mut dock::DockState,
}

/// Build the full editor UI (toolbar, hierarchy, inspector, viewport, status bar).
pub fn build_editor_ui(params: EditorUiParams<'_>) {
    let EditorUiParams {
        ctx,
        runtime,
        scene_info,
        selected_entity,
        viewport_texture_id,
        commands,
        gizmo_state,
        smoothed_delta,
        cam_pos,
        cam_speed,
        dock,
    } = params;
    use egui::RichText;

    let toggle_running = ctx.input(|input| input.key_pressed(Key::Space));
    let step_requested = ctx.input(|input| input.key_pressed(Key::N));
    let reload_requested = ctx.input(|input| input.key_pressed(Key::R));

    let scene_path = &scene_info.scene_path;
    let mesh_count = scene_info.mesh_count;
    let texture_count = scene_info.texture_count;
    let material_count = scene_info.material_count;
    let frame = runtime.simulation().frame();
    let is_running = runtime.is_running();
    let entity_count = runtime.simulation().world().iter().count();

    // ══════════════════════════════════════════════════════════════════════════
    //  Menu Bar — minimal, premium brand mark + ghost menu items
    // ══════════════════════════════════════════════════════════════════════════
    TopBottomPanel::top("menu_bar")
        .frame(uc::menubar_frame())
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(SPACE_SM);
                // Brand mark — subtle branded badge
                Frame {
                    inner_margin: Margin::symmetric(6, 2),
                    outer_margin: Margin::ZERO,
                    corner_radius: RADIUS_SM,
                    shadow: egui::Shadow::NONE,
                    fill: ACCENT_PRIMARY.linear_multiply(0.08),
                    stroke: Stroke::new(1.0, ACCENT_PRIMARY.linear_multiply(0.2)),
                }
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("◆ Pie Engine")
                            .color(ACCENT_PRIMARY_LIGHT)
                            .size(FONT_SIZE_SM)
                            .strong(),
                    );
                });

                ui.add_space(SPACE_LG);

                // Menu items — ghost style, restrained
                uc::ghost_button(ui, "File");
                ui.add_space(SPACE_SM);
                uc::ghost_button(ui, "Edit");
                ui.add_space(SPACE_SM);
                uc::ghost_button(ui, "Window");
                ui.add_space(SPACE_SM);
                uc::ghost_button(ui, "Help");
            });
        });

    // ══════════════════════════════════════════════════════════════════════════
    //  Toolbar — cleaner layout, crisper controls, clear grouping
    // ══════════════════════════════════════════════════════════════════════════
    TopBottomPanel::top("toolbar")
        .frame(uc::toolbar_frame())
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Scene path with icon
                ui.label(RichText::new("📂").color(TEXT_DIM).size(FONT_SIZE_SM));
                ui.add_space(SPACE_XS);
                ui.label(
                    RichText::new(scene_path)
                        .color(TEXT_TERTIARY)
                        .size(FONT_SIZE_SM),
                );

                ui.add_space(SPACE_LG);

                // ── Transport controls group ──
                let play_text = if is_running { " ⏸ " } else { " ▶ " };
                if uc::play_button(ui, play_text, is_running)
                    .on_hover_text("Play / Pause (Space)")
                    .clicked()
                {
                    if is_running {
                        runtime.pause();
                    } else {
                        runtime.resume();
                    }
                }

                ui.add_space(SPACE_XS);

                if uc::tool_button(ui, " ⏭ ", false)
                    .on_hover_text("Step one frame (N)")
                    .clicked()
                {
                    runtime.pause();
                    runtime.step();
                }

                ui.add_space(SPACE_XS);

                if uc::tool_button(ui, " ↻ ", false)
                    .on_hover_text("Reload scene (R)")
                    .clicked()
                {
                    commands.reload_scene = true;
                }

                ui.add_space(SPACE_XS);

                // Assets dropdown button
                let assets_btn = egui::Button::new(
                    RichText::new("Assets ▼")
                        .color(TEXT_SECONDARY)
                        .size(FONT_SIZE_BASE),
                )
                .corner_radius(RADIUS_SM)
                .fill(BG_WIDGET)
                .stroke(Stroke::new(1.0, BORDER_STANDARD))
                .min_size(egui::vec2(90.0, 24.0))
                .ui(ui);
                let assets_btn = assets_btn.on_hover_text("Spawn entities");
                if assets_btn.clicked() {
                    ui.memory_mut(|mem| {
                        mem.toggle_popup(egui::Id::new("assets_dropdown"));
                    });
                }

                // Assets popup menu with search
                let search_id = egui::Id::new("assets_search");
                egui::popup_below_widget(
                    ui,
                    egui::Id::new("assets_dropdown"),
                    &assets_btn,
                    egui::PopupCloseBehavior::CloseOnClick,
                    |ui: &mut egui::Ui| {
                        ui.set_min_width(260.0);

                        // Search bar
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("🔍").color(TEXT_DIM).size(FONT_SIZE_SM));
                            let search_text = ui
                                .data_mut(|d| {
                                    d.get_temp_mut_or::<String>(search_id, String::new()).clone()
                                });
                            let mut search = search_text;
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut search)
                                    .hint_text(
                                        RichText::new("Search entities…")
                                            .color(TEXT_DIM)
                                            .size(FONT_SIZE_SM),
                                    )
                                    .desired_width(f32::INFINITY)
                                    .frame(false),
                            );
                            if response.changed() {
                                ui.data_mut(|d| d.insert_temp(search_id, search));
                            }
                        });

                        ui.add_space(SPACE_XS);
                        uc::styled_separator(ui);
                        ui.add_space(SPACE_XS);

                        // Read search text for filtering
                        let search_lower = ui
                            .data_mut(|d| d.get_temp::<String>(search_id))
                            .unwrap_or_default()
                            .to_lowercase();

                        // Filtered entity list
                        let filtered: Vec<SpawnableEntity> = SpawnableEntity::ALL
                            .iter()
                            .filter(|e| {
                                let label = e.label().to_lowercase();
                                let desc = e.description().to_lowercase();
                                search_lower.is_empty()
                                    || label.contains(&search_lower)
                                    || desc.contains(&search_lower)
                            })
                            .copied()
                            .collect();

                        if filtered.is_empty() {
                            ui.add_space(SPACE_SM);
                            ui.label(
                                RichText::new("No matching entities")
                                    .color(TEXT_DIM)
                                    .size(FONT_SIZE_SM),
                            );
                            ui.add_space(SPACE_SM);
                        } else {
                            for entity_type in filtered {
                                let icon = entity_type.icon();
                                let label = entity_type.label();
                                let desc = entity_type.description();

                                let btn = egui::Button::new(
                                    RichText::new(format!("{icon}  {label}"))
                                        .color(TEXT_PRIMARY)
                                        .size(FONT_SIZE_BASE),
                                )
                                .fill(Color32::TRANSPARENT)
                                .frame(false)
                                .min_size(egui::vec2(240.0, 0.0));

                                let resp = ui.add(btn);
                                if resp.hovered() {
                                    resp.clone().on_hover_text(
                                        RichText::new(desc)
                                            .color(TEXT_SECONDARY)
                                            .size(FONT_SIZE_SM),
                                    );
                                }
                                if resp.clicked() {
                                    commands.spawn_entity = Some(entity_type);
                                    ui.memory_mut(|mem| mem.close_popup());
                                    ui.data_mut(|d| {
                                        d.insert_temp(search_id, String::new());
                                    });
                                }
                            }
                        }
                    },
                );

                // Right side: frame counter + status chip
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (dot_color, state_text) = if is_running {
                        (STATUS_PLAYING, "PLAYING")
                    } else {
                        (STATUS_STOPPED, "STOPPED")
                    };
                    uc::status_chip(ui, state_text, dot_color);
                    ui.add_space(SPACE_SM);
                    ui.label(
                        RichText::new(format!("Frame {frame}"))
                            .color(TEXT_TERTIARY)
                            .size(FONT_SIZE_SM)
                            .monospace(),
                    );
                });
            });
        });

    // ══════════════════════════════════════════════════════════════════════════
    //  Dock System (Unreal Engine–style tab stacking + drag-to-dock)
    // ══════════════════════════════════════════════════════════════════════════

    let full_rect = ctx.available_rect();
    let mut dock_area = full_rect;
    dock_area.max.y -= 28.0; // status bar height

    dock::show_dock(dock, ctx, dock_area, |panel_id, ui| {
        match panel_id {
            dock::PanelId::Outliner => {
                show_outliner_content(ui, runtime, scene_info, selected_entity, entity_count, mesh_count, texture_count, material_count);
            }
            dock::PanelId::Details => {
                show_details_content(ui, runtime, selected_entity);
            }
        }
    });

    // ══════════════════════════════════════════════════════════════════════════
    //  Viewport (Central — fills remaining space, framed as hero area)
    // ══════════════════════════════════════════════════════════════════════════
    let viewport_rect = dock.viewport_rect(dock_area);

    if viewport_rect.width() > 1.0 && viewport_rect.height() > 1.0 {
        egui::Area::new(egui::Id::new("viewport_area"))
            .fixed_pos(viewport_rect.min)
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                ui.set_max_size(viewport_rect.size());
                show_viewport_content(ui, commands, viewport_texture_id, gizmo_state, cam_pos);
            });
    }

    // ══════════════════════════════════════════════════════════════════════════
    //  Status Bar (Bottom) — more legible, better hierarchy
    // ══════════════════════════════════════════════════════════════════════════
    TopBottomPanel::bottom("status_bar")
        .frame(uc::statusbar_frame())
        .show(ctx, |ui| {
            show_status_bar(ui, smoothed_delta, is_running, frame, entity_count, cam_pos, cam_speed, gizmo_state);
        });

    // ══════════════════════════════════════════════════════════════════════════
    //  Keyboard Shortcuts
    // ══════════════════════════════════════════════════════════════════════════
    if toggle_running {
        if is_running {
            runtime.pause();
        } else {
            runtime.resume();
        }
    }
    if step_requested {
        runtime.pause();
        runtime.step();
    }
    if reload_requested {
        commands.reload_scene = true;
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Panel Content Functions
// ═══════════════════════════════════════════════════════════════════════════════

fn show_outliner_content(
    ui: &mut egui::Ui,
    runtime: &mut RuntimeApp,
    _scene_info: &EditorSceneInfo,
    selected_entity: &mut Option<Entity>,
    entity_count: usize,
    _mesh_count: usize,
    _texture_count: usize,
    _material_count: usize,
) {
    use egui::RichText;

    // Panel header
    uc::section_header(ui, "Outliner", |ui| {
        ui.label(
            RichText::new(format!("{entity_count}"))
                .color(TEXT_DIM)
                .size(FONT_SIZE_XS)
                .monospace(),
        );
    });

    // Entity list
    let mut entities = Vec::new();
    for (entity, _) in runtime.simulation().world().query::<&Transform>().iter() {
        entities.push(entity);
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(SPACE_XS);

            for entity in entities {
                let name = runtime
                    .simulation()
                    .world()
                    .get::<&Name>(entity)
                    .map(|n| n.0.clone())
                    .unwrap_or_else(|_| format!("{entity:?}"));
                let selected = *selected_entity == Some(entity);

                let item_bg = if selected { SELECTION_BG } else { Color32::TRANSPARENT };
                let item_stroke = if selected {
                    Stroke::new(1.0, ACCENT_PRIMARY.linear_multiply(0.5))
                } else {
                    Stroke::NONE
                };
                let text_color = if selected { SELECTION_TEXT } else { TEXT_PRIMARY };
                let indicator = if selected { "▸" } else { " " };
                let indicator_color = if selected { ACCENT_PRIMARY } else { TEXT_DIM };

                Frame {
                    inner_margin: Margin::symmetric(8, 3),
                    outer_margin: Margin::symmetric(2, 1),
                    corner_radius: RADIUS_SM,
                    shadow: egui::Shadow::NONE,
                    fill: item_bg,
                    stroke: item_stroke,
                }
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // Selection indicator
                        ui.label(
                            RichText::new(indicator)
                                .color(indicator_color)
                                .size(FONT_SIZE_SM)
                                .monospace(),
                        );
                        // Entity type icon
                        ui.label(
                            RichText::new("◆")
                                .color(if selected { ACCENT_PRIMARY_LIGHT } else { TEXT_DIM })
                                .size(8.0),
                        );
                        ui.add_space(2.0);
                        // Entity name — clickable
                        if ui
                            .add(
                                egui::Label::new(
                                    RichText::new(&name).color(text_color).size(FONT_SIZE_BASE),
                                )
                                .sense(Sense::click()),
                            )
                            .clicked()
                        {
                            *selected_entity = Some(entity);
                        }
                    });
                });
            }
        });
}

fn show_details_content(
    ui: &mut egui::Ui,
    runtime: &mut RuntimeApp,
    selected_entity: &mut Option<Entity>,
) {
    use egui::RichText;

    if let Some(entity) = *selected_entity {
        let entity_name = runtime
            .simulation()
            .world()
            .get::<&Name>(entity)
            .map(|n| n.0.clone())
            .unwrap_or_else(|_| format!("{entity:?}"));

        // Entity header with accent bar
        uc::section_header(ui, &entity_name, |ui| {
            ui.label(
                RichText::new(format!("{:?}", entity))
                    .color(TEXT_DIM)
                    .size(FONT_SIZE_XS)
                    .monospace(),
            );
        });

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Transform properties in a card
                uc::card(ui, |ui| {
                    if let Ok(mut transform) = runtime
                        .simulation_mut()
                        .world_mut()
                        .get::<&mut Transform>(entity)
                    {
                        let mut t = transform.translation;
                        uc::axis_row(ui, "Location", &mut t.x, &mut t.y, &mut t.z, 0.05);
                        transform.translation = t;

                        ui.add_space(SPACE_SM);

                        let mut s = transform.scale;
                        uc::axis_row(ui, "Scale", &mut s.x, &mut s.y, &mut s.z, 0.05);
                        transform.scale = s;
                    }

                    // Camera properties
                    if let Ok(mut camera) = runtime
                        .simulation_mut()
                        .world_mut()
                        .get::<&mut Camera>(entity)
                    {
                        uc::styled_separator(ui);

                        uc::property_row(ui, "FOV", |ui| {
                            let mut deg = camera.fov.to_degrees();
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::DragValue::new(&mut deg)
                                        .speed(1.0)
                                        .min_decimals(1)
                                        .max_decimals(1)
                                        .range(1.0..=179.0),
                                );
                                ui.label(RichText::new("deg").color(TEXT_DIM).size(FONT_SIZE_SM));
                            });
                            camera.fov = deg.to_radians().max(0.01);
                        });
                    }

                    // Directional Light properties
                    if let Ok(mut light) = runtime
                        .simulation_mut()
                        .world_mut()
                        .get::<&mut DirectionalLight>(entity)
                    {
                        uc::styled_separator(ui);

                        uc::property_row(ui, "Intensity", |ui| {
                            ui.add(
                                egui::DragValue::new(&mut light.intensity)
                                    .speed(0.1)
                                    .min_decimals(2),
                            );
                        });

                        ui.add_space(SPACE_SM);

                        {
                            let mut d = light.direction;
                            uc::axis_row(ui, "Direction", &mut d.x, &mut d.y, &mut d.z, 0.01);
                            light.direction = d;
                        }
                        if light.direction.length_squared() > f32::EPSILON {
                            light.direction = light.direction.normalize();
                        }

                        ui.add_space(SPACE_SM);

                        {
                            let mut c = light.color;
                            uc::axis_row(ui, "Color", &mut c.x, &mut c.y, &mut c.z, 0.05);
                            light.color = c;
                        }

                        uc::property_row(ui, "Atmosphere Sun", |ui| {
                            ui.checkbox(&mut light.atmosphere_sun_light, "");
                        });
                    }

                    // Sky Light properties
                    if let Ok(mut sky_light) = runtime
                        .simulation_mut()
                        .world_mut()
                        .get::<&mut SkyLight>(entity)
                    {
                        uc::styled_separator(ui);

                        uc::property_row(ui, "Intensity", |ui| {
                            ui.add(
                                egui::DragValue::new(&mut sky_light.intensity)
                                    .speed(0.1)
                                    .min_decimals(2),
                            );
                        });

                        uc::property_row(ui, "Real-Time Capture", |ui| {
                            ui.checkbox(&mut sky_light.real_time_capture, "");
                        });

                        uc::property_row(ui, "Capture Resolution", |ui| {
                            let mut res = sky_light.capture_resolution;
                            let changed = ui
                                .add(
                                    egui::DragValue::new(&mut res)
                                        .speed(1)
                                        .range(16..=256),
                                )
                                .changed();
                            if changed {
                                sky_light.capture_resolution = res;
                            }
                        });
                    }
                });
            });
    } else {
        // Empty state — polished and intentional
        uc::card(ui, |ui| {
            uc::empty_state(ui, "◇", "No Selection", "Select an entity in the viewport to inspect its properties");
        });
    }
}

fn show_viewport_content(
    ui: &mut egui::Ui,
    commands: &mut EditorCommands,
    viewport_texture_id: Option<TextureId>,
    gizmo_state: GizmoState,
    _cam_pos: glam::Vec3,
) {
    use egui::RichText;

    let available = ui.available_size();
    let viewport_size = [
        available.x.max(1.0).round() as u32,
        available.y.max(1.0).round() as u32,
    ];
    commands.viewport_size = Some(viewport_size);

    if let Some(texture_id) = viewport_texture_id {
        // Viewport with clear border — active state uses accent color
        let border_stroke = if commands.viewport_hovered {
            Stroke::new(1.5, VIEWPORT_BORDER_ACTIVE)
        } else {
            Stroke::new(1.0, VIEWPORT_BORDER)
        };

        // Viewport shell — deep dark frame with inner depth cues
        Frame {
            inner_margin: Margin::ZERO,
            outer_margin: Margin::ZERO,
            corner_radius: RADIUS_NONE,
            shadow: egui::Shadow::NONE,
            fill: BG_VIEWPORT,
            stroke: border_stroke,
        }
        .show(ui, |ui| {
            let response = ui.add(
                Image::from_texture(SizedTexture::new(
                    texture_id,
                    vec2(viewport_size[0] as f32, viewport_size[1] as f32),
                ))
                .sense(Sense::click_and_drag()),
            );
            commands.viewport_hovered = response.hovered() || response.dragged();
            commands.viewport_rect = Some(response.rect);
            let pointer_pos = response
                .hover_pos()
                .or_else(|| ui.ctx().input(|i| i.pointer.latest_pos()))
                .filter(|&pos| response.rect.contains(pos));
            commands.viewport_hover_pos = pointer_pos;
            if response.dragged_by(egui::PointerButton::Secondary) {
                let d = response.drag_delta();
                commands.viewport_look_delta = Some((d.x, d.y));
            }
            if response.drag_started_by(egui::PointerButton::Primary) {
                commands.viewport_primary_drag_started = true;
                commands.viewport_primary_drag_start_pos = response.interact_pointer_pos();
            }
            if response.dragged_by(egui::PointerButton::Primary) {
                let d = response.drag_delta();
                if gizmo_state.is_active() {
                    commands.gizmo_drag_delta = Some((d.x, d.y));
                }
            }
            if response.drag_stopped_by(egui::PointerButton::Primary)
                && gizmo_state.is_active()
            {
                commands.gizmo_drag_end = true;
            }
            if response.clicked() {
                commands.viewport_click_pos = response.interact_pointer_pos();
            }
        });
    } else {
        // Viewport initialization state — premium loading screen
        uc::viewport_frame(VIEWPORT_BORDER).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                // Branded diamond icon in a circle
                let icon_size = 48.0;
                let (_, painter) = ui.allocate_painter(
                    egui::vec2(icon_size, icon_size),
                    egui::Sense::hover(),
                );
                let center = ui.min_rect().min + egui::vec2(icon_size / 2.0, icon_size / 2.0);
                painter.circle_filled(center, icon_size / 2.0, BG_ELEVATED);
                painter.circle_stroke(
                    center,
                    icon_size / 2.0,
                    Stroke::new(1.0, BORDER_SUBTLE),
                );

                ui.add_space(SPACE_SM);
                ui.label(
                    RichText::new("◆")
                        .color(ACCENT_PRIMARY.linear_multiply(0.6))
                        .size(FONT_SIZE_DISPLAY)
                        .strong(),
                );
                ui.add_space(SPACE_SM);
                ui.label(
                    RichText::new("Pie Engine")
                        .color(TEXT_SECONDARY)
                        .size(FONT_SIZE_LG),
                );
                ui.add_space(SPACE_XS);
                ui.label(
                    RichText::new("Viewport initializing…")
                        .color(TEXT_DIM)
                        .size(FONT_SIZE_SM),
                );
                ui.add_space(SPACE_MD);
                ui.label(
                    RichText::new("Right-click drag to look  •  WASD to move")
                        .color(TEXT_DIM)
                        .size(FONT_SIZE_SM),
                );
            });
        });
    }
}

fn show_status_bar(
    ui: &mut egui::Ui,
    smoothed_delta: f64,
    is_running: bool,
    frame: u64,
    entity_count: usize,
    cam_pos: glam::Vec3,
    cam_speed: f32,
    gizmo_state: GizmoState,
) {
    use egui::RichText;
    use crate::gizmo::Axis;

    let fps = if smoothed_delta > 0.0 {
        (1.0 / smoothed_delta) as i32
    } else {
        0
    };
    let (dot_color, state_text) = if is_running {
        (STATUS_PLAYING, "Playing")
    } else {
        (STATUS_STOPPED, "Stopped")
    };

    ui.horizontal(|ui| {
        // Status indicator with dot
        ui.label(RichText::new("●").color(dot_color).size(7.0));
        ui.add_space(3.0);
        ui.label(RichText::new(state_text).color(TEXT_SECONDARY).size(FONT_SIZE_SM));

        // Separator
        ui.add_space(SPACE_SM);
        ui.label(RichText::new("│").color(BORDER_STANDARD).size(FONT_SIZE_SM));
        ui.add_space(SPACE_SM);

        // FPS — color coded
        let fps_color = if fps >= 55 {
            ACCENT_SUCCESS
        } else if fps >= 30 {
            ACCENT_WARNING
        } else {
            ACCENT_DANGER
        };
        ui.label(
            RichText::new(format!("{fps} FPS"))
                .color(fps_color)
                .size(FONT_SIZE_SM)
                .monospace(),
        );

        ui.add_space(SPACE_SM);
        ui.label(RichText::new("│").color(BORDER_STANDARD).size(FONT_SIZE_SM));
        ui.add_space(SPACE_SM);

        // Frame count
        ui.label(
            RichText::new(format!("Frame {frame}"))
                .color(TEXT_TERTIARY)
                .size(FONT_SIZE_SM)
                .monospace(),
        );

        ui.add_space(SPACE_SM);
        ui.label(RichText::new("│").color(BORDER_STANDARD).size(FONT_SIZE_SM));
        ui.add_space(SPACE_SM);

        // Entity count
        ui.label(
            RichText::new(format!("{entity_count} entities"))
                .color(TEXT_TERTIARY)
                .size(FONT_SIZE_SM),
        );

        ui.add_space(SPACE_SM);
        ui.label(RichText::new("│").color(BORDER_STANDARD).size(FONT_SIZE_SM));
        ui.add_space(SPACE_SM);

        // Camera position
        ui.label(
            RichText::new(format!(
                "Cam: {:.1}, {:.1}, {:.1}",
                cam_pos.x, cam_pos.y, cam_pos.z
            ))
            .color(TEXT_TERTIARY)
            .size(FONT_SIZE_SM)
            .monospace(),
        );

        ui.add_space(SPACE_SM);
        ui.label(RichText::new("│").color(BORDER_STANDARD).size(FONT_SIZE_SM));
        ui.add_space(SPACE_SM);

        // Camera speed
        ui.label(
            RichText::new(format!("Speed: {:.1}", cam_speed))
                .color(TEXT_TERTIARY)
                .size(FONT_SIZE_SM)
                .monospace(),
        );

        // Gizmo state (if active)
        if let GizmoState::Dragging { axis, .. } = gizmo_state {
            ui.add_space(SPACE_SM);
            ui.label(RichText::new("│").color(BORDER_STANDARD).size(FONT_SIZE_SM));
            ui.add_space(SPACE_SM);
            let (al, ac) = match axis {
                Axis::X => ("X", AXIS_X),
                Axis::Y => ("Y", AXIS_Y),
                Axis::Z => ("Z", AXIS_Z),
            };
            ui.label(
                RichText::new(format!("Gizmo {al}"))
                    .color(ac)
                    .size(FONT_SIZE_SM)
                    .strong(),
            );
        }
        if matches!(gizmo_state, GizmoState::UniformScaling { .. }) {
            ui.add_space(SPACE_SM);
            ui.label(RichText::new("│").color(BORDER_STANDARD).size(FONT_SIZE_SM));
            ui.add_space(SPACE_SM);
            ui.label(
                RichText::new("Gizmo Scale")
                    .color(ACCENT_SECONDARY)
                    .size(FONT_SIZE_SM)
                    .strong(),
            );
        }

        // Keyboard shortcuts hint (right-aligned)
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new("Space: Play  •  N: Step  •  R: Reload")
                    .color(TEXT_DIM)
                    .size(FONT_SIZE_XS),
            );
        });
    });
}
