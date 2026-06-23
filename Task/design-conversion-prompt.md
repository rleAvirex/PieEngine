# PieEngine Editor Design Conversion Prompt

You are working inside the `PieEngine` Rust repository.

## Goal

Redesign the native editor UI so it visually matches the premium design language of my website: [rlestudio.cloud](https://www.rlestudio.cloud/).

Important: do **not** convert the editor to HTML/CSS/webview. Keep the existing native Rust editor architecture and reproduce the visual style inside `egui`.

## Current Tech Stack

The editor is a native Rust app using:

- `egui`
- `egui-winit`
- `egui-wgpu`
- `winit`
- `wgpu`

This is **not** a web app and does not support CSS directly.

## Relevant Files

Primary files to update:

- [theme.rs](file:///workspace/PieEngine/pie_editor/src/theme.rs)
- [ui.rs](file:///workspace/PieEngine/pie_editor/src/ui.rs)
- [main.rs](file:///workspace/PieEngine/pie_editor/src/main.rs)
- [Cargo.toml](file:///workspace/PieEngine/pie_editor/Cargo.toml)

You may create small supporting modules inside `pie_editor/src/` if it improves structure, such as:

- `ui_components.rs`
- `design_tokens.rs`

## What The Current UI Looks Like

The current editor is styled like a UE5-inspired dark tool UI. It has:

- a top menu bar
- a toolbar
- a left outliner panel
- a right details/inspector panel
- a central viewport
- a status bar

The current styling is functional but too “editor utility” / “industrial tool” in feel.

## What I Want Instead

I want the editor to feel closer to the design language of `rlestudio.cloud`:

- premium dark theme
- cleaner spacing
- more polished surfaces
- modern cards/sections
- stronger visual hierarchy
- softer, more intentional borders
- modern accent usage
- more refined typography
- less “old-school game editor”, more “premium product UI”

The goal is not to copy the website literally pixel-for-pixel, but to translate its visual identity into a native editor.

## Design Direction

Use the website’s design language as inspiration for:

- background/surface layering
- accent colors
- panel styling
- button treatments
- badges/chips
- typography hierarchy
- spacing rhythm
- section headers
- hover/active/selected states

Target qualities:

- elegant
- modern
- premium
- dark
- clean
- high contrast but not harsh
- visually cohesive

Avoid:

- flat boring utility gray
- overuse of tiny editor-style text
- overly harsh separators
- visually noisy controls
- “stock egui” appearance

## Constraints

Do **not**:

- rewrite the editor into HTML/CSS
- add a webview/Tauri/Electron layer
- break existing editor behavior
- remove runtime/editor separation
- introduce large architectural changes unless absolutely necessary

Do:

- keep the current editor behavior and layout structure intact
- preserve all existing interactions and editor commands
- refactor styling into reusable helper functions/components where useful
- improve maintainability while redesigning

## Required Work

### 1. Build a design token system

Refactor the current theme into a more deliberate design system:

- primary accent
- secondary accent
- success/warning/error colors
- background layers
- card/panel surfaces
- border strengths
- text hierarchy colors
- selection colors
- spacing scale
- radius scale
- shadow/elevation treatment if possible within `egui`

Implement this in `theme.rs` and/or a new small theme helper module.

### 2. Replace the current UE5-styled theme

Update the styling in the editor so it no longer looks like a UE5 clone.

I want a branded visual identity closer to my website.

### 3. Refactor the UI into reusable primitives

Where helpful, create reusable helper functions/components for things like:

- section headers
- card containers
- accent buttons
- secondary buttons
- chip/tag/pill buttons
- stat badges
- property rows
- panel headers
- toolbar controls

Use these helpers across the UI so the design is consistent.

### 4. Restyle the main UI regions

Restyle all major UI areas:

- top bar / menu bar
- toolbar
- outliner
- details panel
- viewport frame/shell
- status/footer area

Focus on visual polish and consistency.

### 5. Improve typography and spacing

Adjust:

- font sizes
- emphasis hierarchy
- label styles
- spacing between controls
- density of lists/panels

The UI should feel more intentional and premium, not cramped.

### 6. Improve interaction states

Polish:

- hover states
- selected states
- pressed states
- active viewport state
- badges and status indicators

These should feel designed, not default.

## Optional Nice-to-Haves

If straightforward and safe, also consider:

- adding icon-like visual affordances using simple text/symbols
- improving empty states
- improving section grouping
- subtle visual emphasis for important actions like Play, Pause, Reload
- branded accent treatment in headers or panel titles

Do not overcomplicate the implementation.

## Expected Output

Make the changes directly in code.

At the end, provide:

1. a short summary of what changed
2. which files were modified
3. any new helper modules created
4. how the new design maps to the `rlestudio.cloud` visual language
5. any tradeoffs or limitations due to `egui`

## Quality Bar

The result should:

- feel noticeably more premium and modern
- feel visually closer to `rlestudio.cloud`
- still behave like a native editor
- compile cleanly
- avoid obvious regressions
- be maintainable, not just a pile of one-off style tweaks

## Validation

After changes:

- run `cargo check --all-targets --all-features`
- if practical, run `cargo test --all-targets --all-features`
- fix any compile issues introduced by the redesign

## Implementation Preference

Prefer this order:

1. improve theme tokens
2. create reusable style helpers
3. restyle top bar and side panels
4. restyle viewport shell and footer
5. do a final consistency pass

## Important Final Instruction

Do not just recolor a few constants.

This should be a real UI redesign within the existing `egui` architecture, with better composition, spacing, visual hierarchy, and reusable styling primitives.