# Raster CRT

A free analog video and CRT screen effect for Premiere Pro and After Effects, with its own
control panel for Premiere. It is
our own rebuild of the way GLAZE CRT works: the same kinds of controls, the same panel layout
and the same workflow, with our own maths, look, presets and artwork. For personal use.

Started 13-10-2026 · version 0.1.0 · built and tested on Premiere Pro 26.5.2, After Effects 26.5
and Media Encoder 26, Apple M4.

## Set up from GitHub

Needs a Mac with Apple silicon, Premiere Pro / After Effects 2025+, [Node.js](https://nodejs.org), Rust from [rustup.rs](https://rustup.rs) and `just`.

```bash
git clone https://github.com/vishaal2218/raster-crt.git
cd raster-crt
cargo install just
bash scripts/install.sh
```

Quit Premiere and After Effects first; macOS asks for your password once. Then open **Window ▸ Extensions ▸ Raster CRT**. (The "Install" notes below describe the original Mac this was built on.)

## Install

Raster CRT is already installed on this Mac: the effect is in Adobe's plug-in folder and the
panel is linked from `panel/` here. Nothing else is needed to use it.

### Rebuilding

The build tools (Rust and `just`) and their 3.4 GB of build files were removed on 15-10-2026
to free space, so changing the effect means installing them again first (ask Claude: it
installs Rust with `rustup`, points the `.build` link at the tools, then runs
`bash scripts/install.sh`, which tests, builds and installs; macOS asks for your password once).
Changes to the panel alone need no rebuild: they show after reopening the panel.

## Use it

1. Drag **Effects › Video Effects › Raster › Raster CRT** onto a clip (or double-click it with
   the clip selected).
2. Open **Window › Extensions › Raster CRT**. The panel follows whichever clip you select.
3. Click the amber screen (the LCD) to open the presets. Pick a look; it applies at once.
   **‹ ›** step through the looks, **RESET** clears everything back to the start.
4. Turn a module's knob to bring it in. Every module starts at 0 (off). The **+** opens its
   detail controls; **RESET** on a row resets just that module.
   To turn a knob: drag up or right to turn it up, down or left to turn it down, or scroll on
   it with the trackpad or mouse wheel. Hold **Shift** for fine steps, double-click to reset
   it, or click it and use the arrow keys. Hover over a knob for a reminder.
5. **SERVICE** shows the GLOBAL knobs: Dry/Wet, Brightness, Contrast, every module's amount,
   Chroma Bleed, Clean Signal and Quality. Type a value into any box under a knob.
6. **SAVE** keeps the current look in **My presets**. Same name replaces it (you're asked
   first); the bin icon deletes (click twice).

The LCD shows the look's name, or **CUSTOM · NAME** once you've changed something.
Everything also works from Effect Controls. If a control has keyframes, a change from the
panel adds a keyframe at the playhead, and the module shows a ◆.

**Quality** sets how many lines the picture is rebuilt from (SD 480, HI 720, FULL), so it
changes the look, not the export size. Previews at 1/2 or 1/4 resolution look the same as the
export.

## The modules

| Module | What it does |
| --- | --- |
| Colour Signal | signal gain, chroma level, hue drift, chroma bleed, RGB split, edge sharpen, signal glow, halation |
| Circuit Bend | a resonant filter on the composite TV signal (0.05–5.5 MHz). Near the 3.58 MHz colour subcarrier a hard resonance tints the whole picture: 3.58 green, 3.8 indigo, 3.3 purple, 3.4 orange |
| Damage | line shake, sync tear, vertical roll, exposure flashes, dropouts in 5 styles, hum bar, ghost echo, head-switch band, top-line noise |
| Noise | RF specks (with size) and tape grain |
| Waves | moving bands: count, picture react, smoothness, horizontal or vertical, colour bands, backward, 4 blend modes |
| Screen | 5 phosphor masks, mask scale, phosphor strength, beam softness, phosphor bloom, overscan, tube flicker; with Capture on, also refresh rate, scan wander, supply ripple and **persistence**: bright things that move leave a fading, warm phosphor trail made from the previous three frames |
| Capture | a camera filming the tube: shutter vs refresh scan band, band angle, sensor readout, moiré, rainbow interference |
| Convergence | beam and phosphor misalignment at an angle |
| Glass & Optics | glow, spectral bloom, glass curve, line stretch, fisheye, outside screen opaque or transparent, extend edges |

## Speed

On the M4, steady state, a 1080p frame takes about 11 ms with Screen and Glass on and 16 ms
with every module on; 4K with everything on takes about 48 ms (use 1/2 preview resolution for
smooth 4K playback). Persistence (with Capture on) adds about 1 ms at 1080p and 18 ms at 4K,
plus the time Premiere takes to fetch the three earlier frames. Measure on any picture with
`raster-cli bench`.

## What isn't finished or wasn't tested

Tested live (15-10-2026): every panel control with real mouse drags and clicks (and all 90 by
the automated live test), presets, save and delete, keyframes, the Program Monitor, File ›
Export at 4K, a **Media Encoder** queue render, the **Preset menu in Effect Controls with the
panel paused** (all 79 controls land on the look), **adjustment layers** in Premiere and After
Effects, **After Effects** at 8 and 16 bits, and the persistence trail in both.

- **Undo:** each change from the panel is its own undo step, because Premiere gives panels no
  way to group them, so undoing a long knob drag can take a few presses of Cmd+Z. While you
  drag, the panel sends at most ten changes a second.
- **Premiere's own effect presets** (the Presets bin) weren't made; the effect's Preset menu
  and the panel's browser do that job.
- **Other Macs:** the effect is signed only for this Mac (no Apple developer account), so a
  friend's Mac would need a Gatekeeper exception, and an Intel Mac would need another build.
  Personal use only.
- **Wispr Flow** keeps an invisible window over the right third of the screen. If anything on
  the right of the panel ever ignores clicks, move the panel left or quit Wispr Flow to check.

## Where things live

| What | Where |
| --- | --- |
| The effect | `/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/Raster/Raster CRT.plugin` |
| The panel | a link in `~/Library/Application Support/Adobe/CEP/extensions/com.rastercrt.panel` to `panel/` here |
| Your saved looks | `~/Library/Application Support/Raster CRT/presets.json` (previous version kept as `.bak`) |
| Factory looks | `panel/presets/factory.json` (order is permanent) and `panel/presets/thumbnails/` |
| Panel fonts | `panel/fonts/` (IBM Plex Sans Condensed, IBM Plex Mono, VT323; SIL Open Font Licence), so it looks right offline |
| Problem log | `~/Library/Logs/Raster CRT.log`: things Premiere never shows, such as a GPU failure (each message once per session) |

## For changes later

- `panel/params.json` lists all 80 controls (their ids never change); the effect, engine, panel
  and tests all read it.
- `effect/` (Premiere plug-in, Rust), `engine/` (the GPU pipeline: wgpu + WGSL shaders),
  `cli/` (`raster-cli`: render, presets, scene, card, bench), `panel/` (CEP panel).
- Tests: `cargo test --workspace` (effect, engine, and the GPU render tests) and
  `node --test tests/panel/`.
- Live tests drive the panel through its debug port: `node dev/cdp.mjs <script.js>`, and
  `node dev/cdp-shot.mjs out.png` saves a picture of it. `node dev/controls-test.mjs` drags
  every knob and fader and clicks every switch in the panel, then checks Premiere got the value
  (select a clip with Raster CRT in a test project first; `--only=knobs`, `--module=Screen`).
