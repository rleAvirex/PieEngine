//! Editor UI panels — UE5-inspired toolbar, hierarchy, inspector, viewport, and status bar.

use egui::{
    CentralPanel, Color32, Context, Image, Key, Sense, SidePanel, TextureId, TopBottomPanel,
    load::SizedTexture, vec2, CornerRadius, Stroke, Margin, Frame,
};
use hecs::Entity;
use pie_runtime::components::{Camera, DirectionalLight, Name, Transform};
use pie_runtime::core::RuntimeApp;

use crate::gizmo::{Axis, GizmoState};
use crate::theme;

/// Commands emitted by the UI that the main app loop must process.
#[derive(Default)]
pub struct EditorCommands {
    pub reload_scene: bool,
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
}

/// Build the full editor UI (toolbar, hierarchy, inspector, viewport, status bar).
#[allow(clippy::too_many_arguments)]
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
    } = params;
    use egui::RichText;
    use theme::*;

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

    // ---- Menu Bar (UE5 style — thin strip at very top) ----
    TopBottomPanel::top("menu_bar")
        .frame(Frame {
            inner_margin: Margin::symmetric(4, 1),
            outer_margin: Margin::ZERO,
            corner_radius: CornerRadius::ZERO,
            shadow: egui::Shadow::NONE,
            fill: BG_TOOLBAR,
            stroke: Stroke::new(1.0, SEPARATOR),
        })
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.label(RichText::new("Pie Engine").color(ACCENT_PRIMARY).size(11.0).strong());
                ui.add_space(SPACING_MD);
                ui.label(RichText::new("File").color(TEXT_SECONDARY).size(11.0));
                ui.add_space(SPACING_SM);
                ui.label(RichText::new("Edit").color(TEXT_SECONDARY).size(11.0));
                ui.add_space(SPACING_SM);
                ui.label(RichText::new("Window").color(TEXT_SECONDARY).size(11.0));
                ui.add_space(SPACING_SM);
                ui.label(RichText::new("Help").color(TEXT_SECONDARY).size(11.0));
            });
        });

    // ---- Toolbar (below menu bar — UE5 playback controls) ----
    TopBottomPanel::top("toolbar")
        .frame(Frame {
            inner_margin: Margin::symmetric(6, 3),
            outer_margin: Margin::ZERO,
            corner_radius: CornerRadius::ZERO,
            shadow: egui::Shadow::NONE,
            fill: BG_SIDEBAR,
            stroke: Stroke::new(1.0, SEPARATOR),
        })
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Scene name
                ui.label(RichText::new(scene_path).color(TEXT_SECONDARY).size(11.0));
                ui.add_space(SPACING_LG);

                // Play/Pause/Step controls (centered, UE5 style)
                let play_btn = if is_running {
                    egui::Button::new(RichText::new("  ||  ").color(TEXT_PRIMARY).size(12.0))
                        .corner_radius(ROUNDING_SM).fill(BG_WIDGET).stroke(Stroke::new(1.0, BORDER_STRONG))
                } else {
                    egui::Button::new(RichText::new("  >  ").color(ACCENT_PLAY).size(12.0))
                        .corner_radius(ROUNDING_SM).fill(ACCENT_PLAY.linear_multiply(0.1)).stroke(Stroke::new(1.0, ACCENT_PLAY.linear_multiply(0.4)))
                };
                if ui.add(play_btn).on_hover_text("Play / Pause (Space)").clicked() {
                    if is_running { runtime.pause(); } else { runtime.resume(); }
                }

                let step_btn = egui::Button::new(RichText::new(" >| ").color(TEXT_SECONDARY).size(12.0))
                    .corner_radius(ROUNDING_SM).fill(BG_WIDGET).stroke(Stroke::new(1.0, BORDER_SUBTLE));
                if ui.add(step_btn).on_hover_text("Step one frame (N)").clicked() { runtime.pause(); runtime.step(); }

                ui.add_space(SPACING_SM);

                let reload_btn = egui::Button::new(RichText::new("  R  ").color(TEXT_SECONDARY).size(12.0))
                    .corner_radius(ROUNDING_SM).fill(BG_WIDGET).stroke(Stroke::new(1.0, BORDER_SUBTLE));
                if ui.add(reload_btn).on_hover_text("Reload scene (R)").clicked() { commands.reload_scene = true; }

                // Right side: state indicator
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (dot_color, state_text) = if is_running { (ACCENT_SUCCESS, "PLAYING") } else { (TEXT_DIM, "STOPPED") };
                    ui.label(RichText::new(state_text).color(TEXT_DIM).size(10.0).monospace());
                    ui.add_space(SPACING_XS);
                    ui.label(RichText::new("|").color(BORDER_SUBTLE).size(10.0));
                    ui.add_space(SPACING_XS);
                    ui.label(RichText::new(format!("Frame {frame}")).color(TEXT_DIM).size(10.0).monospace());
                    ui.add_space(SPACING_XS);
                    ui.label(RichText::new("|").color(BORDER_SUBTLE).size(10.0));
                    ui.add_space(SPACING_XS);
                    ui.label(RichText::new("●").color(dot_color).size(8.0));
                });
            });
        });

    // ---- Hierarchy Panel (left — UE5 Outliner style) ----
    SidePanel::left("outliner")
        .resizable(true).default_width(240.0).min_width(180.0)
        .frame(Frame {
            inner_margin: Margin::ZERO,
            outer_margin: Margin::ZERO,
            corner_radius: CornerRadius::ZERO,
            shadow: egui::Shadow::NONE,
            fill: BG_SIDEBAR,
            stroke: Stroke::new(1.0, SEPARATOR),
        })
        .show(ctx, |ui| {
            // Section header (UE5 dark header bar)
            Frame { inner_margin: Margin::symmetric(8, 5), outer_margin: Margin::ZERO, corner_radius: CornerRadius::ZERO, shadow: egui::Shadow::NONE, fill: BG_SECTION, stroke: Stroke::NONE }
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("OUTLINER").color(TEXT_SECONDARY).size(10.0).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(format!("{entity_count}")).color(TEXT_DIM).size(10.0).monospace());
                        });
                    });
                });

            // Stats bar
            Frame { inner_margin: Margin::symmetric(8, 3), outer_margin: Margin::ZERO, corner_radius: CornerRadius::ZERO, shadow: egui::Shadow::NONE, fill: Color32::from_rgb(26, 26, 26), stroke: Stroke::NONE }
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("Meshes: {mesh_count}")).color(TEXT_DIM).size(9.0));
                        ui.label(RichText::new(format!("Mats: {material_count}")).color(TEXT_DIM).size(9.0));
                        ui.label(RichText::new(format!("Tex: {texture_count}")).color(TEXT_DIM).size(9.0));
                    });
                });

            ui.add_space(SPACING_XS);

            // Entity list (UE5 outliner items — flat, no icons, highlight on hover)
            let mut entities = Vec::new();
            for (entity, _) in runtime.simulation().world().query::<&Transform>().iter() { entities.push(entity); }
            for entity in entities {
                let name = runtime.simulation().world().get::<&Name>(entity).map(|n| n.0.clone()).unwrap_or_else(|_| format!("{entity:?}"));
                let selected = *selected_entity == Some(entity);

                let item_bg = if selected { SELECTION_BG } else { Color32::TRANSPARENT };
                let item_stroke = if selected { Stroke::new(1.0, ACCENT_PRIMARY.linear_multiply(0.4)) } else { Stroke::NONE };
                let text_color = if selected { ACCENT_SECONDARY } else { TEXT_PRIMARY };

                Frame { inner_margin: Margin::symmetric(8, 3), outer_margin: Margin::ZERO, corner_radius: CornerRadius::ZERO, shadow: egui::Shadow::NONE, fill: item_bg, stroke: item_stroke }
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(if selected { ">" } else { " " }).color(if selected { ACCENT_PRIMARY } else { TEXT_DIM }).size(10.0).monospace());
                            if ui.label(RichText::new(&name).color(text_color).size(11.0)).clicked() {
                                *selected_entity = Some(entity);
                            }
                        });
                    });
            }
        });

    // ---- Details Panel (right — UE5 Details panel style) ----
    SidePanel::right("details")
        .resizable(true).default_width(300.0).min_width(200.0)
        .frame(Frame {
            inner_margin: Margin::ZERO,
            outer_margin: Margin::ZERO,
            corner_radius: CornerRadius::ZERO,
            shadow: egui::Shadow::NONE,
            fill: BG_SIDEBAR,
            stroke: Stroke::new(1.0, SEPARATOR),
        })
        .show(ctx, |ui| {
            // Section header
            Frame { inner_margin: Margin::symmetric(8, 5), outer_margin: Margin::ZERO, corner_radius: CornerRadius::ZERO, shadow: egui::Shadow::NONE, fill: BG_SECTION, stroke: Stroke::NONE }
                .show(ui, |ui| {
                    ui.label(RichText::new("DETAILS").color(TEXT_SECONDARY).size(10.0).strong());
                });

            ui.add_space(SPACING_XS);

            // Lighting section
            if let Some(light) = runtime.simulation_mut().resource_mut::<DirectionalLight>() {
                Frame { inner_margin: Margin::symmetric(8, 4), outer_margin: Margin::ZERO, corner_radius: CornerRadius::ZERO, shadow: egui::Shadow::NONE, fill: BG_SECTION, stroke: Stroke::NONE }
                    .show(ui, |ui| {
                        ui.label(RichText::new("LIGHTING").color(TEXT_SECONDARY).size(10.0).strong());
                    });
                Frame { inner_margin: Margin::symmetric(8, 4), outer_margin: Margin::ZERO, corner_radius: CornerRadius::ZERO, shadow: egui::Shadow::NONE, fill: BG_SIDEBAR, stroke: Stroke::NONE }
                    .show(ui, |ui| {
                        // Intensity
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Intensity").color(TEXT_SECONDARY).size(11.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.add(egui::DragValue::new(&mut light.intensity).speed(0.1).min_decimals(2));
                            });
                        });
                        ui.add_space(SPACING_XS);
                        // Direction
                        ui.label(RichText::new("Direction").color(TEXT_DIM).size(10.0));
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("X").color(ACCENT_DANGER).size(9.0).monospace());
                            ui.add(egui::DragValue::new(&mut light.direction.x).speed(0.01));
                            ui.label(RichText::new("Y").color(ACCENT_SUCCESS).size(9.0).monospace());
                            ui.add(egui::DragValue::new(&mut light.direction.y).speed(0.01));
                            ui.label(RichText::new("Z").color(ACCENT_PRIMARY).size(9.0).monospace());
                            ui.add(egui::DragValue::new(&mut light.direction.z).speed(0.01));
                        });
                        if light.direction.length_squared() > f32::EPSILON { light.direction = light.direction.normalize(); }
                        ui.add_space(SPACING_XS);
                        // Color
                        ui.label(RichText::new("Color").color(TEXT_DIM).size(10.0));
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("R").color(ACCENT_DANGER).size(9.0).monospace());
                            ui.add(egui::DragValue::new(&mut light.color.x).speed(0.05));
                            ui.label(RichText::new("G").color(ACCENT_SUCCESS).size(9.0).monospace());
                            ui.add(egui::DragValue::new(&mut light.color.y).speed(0.05));
                            ui.label(RichText::new("B").color(ACCENT_PRIMARY).size(9.0).monospace());
                            ui.add(egui::DragValue::new(&mut light.color.z).speed(0.05));
                        });
                    });
                ui.add_space(SPACING_XS);
            }

            // Entity details
            if let Some(entity) = *selected_entity {
                let entity_name = runtime.simulation().world().get::<&Name>(entity).map(|n| n.0.clone()).unwrap_or_else(|_| format!("{entity:?}"));

                Frame { inner_margin: Margin::symmetric(8, 4), outer_margin: Margin::ZERO, corner_radius: CornerRadius::ZERO, shadow: egui::Shadow::NONE, fill: BG_SECTION, stroke: Stroke::NONE }
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&entity_name).color(ACCENT_SECONDARY).size(11.0).strong());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(format!("{:?}", entity)).color(TEXT_DIM).size(9.0).monospace());
                            });
                        });
                    });

                Frame { inner_margin: Margin::symmetric(8, 4), outer_margin: Margin::ZERO, corner_radius: CornerRadius::ZERO, shadow: egui::Shadow::NONE, fill: BG_SIDEBAR, stroke: Stroke::NONE }
                    .show(ui, |ui| {
                        if let Ok(mut transform) = runtime.simulation_mut().world_mut().get::<&mut Transform>(entity) {
                            // Location (UE5 calls it Location, not Position)
                            ui.label(RichText::new("LOCATION").color(TEXT_DIM).size(9.0).strong());
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("X").color(ACCENT_DANGER).size(9.0).monospace());
                                ui.add(egui::DragValue::new(&mut transform.translation.x).speed(0.05));
                                ui.label(RichText::new("Y").color(ACCENT_SUCCESS).size(9.0).monospace());
                                ui.add(egui::DragValue::new(&mut transform.translation.y).speed(0.05));
                                ui.label(RichText::new("Z").color(ACCENT_PRIMARY).size(9.0).monospace());
                                ui.add(egui::DragValue::new(&mut transform.translation.z).speed(0.05));
                            });
                            ui.add_space(SPACING_XS);

                            // Scale
                            ui.label(RichText::new("SCALE").color(TEXT_DIM).size(9.0).strong());
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("X").color(ACCENT_DANGER).size(9.0).monospace());
                                ui.add(egui::DragValue::new(&mut transform.scale.x).speed(0.05));
                                ui.label(RichText::new("Y").color(ACCENT_SUCCESS).size(9.0).monospace());
                                ui.add(egui::DragValue::new(&mut transform.scale.y).speed(0.05));
                                ui.label(RichText::new("Z").color(ACCENT_PRIMARY).size(9.0).monospace());
                                ui.add(egui::DragValue::new(&mut transform.scale.z).speed(0.05));
                            });
                        }

                        // Camera FOV
                        if let Ok(mut camera) = runtime.simulation_mut().world_mut().get::<&mut Camera>(entity) {
                            ui.add_space(SPACING_SM);
                            ui.separator();
                            ui.add_space(SPACING_XS);
                            ui.label(RichText::new("CAMERA").color(TEXT_DIM).size(9.0).strong());
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("FOV").color(TEXT_SECONDARY).size(11.0));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let mut deg = camera.fov.to_degrees();
                                    ui.add(egui::DragValue::new(&mut deg).speed(1.0).min_decimals(1).max_decimals(1).range(1.0..=179.0));
                                    ui.label(RichText::new("deg").color(TEXT_DIM).size(9.0));
                                    camera.fov = deg.to_radians().max(0.01);
                                });
                            });
                        }
                    });
            } else {
                Frame { inner_margin: Margin::symmetric(8, 4), outer_margin: Margin::ZERO, corner_radius: CornerRadius::ZERO, shadow: egui::Shadow::NONE, fill: BG_SIDEBAR, stroke: Stroke::NONE }
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(SPACING_LG);
                            ui.label(RichText::new("No Selection").color(TEXT_DIM).size(12.0));
                            ui.add_space(SPACING_XS);
                            ui.label(RichText::new("Click an entity in the viewport").color(TEXT_DIM).size(10.0));
                        });
                    });
            }
        });

    // ---- Viewport (Central Panel — fills remaining space) ----
    CentralPanel::default()
        .frame(Frame { inner_margin: Margin::ZERO, outer_margin: Margin::ZERO, corner_radius: CornerRadius::ZERO, shadow: egui::Shadow::NONE, fill: BG_VIEWPORT, stroke: Stroke::NONE })
        .show(ctx, |ui| {
            let available = ui.available_size();
            let viewport_size = [available.x.max(1.0).round() as u32, available.y.max(1.0).round() as u32];
            commands.viewport_size = Some(viewport_size);

            if let Some(texture_id) = viewport_texture_id {
                let border_stroke = if commands.viewport_hovered { Stroke::new(1.0, VIEWPORT_BORDER_ACTIVE) } else { Stroke::new(1.0, VIEWPORT_BORDER) };
                Frame { inner_margin: Margin::ZERO, outer_margin: Margin::same(0), corner_radius: CornerRadius::ZERO, shadow: egui::Shadow::NONE, fill: BG_VIEWPORT, stroke: border_stroke }
                    .show(ui, |ui| {
                        let response = ui.add(Image::from_texture(SizedTexture::new(texture_id, vec2(viewport_size[0] as f32, viewport_size[1] as f32))).sense(Sense::click_and_drag()));
                        commands.viewport_hovered = response.hovered() || response.dragged();
                        commands.viewport_rect = Some(response.rect);
                        let pointer_pos = response.hover_pos()
                            .or_else(|| ui.ctx().input(|i| i.pointer.latest_pos()))
                            .filter(|&pos| response.rect.contains(pos));
                        commands.viewport_hover_pos = pointer_pos;
                        if response.dragged_by(egui::PointerButton::Secondary) { let d = response.drag_delta(); commands.viewport_look_delta = Some((d.x, d.y)); }
                        if response.drag_started_by(egui::PointerButton::Primary) { commands.viewport_primary_drag_started = true; commands.viewport_primary_drag_start_pos = response.interact_pointer_pos(); }
                        if response.dragged_by(egui::PointerButton::Primary) { let d = response.drag_delta(); if gizmo_state.is_active() { commands.gizmo_drag_delta = Some((d.x, d.y)); } }
                        if response.drag_stopped_by(egui::PointerButton::Primary) && gizmo_state.is_active() { commands.gizmo_drag_end = true; }
                        if response.clicked() { commands.viewport_click_pos = response.interact_pointer_pos(); }
                    });
            } else {
                Frame { inner_margin: Margin::same(16), outer_margin: Margin::ZERO, corner_radius: CornerRadius::ZERO, shadow: egui::Shadow::NONE, fill: BG_VIEWPORT, stroke: Stroke::new(1.0, SEPARATOR) }
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(60.0);
                            ui.label(RichText::new("Pie Engine").color(ACCENT_PRIMARY).size(18.0).strong());
                            ui.add_space(SPACING_SM);
                            ui.label(RichText::new("Viewport").color(TEXT_PRIMARY).size(14.0));
                            ui.add_space(SPACING_SM);
                            ui.label(RichText::new("Right-click drag to look  |  WASD to move").color(TEXT_DIM).size(11.0));
                        });
                    });
            }
        });

    // ---- Status Bar (bottom — UE5 thin status bar) ----
    TopBottomPanel::bottom("status_bar")
        .frame(Frame { inner_margin: Margin::symmetric(8, 2), outer_margin: Margin::ZERO, corner_radius: CornerRadius::ZERO, shadow: egui::Shadow::NONE, fill: BG_TOOLBAR, stroke: Stroke::new(1.0, SEPARATOR) })
        .show(ctx, |ui| {
            let fps = if smoothed_delta > 0.0 { (1.0 / smoothed_delta) as i32 } else { 0 };
            let (dot_color, state_text) = if is_running { (ACCENT_SUCCESS, "Playing") } else { (TEXT_DIM, "Stopped") };
            ui.horizontal(|ui| {
                ui.label(RichText::new("●").color(dot_color).size(7.0));
                ui.add_space(3.0);
                ui.label(RichText::new(state_text).color(TEXT_DIM).size(10.0));
                ui.label(RichText::new("|").color(BORDER_SUBTLE).size(10.0));
                let fps_color = if fps >= 55 { ACCENT_SUCCESS } else if fps >= 30 { ACCENT_ORANGE } else { ACCENT_DANGER };
                ui.label(RichText::new(format!("{fps} FPS")).color(fps_color).size(10.0).monospace());
                ui.label(RichText::new("|").color(BORDER_SUBTLE).size(10.0));
                ui.label(RichText::new(format!("Frame {frame}")).color(TEXT_DIM).size(10.0).monospace());
                ui.label(RichText::new("|").color(BORDER_SUBTLE).size(10.0));
                ui.label(RichText::new(format!("{entity_count} entities")).color(TEXT_DIM).size(10.0));
                ui.label(RichText::new("|").color(BORDER_SUBTLE).size(10.0));
                ui.label(RichText::new(format!("Cam: {:.1}, {:.1}, {:.1}", cam_pos.x, cam_pos.y, cam_pos.z)).color(TEXT_DIM).size(10.0).monospace());
                ui.label(RichText::new("|").color(BORDER_SUBTLE).size(10.0));
                ui.label(RichText::new(format!("Speed: {:.1}", cam_speed)).color(TEXT_DIM).size(10.0).monospace());
                if let GizmoState::Dragging { axis, .. } = gizmo_state {
                    ui.label(RichText::new("|").color(BORDER_SUBTLE).size(10.0));
                    let (al, ac) = match axis { Axis::X => ("X", ACCENT_DANGER), Axis::Y => ("Y", ACCENT_SUCCESS), Axis::Z => ("Z", ACCENT_PRIMARY) };
                    ui.label(RichText::new(format!("Drag {al}")).color(ac).size(10.0).strong());
                }
                if matches!(gizmo_state, GizmoState::UniformScaling { .. }) {
                    ui.label(RichText::new("|").color(BORDER_SUBTLE).size(10.0));
                    ui.label(RichText::new("Scale").color(ACCENT_ORANGE).size(10.0).strong());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("60 Hz  |  Space: Play  N: Step  R: Reload").color(TEXT_DIM).size(9.0));
                });
            });
        });

    if toggle_running { if is_running { runtime.pause(); } else { runtime.resume(); } }
    if step_requested { runtime.pause(); runtime.step(); }
    if reload_requested { commands.reload_scene = true; }
}
