# Pie Engine

**P**erformance **I**s **E**verything

A Rust game engine focused on raw performance and high-quality graphics, without the runtime overhead of modern engine features like Nanite or Lumen.

## Goal

Most modern engines (Unreal 5, Unity) ship with heavyweight, always-on systems that trade performance for convenience. Pie Engine takes the opposite bet: a lean, deliberate feature set that gives developers full control over what's running, with AAA-quality PBR visuals and tight frame times.

## Philosophy

Pie Engine takes the opposite bet from Unreal 5 / Unity. Those engines ship
heavyweight, always-on systems (Nanite, Lumen, etc.) that trade raw performance for
convenience and trade developer control for automation. Pie Engine instead aims for:

- **AAA-quality PBR visuals and tight, predictable frame times** — visually competitive
  with UE5-class rendering (physically based materials, real shadows, real
  reflections/IBL, post-processing, high-fidelity lighting) — but achieved with a
  lean, deliberate, opt-in feature set, not virtualized/always-on heavyweight systems.
- **No Nanite-equivalent:** no virtualized micropolygon geometry. Use conventional
  meshes, manual/automatic LOD chains, and efficient culling instead.
- **No Lumen-equivalent:** no fully dynamic, software-raytraced global illumination
  running by default. Prefer baked/precomputed lighting (lightmaps, light probes,
  IBL/environment maps), with real-time dynamic lights layered on top in a way whose
  cost is visible and bounded.
- **Every system must have a knowable, documented performance cost.** If a feature
  can't be toggled off or its cost can't be measured/profiled, it doesn't belong in
  the engine yet.
- **Decisions favor explicitness and control over "magic" automation**, consistent
  with using `hecs` instead of `bevy_ecs`, and consistent with the existing tech stack
  (Rust, wgpu, winit, egui, glam, hecs, mimalloc, bumpalo, rayon, gltf/image/ktx2,
  quinn/renet).

When in doubt about whether a feature fits the philosophy, prefer the option that
keeps the system lean, measurable, and toggleable over the option that is more
"automatic" or "convenient."

## Non-Goals (things Pie Engine will never do by default)

These are explicit boundaries that protect against scope creep across long-running,
multi-session work. Each can be revisited only with a deliberate, written decision
that justifies the cost against the philosophy above.

- **No virtualized / micropolygon geometry.** Conventional meshes + LOD chains + culling
  only. A "Nanite-lite" is the single biggest scope-creep risk and is explicitly out.
- **No mandatory real-time global illumination.** Real-time GI (software raytraced
  diffuse interreflection, irradiance cascades, etc.) is never on by default. Baked
  lighting (lightmaps, light probes, IBL) is the baseline; real-time dynamic lights
  are layered on top with a visible, bounded cost.
- **No interpreted-graph execution at runtime for visual scripting.** Any visual
  scripting (Blueprint-equivalent) compiles/transpiles down to Lua or Rust ahead of
  time. A walked-graph runtime is a documented failure mode of UE5 Blueprints at scale
  and is exactly the kind of hidden, unbounded cost the engine is positioned against.
- **No always-on system whose cost can't be measured or toggled off.** If it can't be
  feature-flagged and profiled, it doesn't ship.
- **No automatic "magic" scheduler/plugin/app framework.** `hecs` (not `bevy_ecs`) is
  the deliberate choice to avoid inheriting a scheduler, plugin system, and app
  structure the team didn't design.
- **No heavyweight always-on editor systems in the runtime.** The editor is a separate
  crate (`pie_editor`) and the runtime (`pie_runtime`) ships with zero editor
  dependencies.
- **No Python in the in-game runtime.** Python is permitted only for editor/pipeline
  tooling (asset import scripts, build automation), never for gameplay logic (see the
  scripting model below).

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
| Scripting (tiered) | Rust (AOT, with hot-reload via dynamic linking) for performance tier; Lua via `mlua` for iteration tier; optional visual scripting that compiles to Lua/Rust (never interpreted at runtime). See "Scripting model" below. |
| Profiling | `tracing` + `tracing-tracy` behind a `profiling` feature flag (zero-cost when disabled); lightweight always-on `FrameTiming` metrics for the editor overlay and benchmarks. See M9. |

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

## Scripting model

Pie Engine needs a way for users to write gameplay logic that is easy enough for
iteration but doesn't betray the "performance is everything" identity. The model is
**tiered**, mirroring the rendering philosophy: every tier has a known, documented
performance cost, and the expensive tiers are opt-in.

### Tier 1 — Rust, AOT-compiled (performance tier)

- For core engine systems and any frame-time-sensitive gameplay code.
- **Hot-reload via dynamic linking** (`libloading` / `hot-lib-reloader`-style
  pattern): gameplay Rust code is compiled into a `cdylib` that the runtime loads
  and can reload on file change, so iterating in Rust doesn't require a full binary
  recompile every time. This is the primary fix for Rust's "easy iteration" problem —
  solve it with hot-reload, not by abandoning Rust.
- Cost: zero runtime overhead (native code), reload cost only in dev.

### Tier 2 — Embedded Lua via `mlua` (iteration tier)

- For day-to-day gameplay logic that needs fast iteration without recompilation.
- **Lua, not Python.** Lua is smaller, faster to embed, faster to execute, and is the
  proven choice in commercial engines for exactly this role. Python is considered only
  for editor/pipeline tooling (asset import scripts, build automation), never for
  in-game runtime logic — its interpreter/GIL overhead conflicts with the engine's
  core philosophy.
- Cost: bounded, knowable per-script-call cost; the engine never calls into Lua in a
  hot inner loop without an explicit, profiled decision.

### Tier 3 — Visual scripting (optional, later)

- Blueprint-equivalent, deliberately deferred.
- **Must compile or transpile down to Lua or Rust** ahead of time, never interpreted
  node-by-node at runtime. A walked-graph runtime is exactly the kind of hidden,
  unbounded cost the engine is positioned against (a documented failure mode of UE5
  Blueprints at scale). This is also captured in Non-Goals.

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

Some of these are now **decided** (marked ✅) and recorded here so downstream
systems depend on a stable choice; others remain **open** (marked ❓) and will be
resolved with a prototype or measurement before the relevant milestone.

### ✅ Scripting / programming model — DECIDED

Tiered: Rust (AOT + hot-reload via `cdylib`/`libloading`) for the performance tier,
embedded Lua via `mlua` for the iteration tier, optional visual scripting that
compiles to Lua/Rust (never interpreted at runtime). See "Scripting model" above and
Non-Goals. Python is excluded from the runtime. This decision is foundational:
hot-reload tooling, the editor's script panel, and the eventual visual-scripting
compiler all depend on it.

### ✅ Asset streaming / threading model — DECIDED (v1 answer)

**v1: everything loads upfront on a single thread; streaming is post-v1.** The
runtime loads all cooked assets from `assets.pak` into the `AssetRegistry` at
startup (or scene-load), synchronously, on the main thread. There is no background
streaming queue, no async asset IO, and no per-frame asset loading in v1. This is
deliberate:

- It keeps the v1 loading path simple and bug-free (no race conditions, no
  partial-load states, no GPU upload threading).
- It makes the startup cost a single, measurable number (total pak load time) —
  consistent with the "knowable, documented cost" principle.
- It avoids baking streaming assumptions into M10's LOD/culling work prematurely.

**Post-v1** (tracked in `pie-engine-future-systems.md`): a background asset-loading
thread + a handle-based streaming queue, gated behind a feature flag, for open-world
/ large-scene use cases. The `AssetRegistry`/`Handle` design already anticipates
this (handles are stable indices), so the upgrade path is non-breaking. This decision
is recorded now so M10's culling/LOD code doesn't assume per-frame asset arrival.

### ✅ Networking crate — DECIDED: `renet` (+ `renetcode` for the transport layer)

Resolved by the M8 spike analysis. Reasoning:

1. **The brief's model maps directly onto renet.** The spec is server-authoritative
   + client-side prediction + snapshot interpolation. `renet` is purpose-built for
   exactly this; `renetcode` adds the connection handshake, optional encryption,
   and packet stats the model needs. `quinn` (QUIC) would require building the
   message-reliability-channel layer on top of QUIC streams — re-implementing what
   renet already provides.
2. **Message-oriented > stream-oriented for game netcode.** Game packets (input
   commands, snapshots) are discrete messages, not byte streams. renet's channel
   model (ReliableUnordered for input commands, Unreliable for snapshots) maps
   1:1 onto the brief's model; QUIC's stream multiplexing is the wrong abstraction.
3. **Toggleable encryption fits the philosophy.** QUIC mandates TLS 1.3 (always-on
   CPU cost per packet). renetcode uses optional AES-GCM encryption — a LAN/dev
   build can disable it, consistent with "every cost must be toggleable."
4. **Knowable, bounded cost.** renet's channels have explicit reliability/ordering
   semantics with documented overhead. QUIC's stream multiplexing + TLS handshake
   + congestion control add less-predictable overhead.
5. **Leaner dependency tree.** renet doesn't pull in rustls/TLS — consistent with
   the lean-engine identity.

**Tradeoff acknowledged:** renet is less battle-tested than QUIC/HTTP-3.
**Mitigation:** the M8 prototype validates it end-to-end (two clients + one
server, prediction, interpolation). The protocol types in `pie_runtime::net` are
designed against the message-channel abstraction, not renet specifically, so a
future transport swap is bounded to the transport adapter, not the protocol layer.

### ❓ Asset format for the cooked `.pak` — OPEN (leaning custom binary)

Current v1 uses a simple custom binary format (magic + version + type-tagged
entries). This is adequate for v1; revisit if versioning, incremental updates, or
compression demand a more sophisticated container. Decision recorded here once made.

### ❓ Fixed-point vs floating-point physics for cross-platform determinism — OPEN

v1 starts with floating-point and strict determinism testing. Revisit fixed-point
only if cross-platform drift becomes unacceptable. The determinism requirement is
load-bearing for the M8 networking model (prediction/reconciliation assumes the
client and server produce identical state from identical input).

### ❓ When to move from rayon to a custom fiber-based job system — OPEN

Begin with `rayon` (M9). Only move to a custom fiber scheduler when profiling proves
it is necessary. The bar is high: the custom system must be measurably faster on
real workloads, not just theoretically cleaner.

### ❓ GPU-driven culling scope boundary — OPEN

Set explicit limits so culling and LOD do not expand into a runaway virtual-geometry
project (see Non-Goals). CPU frustum + occlusion culling is the v1/M10 default;
GPU-driven culling is a later, explicitly opt-in upgrade, never the default. The
boundary: GPU-driven culling is in scope only when CPU culling is profiled as the
bottleneck on representative scenes, and even then it must remain toggleable.
