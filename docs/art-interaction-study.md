# Art and interaction study

This note is the visual/UX companion to [`ecosystem-study.md`](ecosystem-study.md).
That document decides what belongs in the framework; this one decides what a
Ferriso surface should *look* and *feel* like, based on the same GUI list.
Repeated entries are grouped by shared lesson here; the ecosystem note keeps
the row-by-row ledger.

The boundary is unchanged: artwork is renderer-local, interaction models are
renderer-local, and neither owns domain state. `design`, `art`, and `interaction`
stay small composable layers rather than a mandatory widget catalog.

## Principles

1. **Information first.** Developer tools and data clients earn their density:
   compact rows, monospace numbers, restrained chrome, and one obvious focus or
   selection state. Decoration must not compete with content.
2. **Keyboard first, not keyboard only.** Every palette, list, and dialog needs
   an escape path and arrow/Home/End/Page navigation. Pointer interactions stay
   available.
3. **Semantic color.** Accent, success, warning, and danger are tokens. Charts
   and status pills also carry text or shape, so color is not the only signal.
4. **Renderer-local motion.** Ambient artwork may run on a frame clock; a
   functional transition settles quickly. Neither wakes the actor.
5. **Compose, don't vendor.** A chart or panel is a canvas/style template.
   Terminal, video, audio, and editor widgets remain opt-in ecosystem crates.

## Project-ledger lessons

| Entries | Art lesson | Interaction lesson | Ferriso decision |
| --- | --- | --- | --- |
| Zed, Fulgur, Arbor, Codux, Farcaster | Native surfaces, restrained chrome, dense typography | Command-first actions, visible focus, terminal/diff panes | Keep the visual language; keep panes and multi-window out of the framework. |
| Lapce, Floem | GPU rendering with a compact workbench | Palette, notifications, breadcrumbs | Reuse palette/toast models; do not add Floem or a reactive store. |
| Helix, helix-tui | Statusline and modal key hints communicate state | Selection-first editing, modal actions | Keyboard hints and selection contrast; modal editing stays in the editor app. |
| GitComet, Rulogman, Baudrun | Dense toolbars and monospace status | Long-running jobs, progress, cancellation, multiple sessions | `Gauge` plus existing command/subscription state; tabs/sessions stay app-local. |
| GetCat, arc | Split request/response view, method/status badges | Import, send, cancel, inspect large bodies | Badges and panels are styles; HTTP and paging stay in the application. |
| DBFlux, DBDelve, FluxDB, based, dbui | Grids with strong row focus and compact metadata | Keyboard navigation, filters, connection state | Palette navigation keys are shared; virtualized table policy stays in the app. |
| Sniffnet, Rerun | Real-time charts, timelines, dark data surfaces | Live counters and frame refresh without blocking | `Bars` is the small shared chart primitive; plotting libraries stay optional. |
| OctaSine, Neothesia, Frostbyte Terminal | Meters, spectra, animation driven by media events | Audio/MIDI/PTY state and background threads | `Bars`/`Gauge` visuals; audio, MIDI, and terminal integration remain opt-in. |
| Halloy, Cfait, Furtherance, onagre, super_clipboard | Message streams, timers, compact list rows | Fuzzy search, unread/selection state, explicit dismissal | Fuzzy ranking and bounded toasts in `interaction`; app state stays app-owned. |
| Airshipper, ajour, microlaunch | Progress-first launcher surfaces | Download/install state machines, concurrent metadata | `Gauge` for bounded progress; command scheduling stays in core/apps. |
| iced_7guis, Iced examples, awesome-iced, iced_aw, iced_video_player, iced_code_editor, iced_audio, iced_term | Small, composable widgets, examples, and an ecosystem index | Direct messages and local widget state | Re-export iced and compose; do not make a catalog mandatory. |
| COSMIC Settings, Launcher, App Library, Applets, Text Editor | Grouped forms, cards, grid/list rhythm | Routing, IPC search, virtualization, incremental text | Adopt spacing/hierarchy lessons; no libcosmic dependency. |
| Windows 11, macOS, Windows 95, Mac OS Classic, CDE | Material, geometry, and typography change with platform | Hit targets and focus remain legible across skins | Existing `Skin` presets; classic bevels are deliberate, not global defaults. |
| Gyroflow, Czkawka/Krokiet, Spacedrive, RustDesk, AppFlowy | Separate shells can share a Rust core | Shell-specific interaction models | Useful comparisons; no Qt/QML/WebView/GTK/Flutter/Sciter dependency. |
| libcosmic, GPUI, Floem, Iced, egui/eframe, Slint, wgpu, helix-tui | Rendering model affects what art is cheap | Ownership model affects focus and scheduling | Keep Iced as the native adapter and the core frontend-neutral; compare, do not clone. |

## Adopted in this pass

### Art

- **`art::Bars`** — a scalable bar/spark strip for traffic, spectrum, activity,
  and progress history. Negative samples are zero; the caller owns sampling,
  windowing, and the clock.
- **`art::Gauge`** — a bounded circular progress arc with center and caption
  text. It fits download/install/import states and countdowns; it does not own
  the state machine.
- **`art::Pulse`** — concentric rings driven by a normalized phase for focus,
  activity, and live beacons. It is an effect, not a state machine.
- **`motion::{Easing, Animation}`** — small renderer-local easing and timeline
  primitives for short transitions. They do not schedule frames, own application
  state, or wake the actor; applications advance them from their own frame
  subscription.
- Existing **aurora** (atmosphere), **orbit** (composition), skins, surfaces,
  chips, and key hints remain the base layer.

### Interaction

- **Fuzzy command ranking.** Exact title matches rank above fuzzy title
  matches; title outranks group, which outranks detail. Ties retain declaration
  order, so results do not jump between identical queries.
- **Page, Home, and End navigation** in `CommandPalette`, alongside the existing
  arrows, Enter, Escape, and Backspace.
- **Empty palette feedback** instead of an unexplained blank list.
- **Searchable combobox.** `Combobox` filters labels and details, skips disabled
  rows during arrow/Page/Home/End navigation, clamps no-match activation, and
  reports the selected original index. It starts unselected so forms control
  their own defaults.
- Existing bounded toasts remain the explicit-dismissal pattern.

The design gallery uses the artwork, fuzzy palette, and toasts in one window.
Its bar strip is sampled from the local clock, the gauge is a fixed bounded
value, and typing a scattered query such as `srf` finds “Run surface audit”.

### Workspace models

- **`ferriso_workspace::Tabs<T>`** — a bounded tab set with exactly one active index when
  non-empty. Closing the active tab selects its neighbor; closing before the
  active tab shifts the index. The application owns tab content and save/close
  policy.
- **`ferriso_workspace::SplitPane<K>`** — a fixed two-pane split with axis, focus, and a
  clamped ratio. Recursive trees and arbitrary pane grids remain application
  policy (or Iced's `pane_grid`).
- **`ferriso_workspace::TableState`** — a fixed-row-height virtual viewport for large
  result sets. It exposes the visible range, selection, page movement, scroll
  clamping, and reveal-on-select; the application owns row data, stable ids,
  columns, and rendering.

The workspace gallery demonstrates all three against 10,000 rows without
building 10,000 widgets. Run it with
`cargo run -p ferriso-gui --example workspace-gallery`.

## Deliberately not adopted

| Temptation | Why not |
| --- | --- |
| A recursive pane tree / rendered tab widget | `Tabs` and `SplitPane` cover the common invariants; arbitrary topology and tab chrome stay application policy. Use Iced's `pane_grid` for recursive splits. |
| A rendered virtual table widget | `TableState` covers viewport and selection math; row data, stable ids, columns, and cell rendering remain application contracts. |
| A bundled icon or asset library | Visual consistency comes from tokens and spacing first; an asset pack would become a dependency and a style constraint. |
| A full plotting library | `Bars` covers compact live strips. Axes, legends, zoom, and tooltips belong in an opt-in crate when an app needs them. |
| A global animation framework | The existing renderer-local clock is enough. Add easing helpers only after a second application repeats the same nontrivial transition. |
| Built-in terminal/video/audio widgets | These have heavy native and platform dependencies. Keep them optional, as the Iced ecosystem does. |
| Accessibility as a token-only change | Focus order, roles, labels, and announcements need a renderer contract. This remains a P2 design task, not a color tweak. |

## Interaction rules for new surfaces

- A list or palette must expose selection, activation, dismissal, and a visible
  empty state. Selection wraps only when the surface is a launcher/palette;
  forms and grids should clamp.
- A destructive action needs a danger tone and an explicit verb; an icon alone
  is insufficient.
- Long-running work needs a determinate gauge when progress is known and a
  textual state when it is not. Do not fake a percentage.
- Functional motion should be brief enough to stay out of the way. Ambient
  shaders may loop, but they must stop when the surface is hidden or idle.
- Charts need a fixed or documented scale. A live strip without a maximum is a
  decoration, not a measurement.
- A keyboard hint describes the current surface; it must not advertise a key
  the renderer does not handle.

## Follow-up

- P1: focus-ring visibility driven by the same focus state the keyboard layer
  exposes.
- P1: a recursive pane tree only if two applications repeat the same
  split/close/overflow logic; use Iced's `pane_grid` until then.
- P2: accessibility semantics; roles, labels, and announcements need a renderer
  contract rather than a color token.
- P2: optional waveform/spectrum primitives if audio applications need more
  than `Bars`.
- P2: shared visual-regression screenshots for the design gallery across skins.
