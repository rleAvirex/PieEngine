# Pie Engine V1 Checklist

## Goal

Deliver a realistic v1 that proves the engine structure, shared runtime model, rendering direction, editor integration, and export flow without taking on unnecessary high-risk scope.

## V1 boundaries

### Include

- runtime, editor, and tools crate split
- client and headless runtime modes
- fixed-step shared simulation core
- `hecs`-based scene update path
- basic `wgpu` rendering with a simple 3D scene
- glTF and texture loading for one sample scene path
- minimal PBR baseline
- minimal editor shell
- basic asset packaging and export flow

### Exclude

- advanced render graph systems
- GPU-driven culling and LOD
- production-grade networking
- custom job system
- fixed-point simulation by default
- large extra subsystems not needed for the first vertical slice

## Checklist

### 0. Workspace and project hygiene

- [x] Create Cargo workspace with `pie_runtime`, `pie_editor`, and `pie_tools`
- [x] Set shared release profile defaults
- [x] Add top-level README describing crate responsibilities
- [x] Add formatting and linting commands to the workflow
- [x] Add basic CI for `cargo fmt`, `cargo check`, and `cargo test`

### 1. Runtime foundation

- [x] Add runtime application entry layer
- [x] Add CLI parsing for default client mode and `--headless`
- [x] Add startup and shutdown flow
- [x] Add fixed timestep main loop
- [x] Add timing utilities and frame counters
- [x] Add structured logging setup
- [x] Ensure headless mode compiles without rendering dependencies

### 2. Simulation core

- [x] Add `hecs` dependency
- [x] Define core components: transform, velocity, camera, name/tag
- [x] Add resource storage for global runtime state
- [x] Define simulation update phases
- [x] Add a minimal scene bootstrap path
- [x] Add tests for entity spawn, update, and frame stepping

### 3. Rendering bootstrap

- [x] Add `winit` behind the `rendering` feature
- [x] Add `wgpu` behind the `rendering` feature
- [x] Create window and event loop integration
- [x] Initialize GPU instance, adapter, device, and surface
- [x] Add resize handling and frame presentation
- [x] Render a clear color frame reliably
- [x] Render a simple mesh with a movable camera

### 4. Asset loading

- [x] Add asset handle and registry model
- [x] Add glTF loading for meshes
- [x] Add texture loading path
- [x] Define shader asset conventions
- [x] Import one sample scene into runtime entities and renderer resources
- [x] Add useful load failure messages

### 5. Visual baseline

- [x] Add basic PBR material data model
- [x] Add directional light support
- [x] Add metallic-roughness material workflow
- [x] Add normal mapping if feasible within the first pass
- [x] Add tone mapping
- [x] Validate output on one representative test scene

### 6. Editor shell

- [x] Integrate `egui` into `pie_editor`
- [x] Launch and control the shared runtime from the editor
- [x] Add hierarchy panel
- [x] Add inspector for core components
- [x] Add play, pause, and step controls
- [x] Show a runtime viewport or synchronized preview

### 7. Tools and export

- [x] Define `pie_tools` command structure
- [x] Add asset cooking command skeleton
- [x] Add shader compilation path
- [x] Add first packaging format for `assets.pak`
- [x] Add export command that builds runtime plus packaged assets
- [x] Verify exported build loads cooked content

### 8. Performance basics

- [ ] Add frame timing metrics
- [ ] Add simple profiling markers or scoped timing
- [ ] Add `mimalloc` global allocator
- [ ] Add frame-temporary allocation strategy with `bumpalo`
- [ ] Add a small benchmark or measurement scene for regressions

## V1 exit criteria

- [x] Client mode launches and renders a sample scene
- [x] Headless mode launches and runs the same shared simulation logic
- [x] Runtime remains free of editor dependencies
- [x] Editor can inspect and control the running scene
- [ ] Export produces a runtime binary plus packaged assets
- [x] Core tests pass cleanly

## Recommended build order

1. runtime foundation
2. simulation core
3. rendering bootstrap
4. asset loading
5. visual baseline
6. editor shell
7. tools and export
8. performance basics

## Later, not now

- networking prototype
- determinism hardening beyond initial testing
- render graph
- GPU-driven culling
- LOD system
- custom job system
