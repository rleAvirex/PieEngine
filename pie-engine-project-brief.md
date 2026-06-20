# Pie Engine

**P**erformance **I**s **E**verything

A Rust game engine focused on raw performance and high-quality graphics, without the runtime overhead of modern engine features like Nanite or Lumen.

## Goal

Most modern engines (Unreal 5, Unity) ship with heavyweight, always-on systems that trade performance for convenience. Pie Engine takes the opposite bet: a lean, deliberate feature set that gives developers full control over what's running, with AAA-quality PBR visuals and tight frame times.

## Tech stack

| Area | Choice |
|---|---|
| Language | Rust |
| Rendering API | wgpu |
| Windowing | winit |
| Editor UI | egui |
| Math | glam |
| ECS | hecs (not bevy_ecs — avoids inheriting Bevy's scheduler, plugin system, and app structure) |
| Allocator | mimalloc as global allocator, bumpalo for per-frame transient data |
| Job system | rayon to start; revisit a custom fiber-based job system once data-parallel scheduling isn't enough |
| Asset loading | gltf, image, ktx2 |
| Networking | quinn (QUIC) or renet |

## Platforms

- Windows, Linux, macOS at launch
- Consoles planned for later
- Dedicated server builds (headless) on Windows, Linux, macOS

## Architecture

The engine is split into three crates so the editor and runtime are fully decoupled:

```
pie-engine/
├── pie_runtime/     # ships with the game and the server — no editor code
├── pie_editor/      # depends on pie_runtime, adds dev tooling only
└── pie_tools/       # CLI: asset cooker, export/build pipeline
```

**pie_runtime** contains everything that ships: ECS, renderer, audio, input, asset loader, networking, and the simulation core. It must compile and run with zero editor dependencies.

**pie_editor** depends on pie_runtime and adds egui panels, asset hot-reload, scene hierarchy, gizmos, and play/pause/step controls. This crate is never shipped to players.

**pie_tools** is the CLI used for cooking assets (shader compilation, texture compression, mesh packing) and running platform exports.

### Export pipeline

1. User clicks "Export" in the editor
2. `pie_tools` cooks assets into a `.pak` (compiled shaders, compressed textures, packed meshes)
3. `cargo build --release -p pie_runtime` cross-compiled for the target platform
4. Output: a single binary + `assets.pak`, nothing else

### Client / server split

A single `pie_runtime` binary handles both client and dedicated server roles:

- Default: full client (renderer, audio, input, prediction)
- `--headless` flag, or a `rendering` Cargo feature disabled at compile time for production server builds: no wgpu, no winit, no audio — smaller binary, no GPU driver needed
- Both modes run the exact same `SimulationCore` and gameplay logic — this is what keeps client and server deterministic and in sync

### Networking model

Server-authoritative with client-side prediction and reconciliation:

1. Client captures input, tags it with a sequence number, predicts the result locally and instantly
2. Client sends the input to the server
3. Server runs the same simulation code authoritatively, validates the input, and broadcasts a snapshot (state + last acknowledged input sequence)
4. Client receives the snapshot, discards acknowledged inputs, hard-sets to server state, and replays any unacknowledged inputs
5. If predicted and replayed state match, the player sees nothing change. If they don't match, the correction is absorbed smoothly over a frame or two

Other players (non-local) are not predicted — they're interpolated between the last two received snapshots with a small buffer delay (~100ms).

Determinism matters: client and server must run identical simulation code. Watch out for floating-point nondeterminism across different CPUs/compiler flags — consider fixed-point math for physics if this becomes an issue.

## Performance principles

These apply across every system in the engine, not just the renderer.

**Memory**
- Global allocator is mimalloc, not system malloc
- Anything allocated and discarded within a single frame (render commands, transient query results) comes from a frame-scoped bump allocator, never the global heap
- Structure-of-Arrays layout wherever SIMD auto-vectorization can help (particles, transform batches) — hecs already gives this for ECS component storage

**Compile-time**
- Release profile uses `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`
- `target-cpu=native` is fine for local dev/benchmark builds, never for shipped binaries (breaks portability across CPU generations)

**Runtime**
- GPU-driven culling and LOD selection (compute shaders) over CPU-side loops once scene complexity demands it — a deliberately simpler, controllable alternative to Nanite-style virtualized geometry
- Render graph with automatic barrier/transition insertion to cut redundant GPU state changes

**Asset pipeline**
- Shaders compiled to SPIR-V/WGSL bytecode at cook time — zero shader-compilation stutter during gameplay
- Textures compressed (BC7 desktop, ASTC if mobile/console ever enters scope) at cook time, not at load
- Meshes packed in GPU-ready vertex layouts so loading is a direct upload, no CPU-side reformatting

## Open questions to revisit

- Networking crate (quinn vs renet vs custom QUIC/UDP layer)
- Asset format for the cooked `.pak` (custom binary vs existing format)
- Fixed-point vs floating-point physics for cross-platform determinism
- When to move from rayon to a custom fiber-based job system
- How far to take GPU-driven culling before it becomes its own "Nanite-lite" scope risk
