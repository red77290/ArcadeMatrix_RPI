🇬🇧 English | 🇫🇷 [Français](DEVELOPER_FR.md) | 🇪🇸 [Español](DEVELOPER_ES.md)

# Developer Guide

Welcome to the ArcadeMatrix development guide. This document explains the core architecture of the project and provides step-by-step instructions on how to extend it.

## Architecture: Renderers vs. Clocks

Since the major refactoring, ArcadeMatrix strictly separates the **visual aesthetics (Renderers)** from the **behavioral logic (Clocks)**. Understanding this difference is crucial before you start coding.

To automatically format code before every commit, we have included a pre-commit hook in the repository. Run the following command once to enable it locally:
```bash
git config core.hooksPath .githooks
```
This will ensure `cargo fmt` is always run before any code is committed.

### 1. Renderers (The "Theme")
Located in `engines/renderers/`.
A **Renderer** (e.g., `CyberpunkRenderer`, `FlipRenderer`) is purely aesthetic. It doesn't care if it's displaying the time, the date, or the weather. It takes a text string, a font, and draws it on top of a styled background or visual effect.
- **Responsibility:** Backgrounds, colors, particle effects, transition animations.
- **Advantage:** Highly reusable across different Engines (`ClockEngine`, `DateEngine`, etc.).

### 2. Specialized Clocks (The "Mini-Game")
Located in `engines/clocks/`.
A **Specialized Clock** (e.g., `PongClock`, `TetrisClock`, `PacManClock`) is a dynamic logic engine. It manages an internal state (like a ball bouncing or blocks falling) to construct the time display visually.
- **Responsibility:** Game state, physics, sprite drawing, and generating the time visually rather than just writing a string.
- **Advantage:** Completely autonomous and allows for highly complex, frame-by-frame visualizations.

---

## Extending the Rust Codebase

*Note: ArcadeMatrix was recently rewritten in Rust. The developer tutorials for adding Renderers, Clocks, and Engines are currently being updated to reflect the new Rust architecture (`src/engines/`). In the meantime, you can inspect the existing implementations in `src/engines/renderers` to see how the `Renderer` trait is implemented.*

---

## API & Web UI Integration

Whenever you create a new theme or clock:
1. Update `src/api/server.rs` if your new feature requires new settings.
2. Update `api/www/index.html` to add your new Theme ID to the dropdown menus (`<select id="time_theme">`).

### ⚠️ Frontend source is not in this repository

`api/www/` only contains the **built/bundled** dashboard (`index.html`, `assets/index-*.js`,
`assets/index-*.css` - a minified Vite build, plain JS/HTML/CSS, **not** Vue.js despite older
documentation claiming otherwise). There is no `package.json`, no component sources, and no Vite
config committed here, so the bundle **cannot be rebuilt or meaningfully modified** from this repo
alone - only hand-edited in the already-minified output, which doesn't scale for anything beyond
trivial tweaks (like the theme dropdown entries mentioned above).

If you need to make substantial UI changes, you have two options:
1. Track down wherever the original frontend source project lives (if it still exists) and add it
   back into this repo, e.g. under a new `frontend/` folder, with a build step that outputs into
   `api/www/`.
2. Rebuild a small frontend project from scratch against the existing REST API (see `src/api/server.rs`
   for the full route list) if the original source is truly lost.

Either way, **do not silently keep shipping only a compiled bundle with no documented source of
truth** - if you find/restore the source, commit it and document the build command here.

## Testing Your Code

We enforce a 100% test coverage on API routes. To verify your code:
```bash
cargo test
```

## Fast Local Development Workflow (Cross-Compilation)

For rapid iteration, you don't need to rebuild the entire 14GB `.img` file or compile directly on the slow Raspberry Pi. ArcadeMatrix includes cross-compilation scripts that work on any host OS (Windows, Linux, macOS) as long as Docker is installed.

We provide a comprehensive suite of scripts for building locally and deploying directly to the Raspberry Pi over SSH.

**Please see [SCRIPTS.md](SCRIPTS.md) for full details on how to configure and use the cross-compilation and deployment scripts for your operating system (macOS, Linux, or Windows).**
