# Pie Engine — Future Systems Backlog

This document tracks engine subsystems that a commercial engine eventually needs but
that **v1 intentionally excludes**. It exists so future work is planned against a
complete picture, not so v1 grows to include these. **Do not start implementing these
without explicit instruction** — they are out of v1 scope by design (see
`pie-engine-project-brief.md` → "Realistic v1 scope" and "Non-Goals").

Each entry notes: what it is, why it's deferred, the likely tech choice, and the
dependency that gates it. Priorities shift as v1 stabilizes.

## 1. Animation

- **What:** skeletal animation, animation blend trees, state machines, IK
  (inverse kinematics).
- **Why deferred:** v1 proves the rendering/simulation/editor architecture; animation
  is a large, self-contained subsystem that doesn't validate any v1 architectural
  decision.
- **Likely tech:** custom skinning + blend-tree implementation on top of the existing
  mesh/transform pipeline; glTF animation import already partially supported by the
  `gltf` crate. IK likely a solver library or custom (FABRIK / CCD).
- **Gated by:** M10 (the renderer needs to support skinned-mesh vertex pipelines
  cleanly before animation is useful).

## 2. Physics

- **What:** rigid bodies, collision detection, character controllers.
- **Why deferred:** v1's simulation core is a fixed-step movement integrator only.
  Real physics is a major subsystem and raises the determinism question (see the
  open question on fixed-point vs floating-point).
- **Likely tech:** `rapier` (determinism-focused Rust physics) is the leading
  candidate; the determinism requirement from M8 networking gates this choice.
- **Gated by:** M8 (determinism strategy must be settled before picking a physics
  backend that the client and server both run).

## 3. Audio

- **What:** 3D spatial audio, mixing/DSP, occlusion.
- **Why deferred:** no audio is needed to validate the v1 architecture.
- **Likely tech:** `kira` (Rust game audio) or `rodio` + `cpal`; spatialization via
  HRTF or simple panning. Occlusion ties into the future physics/collision system.
- **Gated by:** nothing hard; can be picked up independently. Low priority for v1.

## 4. In-game UI / HUD system

- **What:** a runtime UI/HUD system for the shipped game, distinct from the
  `egui`-based editor UI. The editor UI never ships to players.
- **Why deferred:** egui is editor-only; a shipping UI system is a separate concern
  with different constraints (retained vs immediate, theming, accessibility).
- **Likely tech:** `egui` for dev UI only; a separate retained-mode UI system (custom
  or `iced`/`bevy_ui`-style) for shipping HUD. Decision deferred.
- **Gated by:** nothing hard; independent subsystem.

## 5. VFX / particle system

- **What:** GPU particle simulation, emitters, lifetime/force fields, rendering.
- **Why deferred:** large subsystem; benefits from M10's render graph + compute
  culling infrastructure.
- **Likely tech:** GPU-compute particle simulation via wgpu compute shaders; SoA
  layout for SIMD (already aligned with the engine's memory philosophy). CPU
  fallback for low-end.
- **Gated by:** M10 (render graph + compute infrastructure).

## 6. Navigation / AI

- **What:** navmesh generation, pathfinding (A*), behavior trees / state machines.
- **Why deferred:** no gameplay AI in v1.
- **Likely tech:** `polyanya` or custom navmesh; behavior trees likely custom
  (compiled, not interpreted — consistent with the scripting philosophy).
- **Gated by:** physics (navmesh needs collision geometry).

## 7. Terrain / world-building tools

- **What:** heightmap/voxel terrain, terrain editing tools, foliage scattering.
- **Why deferred:** large subsystem; the LOD/culling work in M10 informs how terrain
  should be chunked and streamed.
- **Likely tech:** heightmap terrain with clipmap LOD (NOT virtualized geometry — see
  Non-Goals). Streaming ties into the post-v1 asset streaming model.
- **Gated by:** M10 (LOD) + post-v1 asset streaming.

## 8. Cinematics / sequencer

- **What:** cutscene system, timeline-driven event sequencing, camera animation.
- **Why deferred:** no cinematics in v1.
- **Likely tech:** timeline/track-based sequencer driving the existing transform/
  camera systems; keyframe evaluation compiled, not interpreted.
- **Gated by:** animation (for animated actor tracks) + the transform/camera core.

## 9. Localization, save/load, serialization

- **What:** string localization, save-game serialization, scene serialization.
- **Why deferred:** v1 loads scenes from glTF/cooked pak; no save/load yet.
- **Likely tech:** `serde` for serialization (already a transitive dependency via
  `serde_json`); localization format TBD (custom or `fluent`).
- **Gated by:** nothing hard; the ECS component model needs to be serialization-
  stable first (hecs archetype layout).

## 10. Cross-device, rebindable input system

- **What:** input abstraction over keyboard/mouse/gamepad/touch, with rebindable
  mappings and a action-set model.
- **Why deferred:** v1's editor uses raw winit input; the runtime input model is
  minimal.
- **Likely tech:** `gilrs` for gamepad + winit for kb/mouse, wrapped in an
  action-mapping layer. Action mapping is the load-bearing part for gameplay.
- **Gated by:** M8 (the input command stream with sequence numbers is part of the
  networking model; the action-mapping layer builds on that).

## 11. Lighting-baking tools — LOAD-BEARING

- **What:** lightmap baking, light probe baking, IBL/environment map precomputation.
- **Why deferred but NOT optional:** because real-time GI is a deliberate Non-Goal,
  **baked lighting is the only path to high-fidelity global illumination** in Pie
  Engine. This is load-bearing, not optional: the moment a project needs indirect
  lighting, it needs baking tools. This must be prioritized before any project
  ships, even if other items in this list wait.
- **Likely tech:** CPU/GPU lightmap baking (path-tracing or irradiance caching),
  output to BC-compressed lightmap textures + spherical-harmonic light probes.
- **Gated by:** M10 (the renderer needs lightmap/light-probe sampling in the PBR
  shader). The baking tooling itself is a `pie_tools` subcommand.

## 12. Node-based material editor

- **What:** a visual editor for authoring PBR materials as node graphs.
- **Why deferred:** v1 materials are data (base color, metallic, roughness, textures).
- **Likely tech:** node graph that **compiles to WGSL** (consistent with the
  "compiles, never interpreted" philosophy from the scripting model). This is the
  material-graph analogue of the visual-scripting decision.
- **Gated by:** M10 (shader system needs to be stable enough to codegen into).

## 13. Crash reporting / telemetry

- **What:** minidump capture on crash, telemetry upload, opt-in analytics.
- **Why deferred:** no shipping builds in v1.
- **Likely tech:** `crash-handler` / `minidumper` for capture; custom or
  third-party for upload. Strictly opt-in for privacy.
- **Gated by:** nothing hard; pick up near ship.

## 14. Multi-platform packaging

- **What:** packaging beyond the current single-binary export — per-platform
  installers, app bundles, signing, notarization, console packaging.
- **Why deferred:** v1 export produces a single binary + `assets.pak`, which is
  enough to validate the export pipeline.
- **Likely tech:** extend `pie_tools export` with per-platform packaging steps;
  likely wraps `cargo-bundle` or platform-specific tooling.
- **Gated by:** nothing hard; near ship.

## 15. Plugin / extension ecosystem

- **What:** a plugin/extension model so third-party code can extend the engine
  without forking.
- **Why deferred:** v1 is monolithic by design (explicit control over what's
  running). A plugin model is a later, deliberate addition.
- **Likely tech:** dynamic libraries (`cdylib`) loaded at runtime — the same
  mechanism as the Rust hot-reload scripting tier. A plugin is essentially a
  registered hot-reloadable module. This reuses M9/Tier-1-scripting infrastructure.
- **Gated by:** Tier 1 scripting (hot-reload `cdylib` loading) must exist first.

---

## Relationship to v1 milestones

| Future system | Gated by v1 milestone |
|---|---|
| Animation | M10 |
| Physics | M8 (determinism) |
| VFX / particles | M10 |
| Terrain | M10 + post-v1 streaming |
| Lighting-baking tools | M10 (load-bearing) |
| Node-based material editor | M10 |
| Plugin ecosystem | M9 (hot-reload infra) |

The v1 milestones (M0–M10) are the foundation; everything in this document builds on
top of a completed v1. No item here should pull v1 scope forward without an explicit
decision recorded in `pie-engine-project-brief.md` → "Open questions."
