# Pie Engine

Pie Engine is a lean Rust game engine focused on raw performance and high-quality graphics.

## Crates

- `pie_runtime`: the shippable runtime used by both the client and headless server
- `pie_editor`: editor-only tooling that depends on `pie_runtime`
- `pie_tools`: CLI utilities for asset cooking and export

## Repository Layout

- `pie_runtime/` contains the runtime application, simulation core, rendering bootstrap, and asset loading
- `pie_editor/` contains the editor entrypoint
- `pie_tools/` contains the headless tooling entrypoint
- `assets/` contains the sample scene, textures, and shaders used by the current vertical slice

## Current Scope

The project is organized around a shared simulation core with separate client and headless modes.

- client mode runs the runtime with rendering enabled
- headless mode runs the same simulation loop without windowing or GPU dependencies
- the current sample content path loads a glTF scene with textures and a basic rendering path

## Common Commands

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo check --all-targets --all-features
cargo test --all-targets --all-features
```

## Notes

- the workspace targets Windows, Linux, and macOS
- `pie_runtime` is designed to stay free of editor dependencies
- the long-term plan is documented in `pie-engine-project-brief.md` and `pie-engine-milestones-and-plan.md`
