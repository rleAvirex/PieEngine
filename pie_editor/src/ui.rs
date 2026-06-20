//! Editor UI panels — toolbar, hierarchy, inspector, viewport, and status bar.

use egui::{
    CentralPanel, Context, Image, Key, Sense, SidePanel, TextureId, TopBottomPanel,
    load::SizedTexture, vec2, CornerRadius, Stroke, Margin, Frame,
};
use hecs::Entity;
use pie_runtime::components::{DirectionalLight, Name, Transform};
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

    // ---- Toolbar ----
    TopBottomPanel::top("toolbar")
        .frame(Frame {
            inner_margin: Margin::symmetric(8, 4),
            outer_margin: Margin::ZERO,
            corner_radius: CornerRadius::ZERO,
            shadow: egui::Shadow::NONE,
            fill: BG_TOOLBAR,
            stroke: Stroke::new(1.0, SEPARATOR),
        })
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let title = RichText::new("◆  PIE EDITOR").color(ACCENT_PRIMARY).size(15.0).strong();
                ui.label(title);
                ui.add_space(SPACING_LG);
                ui.add(egui::Separator::default().spacing(0.0).vertical());
                ui.add_space(SPACING_MD);
                let scene_label = RichText::new(format!("⬡  {scene_path}")).color(TEXT_SECONDARY).size(11.0);
                ui.label(scene_label);
                ui.add_space(SPACING_LG);
                ui.add(egui::Separator::default().spacing(0.0).vertical());
                ui.add_space(SPACING_MD);

                let reload_btn = egui::Button::new(RichText::new("↻  Reload").color(TEXT_PRIMARY).size(12.0))
                    .corner_radius(ROUNDING_SM).fill(BG_WIDGET).stroke(Stroke::new(1.0, BORDER_SUBTLE));
                if ui.add(reload_btn).on_hover_text("Reload scene (R)").clicked() { commands.reload_scene = true; }
                ui.add_space(SPACING_XS);

                if is_running {
                    let pause_btn = egui::Button::new(RichText::new("⏸  Pause").color(TEXT_PRIMARY).size(12.0))
                        .corner_radius(ROUNDING_SM).fill(BG_WIDGET).stroke(Stroke::new(1.0, BORDER_SUBTLE));
                    if ui.add(pause_btn).clicked() { runtime.pause(); }
                } else {
                    let play_btn = egui::Button::new(RichText::new("▶  Play").color(ACCENT_PLAY).size(12.0))
                        .corner_radius(ROUNDING_SM).fill(ACCENT_PLAY.linear_multiply(0.12)).stroke(Stroke::new(1.0, ACCENT_PLAY.linear_multiply(0.4)));
                    if ui.add(play_btn).clicked() { runtime.resume(); }
                }
                ui.add_space(SPACING_XS);

                let step_btn = egui::Button::new(RichText::new("⏭  Step").color(TEXT_PRIMARY).size(12.0))
                    .corner_radius(ROUNDING_SM).fill(BG_WIDGET).stroke(Stroke::new(1.0, BORDER_SUBTLE));
                if ui.add(step_btn).on_hover_text("Step one frame (N)").clicked() { runtime.pause(); runtime.step(); }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (dot_color, state_text) = if is_running { (ACCENT_SUCCESS, "Running") } else { (TEXT_DIM, "Paused") };
                    ui.label(RichText::new(state_text).color(TEXT_SECONDARY).size(11.0));
                    ui.add_space(SPACING_XS);
                    ui.label(RichText::new("●").color(dot_color).size(10.0));
                    ui.add_space(SPACING_LG);
                    ui.label(RichText::new(format!("Frame {frame}")).color(TEXT_SECONDARY).size(11.0).monospace());
                    ui.add_space(SPACING_LG);
                    ui.label(RichText::new("Space: toggle  ·  N: step  ·  R: reload").color(TEXT_DIM).size(10.0));
                });
            });
        });

    // ---- Hierarchy Panel ----
    SidePanel::left("hierarchy")
        .resizable(true).default_width(280.0)
        .frame(Frame { inner_margin: Margin::same(8), outer_margin: Margin::ZERO, corner_radius: CornerRadius::ZERO, shadow: egui::Shadow::NONE, fill: BG_SIDEBAR, stroke: Stroke::new(1.0, SEPARATOR) })
        .show(ctx, |ui| {
            ui.label(RichText::new("SCENE").color(ACCENT_PRIMARY).size(11.0).strong());
            ui.add_space(SPACING_SM);
            ui.horizontal(|ui| {
                let stat = |l: &str, v: usize| RichText::new(format!("{l}: {v}")).color(TEXT_SECONDARY).size(10.0).monospace();
                ui.label(stat("entities", entity_count)); ui.add_space(SPACING_MD);
                ui.label(stat("meshes", mesh_count)); ui.add_space(SPACING_MD);
                ui.label(stat("materials", material_count)); ui.add_space(SPACING_MD);
                ui.label(stat("textures", texture_count));
            });
            ui.add_space(SPACING_XS);
            ui.label(RichText::new(format!("⬡  {scene_path}")).color(TEXT_DIM).size(10.0));
            ui.add_space(SPACING_MD); ui.separator(); ui.add_space(SPACING_SM);
            ui.label(RichText::new("HIERARCHY").color(TEXT_SECONDARY).size(10.0).strong());
            ui.add_space(SPACING_SM);

            let mut entities = Vec::new();
            for (entity, _) in runtime.simulation().world().query::<&Transform>().iter() { entities.push(entity); }
            for entity in entities {
                let name = runtime.simulation().world().get::<&Name>(entity).map(|n| n.0.clone()).unwrap_or_else(|_| format!("{entity:?}"));
                let selected = *selected_entity == Some(entity);
                let text = if selected { RichText::new(format!("  ▸  {name}")).color(ACCENT_PRIMARY).size(12.0) } else { RichText::new(format!("  ○  {name}")).color(TEXT_PRIMARY).size(12.0) };
                if ui.selectable_label(selected, text).clicked() { *selected_entity = Some(entity); }
            }
        });

    // ---- Inspector Panel ----
    SidePanel::right("inspector")
        .resizable(true).default_width(320.0)
        .frame(Frame { inner_margin: Margin::same(8), outer_margin: Margin::ZERO, corner_radius: CornerRadius::ZERO, shadow: egui::Shadow::NONE, fill: BG_SIDEBAR, stroke: Stroke::new(1.0, SEPARATOR) })
        .show(ctx, |ui| {
            ui.label(RichText::new("INSPECTOR").color(ACCENT_PRIMARY).size(11.0).strong());
            ui.add_space(SPACING_SM); ui.separator(); ui.add_space(SPACING_MD);

            if let Some(light) = runtime.simulation_mut().resource_mut::<DirectionalLight>() {
                ui.collapsing(RichText::new("☀  Lighting").color(TEXT_PRIMARY).size(12.0).strong(), |ui| {
                    ui.add_space(SPACING_SM);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Intensity").color(TEXT_SECONDARY).size(11.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { ui.add(egui::DragValue::new(&mut light.intensity).speed(0.1).min_decimals(2)); });
                    });
                    ui.add_space(SPACING_XS);
                    ui.label(RichText::new("Direction").color(TEXT_SECONDARY).size(11.0));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("X").color(TEXT_DIM).size(10.0).monospace()); ui.add(egui::DragValue::new(&mut light.direction.x).speed(0.01));
                        ui.label(RichText::new("Y").color(TEXT_DIM).size(10.0).monospace()); ui.add(egui::DragValue::new(&mut light.direction.y).speed(0.01));
                        ui.label(RichText::new("Z").color(TEXT_DIM).size(10.0).monospace()); ui.add(egui::DragValue::new(&mut light.direction.z).speed(0.01));
                    });
                    if light.direction.length_squared() > f32::EPSILON { light.direction = light.direction.normalize(); }
                    ui.add_space(SPACING_XS);
                    ui.label(RichText::new("Color").color(TEXT_SECONDARY).size(11.0));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("R").color(TEXT_DIM).size(10.0).monospace()); ui.add(egui::DragValue::new(&mut light.color.x).speed(0.05));
                        ui.label(RichText::new("G").color(TEXT_DIM).size(10.0).monospace()); ui.add(egui::DragValue::new(&mut light.color.y).speed(0.05));
                        ui.label(RichText::new("B").color(TEXT_DIM).size(10.0).monospace()); ui.add(egui::DragValue::new(&mut light.color.z).speed(0.05));
                    });
                });
            }
            ui.add_space(SPACING_SM); ui.separator(); ui.add_space(SPACING_MD);

            if let Some(entity) = *selected_entity {
                let entity_name = runtime.simulation().world().get::<&Name>(entity).map(|n| n.0.clone()).unwrap_or_else(|_| format!("{entity:?}"));
                ui.label(RichText::new(format!("◆  {entity_name}")).color(ACCENT_SECONDARY).size(12.0).strong());
                ui.label(RichText::new(format!("{entity:?}")).color(TEXT_DIM).size(10.0).monospace());
                ui.add_space(SPACING_SM);
                if let Ok(mut transform) = runtime.simulation_mut().world_mut().get::<&mut Transform>(entity) {
                    ui.label(RichText::new("Position").color(TEXT_SECONDARY).size(11.0).strong());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("X").color(TEXT_DIM).size(10.0).monospace()); ui.add(egui::DragValue::new(&mut transform.translation.x).speed(0.05));
                        ui.label(RichText::new("Y").color(TEXT_DIM).size(10.0).monospace()); ui.add(egui::DragValue::new(&mut transform.translation.y).speed(0.05));
                        ui.label(RichText::new("Z").color(TEXT_DIM).size(10.0).monospace()); ui.add(egui::DragValue::new(&mut transform.translation.z).speed(0.05));
                    });
                    ui.add_space(SPACING_XS);
                    ui.label(RichText::new("Scale").color(TEXT_SECONDARY).size(11.0).strong());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("X").color(TEXT_DIM).size(10.0).monospace()); ui.add(egui::DragValue::new(&mut transform.scale.x).speed(0.05));
                        ui.label(RichText::new("Y").color(TEXT_DIM).size(10.0).monospace()); ui.add(egui::DragValue::new(&mut transform.scale.y).speed(0.05));
                        ui.label(RichText::new("Z").color(TEXT_DIM).size(10.0).monospace()); ui.add(egui::DragValue::new(&mut transform.scale.z).speed(0.05));
                    });
                } else {
                    ui.label(RichText::new("Selected entity has no editable transform.").color(TEXT_DIM).size(11.0));
                }
            } else {
                ui.label(RichText::new("No entity selected.").color(TEXT_DIM).size(12.0));
                ui.add_space(SPACING_XS);
                ui.label(RichText::new("Click an entity in the viewport to select it.").color(TEXT_DIM).size(10.0));
            }
        });

    // ---- Viewport (Central Panel) ----
    CentralPanel::default()
        .frame(Frame { inner_margin: Margin::ZERO, outer_margin: Margin::ZERO, corner_radius: CornerRadius::ZERO, shadow: egui::Shadow::NONE, fill: BG_VIEWPORT, stroke: Stroke::NONE })
        .show(ctx, |ui| {
            let available = ui.available_size();
            let viewport_size = [available.x.max(1.0).round() as u32, available.y.max(1.0).round() as u32];
            commands.viewport_size = Some(viewport_size);

            if let Some(texture_id) = viewport_texture_id {
                let border_stroke = if commands.viewport_hovered { Stroke::new(1.5, VIEWPORT_BORDER_ACTIVE) } else { Stroke::new(1.0, VIEWPORT_BORDER) };
                Frame { inner_margin: Margin::ZERO, outer_margin: Margin::same(0), corner_radius: ROUNDING_SM, shadow: egui::Shadow::NONE, fill: BG_VIEWPORT, stroke: border_stroke }
                    .show(ui, |ui| {
                        let response = ui.add(Image::from_texture(SizedTexture::new(texture_id, vec2(viewport_size[0] as f32, viewport_size[1] as f32))).sense(Sense::click_and_drag()));
                        commands.viewport_hovered = response.hovered() || response.dragged();
                        commands.viewport_rect = Some(response.rect);
                        // Use ctx pointer position for hover detection — response.hover_pos()
                        // returns None during drag, but we still need the position for gizmo
                        // hover tracking. Fall back to latest pointer position in the rect.
                        let pointer_pos = response.hover_pos()
                            .or_else(|| ui.ctx().input(|i| i.pointer.latest_pos()))
                            .filter(|&pos| response.rect.contains(pos));
                        commands.viewport_hover_pos = pointer_pos;
                        if response.dragged_by(egui::PointerButton::Secondary) { let d = response.drag_delta(); commands.viewport_look_delta = Some((d.x, d.y)); }
                        if response.drag_started_by(egui::PointerButton::Primary) { commands.viewport_primary_drag_started = true; commands.viewport_primary_drag_start_pos = response.interact_pointer_pos(); }
                        if response.dragged_by(egui::PointerButton::Primary) { let d = response.drag_delta(); if matches!(gizmo_state, GizmoState::Dragging { .. }) { commands.gizmo_drag_delta = Some((d.x, d.y)); } }
                        if response.drag_stopped_by(egui::PointerButton::Primary) && matches!(gizmo_state, GizmoState::Dragging { .. }) { commands.gizmo_drag_end = true; }
                        if response.clicked() { commands.viewport_click_pos = response.interact_pointer_pos(); }
                    });
            } else {
                Frame { inner_margin: Margin::same(16), outer_margin: Margin::ZERO, corner_radius: CornerRadius::ZERO, shadow: egui::Shadow::NONE, fill: BG_VIEWPORT, stroke: Stroke::new(1.0, VIEWPORT_BORDER) }
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(60.0);
                            ui.label(RichText::new("◆").color(ACCENT_PRIMARY).size(28.0));
                            ui.add_space(SPACING_SM);
                            ui.label(RichText::new("Viewport").color(TEXT_PRIMARY).size(16.0).strong());
                            ui.add_space(SPACING_SM);
                            ui.label(RichText::new("The runtime preview comes next.\nRight-click drag to look, WASD to move.").color(TEXT_SECONDARY).size(11.0));
                        });
                    });
            }
        });

    // ---- Status Bar ----
    TopBottomPanel::bottom("status_bar")
        .frame(Frame { inner_margin: Margin::symmetric(8, 2), outer_margin: Margin::ZERO, corner_radius: CornerRadius::ZERO, shadow: egui::Shadow::NONE, fill: BG_TOOLBAR, stroke: Stroke::new(1.0, SEPARATOR) })
        .show(ctx, |ui| {
            let fps = if smoothed_delta > 0.0 { (1.0 / smoothed_delta) as i32 } else { 0 };
            let (dot_color, state_text) = if is_running { (ACCENT_SUCCESS, "Running") } else { (TEXT_DIM, "Paused") };
            ui.horizontal(|ui| {
                ui.label(RichText::new("●").color(dot_color).size(9.0)); ui.add_space(4.0);
                ui.label(RichText::new(state_text).color(TEXT_SECONDARY).size(10.0)); ui.add_space(SPACING_MD);
                let fps_color = if fps >= 55 { ACCENT_SUCCESS } else if fps >= 30 { ACCENT_PRIMARY } else { ACCENT_PLAY };
                ui.label(RichText::new(format!("{fps} FPS")).color(fps_color).size(10.0).monospace()); ui.add_space(SPACING_MD);
                ui.label(RichText::new(format!("Frame {frame}")).color(TEXT_SECONDARY).size(10.0).monospace()); ui.add_space(SPACING_MD);
                ui.label(RichText::new(format!("{entity_count} entities")).color(TEXT_DIM).size(10.0)); ui.add_space(SPACING_SM);
                ui.label(RichText::new(format!("{mesh_count} meshes")).color(TEXT_DIM).size(10.0)); ui.add_space(SPACING_MD);
                ui.label(RichText::new(format!("Cam: ({:.1}, {:.1}, {:.1})", cam_pos.x, cam_pos.y, cam_pos.z)).color(TEXT_DIM).size(10.0).monospace()); ui.add_space(SPACING_SM);
                ui.label(RichText::new(format!("Speed: {:.1}", cam_speed)).color(TEXT_DIM).size(10.0).monospace());
                if let GizmoState::Dragging { axis, .. } = gizmo_state {
                    ui.add_space(SPACING_MD);
                    let (al, ac) = match axis { Axis::X => ("X", ACCENT_DANGER), Axis::Y => ("Y", ACCENT_SUCCESS), Axis::Z => ("Z", ACCENT_SECONDARY) };
                    ui.label(RichText::new(format!("Dragging {al}")).color(ac).size(10.0).strong());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { ui.label(RichText::new("60 Hz fixed step").color(TEXT_DIM).size(9.0)); });
            });
        });

    if toggle_running { if is_running { runtime.pause(); } else { runtime.resume(); } }
    if step_requested { runtime.pause(); runtime.step(); }
    if reload_requested { commands.reload_scene = true; }
}
