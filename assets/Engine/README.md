# Engine Assets

Built-in assets that ship with the PieEngine editor. These are **not** user scene
files — they are editor primitives (gizmo meshes, default textures, etc.).

## Directory Structure

```
assets/Engine/
├── manifest.toml           ← asset registry (paths, descriptions, categories)
├── Gizmos/                 ← gizmo tool meshes
│   ├── GizmosMoveTool.fbx  ← translate gizmo (3-axis arrows + center sphere)
│   └── GizmosSphere.fbx    ← uniform scale handle (center sphere)
└── ...                     ← future engine assets
```

## Naming Convention

All engine assets follow the pattern:

```
<AssetType>/<AssetName>.<ext>
```

| Category | Directory | Prefix | Example |
|---|---|---|---|
| Gizmos | `Gizmos/` | `Gizmos` | `GizmosMoveTool.fbx` |
| Grid | `Grid/` | `Grid` | `GridInfinite.fbx` |
| Icons | `Icons/` | `Icon` | `IconPlay.png` |
| Textures | `Textures/` | `Tex` | `TexDefaultNormal.png` |

## Adding New Engine Assets

1. Place the file in the appropriate subdirectory under `assets/Engine/`
2. Add an entry to `manifest.toml` with the relative path and description
3. Reference the asset in code via `assets_root.join("Engine/<path>")`

## Scale Convention

All gizmo models should be authored at **unit scale** (1 world unit = 1 meter).
The engine applies a runtime scale factor (`GIZMO_WORLD_SCALE` in `gizmo.rs`)
to control the displayed size. Do NOT bake scale into the FBX models.
