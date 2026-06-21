# Pie Engine Milestones and Plan

## Purpose

This document turns the project brief into a practical execution plan for building Pie Engine in stages. The priorities are:

- keep `pie_runtime` shippable and free of editor dependencies
- build the client and headless server around the same simulation core
- focus on measurable performance from the start
- delay high-risk scope until the fundamentals are stable

## Delivery principles

- Build the engine in vertical slices, not isolated subsystems with no integration path
- Keep the runtime usable at every major milestone
- Prefer simple, controllable implementations before ambitious systems
- Add instrumentation and benchmarks early so performance decisions are evidence-based
- Treat determinism and shipping constraints as architecture requirements, not polish

## Source of truth for status

**`pie-engine-v1-checklist.md` is the canonical source of truth for item-level v1
completion status.** The per-milestone "Status" blocks below are summaries that
reference the checklist; they must not contradict it. When the two disagree, the
checklist wins and this document is the one that gets fixed. Doc drift is treated
as a bug, not a backlog item.

## Milestone 0: Workspace foundation

### Goal

Establish the repository and crate structure so runtime, editor, and tools can evolve independently.

### Deliverables

- Cargo workspace with `pie_runtime`, `pie_editor`, and `pie_tools`
- shared workspace dependency and release profile configuration
- base runtime configuration for client and headless modes
- starter test coverage for core runtime behavior

### Exit criteria

- the workspace builds on macOS, Windows, and Linux targets
- `pie_runtime` has no editor dependencies
- client and headless startup paths are represented in the API shape

### Status

- complete

## Milestone 1: Runtime application skeleton

### Goal

Turn `pie_runtime` into a real executable foundation for both client and dedicated server modes.

### Deliverables

- runtime app layer with startup, shutdown, and main loop structure
- CLI argument parsing for default client mode and `--headless`
- fixed timestep simulation loop
- separate platform modules for windowed and headless execution
- logging, timing, and basic error handling

### Exit criteria

- runtime launches in client mode and headless mode from the same crate
- the simulation loop ticks deterministically at a fixed rate
- headless mode runs without windowing or rendering dependencies enabled

### Status

- complete
- runtime app startup, shutdown, and deterministic fixed-step update flow are implemented in `pie_runtime`
- client and headless execution paths are exposed through explicit platform modules
- lightweight runtime logging initialization and basic runtime error handling are in place

## Milestone 2: ECS and simulation core

### Goal

Build the gameplay and state-management foundation that both client and server will share.

### Deliverables

- `hecs` world integration
- core components such as transform, velocity, camera, and tags
- system scheduling model for simulation update phases
- resource storage for global engine state
- scene bootstrap API for spawning and updating entities

### Exit criteria

- the engine can simulate a minimal scene entirely through the shared simulation core
- runtime tests cover entity creation, update order, and fixed-step progression
- no gameplay logic depends on editor-only code paths

### Status

- complete
- `hecs` world integration, core components, phased update model, resource storage, and scene bootstrap API are implemented in `pie_runtime`
- runtime tests cover entity spawn, velocity integration, update order, and fixed-step progression

## Milestone 3: Windowing and rendering bootstrap

### Goal

Get a visible client build running with a simple but correct rendering pipeline.

### Deliverables

- `winit` window creation and event loop integration
- `wgpu` device, swapchain, and surface setup
- basic render pipeline for clearing and drawing simple geometry
- camera data flow from simulation to renderer
- frame timing and resize handling

### Exit criteria

- client mode opens a window and renders a stable scene
- headless mode still compiles and runs without rendering
- rendering code is isolated behind the `rendering` feature

### Status

- complete
- `pie_runtime` binary opens a window in client mode and runs headless without the `rendering` feature
- `winit`/`wgpu` live behind the `rendering` feature with resize handling and a camera-driven colored cube draw path
- sample scene bootstrap provides the Milestone 3 visual bring-up target

## Milestone 4: Asset loading and content path

### Goal

Load real content instead of hardcoded scene data.

### Deliverables

- asset registry and handle system
- glTF mesh loading path
- texture loading through `image` and KTX2 support planning or integration
- shader asset layout and loading conventions
- basic scene import path into runtime entities and renderer resources

### Exit criteria

- the engine can load a simple glTF scene with textures
- asset failures are surfaced clearly
- runtime loading path is compatible with future cooked assets

### Status

- complete
- typed asset handles and registry store meshes, textures, and materials
- glTF scene import populates runtime entities with `MeshRenderer` components
- textures load through `image`; shaders load from `{assets_root}/shaders/*.wgsl`
- sample scene lives at `assets/sample/scene.gltf` with external mesh and texture files

## Milestone 5: PBR baseline

### Goal

Reach the first credible visual target for the engine.

### Deliverables

- physically based material system
- directional light and image-based lighting foundation
- normal mapping, metallic-roughness workflow, and tone mapping
- depth buffer, basic visibility control, and frame graph groundwork
- validation scenes for visual regression checks

### Exit criteria

- the engine renders a simple PBR scene with stable, correct-looking output
- frame time is measured and tracked on representative scenes
- renderer architecture still supports headless exclusion cleanly

### Status

- complete
- full PBR shader with GGX distribution, Smith geometry, Fresnel-Schlick, metallic-roughness workflow, normal mapping, ACES tone mapping, and gamma correction
- directional light stored as simulation resource with editor-adjustable intensity/direction/color
- material system supports base color, metallic factor, roughness factor, and normal textures
- validated on fallback cube and glTF sample scenes; all 81 tests pass

## Milestone 6: Editor foundation

### Goal

Add the first useful development tooling without contaminating the runtime.

### Deliverables

- `egui` integration in `pie_editor`
- scene hierarchy panel
- inspector for basic components
- play, pause, and step controls backed by runtime APIs
- simple viewport embedding or synchronized runtime preview

### Exit criteria

- editor launches and drives the shared runtime
- editing workflows use runtime APIs rather than private editor hacks
- runtime remains independently shippable

### Status

- complete
- `pie_editor` opens a window with egui panels, 3D viewport, hierarchy, inspector, and play/pause/step controls
- first-person fly camera (WASD + mouse look) for viewport navigation
- entity picking via ray-AABB intersection with selection highlighting (AABB wireframe)
- scene reload support; runtime remains independently shippable

## Milestone 7: Asset cooking and export pipeline

### Goal

Create the path from editable source assets to shippable builds.

### Deliverables

- `pie_tools` CLI commands for cooking and export
- shader compilation pipeline
- texture compression pipeline
- mesh packing into GPU-ready layouts
- initial `.pak` packaging format

### Exit criteria

- a project can be exported into a runtime binary plus `assets.pak`
- cooked assets load without runtime conversion work
- editor export flow can call into the tools pipeline reliably

### Status

- **Not fully verified — see `pie-engine-v1-checklist.md` section 7 and the V1 exit
  criteria.** The checklist is canonical; this block is a summary.
- Implemented: `pie_tools` CLI with `cook` and `export` commands (clap-based); `.pak`
  binary format (header + type-tagged asset entries: mesh, texture, shader, material);
  cooking pipeline (shaders as WGSL source, textures as raw RGBA, meshes in GPU-ready
  vertex/index layout, materials as PBR params); runtime `load_pak` populates
  `AssetRegistry` directly from cooked data with no decode/parse; `pie_tools export`
  runs cook + `cargo build --release -p pie_runtime` and copies the binary to the
  output directory.
- Tested: `cook_assets` produces the expected asset set (`cook_sample_assets_produces_expected_assets`);
  `PakFile` write/read round-trip and rejection of invalid magic/version/kind
  (5 tests in `pie_runtime::assets::pak`); `load_pak` round-trips a cooked pak into a
  populated `AssetRegistry` (`load_pak_round_trip`).
- **Gap (unchecked checklist item):** the full `export` command producing **both** a runtime
  binary **and** `assets.pak` together is not exercised by an automated test, because
  doing so requires a release build of `pie_runtime` (minutes-scale). The export code
  path exists and the cook + `load_pak` halves are each verified, but the end-to-end
  "binary + packaged assets in one output dir" claim is not yet test-verified. This is
  tracked by the unchecked V1 exit-criterion "Export produces a runtime binary plus
  packaged assets" and will be closed by a CI-runnable integration test (see M9's
  benchmark/regression work, which establishes the CI harness that such a test belongs
  in).

## Milestone 8: Networking prototype

### Goal

Validate the client-server architecture before deeper engine investment.

### Deliverables

- networking spike using either `quinn` or `renet`
- input command stream with sequence numbers
- authoritative server snapshot broadcast
- client prediction and reconciliation prototype
- interpolation for remote entities

### Exit criteria

- two clients can connect to one authoritative server
- local prediction and server correction both function visibly
- networking choice is documented with tradeoffs and next steps

## Milestone 9: Performance systems

### Goal

Add the engine-level optimizations that support the project's identity.

### Deliverables

- `mimalloc` global allocator integration
- frame allocator pattern with `bumpalo`
- profiling zones and frame metrics
- parallel jobs via `rayon`
- performance regression benchmarks for simulation, asset load, and rendering

### Exit criteria

- transient frame allocations avoid the global heap in hot paths
- core engine tasks show measurable benefit from parallel execution where appropriate
- profiling data can guide future optimization work

### Status

- **In progress — see `pie-engine-v1-checklist.md` section 8 (canonical).**
- ✅ Frame timing metrics: `pie_runtime::profiling` module with `FrameTiming`
  (input/sim/render/present phases), `FrameTimingHistory` (bounded ring buffer +
  average + max_total), and `PhaseTimer` (RAII guard). Wired into
  `run_main_loop_with_time_source` (records input + sim per frame; render/present
  zero in the headless/main-loop path, populated by the client/editor render path
  when present). `RuntimeApp` exposes `frame_timing_history()`/`_mut()` for the
  editor overlay and benchmarks. 12 new tests. Cost: a handful of
  `Instant::now()` calls per frame (ns-scale, well below noise floor).
- ⬜ Scoped profiling markers (tracing/tracy, feature-gated, zero-cost when off).
- ⬜ `mimalloc` global allocator.
- ⬜ `bumpalo` frame-temporary allocator.
- ⬜ Benchmark/regression scene with tracked budgets (CI-runnable).

## Milestone 10: Advanced rendering path

### Goal

Pursue the higher-end rendering features only after the baseline is stable.

### Deliverables

- render graph with explicit pass modeling
- automated resource transitions and barriers
- compute-driven culling
- GPU-driven LOD selection
- stress-test scenes for scene complexity scaling

### Exit criteria

- advanced rendering features improve measurable performance on larger scenes
- the complexity remains controllable and does not derail core usability
- the feature set stays aligned with the project's lean-engine philosophy

## Open decisions to resolve during execution

### Networking backend

- compare `quinn` and `renet` with a prototype
- choose based on control, simplicity, and prediction support

### Cooked asset format

- decide whether `.pak` is custom binary or based on an existing container approach
- optimize for fast load, versioning, and tooling simplicity

### Determinism strategy

- start with floating point and strict testing
- revisit fixed-point if cross-platform drift becomes unacceptable

### Job system evolution

- begin with `rayon`
- only move to a custom fiber scheduler when profiling proves it is necessary

### GPU-driven scope boundary

- set explicit limits so culling and LOD do not expand into a runaway virtual-geometry project

## Recommended implementation order

1. finish runtime app skeleton
2. add ECS and shared simulation loop
3. bring up windowing and minimal rendering
4. load real assets
5. reach first PBR visual milestone
6. add editor basics
7. add asset cooking and export
8. prototype networking
9. optimize memory, jobs, and hot paths
10. expand into advanced rendering systems

## Immediate next steps

- Milestone 7 (asset cooking and export pipeline) is implemented; the only open v1
  item is the end-to-end export integration test (tracked in the checklist).
- **Milestone 9 (Performance basics) is the current focus** — see
  `pie-engine-v1-checklist.md` section 8. Items: frame timing metrics, scoped
  profiling markers (feature-gated, zero-cost when off), `mimalloc` global allocator,
  `bumpalo` frame-temporary allocator, and a CI-runnable benchmark/regression scene
  with tracked frame-time budgets.
- Plan KTX2 integration for cooked texture assets (post-v1).

## Realistic v1 scope

### What v1 should be

v1 should prove the engine architecture, not solve every long-term engine problem. A realistic first version is:

- a Rust engine workspace with a clean runtime-editor-tools split
- one runtime binary that supports client and headless server modes
- a fixed-step shared simulation core using `hecs`
- a basic `wgpu` renderer that can open a window and draw a simple 3D scene
- asset loading for a simple glTF scene with textures
- a minimal PBR pipeline good enough to validate visual direction
- a basic editor shell for play, pause, step, hierarchy, and inspector
- an export path that produces a runtime build plus packaged assets

### What v1 should not include

- advanced render graph work
- GPU-driven culling and LOD systems
- full networking and prediction stack
- custom job system beyond `rayon`
- fixed-point simulation unless determinism tests force it
- deep content pipeline optimization beyond what is needed for one working export flow
- broad engine subsystems like audio, scripting, animation graphs, or full prefab tooling

### Why this is the right cut

- it validates the three-crate architecture
- it proves the client and headless split around one simulation core
- it gets a visible graphics result early enough to guide renderer decisions
- it leaves room to test the asset and editor workflows without overbuilding them
- it avoids locking the project into overly ambitious systems before the fundamentals are stable

### V1 milestones

#### V1-A: Runtime foundation

- runtime app structure
- CLI mode selection
- fixed-step loop
- headless and client startup paths

#### V1-B: Simulation core

- `hecs` integration
- transform and camera components
- simple system update phases
- minimal scene bootstrap

#### V1-C: First render

- `winit` window
- `wgpu` device and surface
- simple mesh draw path
- camera-driven scene rendering

#### V1-D: Real content

- glTF mesh loading
- texture loading
- simple material binding
- sample scene import

#### V1-E: Visual baseline

- basic PBR shading
- directional light
- tone mapping
- stable test scene output

#### V1-F: Minimal editor

- `egui` shell
- hierarchy view
- inspector for key components
- play, pause, step controls

#### V1-G: Basic export

- tools command structure
- shader and asset packaging path
- runtime build plus `assets.pak` output

### V1 success definition

V1 is successful when you can:

- launch the engine as a client and see a rendered sample scene
- run the same runtime in headless mode without rendering enabled
- update the same shared simulation core in both modes
- inspect and control the running scene from the editor shell
- export a simple project into a distributable runtime binary plus packaged assets

### Post-v1 priorities

After v1, the best next bets are:

- networking prototype and determinism validation
- performance instrumentation and allocator integration
- improved asset cooking
- renderer scalability features like culling, LOD, and graph-driven pass management
